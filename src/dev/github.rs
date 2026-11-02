//! Cliente de GitHub (PRs por GraphQL, Actions por REST) con el token de `gh auth token`.

use std::collections::HashMap;

use serde_json::Value;

use super::accounts::{self, Access};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    ReviewRequired,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksState {
    Success,
    Failure,
    Pending,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pr {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub draft: bool,
    pub review: ReviewState,
    pub checks: ChecksState,
    pub conflicts: bool,
    /// false = me piden revisión.
    pub mine: bool,
    pub updated: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStatus {
    Queued,
    InProgress,
    Completed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub repo: String,
    pub id: u64,
    pub name: String,
    pub branch: String,
    pub title: String,
    pub status: RunStatus,
    pub conclusion: Option<String>,
    pub url: String,
    pub updated: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GhError {
    NoGh,
    NotLoggedIn,
    Network(String),
    RateLimited {
        until: Option<u64>,
    },
    Http(u16),
    /// GraphQL devolvió errores (repo inexistente o sin acceso, etc.).
    Query,
}

/// Token en memoria; su `Debug` nunca lo muestra.
#[derive(Clone)]
pub struct Token(String);

impl Token {
    pub fn new(s: String) -> Self {
        Token(s)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(***)")
    }
}

/// Cuentas de `gh`, sus tokens (solo en memoria) y qué cuenta ve cada repo.
#[derive(Clone, Debug, Default)]
pub struct GhState {
    /// Inicios de sesión de github.com (la cuenta activa primero); vacío = sin descubrir.
    pub accounts: Vec<String>,
    pub tokens: HashMap<String, Token>,
    pub access: HashMap<String, Access>,
    /// Última lista de cuentas descubierta (para notar un `gh auth login` nuevo).
    pub known: Vec<String>,
    /// Cuentas cuyo `gh auth token --user` falló, hasta el próximo descubrimiento.
    pub failed: std::collections::HashSet<String>,
}

/// Resultado de una lectura completa (estado para reutilizarlo, y datos o error).
#[derive(Clone, Debug)]
pub struct Refresh {
    pub state: GhState,
    /// Repos elegidos que ninguna cuenta ve.
    pub no_access: usize,
    pub prs: Result<Vec<Pr>, GhError>,
    pub runs: Result<Vec<Run>, GhError>,
}

impl GhError {
    /// Texto para la interfaz; `age_min` = minutos desde el último dato bueno.
    pub fn message(&self, age_min: Option<u64>) -> String {
        match self {
            GhError::NoGh => crate::fl!("gh-err-not-installed"),
            GhError::NotLoggedIn => crate::fl!("gh-err-login"),
            GhError::Network(_) => match age_min {
                Some(n) => crate::fl!("dev-err-offline-ago", n = n),
                None => crate::fl!("dev-err-offline"),
            },
            GhError::RateLimited { .. } => crate::fl!("gh-err-rate-limited"),
            GhError::Query => crate::fl!("gh-err-query"),
            GhError::Http(c) => crate::fl!("gh-err-http", code = u32::from(*c)),
        }
    }
}

/// Error de una respuesta de GraphQL (200 con `errors`); None si no hay.
pub fn graphql_error(v: &Value) -> Option<GhError> {
    let errs = v.get("errors")?.as_array().filter(|e| !e.is_empty())?;
    if errs
        .iter()
        .any(|e| e.get("type").and_then(Value::as_str) == Some("RATE_LIMITED"))
    {
        Some(GhError::RateLimited { until: None })
    } else {
        Some(GhError::Query)
    }
}

/// Ejecuciones nuevas más las previas de los repos cuya lectura falló, por fecha, máx. 10 más las pendientes.
pub fn merge_runs(prev: &[Run], failed_repos: &[String], fetched: Vec<Run>) -> Vec<Run> {
    let mut out = fetched;
    out.extend(
        prev.iter()
            .filter(|r| failed_repos.contains(&r.repo))
            .cloned(),
    );
    out.sort_by(|a, b| b.updated.cmp(&a.updated));
    // El tope de 10 no descarta ejecuciones pendientes: así no se pierde su aviso de fin.
    let rest = out.split_off(out.len().min(10));
    out.extend(
        rest.into_iter()
            .filter(|r| r.status != RunStatus::Completed),
    );
    out
}

pub fn search_queries(repos: &[String]) -> Vec<String> {
    repos
        .chunks(20)
        .map(|c| {
            let mut q = String::from("is:pr is:open archived:false");
            for r in c {
                q.push_str(" repo:");
                q.push_str(r);
            }
            q
        })
        .collect()
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn parse_pr(n: &Value, mine_login: &str) -> Option<Pr> {
    let url = n.get("url")?.as_str()?.to_string();
    let number = n.get("number")?.as_u64()?;
    let repo = n
        .pointer("/repository/nameWithOwner")?
        .as_str()?
        .to_string();
    let review = match n.get("reviewDecision").and_then(Value::as_str) {
        Some("APPROVED") => ReviewState::Approved,
        Some("CHANGES_REQUESTED") => ReviewState::ChangesRequested,
        Some("REVIEW_REQUIRED") => ReviewState::ReviewRequired,
        _ => ReviewState::None,
    };
    let checks = match n
        .pointer("/commits/nodes/0/commit/statusCheckRollup/state")
        .and_then(Value::as_str)
    {
        Some("SUCCESS") => ChecksState::Success,
        Some("FAILURE" | "ERROR") => ChecksState::Failure,
        Some("PENDING" | "EXPECTED") => ChecksState::Pending,
        _ => ChecksState::None,
    };
    let author = n.pointer("/author/login").and_then(Value::as_str);
    Some(Pr {
        repo,
        number,
        title: s(n, "title"),
        url,
        draft: n.get("isDraft").and_then(Value::as_bool).unwrap_or(false),
        review,
        checks,
        conflicts: n.get("mergeable").and_then(Value::as_str) == Some("CONFLICTING"),
        mine: author == Some(mine_login),
        updated: s(n, "updatedAt"),
    })
}

/// `json` es la respuesta de GraphQL (o su `data`): lee todos los alias de búsqueda,
/// sin duplicados por URL. `mine` = el autor es `mine_login`.
pub fn parse_prs(json: &Value, mine_login: &str) -> Vec<Pr> {
    let data = json.get("data").unwrap_or(json);
    let mut out: Vec<Pr> = Vec::new();
    let Some(obj) = data.as_object() else {
        return out;
    };
    for (_, v) in obj {
        let Some(nodes) = v.get("nodes").and_then(Value::as_array) else {
            continue;
        };
        for n in nodes {
            if let Some(pr) = parse_pr(n, mine_login)
                && !out.iter().any(|p| p.url == pr.url)
            {
                out.push(pr);
            }
        }
    }
    out
}

pub fn parse_runs(repo: &str, json: &Value) -> Vec<Run> {
    let Some(list) = json.get("workflow_runs").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|r| {
            let id = r.get("id")?.as_u64()?;
            let status = match r.get("status").and_then(Value::as_str) {
                Some("completed") => RunStatus::Completed,
                Some("in_progress") => RunStatus::InProgress,
                _ => RunStatus::Queued,
            };
            Some(Run {
                repo: repo.to_string(),
                id,
                name: s(r, "name"),
                branch: s(r, "head_branch"),
                title: s(r, "display_title"),
                status,
                conclusion: r
                    .get("conclusion")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                url: s(r, "html_url"),
                updated: s(r, "updated_at"),
            })
        })
        .collect()
}

/// Error a partir de un código HTTP y cabeceras de límite (`now` = epoch en segundos).
pub fn error_from_status(
    status: u16,
    retry_after: Option<u64>,
    reset: Option<u64>,
    remaining: Option<u64>,
    now: u64,
) -> GhError {
    match status {
        401 => GhError::NotLoggedIn,
        429 => GhError::RateLimited {
            until: retry_after.map(|r| now + r).or(reset),
        },
        403 if retry_after.is_some() || remaining == Some(0) => GhError::RateLimited {
            until: retry_after.map(|r| now + r).or(reset),
        },
        c => GhError::Http(c),
    }
}

// ---- Red ----

const API: &str = "https://api.github.com";

// `first:50` por búsqueda (autor / revisión): suficiente para uso personal; si hubiera más,
// los más recientes quedan fuera sin paginar.
const PR_QUERY: &str = "query($q1:String!,$q2:String!){viewer{login} \
mine:search(query:$q1,type:ISSUE,first:50){nodes{...F}} \
review:search(query:$q2,type:ISSUE,first:50){nodes{...F}}} \
fragment F on PullRequest{number title url updatedAt isDraft reviewDecision mergeable \
repository{nameWithOwner} author{login} \
commits(last:1){nodes{commit{statusCheckRollup{state}}}}}";

fn client() -> &'static reqwest::Client {
    use std::sync::OnceLock;
    static C: OnceLock<reqwest::Client> = OnceLock::new();
    C.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("morph")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("cliente HTTP de GitHub")
    })
}

