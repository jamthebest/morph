//! Cliente de GitLab (REST v4) con el token guardado en el llavero del sistema.

use serde_json::Value;

use super::forge::{ChangeRequest, Forge, Pipeline};
use super::github::{ChecksState, ReviewState, RunStatus};

/// Token en memoria; su `Debug` nunca lo muestra.
#[derive(Clone, Default)]
pub struct GlToken(String);

impl GlToken {
    pub fn new(s: String) -> Self {
        GlToken(s)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for GlToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GlToken(***)")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum GlError {
    NoToken,
    Keyring,
    Unauthorized,
    MissingScope,
    Network(&'static str),
    RateLimited { until: Option<u64> },
    Http(u16),
}

/// Id de proyecto para la URL: el path con `/` codificado (`grupo%2Fsub%2Fproyecto`).
pub fn project_id(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn parse_mr(m: &Value, me: &str) -> Option<ChangeRequest> {
    let number = m.get("iid")?.as_u64()?;
    let url = m.get("web_url")?.as_str()?.to_string();
    let project = m
        .pointer("/references/full")?
        .as_str()?
        .rsplit_once('!')?
        .0
        .to_string();
    let detailed = m.get("detailed_merge_status").and_then(Value::as_str);
    let review = match detailed {
        Some("mergeable") => ReviewState::Approved,
        Some("not_approved") => ReviewState::ReviewRequired,
        Some("requested_changes") => ReviewState::ChangesRequested,
        _ => ReviewState::None,
    };
    let pipeline = m.pointer("/head_pipeline/status").and_then(Value::as_str);
    let checks = match pipeline {
        Some("success") => ChecksState::Success,
        Some("failed") => ChecksState::Failure,
        Some(
            "running" | "pending" | "created" | "waiting_for_resource" | "preparing" | "scheduled",
        ) => ChecksState::Pending,
        Some(_) => ChecksState::None,
        None => match detailed {
            Some("ci_still_running") => ChecksState::Pending,
            Some("ci_must_pass") => ChecksState::Failure,
            _ => ChecksState::None,
        },
    };
    let author = m.pointer("/author/username").and_then(Value::as_str);
    Some(ChangeRequest {
        forge: Forge::GitLab,
        project,
        number,
        title: s(m, "title"),
        url,
        draft: m.get("draft").and_then(Value::as_bool).unwrap_or(false),
        review,
        checks,
        conflicts: m.get("has_conflicts").and_then(Value::as_bool) == Some(true)
            || detailed == Some("conflict"),
        mine: author == Some(me),
        updated: s(m, "updated_at"),
    })
}

/// `json` es el arreglo de MRs de la API; sin duplicados por URL. `mine` = el autor es `me`.
pub fn parse_mrs(json: &Value, me: &str) -> Vec<ChangeRequest> {
    let mut out: Vec<ChangeRequest> = Vec::new();
    for m in json.as_array().into_iter().flatten() {
        if let Some(c) = parse_mr(m, me)
            && !out.iter().any(|x| x.url == c.url)
        {
            out.push(c);
        }
    }
    out
}

pub fn parse_pipelines(project: &str, json: &Value) -> Vec<Pipeline> {
    json.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = p.get("id")?.as_u64()?;
            let (status, conclusion) = match p.get("status").and_then(Value::as_str) {
                Some("running") => (RunStatus::InProgress, None),
                Some("success") => (RunStatus::Completed, Some("success")),
                Some("failed") => (RunStatus::Completed, Some("failure")),
                Some("canceled") => (RunStatus::Completed, Some("cancelled")),
                Some("skipped") => (RunStatus::Completed, Some("skipped")),
                Some("manual") => (RunStatus::Completed, Some("manual")),
                // created, waiting_for_resource, preparing, pending, scheduled...
                _ => (RunStatus::Queued, None),
            };
            let name = s(p, "name");
            Some(Pipeline {
                forge: Forge::GitLab,
                project: project.to_string(),
                id,
                name: if name.is_empty() { "CI".into() } else { name },
                branch: s(p, "ref"),
                status,
                conclusion: conclusion.map(str::to_string),
                url: s(p, "web_url"),
                updated: s(p, "updated_at"),
            })
        })
        .collect()
}

pub fn error_from_status(
    status: u16,
    retry_after: Option<u64>,
    reset: Option<u64>,
    now: u64,
) -> GlError {
    let limited = || GlError::RateLimited {
        until: retry_after.map(|r| now + r).or(reset),
    };
    match status {
        401 => GlError::Unauthorized,
        429 => limited(),
        403 if retry_after.is_some() => limited(),
        403 => GlError::MissingScope,
        c => GlError::Http(c),
    }
}

impl GlError {
    /// Texto para la interfaz; `age_min` = minutos desde el último dato bueno.
    pub fn message(&self, age_min: Option<u64>) -> String {
        match self {
            GlError::NoToken => crate::fl!("gl-err-no-token"),
            GlError::Keyring => crate::fl!("gl-err-keyring"),
            GlError::Unauthorized => crate::fl!("gl-err-unauthorized"),
            GlError::MissingScope => crate::fl!("gl-err-scope"),
            GlError::Network(_) => match age_min {
                Some(n) => crate::fl!("dev-err-offline-ago", n = n),
                None => crate::fl!("dev-err-offline"),
            },
            GlError::RateLimited { .. } => crate::fl!("gl-err-rate-limited"),
            GlError::Http(c) => crate::fl!("gl-err-http", code = u32::from(*c)),
        }
    }

