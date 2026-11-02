use std::sync::OnceLock;
use std::time::Duration;

/// Cliente HTTP compartido (rustls, UA de Morph, 5 s de timeout).
pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("Morph/", env!("CARGO_PKG_VERSION"), " (COSMIC)"))
            .timeout(Duration::from_secs(5))
            .build()
            .expect("cliente HTTP")
    })
}