fn net_err(e: &reqwest::Error) -> GhError {
    // Mensajes fijos: nunca el texto del error (podría incluir detalles de la petición).
    GhError::Network(
        if e.is_timeout() {
            "tiempo de espera agotado"
        } else if e.is_connect() {
            "no se pudo conectar"
        } else {
            "error de red"
        }
        .into(),
    )
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn header_u64(h: &reqwest::header::HeaderMap, name: &str) -> Option<u64> {
    h.get(name)?.to_str().ok()?.trim().parse().ok()
}

async fn send(rb: reqwest::RequestBuilder, token: &str) -> Result<Value, GhError> {
    let resp = rb
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| net_err(&e))?;
    let status = resp.status();
    if !status.is_success() {
        let h = resp.headers();
        return Err(error_from_status(
            status.as_u16(),
            header_u64(h, "retry-after"),
            header_u64(h, "x-ratelimit-reset"),
            header_u64(h, "x-ratelimit-remaining"),
            now_epoch(),
        ));
    }
    resp.json::<Value>().await.map_err(|e| net_err(&e))
}

/// Nombre interno de la cuenta cuando no se pudo leer `hosts.yml` (se usa la activa).
pub const ACTIVE_FALLBACK: &str = "(activa)";

/// `gh auth token` de una cuenta (proceso directo, sin shell). El token nunca se registra.
pub async fn token_for(user: Option<&str>) -> Result<String, GhError> {
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args(["auth", "token"]);
    if let Some(u) = user {
        if !accounts::valid_login(u) {
            return Err(GhError::NotLoggedIn);
        }
        cmd.args(["--hostname", "github.com", "--user", u]);
    }
    let out = cmd
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                GhError::NoGh
            } else {
                GhError::Network("no se pudo ejecutar gh".into())
            }
        })?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !t.is_empty() {
        Ok(t)
    } else {
        Err(GhError::NotLoggedIn)
    }
}