    /// Errores que no se arreglan reintentando: el sondeo se detiene hasta cambiar algo.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            GlError::NoToken | GlError::Keyring | GlError::Unauthorized | GlError::MissingScope
        )
    }
}

/// Resultado de una lectura completa.
#[derive(Clone, Debug)]
pub struct GlRefresh {
    pub mrs: Result<Vec<ChangeRequest>, GlError>,
    /// `Err` solo si falló la lectura de todos los proyectos.
    pub pipelines: Result<Vec<Pipeline>, GlError>,
    /// Proyectos cuyos pipelines no se pudieron leer (se conservan los previos).
    pub failed: Vec<String>,
    /// Proyectos sin acceso (404/403), un subconjunto de `failed`.
    pub no_access: usize,
}

/// Rutas de las dos listas de MRs: creados por mí y con mi revisión pendiente (`scope=all`:
/// sin él, GitLab limita a los MRs creados por el usuario).
pub fn mr_queries(me: &str) -> [String; 2] {
    [
        "/merge_requests?state=opened&scope=created_by_me&per_page=100".to_string(),
        format!(
            "/merge_requests?state=opened&scope=all&reviewer_username={}&per_page=100",
            project_id(me)
        ),
    ]
}

/// Mismo proyecto sin distinguir mayúsculas (GitLab no las distingue en los paths).
pub fn same_project(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Token pegado sin espacios ni saltos de línea alrededor; None si queda vacío.
pub fn clean_token(s: &str) -> Option<GlToken> {
    let t = s.trim();
    (!t.is_empty()).then(|| GlToken::new(t.to_string()))
}

/// Pipelines nuevos más los previos de los proyectos cuya lectura falló.
pub fn keep_failed(prev: &[Pipeline], failed: &[String], fetched: Vec<Pipeline>) -> Vec<Pipeline> {
    let mut out = fetched;
    out.extend(
        prev.iter()
            .filter(|p| failed.iter().any(|f| same_project(f, &p.project)))
            .cloned(),
    );
    out
}

// ---- Red ----

fn client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("morph")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("cliente HTTP de GitLab")
    })
}

