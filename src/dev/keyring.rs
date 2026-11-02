//! Token de GitLab en el Secret Service (colección por defecto). Nunca en disco ni en logs.

use std::collections::HashMap;

use secret_service::{EncryptionType, SecretService};

use super::gitlab::{GlError, GlToken};

pub fn attributes(host: &str) -> [(&'static str, String); 3] {
    [
        ("application", crate::app::APP_ID.to_string()),
        ("service", "gitlab".to_string()),
        ("host", host.to_string()),
    ]
}

/// IDs de aplicación antiguos, del más reciente al más antiguo; solo para migrar.
pub const LEGACY_APPLICATION_IDS: [&str; 2] = ["dev.jam.Morph", "dev.jam.Isla"];

/// Atributos del elemento guardado bajo un ID antiguo; solo para migrar.
pub fn legacy_attributes(host: &str, application: &str) -> [(&'static str, String); 3] {
    let mut a = attributes(host);
    a[0].1 = application.to_string();
    a
}

pub fn label(host: &str) -> String {
    crate::fl!("gl-keyring-label", host = host)
}

fn attrs_map<'a>(a: &'a [(&'static str, String); 3]) -> HashMap<&'a str, &'a str> {
    a.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

// Los errores del llavero se reducen a `GlError::Keyring`: sin detalles (podrían incluir datos).

/// Token guardado para `host`; `None` si no hay. Puede pedir desbloquear el llavero.
pub async fn load(host: &str) -> Result<Option<GlToken>, GlError> {
    let ss = SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(|_| GlError::Keyring)?;
    let col = ss
        .get_default_collection()
        .await
        .map_err(|_| GlError::Keyring)?;
    // Primero se busca (sin desbloquear): solo si hay un elemento y está bloqueado se pide
    // desbloquear, así quien no usa GitLab nunca ve un aviso.
    let attrs = attributes(host);
    let items = col
        .search_items(attrs_map(&attrs))
        .await
        .map_err(|_| GlError::Keyring)?;
    let Some(item) = items.first() else {
        return migrate_legacy(&col, host).await;
    };
    item.ensure_unlocked().await.map_err(|_| GlError::Keyring)?;
    let secret = item.get_secret().await.map_err(|_| GlError::Keyring)?;
    let s = String::from_utf8(secret).map_err(|_| GlError::Keyring)?;
    Ok(Some(GlToken::new(s)))
}

/// Migración única: si hay un elemento con algún `application` antiguo para `host`, lo guarda
/// con los atributos y la etiqueta nuevos y borra los antiguos.
async fn migrate_legacy(
    col: &secret_service::Collection<'_>,
    host: &str,
) -> Result<Option<GlToken>, GlError> {
    let mut found = None;
    for app in LEGACY_APPLICATION_IDS {
        let old_attrs = legacy_attributes(host, app);
        let old = col
            .search_items(attrs_map(&old_attrs))
            .await
            .map_err(|_| GlError::Keyring)?;
        if let Some(item) = old.into_iter().next() {
            found = Some(item);
            break;
        }
    }
    let Some(item) = found else {
        return Ok(None);
    };
    item.ensure_unlocked().await.map_err(|_| GlError::Keyring)?;
    let secret = item.get_secret().await.map_err(|_| GlError::Keyring)?;
    let s = String::from_utf8(secret.clone()).map_err(|_| GlError::Keyring)?;
    let attrs = attributes(host);
    col.create_item(&label(host), attrs_map(&attrs), &secret, true, "text/plain")
        .await
        .map_err(|_| GlError::Keyring)?;
    item.delete().await.map_err(|_| GlError::Keyring)?;
    eprintln!("morph: llavero: token de GitLab migrado a los atributos nuevos");
    Ok(Some(GlToken::new(s)))
}

/// Guarda (o reemplaza) el token de `host` en la colección por defecto.
pub async fn store(host: &str, token: &str) -> Result<(), GlError> {
    let ss = SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(|_| GlError::Keyring)?;
    let col = ss
        .get_default_collection()
        .await
        .map_err(|_| GlError::Keyring)?;
    col.ensure_unlocked().await.map_err(|_| GlError::Keyring)?;
    let attrs = attributes(host);
    col.create_item(
        &label(host),
        attrs_map(&attrs),
        token.as_bytes(),
        true,
        "text/plain",
    )
    .await
    .map_err(|_| GlError::Keyring)?;
    Ok(())
}

/// Borra del llavero el token de `host` (sin error si no había).
pub async fn forget(host: &str) -> Result<(), GlError> {
    let ss = SecretService::connect(EncryptionType::Dh)
        .await
        .map_err(|_| GlError::Keyring)?;
    let col = ss
        .get_default_collection()
        .await
        .map_err(|_| GlError::Keyring)?;
    col.ensure_unlocked().await.map_err(|_| GlError::Keyring)?;
    let attrs = attributes(host);
    let items = col
        .search_items(attrs_map(&attrs))
        .await
        .map_err(|_| GlError::Keyring)?;
    for item in items {
        item.delete().await.map_err(|_| GlError::Keyring)?;
    }
    // Un elemento antiguo sin migrar reaparecería al cargar: se borra también.
    for app in LEGACY_APPLICATION_IDS {
        let old_attrs = legacy_attributes(host, app);
        let old = col
            .search_items(attrs_map(&old_attrs))
            .await
            .map_err(|_| GlError::Keyring)?;
        for item in old {
            item.delete().await.map_err(|_| GlError::Keyring)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_and_label() {
        crate::i18n::apply_for_test("es");
        assert_eq!(
            attributes("gitlab.com"),
            [
                ("application", "io.github.jamthebest.Morph".to_string()),
                ("service", "gitlab".to_string()),
                ("host", "gitlab.com".to_string()),
            ]
        );
        assert_eq!(LEGACY_APPLICATION_IDS, ["dev.jam.Morph", "dev.jam.Isla"]);
        assert_eq!(legacy_attributes("h", "dev.jam.Isla")[0].1, "dev.jam.Isla");
        assert_eq!(
            legacy_attributes("h", "dev.jam.Morph")[0].1,
            "dev.jam.Morph"
        );
        assert_eq!(
            legacy_attributes("h", "dev.jam.Isla")[2],
            ("host", "h".to_string())
        );
        assert_eq!(
            label("gitlab.example.org"),
            "Morph · GitLab (gitlab.example.org)"
        );
    }
}