/// `token_for` con tope de tiempo (un `gh` colgado no bloquea el sondeo).
async fn token_timeout(user: Option<&str>) -> Result<String, GhError> {
    tokio::time::timeout(std::time::Duration::from_secs(10), token_for(user))
        .await
        .unwrap_or_else(|_| Err(GhError::Network("gh no respondió".into())))
}

/// Carpeta de configuración de `gh` (`$GH_CONFIG_DIR`, `$XDG_CONFIG_HOME/gh` o `~/.config/gh`).
fn gh_config_dir() -> Option<std::path::PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    var("GH_CONFIG_DIR")
        .map(Into::into)
        .or_else(|| var("XDG_CONFIG_HOME").map(|x| std::path::PathBuf::from(x).join("gh")))
        .or_else(|| var("HOME").map(|h| std::path::PathBuf::from(h).join(".config/gh")))
}

/// Cuentas de github.com según `hosts.yml` (la activa primero); vacío si no se puede leer.
async fn discover_accounts() -> Vec<String> {
    tokio::task::spawn_blocking(|| {
        gh_config_dir()
            .and_then(|d| std::fs::File::open(d.join("hosts.yml")).ok())
            .map(|f| accounts::parse_hosts_reader(std::io::BufReader::new(f)))
            .unwrap_or_default()
    })
    .await
    .unwrap_or_default()
}

fn user_arg(login: &str) -> Option<&str> {
    (login != ACTIVE_FALLBACK).then_some(login)
}

/// Resultado de leer los repos de una cuenta.
pub struct GroupOut {
    pub repos: Vec<String>,
    pub prs: Result<Vec<Pr>, GhError>,
    pub runs: Result<RunsRaw, GhError>,
}

/// Datos combinados de todas las cuentas y los repos cuya cuenta hay que volver a resolver.
pub struct Combined {
    pub prs: Result<Vec<Pr>, GhError>,
    pub runs: Result<RunsRaw, GhError>,
    pub dropped: Vec<String>,
}

/// Une los grupos: un 401/HTTP en una cuenta solo la descarta (sus repos se vuelven a
/// resolver); límite, red o consulta fallida son globales; si fallan todas, error global.
pub fn combine_groups(groups: Vec<GroupOut>) -> Combined {
    let local = |e: &GhError| matches!(e, GhError::NotLoggedIn | GhError::Http(_));
    let mut prs: Vec<Pr> = Vec::new();
    let mut runs: Vec<Run> = Vec::new();
    let mut bad: Vec<(String, u16)> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    let (mut prs_err, mut runs_err): (Option<GhError>, Option<GhError>) = (None, None);
    let mut local_errs: Vec<GhError> = Vec::new();
    let mut succeeded = 0;
    for g in groups {
        let lerr = [g.prs.as_ref().err(), g.runs.as_ref().err()]
            .into_iter()
            .flatten()
            .find(|e| local(e));
        if let Some(e) = lerr {
            local_errs.push(e.clone());
            bad.extend(g.repos.iter().map(|r| (r.clone(), 0)));
            dropped.extend(g.repos);
            continue;
        }
        succeeded += 1;
        match g.prs {
            Ok(v) => {
                for pr in v {
                    if !prs.iter().any(|x| x.url == pr.url) {
                        prs.push(pr);
                    }
                }
            }
            Err(e) => prs_err = prs_err.or(Some(e)),
        }
        match g.runs {
            Ok((v, f)) => {
                runs.extend(v);
                bad.extend(f);
            }
            Err(e) => runs_err = runs_err.or(Some(e)),
        }
    }
    if succeeded == 0
        && let Some(e) = local_errs.into_iter().next()
    {
        return Combined {
            prs: Err(e.clone()),
            runs: Err(e),
            dropped,
        };
    }
    Combined {
        prs: prs_err.map_or(Ok(prs), Err),
        runs: runs_err.map_or(Ok((runs, bad)), Err),
        dropped,
    }
}