fn net_err(e: &reqwest::Error) -> GlError {
    // Mensajes fijos: nunca el texto del error (podría incluir detalles de la petición).
    GlError::Network(if e.is_timeout() {
        "tiempo de espera agotado"
    } else if e.is_connect() {
        "no se pudo conectar"
    } else {
        "error de red"
    })
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn header_u64(h: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    h.get(name)?.to_str().ok()?.trim().parse().ok()
}

/// Servidor válido para formar la URL (solo nombre de host y puerto opcional).
fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
}

async fn get(host: &str, t: &GlToken, path: &str) -> Result<Value, GlError> {
    if !valid_host(host) {
        return Err(GlError::Network("servidor no válido"));
    }
    let resp = client()
        .get(format!("https://{host}/api/v4{path}"))
        .header("PRIVATE-TOKEN", t.as_str())
        .send()
        .await
        .map_err(|e| net_err(&e))?;
    let status = resp.status();
    if !status.is_success() {
        let h = resp.headers();
        return Err(error_from_status(
            status.as_u16(),
            header_u64(h, "retry-after"),
            header_u64(h, "ratelimit-reset"),
            now_epoch(),
        ));
    }
    resp.json::<Value>().await.map_err(|e| net_err(&e))
}

/// Usuario del token. También comprueba el alcance `read_api` (un token con solo
/// `read_user` lee `/user` pero no la API de proyectos).
pub async fn whoami(host: &str, t: &GlToken) -> Result<String, GlError> {
    let me = get(host, t, "/user").await?;
    let name = me
        .get("username")
        .and_then(Value::as_str)
        .ok_or(GlError::Http(200))?
        .to_string();
    get(host, t, "/projects?membership=true&simple=true&per_page=1").await?;
    Ok(name)
}