impl GhState {
    /// Anota el resultado de un descubrimiento de cuentas.
    fn note_discovered(&mut self, found: &[String]) {
        if self.known != found {
            accounts::drop_no_access(&mut self.access);
            self.known = found.to_vec();
        }
        self.failed.clear();
    }

    /// Descubre las cuentas si falta; sin `hosts.yml` legible usa la sesión activa de `gh`.
    async fn ensure_accounts(&mut self) -> Result<(), GhError> {
        if !self.accounts.is_empty() {
            return Ok(());
        }
        let found = discover_accounts().await;
        self.note_discovered(&found);
        if found.is_empty() {
            let t = token_timeout(None).await?;
            self.accounts = vec![ACTIVE_FALLBACK.to_string()];
            self.tokens = HashMap::from([(ACTIVE_FALLBACK.to_string(), Token::new(t))]);
        } else {
            self.tokens.retain(|k, _| found.contains(k));
            self.accounts = found;
        }
        Ok(())
    }

    async fn ensure_token(&mut self, login: &str) -> Result<Token, GhError> {
        if let Some(t) = self.tokens.get(login) {
            return Ok(t.clone());
        }
        if self.failed.contains(login) {
            return Err(GhError::NotLoggedIn);
        }
        let t = match token_timeout(user_arg(login)).await {
            Ok(t) => Token::new(t),
            Err(GhError::NotLoggedIn) => {
                self.failed.insert(login.to_string());
                return Err(GhError::NotLoggedIn);
            }
            Err(e) => return Err(e),
        };
        self.tokens.insert(login.to_string(), t.clone());
        Ok(t)
    }

    async fn renew_token(&mut self, login: &str) -> Result<Token, GhError> {
        self.tokens.remove(login);
        self.ensure_token(login).await
    }
}

pub async fn fetch_prs(token: &str, repos: &[String]) -> Result<Vec<Pr>, GhError> {
    let mut out: Vec<Pr> = Vec::new();
    for base in search_queries(repos) {
        let body = serde_json::json!({
            "query": PR_QUERY,
            "variables": {
                "q1": format!("{base} author:@me"),
                "q2": format!("{base} review-requested:@me"),
            },
        });
        let v = send(client().post(format!("{API}/graphql")).json(&body), token).await?;
        if let Some(e) = graphql_error(&v) {
            return Err(e);
        }
        let Some(login) = v.pointer("/data/viewer/login").and_then(Value::as_str) else {
            return Err(GhError::Network("respuesta de GraphQL inválida".into()));
        };
        for pr in parse_prs(&v, login) {
            if !out.iter().any(|p| p.url == pr.url) {
                out.push(pr);
            }
        }
    }
    Ok(out)
}

/// Ejecuciones de los repos y los repos cuya lectura falló (con su código HTTP).
pub async fn fetch_runs_raw(token: &str, repos: &[String]) -> Result<RunsRaw, GhError> {
    let mut out: Vec<Run> = Vec::new();
    let mut failed: Vec<(String, u16)> = Vec::new();
    for r in repos {
        let url = format!("{API}/repos/{r}/actions/runs?per_page=5");
        match send(client().get(url), token).await {
            Ok(v) => out.extend(parse_runs(r, &v)),
            Err(GhError::Http(c)) => failed.push((r.clone(), c)),
            Err(e) => return Err(e),
        }
    }
    Ok((out, failed))
}

type RunsRaw = (Vec<Run>, Vec<(String, u16)>);

pub async fn list_my_repos(token: &str) -> Result<Vec<String>, GhError> {
    let url = format!(
        "{API}/user/repos?per_page=100&sort=pushed&affiliation=owner,collaborator,organization_member"
    );
    let v = send(client().get(url), token).await?;
    Ok(v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| r.get("full_name")?.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}

/// Repos de una cuenta tal como los muestra el selector (o el error ya en texto).
pub type AccountRepos = Vec<(String, Result<Vec<String>, String>)>;

/// Repos de cada cuenta para el selector «+ Repos»; el error de una cuenta no afecta a las demás.
pub async fn picker_accounts(mut st: GhState) -> Result<AccountRepos, String> {
    st.ensure_accounts().await.map_err(|e| e.message(None))?;
    let mut out = Vec::new();
    for a in st.accounts.clone() {
        let r = match st.ensure_token(&a).await {
            Ok(t) => list_my_repos(t.as_str()).await,
            Err(e) => Err(e),
        };
        out.push((a, r.map_err(|e| e.message(None))));
    }
    Ok(out)
}

/// ¿La cuenta ve el repo? 404/403 = no; otros errores se propagan.
async fn repo_visible(token: &str, repo: &str) -> Result<bool, GhError> {
    match send(client().get(format!("{API}/repos/{repo}")), token).await {
        Ok(_) => Ok(true),
        Err(GhError::Http(403 | 404)) => Ok(false),
        Err(e) => Err(e),
    }
}

/// `repo_visible` que renueva el token una vez ante un 401.
async fn visible_by(st: &mut GhState, login: &str, repo: &str) -> Result<bool, GhError> {
    let tok = st.ensure_token(login).await?;
    match repo_visible(tok.as_str(), repo).await {
        Err(GhError::NotLoggedIn) => {
            let tok = st.renew_token(login).await?;
            repo_visible(tok.as_str(), repo).await
        }
        r => r,
    }
}

/// Averigua qué cuenta ve cada repo aún sin resolver (la activa primero).
async fn resolve_access(
    st: &mut GhState,
    repos: &[String],
    usable: &[String],
) -> Result<(), GhError> {
    for r in repos {
        if st.access.contains_key(r) {
            continue;
        }
        let mut found = Access::NoAccess;
        if accounts::normalize_manual(r).as_deref() == Some(r) {
            for a in usable {
                match visible_by(st, a, r).await {
                    Ok(true) => {
                        found = Access::Account(a.clone());
                        break;
                    }
                    Ok(false) | Err(GhError::NotLoggedIn) => {}
                    Err(e) => return Err(e),
                }
            }
        }
        st.access.insert(r.clone(), found);
    }
    Ok(())
}

type GroupFetch = (Result<Vec<Pr>, GhError>, Result<RunsRaw, GhError>);

async fn fetch_group_once(token: &str, repos: &[String]) -> GroupFetch {
    let runs = fetch_runs_raw(token, repos).await;
    let prs = fetch_prs(token, repos).await;
    (prs, runs)
}

fn unauthorized(g: &GroupFetch) -> bool {
    matches!(g.0, Err(GhError::NotLoggedIn)) || matches!(g.1, Err(GhError::NotLoggedIn))
}

/// PRs y ejecuciones de los repos de una cuenta; renueva su token una vez ante un 401.
async fn fetch_group(st: &mut GhState, login: &str, repos: &[String]) -> GroupFetch {
    let tok = match st.ensure_token(login).await {
        Ok(t) => t,
        Err(e) => return (Err(e.clone()), Err(e)),
    };
    let mut res = fetch_group_once(tok.as_str(), repos).await;
    if unauthorized(&res) {
        match st.renew_token(login).await {
            Ok(t) => res = fetch_group_once(t.as_str(), repos).await,
            Err(e) => return (Err(e.clone()), Err(e)),
        }
        if unauthorized(&res) {
            st.tokens.remove(login);
        }
    }
    res
}

fn fail(state: GhState, repos: &[String], e: GhError) -> Refresh {
    Refresh {
        no_access: accounts::count_no_access(repos, &state.access),
        state,
        prs: Err(e.clone()),
        runs: Err(e),
    }
}

/// Lectura completa: descubre cuentas, resuelve qué cuenta ve cada repo y lee por cuenta.
pub async fn refresh(mut st: GhState, repos: Vec<String>, prev: Vec<Run>) -> Refresh {
    if let Err(e) = st.ensure_accounts().await {
        return fail(st, &repos, e);
    }
    let mut usable = Vec::new();
    let mut last_err = None;
    for a in st.accounts.clone() {
        match st.ensure_token(&a).await {
            Ok(_) => usable.push(a),
            Err(e) => last_err = Some(e),
        }
    }
    if usable.is_empty() {
        // Se vuelve a descubrir en la próxima lectura (p. ej. tras `gh auth login`).
        st.accounts.clear();
        return fail(st, &repos, last_err.unwrap_or(GhError::NotLoggedIn));
    }
    accounts::prune_access(&mut st.access, &repos, &usable);
    if let Err(e) = resolve_access(&mut st, &repos, &usable).await {
        return fail(st, &repos, e);
    }
    let mut outs: Vec<GroupOut> = Vec::new();
    for (login, group) in accounts::group_by_account(&repos, &st.access) {
        let (prs, runs) = fetch_group(&mut st, &login, &group).await;
        let limited = matches!(prs, Err(GhError::RateLimited { .. }))
            || matches!(runs, Err(GhError::RateLimited { .. }));
        outs.push(GroupOut {
            repos: group,
            prs,
            runs,
        });
        // Con el límite agotado no se sigue con las demás cuentas.
        if limited {
            break;
        }
    }
    let c = combine_groups(outs);
    // Cuentas que fallaron: sus repos se vuelven a resolver en la próxima lectura.
    for r in &c.dropped {
        st.access.remove(r);
    }
    let runs = c.runs.map(|(fetched, bad)| {
        // Un 404 en un repo invalida su cuenta: se vuelve a resolver en la próxima lectura.
        for (r, code) in &bad {
            if *code == 404 {
                st.access.remove(r);
            }
        }
        let names: Vec<String> = bad.into_iter().map(|(r, _)| r).collect();
        merge_runs(&prev, &names, fetched)
    });
    Refresh {
        no_access: accounts::count_no_access(&repos, &st.access),
        state: st,
        prs: c.prs,
        runs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(
        n: u64,
        login: &str,
        draft: bool,
        review: Value,
        state: Value,
        mergeable: &str,
    ) -> Value {
        json!({
            "number": n, "title": format!("PR {n}"), "url": format!("https://github.com/o/r/pull/{n}"),
            "isDraft": draft, "reviewDecision": review, "mergeable": mergeable,
            "repository": {"nameWithOwner": "o/r"}, "updatedAt": "2026-10-30T12:00:00Z",
            "author": {"login": login},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": state.as_str().map(|s| json!({"state": s}))}}]}
        })
    }

    fn data(mine: Vec<Value>, req: Vec<Value>) -> Value {
        json!({"data": {"viewer": {"login": "yo"},
            "mine": {"nodes": mine}, "review": {"nodes": req}}})
    }

    #[test]
    fn parse_prs_states() {
        let d = data(
            vec![
                node(
                    1,
                    "yo",
                    false,
                    json!("APPROVED"),
                    json!("SUCCESS"),
                    "MERGEABLE",
                ),
                node(
                    2,
                    "yo",
                    true,
                    json!("CHANGES_REQUESTED"),
                    json!("FAILURE"),
                    "CONFLICTING",
                ),
                node(
                    3,
                    "yo",
                    false,
                    json!("REVIEW_REQUIRED"),
                    json!("PENDING"),
                    "UNKNOWN",
                ),
                node(4, "yo", false, Value::Null, Value::Null, "MERGEABLE"),
            ],
            vec![node(
                5,
                "otra",
                false,
                Value::Null,
                json!("ERROR"),
                "MERGEABLE",
            )],
        );
        let p = parse_prs(&d, "yo");
        assert_eq!(p.len(), 5);
        assert_eq!(p[0].review, ReviewState::Approved);
        assert_eq!(p[0].checks, ChecksState::Success);
        assert!(!p[0].conflicts && !p[0].draft && p[0].mine);
        assert_eq!(p[0].repo, "o/r");
        assert_eq!(p[0].updated, "2026-10-30T12:00:00Z");
        assert_eq!(p[0].url, "https://github.com/o/r/pull/1");
        assert_eq!(p[1].review, ReviewState::ChangesRequested);
        assert_eq!(p[1].checks, ChecksState::Failure);
        assert!(p[1].conflicts && p[1].draft);
        assert_eq!(p[2].review, ReviewState::ReviewRequired);
        assert_eq!(p[2].checks, ChecksState::Pending);
        assert_eq!(p[3].review, ReviewState::None);
        assert_eq!(p[3].checks, ChecksState::None);
        assert!(!p[4].mine);
        assert_eq!(p[4].checks, ChecksState::Failure);
    }

    #[test]
    fn parse_prs_dedupes_and_tolerates_bad_nodes() {
        let n = node(1, "yo", false, Value::Null, Value::Null, "MERGEABLE");
        let d = data(vec![n.clone(), json!({})], vec![n, Value::Null]);
        assert_eq!(parse_prs(&d, "yo").len(), 1);
        assert!(parse_prs(&json!({}), "yo").is_empty());
    }

    #[test]
    fn parse_runs_fields() {
        let j = json!({"workflow_runs": [
            {"id": 7, "name": "CI", "head_branch": "main", "display_title": "Arregla",
             "status": "in_progress", "conclusion": null,
             "html_url": "https://github.com/o/r/actions/runs/7", "updated_at": "2026-10-30T10:00:00Z"},
            {"id": 8, "name": "CI", "head_branch": "dev", "display_title": "Otro",
             "status": "completed", "conclusion": "failure",
             "html_url": "https://github.com/o/r/actions/runs/8", "updated_at": "2026-10-30T11:00:00Z"},
            {"id": 9, "status": "queued", "conclusion": null}
        ]});
        let r = parse_runs("o/r", &j);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].status, RunStatus::InProgress);
        assert_eq!(r[0].conclusion, None);
        assert_eq!(r[0].title, "Arregla");
        assert_eq!(r[1].status, RunStatus::Completed);
        assert_eq!(r[1].conclusion.as_deref(), Some("failure"));
        assert_eq!(r[1].branch, "dev");
        assert_eq!(r[2].status, RunStatus::Queued);
        assert_eq!(r[2].repo, "o/r");
        assert!(parse_runs("o/r", &json!({})).is_empty());
    }

    #[test]
    fn errors_from_status() {
        assert_eq!(
            error_from_status(401, None, None, None, 100),
            GhError::NotLoggedIn
        );
        assert_eq!(
            error_from_status(403, None, Some(500), Some(0), 100),
            GhError::RateLimited { until: Some(500) }
        );
        assert_eq!(
            error_from_status(429, Some(30), None, None, 100),
            GhError::RateLimited { until: Some(130) }
        );
        assert_eq!(
            error_from_status(429, None, None, None, 100),
            GhError::RateLimited { until: None }
        );
        assert_eq!(
            error_from_status(403, None, None, Some(50), 100),
            GhError::Http(403)
        );
        assert_eq!(
            error_from_status(500, None, None, None, 100),
            GhError::Http(500)
        );
    }

    #[test]
    fn token_debug_is_redacted() {
        let t = Token::new("ghp_secreto".into());
        assert_eq!(format!("{t:?}"), "Token(***)");
        let mut state = GhState::default();
        state.tokens.insert("yo".into(), t);
        assert!(
            !format!(
                "{:?}",
                Refresh {
                    no_access: 0,
                    state,
                    prs: Err(GhError::NoGh),
                    runs: Err(GhError::NoGh)
                }
            )
            .contains("secreto")
        );
    }

    #[test]
    fn error_messages_do_not_leak() {
        crate::i18n::apply_for_test("es");
        assert!(GhError::NotLoggedIn.message(None).contains("gh auth login"));
        assert!(GhError::NoGh.message(None).contains("no está instalado"));
        crate::i18n::apply_for_test("en");
        assert!(GhError::NoGh.message(None).contains("not installed"));
        crate::i18n::apply_for_test("es");
        assert_eq!(
            GhError::Network("x".into()).message(Some(3)),
            "sin conexión · hace 3 min"
        );
        assert_eq!(GhError::Network("x".into()).message(None), "sin conexión");
    }

    #[test]
    fn graphql_errors_map() {
        crate::i18n::apply_for_test("es");
        let nf = json!({"data": null, "errors": [{"type": "NOT_FOUND", "message": "x"}]});
        assert_eq!(graphql_error(&nf), Some(GhError::Query));
        let rl = json!({"data": null, "errors": [{"type": "RATE_LIMITED"}]});
        assert_eq!(
            graphql_error(&rl),
            Some(GhError::RateLimited { until: None })
        );
        assert_eq!(graphql_error(&data(vec![], vec![])), None);
        assert_eq!(graphql_error(&json!({"errors": []})), None);
        assert!(GhError::Query.message(None).contains("no tienes acceso"));
    }

    fn run(repo: &str, id: u64, updated: &str) -> Run {
        Run {
            repo: repo.into(),
            id,
            name: String::new(),
            branch: String::new(),
            title: String::new(),
            status: RunStatus::InProgress,
            conclusion: None,
            url: String::new(),
            updated: updated.into(),
        }
    }

    #[test]
    fn merge_keeps_runs_of_failed_repos() {
        let prev = vec![run("a/a", 1, "2026-01-01"), run("b/b", 2, "2026-01-02")];
        let fetched = vec![run("a/a", 3, "2026-01-03")];
        let m = merge_runs(&prev, &["b/b".to_string()], fetched);
        let ids: Vec<u64> = m.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![3, 2]);
    }

    #[test]
    fn merge_keeps_pending_runs_beyond_the_cap() {
        let done = |id: u64| Run {
            status: RunStatus::Completed,
            conclusion: Some("success".into()),
            ..run("a/a", id, &format!("2026-02-{id:02}"))
        };
        let mut fetched: Vec<Run> = (10..21).map(done).collect();
        fetched.push(run("a/a", 99, "2026-01-01"));
        let m = merge_runs(&[], &[], fetched);
        assert_eq!(m.len(), 11);
        assert!(m.iter().any(|r| r.id == 99));
        assert_eq!(
            m.iter()
                .filter(|r| r.status == RunStatus::Completed)
                .count(),
            10
        );
    }

    #[test]
    fn batches_repos_by_twenty() {
        let repos: Vec<String> = (0..45).map(|i| format!("o/r{i}")).collect();
        let q = search_queries(&repos);
        assert_eq!(q.len(), 3);
        assert_eq!(q[0].matches("repo:").count(), 20);
        assert_eq!(q[2].matches("repo:").count(), 5);
        assert!(q[0].starts_with("is:pr is:open archived:false repo:o/r0 "));
    }

    fn pr_at(url: &str) -> Pr {
        Pr {
            repo: "o/r".into(),
            number: 1,
            title: String::new(),
            url: url.into(),
            draft: false,
            review: ReviewState::None,
            checks: ChecksState::None,
            conflicts: false,
            mine: true,
            updated: String::new(),
        }
    }

    fn grp(
        repos: &[&str],
        prs: Result<Vec<Pr>, GhError>,
        runs: Result<RunsRaw, GhError>,
    ) -> GroupOut {
        GroupOut {
            repos: repos.iter().map(|s| s.to_string()).collect(),
            prs,
            runs,
        }
    }

    fn ok(repo: &str, url: &str) -> GroupOut {
        grp(
            &[repo],
            Ok(vec![pr_at(url)]),
            Ok((vec![run(repo, 1, "2026-01-01")], vec![])),
        )
    }

    #[test]
    fn combine_merges_successful_groups() {
        let c = combine_groups(vec![ok("a/a", "u1"), ok("b/b", "u1"), ok("c/c", "u2")]);
        assert_eq!(c.prs.unwrap().len(), 2);
        assert_eq!(c.runs.unwrap().0.len(), 3);
        assert!(c.dropped.is_empty());
        assert!(combine_groups(vec![]).prs.unwrap().is_empty());
    }

    #[test]
    fn combine_isolates_one_failed_account() {
        let bad = grp(
            &["b/b", "b/c"],
            Ok(vec![pr_at("zzz")]),
            Err(GhError::NotLoggedIn),
        );
        let c = combine_groups(vec![ok("a/a", "u1"), bad]);
        let prs = c.prs.unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].url, "u1");
        assert_eq!(c.dropped, vec!["b/b", "b/c"]);
        let (runs, failed) = c.runs.unwrap();
        assert_eq!(runs.len(), 1);
        let names: Vec<&str> = failed.iter().map(|(r, _)| r.as_str()).collect();
        assert_eq!(names, ["b/b", "b/c"]);
        // Un Http(500) también se aísla.
        let c = combine_groups(vec![
            ok("a/a", "u1"),
            grp(&["b/b"], Err(GhError::Http(500)), Ok((vec![], vec![]))),
        ]);
        assert!(c.prs.is_ok());
        assert_eq!(c.dropped, vec!["b/b"]);
    }

    #[test]
    fn combine_global_errors() {
        let rl = grp(
            &["b/b"],
            Err(GhError::RateLimited { until: Some(5) }),
            Ok((vec![], vec![])),
        );
        let c = combine_groups(vec![ok("a/a", "u1"), rl]);
        assert_eq!(c.prs, Err(GhError::RateLimited { until: Some(5) }));
        assert_eq!(c.runs.unwrap().0.len(), 1);
        let net = grp(&["b/b"], Ok(vec![]), Err(GhError::Network("x".into())));
        assert!(combine_groups(vec![ok("a/a", "u1"), net]).runs.is_err());
        // Si fallan todas, el error es global.
        let all = combine_groups(vec![
            grp(
                &["a/a"],
                Err(GhError::NotLoggedIn),
                Err(GhError::NotLoggedIn),
            ),
            grp(&["b/b"], Err(GhError::Http(404)), Ok((vec![], vec![]))),
        ]);
        assert_eq!(all.prs, Err(GhError::NotLoggedIn));
        assert!(all.runs.is_err());
    }

    #[test]
    fn discovery_change_drops_no_access_and_failures() {
        let mut st = GhState {
            known: vec!["a".into()],
            ..Default::default()
        };
        st.access.insert("x/y".into(), Access::NoAccess);
        st.access.insert("x/z".into(), Access::Account("a".into()));
        st.failed.insert("b".into());
        st.note_discovered(&["a".to_string()]);
        assert!(st.failed.is_empty());
        assert_eq!(st.access.len(), 2);
        st.note_discovered(&["a".to_string(), "b".to_string()]);
        assert_eq!(st.access.len(), 1);
        assert_eq!(st.known, vec!["a", "b"]);
    }
}