/// Proyectos del usuario (paths completos), los de actividad más reciente primero.
pub async fn my_projects(host: &str, t: &GlToken) -> Result<Vec<String>, GlError> {
    let v = get(
        host,
        t,
        "/projects?membership=true&simple=true&per_page=100&order_by=last_activity_at",
    )
    .await?;
    Ok(v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| p.get("path_with_namespace")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

fn mr_project(v: &Value) -> Option<&str> {
    v.pointer("/references/full")?
        .as_str()?
        .rsplit_once('!')
        .map(|(p, _)| p)
}

/// MRs abiertos creados por mí o con mi revisión pendiente, de los `projects` elegidos.
async fn fetch_mrs(
    host: &str,
    t: &GlToken,
    me: &str,
    projects: &[String],
) -> Result<Vec<ChangeRequest>, GlError> {
    // Un 403 en una de las listas no es fatal: se sigue con la otra (error solo si fallan ambas).
    let mut lists: Vec<Value> = Vec::new();
    let mut scope_err = None;
    for q in mr_queries(me) {
        match get(host, t, &q).await {
            Ok(v) => lists.push(v),
            Err(GlError::MissingScope) => scope_err = Some(GlError::MissingScope),
            Err(e) => return Err(e),
        }
    }
    if lists.is_empty()
        && let Some(e) = scope_err
    {
        return Err(e);
    }
    let mut raw: Vec<Value> = Vec::new();
    for v in lists {
        for m in v.as_array().into_iter().flatten() {
            if mr_project(m).is_some_and(|p| projects.iter().any(|x| same_project(x, p))) {
                raw.push(m.clone());
            }
        }
    }
    // Sin `head_pipeline` en la lista: se pide el MR suelto (acotado por ciclo).
    let mut detail = 0;
    for m in raw.iter_mut() {
        if m.get("head_pipeline").is_some() || detail >= 10 {
            continue;
        }
        let (Some(pid), Some(iid)) = (
            m.get("project_id").and_then(Value::as_u64),
            m.get("iid").and_then(Value::as_u64),
        ) else {
            continue;
        };
        detail += 1;
        match get(host, t, &format!("/projects/{pid}/merge_requests/{iid}")).await {
            Ok(full) => {
                if let Some(hp) = full.get("head_pipeline") {
                    m["head_pipeline"] = hp.clone();
                }
            }
            // Con el límite alcanzado no se sigue pidiendo.
            Err(e @ GlError::RateLimited { .. }) => return Err(e),
            Err(_) => {}
        }
    }
    Ok(parse_mrs(&Value::Array(raw), me))
}

/// MRs y pipelines de los proyectos elegidos; el error de un proyecto no afecta a los demás.
pub async fn fetch(host: &str, t: &GlToken, projects: &[String]) -> GlRefresh {
    let me = match get(host, t, "/user").await {
        Ok(v) => v
            .get("username")
            .and_then(Value::as_str)
            .map(str::to_string),
        Err(e) => {
            return GlRefresh {
                mrs: Err(e.clone()),
                pipelines: Err(e),
                failed: projects.to_vec(),
                no_access: 0,
            };
        }
    };
    let Some(me) = me else {
        let e = GlError::Http(200);
        return GlRefresh {
            mrs: Err(e.clone()),
            pipelines: Err(e),
            failed: projects.to_vec(),
            no_access: 0,
        };
    };
    let mrs = fetch_mrs(host, t, &me, projects).await;
    let mut out: Vec<Pipeline> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    let mut no_access = 0;
    let mut first_err: Option<GlError> = None;
    for p in projects {
        let path = format!(
            "/projects/{}/pipelines?per_page=5&order_by=updated_at",
            project_id(p)
        );
        match get(host, t, &path).await {
            Ok(v) => out.extend(parse_pipelines(p, &v)),
            Err(e) => {
                failed.push(p.clone());
                if matches!(e, GlError::Http(404) | GlError::MissingScope) {
                    no_access += 1;
                } else {
                    first_err.get_or_insert(e.clone());
                    if matches!(e, GlError::RateLimited { .. } | GlError::Unauthorized) {
                        // Sin sentido seguir pidiendo con el límite o el token rechazado.
                        break;
                    }
                }
            }
        }
    }
    let pipelines = match first_err {
        Some(e) if out.is_empty() => Err(e),
        _ => Ok(out),
    };
    GlRefresh {
        mrs,
        pipelines,
        failed,
        no_access,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::forge::Forge;
    use crate::dev::github::{ChecksState, ReviewState, RunStatus};
    use serde_json::json;

    #[test]
    fn project_id_encodes_slashes() {
        assert_eq!(project_id("grupo/sub/proyecto"), "grupo%2Fsub%2Fproyecto");
        assert_eq!(project_id("a/b"), "a%2Fb");
        assert_eq!(project_id("a/b.c-d_e"), "a%2Fb.c-d_e");
    }

    fn mr(extra: Value) -> Value {
        let mut v = json!({
            "iid": 7, "title": "Arreglo", "web_url": "https://gitlab.com/g/s/p/-/merge_requests/7",
            "draft": false, "has_conflicts": false, "detailed_merge_status": "mergeable",
            "head_pipeline": {"status": "success"}, "updated_at": "2026-10-30T10:00:00Z",
            "author": {"username": "yo"}, "references": {"full": "g/s/p!7"}
        });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        json!([v])
    }

    #[test]
    fn mr_basic_fields_and_mine() {
        let out = parse_mrs(&mr(json!({})), "yo");
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.forge, Forge::GitLab);
        assert_eq!((c.project.as_str(), c.number), ("g/s/p", 7));
        assert_eq!(c.title, "Arreglo");
        assert_eq!(c.updated, "2026-10-30T10:00:00Z");
        assert!(c.mine);
        assert_eq!(c.review, ReviewState::Approved);
        assert_eq!(c.checks, ChecksState::Success);
        assert!(!parse_mrs(&mr(json!({})), "otro")[0].mine);
    }

    #[test]
    fn mr_draft_and_conflicts() {
        let c = &parse_mrs(&mr(json!({"draft": true})), "yo")[0];
        assert!(c.draft);
        let c = &parse_mrs(&mr(json!({"has_conflicts": true})), "yo")[0];
        assert!(c.conflicts);
        let c = &parse_mrs(&mr(json!({"detailed_merge_status": "conflict"})), "yo")[0];
        assert!(c.conflicts);
    }

    #[test]
    fn mr_detailed_status_maps_states() {
        let st = |s: &str, hp: Value| {
            parse_mrs(
                &mr(json!({"detailed_merge_status": s, "head_pipeline": hp})),
                "yo",
            )
            .remove(0)
        };
        assert_eq!(
            st("not_approved", Value::Null).review,
            ReviewState::ReviewRequired
        );
        assert_eq!(
            st("requested_changes", Value::Null).review,
            ReviewState::ChangesRequested
        );
        assert_eq!(st("checking", Value::Null).review, ReviewState::None);
        assert_eq!(
            st("ci_still_running", Value::Null).checks,
            ChecksState::Pending
        );
        assert_eq!(st("ci_must_pass", Value::Null).checks, ChecksState::Failure);
        assert_eq!(
            st("mergeable", json!({"status": "failed"})).checks,
            ChecksState::Failure
        );
        assert_eq!(
            st("mergeable", json!({"status": "running"})).checks,
            ChecksState::Pending
        );
        assert_eq!(
            st("mergeable", json!({"status": "pending"})).checks,
            ChecksState::Pending
        );
    }

    #[test]
    fn mr_null_head_pipeline_is_none() {
        let c = &parse_mrs(
            &mr(json!({"head_pipeline": Value::Null, "detailed_merge_status": "not_approved"})),
            "yo",
        )[0];
        assert_eq!(c.checks, ChecksState::None);
    }

    #[test]
    fn mr_dedup_by_url_and_skips_malformed() {
        let a = mr(json!({}));
        let mut both = a.as_array().unwrap().clone();
        both.push(a[0].clone());
        both.push(json!({"title": "sin iid"}));
        assert_eq!(parse_mrs(&Value::Array(both), "yo").len(), 1);
        assert!(parse_mrs(&json!({"message": "x"}), "yo").is_empty());
    }

    #[test]
    fn pipelines_status_mapping() {
        let j = json!([
            {"id": 1, "status": "running", "ref": "main", "web_url": "u1", "updated_at": "t1"},
            {"id": 2, "status": "pending", "ref": "dev", "web_url": "u2", "updated_at": "t2"},
            {"id": 3, "status": "failed", "ref": "main", "web_url": "u3", "updated_at": "t3"},
            {"id": 4, "status": "success", "ref": "main", "web_url": "u4", "updated_at": "t4"},
            {"id": 5, "status": "canceled", "ref": "main", "web_url": "u5", "updated_at": "t5"},
            {"id": 6, "status": "manual", "ref": "main", "web_url": "u6", "updated_at": "t6"},
            {"id": 7, "status": "created", "ref": "main", "web_url": "u7", "updated_at": "t7"},
            {"id": 8, "status": "waiting_for_resource", "ref": "main", "web_url": "u8", "updated_at": "t8"},
            {"id": 9, "status": "preparing", "ref": "main", "web_url": "u9", "updated_at": "t9"},
            {"status": "failed"}
        ]);
        let p = parse_pipelines("g/s/p", &j);
        assert_eq!(p.len(), 9);
        assert!(
            p.iter()
                .all(|x| x.forge == Forge::GitLab && x.project == "g/s/p")
        );
        assert_eq!(p[0].status, RunStatus::InProgress);
        assert_eq!(p[0].branch, "main");
        assert_eq!(p[0].url, "u1");
        assert_eq!(p[0].updated, "t1");
        assert_eq!(p[0].name, "CI");
        assert_eq!(p[1].status, RunStatus::Queued);
        assert_eq!(
            (p[2].status, p[2].conclusion.as_deref()),
            (RunStatus::Completed, Some("failure"))
        );
        assert_eq!(
            (p[3].status, p[3].conclusion.as_deref()),
            (RunStatus::Completed, Some("success"))
        );
        assert_eq!(p[4].conclusion.as_deref(), Some("cancelled"));
        assert_eq!(p[5].status, RunStatus::Completed);
        assert_ne!(p[5].conclusion.as_deref(), Some("success"));
        assert_ne!(p[5].conclusion.as_deref(), Some("failure"));
        for i in [6, 7, 8] {
            assert_eq!(p[i].status, RunStatus::Queued, "{i}");
        }
        assert!(parse_pipelines("g/p", &json!({"message": "x"})).is_empty());
    }

    #[test]
    fn errors_from_status() {
        assert_eq!(
            error_from_status(401, None, None, 100),
            GlError::Unauthorized
        );
        assert_eq!(
            error_from_status(403, None, None, 100),
            GlError::MissingScope
        );
        assert_eq!(
            error_from_status(429, Some(30), None, 100),
            GlError::RateLimited { until: Some(130) }
        );
        assert_eq!(
            error_from_status(429, None, Some(500), 100),
            GlError::RateLimited { until: Some(500) }
        );
        assert_eq!(
            error_from_status(429, None, None, 100),
            GlError::RateLimited { until: None }
        );
        assert_eq!(
            error_from_status(403, Some(5), None, 100),
            GlError::RateLimited { until: Some(105) }
        );
        assert_eq!(error_from_status(500, None, None, 100), GlError::Http(500));
    }

    #[test]
    fn debug_never_shows_token() {
        let t = GlToken::new("glpat-SECRET123".into());
        assert!(!format!("{t:?}").contains("SECRET"));
        assert!(!format!("{:?}", Some(t.clone())).contains("SECRET"));
        assert_eq!(t.as_str(), "glpat-SECRET123");
    }

    #[test]
    fn messages_and_fatal() {
        crate::i18n::apply_for_test("es");
        assert_eq!(GlError::Unauthorized.message(None), "Token no válido");
        assert_eq!(
            GlError::MissingScope.message(None),
            "El token necesita permiso read_api"
        );
        assert_eq!(
            GlError::Keyring.message(None),
            "No se pudo usar el llavero del sistema"
        );
        assert!(GlError::Unauthorized.is_fatal() && !GlError::Http(500).is_fatal());
        assert!(!GlError::RateLimited { until: None }.is_fatal());
    }

    #[test]
    fn keep_failed_retains_previous_of_failed_projects() {
        let p = |proj: &str, id| {
            parse_pipelines(proj, &json!([{"id": id, "status": "running"}])).remove(0)
        };
        let prev = vec![p("a/x", 1), p("b/y", 2)];
        let out = keep_failed(&prev, &["b/y".to_string()], vec![p("a/x", 3)]);
        assert_eq!(out.iter().map(|x| x.id).collect::<Vec<_>>(), [3, 2]);
    }

    #[test]
    fn mr_queries_use_scope_all_for_reviewer() {
        let [mine, review] = mr_queries("yo");
        assert_eq!(
            mine,
            "/merge_requests?state=opened&scope=created_by_me&per_page=100"
        );
        assert_eq!(
            review,
            "/merge_requests?state=opened&scope=all&reviewer_username=yo&per_page=100"
        );
        assert!(mr_queries("a b&c")[1].contains("reviewer_username=a%20b%26c"));
    }

    #[test]
    fn project_matching_ignores_case() {
        assert!(same_project("Grupo/Sub/Proy", "grupo/sub/proy"));
        assert!(!same_project("a/b", "a/c"));
        let p = |proj: &str, id| {
            parse_pipelines(proj, &json!([{"id": id, "status": "running"}])).remove(0)
        };
        let out = keep_failed(&[p("B/Y", 2)], &["b/y".to_string()], vec![]);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn clean_token_trims() {
        assert_eq!(clean_token("  glpat-x\n").unwrap().as_str(), "glpat-x");
        assert!(clean_token(" \n ").is_none());
    }
}
