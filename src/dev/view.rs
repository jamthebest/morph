//! Página Dev: servidores locales y GitHub (PRs y Actions), en dos columnas o pestañas.

use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::{Element, widget};

use super::accounts::{Access, LocalRepo, normalize_manual};
use super::forge::{ChangeRequest, Forge, Pipeline};
use super::github::{AccountRepos, ChecksState, ReviewState, RunStatus};
use super::gitlab::same_project;
use super::ports::{Listener, cwd_label};
use crate::agenda::calendar_view::truncate;
use crate::app::Message;
use crate::config::DevLayout;

const WHITE: Color = Color::WHITE;
const DIM: Color = Color::from_rgb(0.6, 0.6, 0.6);
const ROW_BG: Color = Color::from_rgb(0.082, 0.082, 0.082); // #151515
const PORT: Color = Color::from_rgb(0.4, 0.8, 1.0); // #6cf
const TAB_ON: Color = Color::from_rgb(0.176, 0.424, 0.875); // #2d6cdf
const TAB_OFF: Color = Color::from_rgb(0.114, 0.114, 0.114); // #1d1d1d
const GREEN: Color = Color::from_rgb(0.2, 0.8, 0.2);
const RED: Color = Color::from_rgb(0.933, 0.267, 0.267);
const AMBER: Color = Color::from_rgb(0.914, 0.698, 0.227);
const NO_ACCESS: Color = Color::from_rgb(0.914, 0.4, 0.4); // #e96
const GRAY: Color = Color::from_rgb(0.4, 0.4, 0.4);

/// Pestaña activa con `dev_layout == Tabs` (no se guarda).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DevTab {
    #[default]
    Servers,
    Prs,
    Actions,
}

/// Estado de las secciones GitHub/GitLab de Ajustes; `None` en `accounts`/`local` = cargando.
#[derive(Clone, Debug, Default)]
pub struct RepoPicker {
    pub selected: Vec<String>,
    /// Repos de cada cuenta de gh (o el error en texto).
    pub accounts: Option<Result<AccountRepos, String>>,
    /// Repos de GitHub hallados en clones locales.
    pub local: Option<Vec<LocalRepo>>,
    /// Texto de la entrada manual y si no tiene formato válido.
    pub manual: String,
    pub manual_err: bool,
    /// Qué cuenta ve cada repo elegido (de la última lectura).
    pub access: std::collections::HashMap<String, Access>,
    /// GitLab: servidor, proyectos elegidos, hallados en clones y del usuario (o el error).
    pub gl_host: String,
    pub gl_selected: Vec<String>,
    pub gl_local: Option<Vec<LocalRepo>>,
    pub gl_projects: Option<Result<Vec<String>, String>>,
    pub gl_manual: String,
    pub gl_manual_err: bool,
    /// Usuario conectado (None = sin token o sin comprobar).
    pub gl_user: Option<String>,
    pub gl_has_token: bool,
    /// Token pegado (en memoria, enmascarado; `Debug` redactado).
    pub gl_token_input: super::gitlab::GlToken,
    /// Mensaje de estado (error o confirmación) del bloque de GitLab.
    pub gl_msg: Option<(String, bool)>,
    pub gl_busy: bool,
}

impl RepoPicker {
    /// Añade el repo escrito a mano; false si el formato no es `dueño/repo`.
    pub fn add_manual(&mut self) -> bool {
        match normalize_manual(&self.manual) {
            Some(r) => {
                if !self.selected.contains(&r) {
                    self.selected.push(r);
                }
                self.manual.clear();
                self.manual_err = false;
                true
            }
            None => {
                self.manual_err = true;
                false
            }
        }
    }
}

impl RepoPicker {
    /// Añade el proyecto de GitLab escrito a mano; false si el formato no es válido.
    pub fn add_manual_gl(&mut self) -> bool {
        match super::accounts::normalize_gitlab_manual(&self.gl_manual, &self.gl_host) {
            Some(r) => {
                let r = canonical_project(self, &r);
                if !self.gl_selected.iter().any(|s| same_project(s, &r)) {
                    self.gl_selected.push(r);
                }
                self.gl_manual.clear();
                self.gl_manual_err = false;
                true
            }
            None => {
                self.gl_manual_err = true;
                false
            }
        }
    }
}

/// Aviso de repos sin acceso: (texto, en rojo). Todos sin acceso = rojo; algunos = tenue.
pub fn no_access_note(repos: usize, no_access: usize) -> Option<(String, bool)> {
    if repos > 0 && no_access >= repos {
        Some((crate::fl!("dev-no-access-all"), true))
    } else if no_access > 0 {
        Some((crate::fl!("dev-no-access-some", n = no_access), false))
    } else {
        None
    }
}

/// Fila de repo del selector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerRow {
    pub repo: String,
    pub on: bool,
    /// Texto tenue (carpeta local o cuenta) y si va en rojo (sin acceso).
    pub sub: Option<(String, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectionBody {
    Loading,
    Error(String),
    Rows(Vec<PickerRow>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerSection {
    pub title: String,
    pub body: SectionBody,
}

/// Secciones del selector en orden (Elegidos, En tu PC, una por cuenta); un repo sale una
/// sola vez: gana la primera sección donde aparece.
pub fn picker_sections(p: &RepoPicker) -> Vec<PickerSection> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<&str> = p.selected.iter().map(String::as_str).collect();
    if !p.selected.is_empty() {
        let rows = p
            .selected
            .iter()
            .map(|r| PickerRow {
                repo: r.clone(),
                on: true,
                sub: match p.access.get(r) {
                    Some(Access::Account(l)) => Some((l.clone(), false)),
                    Some(Access::NoAccess) => Some((crate::fl!("dev-no-access"), true)),
                    None => None,
                },
            })
            .collect();
        out.push(PickerSection {
            title: crate::fl!("dev-sec-selected"),
            body: SectionBody::Rows(rows),
        });
    }
    match &p.local {
        None => out.push(PickerSection {
            title: crate::fl!("dev-sec-local"),
            body: SectionBody::Loading,
        }),
        Some(list) => {
            let rows: Vec<PickerRow> = list
                .iter()
                .filter(|l| seen.insert(l.repo.as_str()))
                .map(|l| PickerRow {
                    repo: l.repo.clone(),
                    on: false,
                    sub: Some((l.dir.clone(), false)),
                })
                .collect();
            if !rows.is_empty() {
                out.push(PickerSection {
                    title: crate::fl!("dev-sec-local"),
                    body: SectionBody::Rows(rows),
                });
            }
        }
    }
    match &p.accounts {
        None => out.push(PickerSection {
            title: crate::fl!("dev-sec-accounts"),
            body: SectionBody::Loading,
        }),
        Some(Err(e)) => out.push(PickerSection {
            title: crate::fl!("dev-sec-accounts"),
            body: SectionBody::Error(e.clone()),
        }),
        Some(Ok(accs)) => {
            for (login, res) in accs {
                let title = crate::fl!("dev-sec-account", login = login.as_str());
                match res {
                    Err(e) => out.push(PickerSection {
                        title,
                        body: SectionBody::Error(e.clone()),
                    }),
                    Ok(list) => {
                        let rows: Vec<PickerRow> = list
                            .iter()
                            .filter(|r| seen.insert(r.as_str()))
                            .map(|r| PickerRow {
                                repo: r.clone(),
                                on: false,
                                sub: None,
                            })
                            .collect();
                        if !rows.is_empty() {
                            out.push(PickerSection {
                                title,
                                body: SectionBody::Rows(rows),
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

/// Nombre corto de un proyecto: su última parte (`grupo/sub/proyecto` → `proyecto`).
pub fn short_project(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Página de GitLab para crear un token `read_api` con el nombre «Morph».
pub fn token_url(host: &str) -> String {
    format!("https://{host}/-/user_settings/personal_access_tokens?name=Morph&scopes=read_api")
}

/// Path canónico de un proyecto de GitLab: el de «Tus proyectos» si coincide sin distinguir
/// mayúsculas; si no, el mismo texto.
pub fn canonical_project(p: &RepoPicker, name: &str) -> String {
    if let Some(Ok(list)) = &p.gl_projects
        && let Some(c) = list.iter().find(|c| same_project(c, name))
    {
        return c.clone();
    }
    name.to_string()
}

/// Secciones de proyectos de GitLab del selector: Elegidos, En tu PC (GitLab) y Tus
/// proyectos de GitLab; un proyecto sale una sola vez.
pub fn gitlab_sections(p: &RepoPicker) -> Vec<PickerSection> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<String> = p
        .gl_selected
        .iter()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    if !p.gl_selected.is_empty() {
        out.push(PickerSection {
            title: crate::fl!("gl-sec-selected"),
            body: SectionBody::Rows(
                p.gl_selected
                    .iter()
                    .map(|r| PickerRow {
                        repo: r.clone(),
                        on: true,
                        sub: None,
                    })
                    .collect(),
            ),
        });
    }
    match &p.gl_local {
        None => out.push(PickerSection {
            title: crate::fl!("gl-sec-local"),
            body: SectionBody::Loading,
        }),
        Some(list) => {
            let rows: Vec<PickerRow> = list
                .iter()
                .filter(|l| seen.insert(l.repo.to_ascii_lowercase()))
                .map(|l| PickerRow {
                    repo: l.repo.clone(),
                    on: false,
                    sub: Some((l.dir.clone(), false)),
                })
                .collect();
            if !rows.is_empty() {
                out.push(PickerSection {
                    title: crate::fl!("gl-sec-local"),
                    body: SectionBody::Rows(rows),
                });
            }
        }
    }
    let title = crate::fl!("gl-sec-projects");
    match &p.gl_projects {
        None => out.push(PickerSection {
            title,
            body: SectionBody::Loading,
        }),
        Some(Err(e)) => out.push(PickerSection {
            title,
            body: SectionBody::Error(e.clone()),
        }),
        Some(Ok(list)) => {
            let rows: Vec<PickerRow> = list
                .iter()
                .filter(|r| seen.insert(r.to_ascii_lowercase()))
                .map(|r| PickerRow {
                    repo: r.clone(),
                    on: false,
                    sub: None,
                })
                .collect();
            if !rows.is_empty() {
                out.push(PickerSection {
                    title,
                    body: SectionBody::Rows(rows),
                });
            }
        }
    }
    out
}

/// Todo lo que la vista necesita del estado de la app.
pub struct DevCtx<'a> {
    pub listeners: &'a [Listener],
    pub home: &'a std::path::Path,
    pub confirm: Option<u32>,
    pub layout: DevLayout,
    pub tab: DevTab,
    pub repos: usize,
    /// Repos elegidos que ninguna cuenta ve.
    pub no_access: usize,
    pub prs: &'a [ChangeRequest],
    pub runs: &'a [Pipeline],
    /// Mensaje de error vigente de GitHub (ya con "hace N min").
    pub error: Option<String>,
    /// Mensaje de error vigente de GitLab (con su propia antigüedad).
    pub gl_error: Option<String>,
    /// Segundos desde la última lectura buena.
    pub age_secs: Option<u64>,
    /// Sondeo en pausa por el límite de la API.
    pub paused: bool,
}

// ---- Helpers puros ----

/// Etiqueta de estado de un PR (por prioridad).
pub fn pr_label(p: &ChangeRequest) -> String {
    if p.draft {
        crate::fl!("dev-pr-draft")
    } else if !p.mine {
        crate::fl!("dev-pr-review")
    } else if p.conflicts {
        crate::fl!("dev-pr-conflicts")
    } else if p.checks == ChecksState::Failure {
        crate::fl!("dev-pr-checks-failed")
    } else if p.review == ReviewState::ChangesRequested {
        crate::fl!("dev-pr-changes")
    } else if p.review == ReviewState::Approved {
        crate::fl!("dev-pr-approved")
    } else if p.checks == ChecksState::Pending {
        crate::fl!("dev-pr-checks-pending")
    } else {
        crate::fl!("dev-pr-open")
    }
}

/// Etiqueta en la fila: abreviada en el modo estrecho.
pub fn pr_tag(p: &ChangeRequest, wide: bool) -> String {
    let changes = !p.draft
        && p.mine
        && !p.conflicts
        && p.checks != ChecksState::Failure
        && p.review == ReviewState::ChangesRequested;
    if changes && !wide {
        crate::fl!("dev-pr-changes-short")
    } else {
        pr_label(p)
    }
}

/// Texto de la confirmación; el nombre solo cabe en el modo ancho.
pub fn confirm_label(l: &Listener, wide: bool) -> String {
    if wide {
        crate::fl!(
            "dev-stop-confirm-named",
            name = truncate(&l.name, 12),
            port = u32::from(l.port)
        )
    } else {
        crate::fl!("dev-stop-confirm", port = u32::from(l.port))
    }
}

pub fn pr_dot(p: &ChangeRequest) -> Color {
    if p.draft {
        GRAY
    } else if !p.mine {
        AMBER
    } else if p.conflicts
        || p.checks == ChecksState::Failure
        || p.review == ReviewState::ChangesRequested
    {
        RED
    } else if p.review == ReviewState::Approved {
        GREEN
    } else if p.checks == ChecksState::Pending {
        AMBER
    } else {
        GRAY
    }
}

pub fn run_dot(status: RunStatus, conclusion: Option<&str>) -> Color {
    match status {
        RunStatus::InProgress => AMBER,
        RunStatus::Queued => GRAY,
        RunStatus::Completed => match conclusion {
            Some("success") => GREEN,
            Some("failure" | "timed_out") => RED,
            _ => GRAY,
        },
    }
}

pub fn run_tag(status: RunStatus, conclusion: Option<&str>) -> String {
    match status {
        RunStatus::InProgress => crate::fl!("dev-run-running"),
        RunStatus::Queued => crate::fl!("dev-run-queued"),
        RunStatus::Completed => match conclusion {
            Some("success") => crate::fl!("dev-run-success"),
            Some("failure" | "timed_out") => crate::fl!("dev-run-failed"),
            Some("cancelled") => crate::fl!("dev-run-cancelled"),
            _ => crate::fl!("dev-run-finished"),
        },
    }
}

pub fn age_label(secs: u64) -> String {
    if secs < 60 {
        crate::fl!("dev-age-s", n = secs)
    } else if secs < 3600 {
        crate::fl!("dev-age-min", n = ((secs / 60) as u32))
    } else {
        crate::fl!("dev-age-h", n = ((secs / 3600) as u32))
    }
}

// ---- Widgets ----

fn text(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

/// Texto que se parte en varias líneas (mensajes de estado y error).
fn wtext(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .width(Length::Fill)
        .class(cosmic::theme::Text::Color(color))
}

fn mono(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    text(s, size, color).font(cosmic::iced::Font::MONOSPACE)
}

fn btn(label: &str, msg: Message) -> Element<'static, Message> {
    widget::button::custom(text(label, 11, WHITE))
        .padding([2, 8])
        .class(cosmic::theme::Button::Text)
        .on_press(msg)
        .into()
}

fn boxed(
    content: impl Into<Element<'static, Message>>,
    color: Color,
    radius: f32,
    pad: [u16; 2],
) -> Element<'static, Message> {
    widget::container(content)
        .padding(pad)
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn dot(color: Color) -> Element<'static, Message> {
    widget::container(widget::space())
        .width(Length::Fixed(9.0))
        .height(Length::Fixed(9.0))
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 5.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn tag(s: &str) -> Element<'static, Message> {
    boxed(
        text(s, 10, Color::from_rgb(0.73, 0.73, 0.73)),
        Color::from_rgb(0.133, 0.133, 0.133),
        8.0,
        [1, 6],
    )
}

fn row_box(r: impl Into<Element<'static, Message>>) -> Element<'static, Message> {
    widget::container(r)
        .padding([4, 8])
        .width(Length::Fill)
        .class(cosmic::theme::Container::custom(|_| {
            widget::container::Style {
                background: Some(Background::Color(ROW_BG)),
                border: Border {
                    radius: 8.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn header(title: &str, right: impl Into<Element<'static, Message>>) -> Element<'static, Message> {
    widget::Row::new()
        .align_y(Alignment::Center)
        .push(text(title, 13, WHITE))
        .push(widget::space().width(Length::Fill))
        .push(right)
        .into()
}

fn server_row(
    l: &Listener,
    home: &std::path::Path,
    confirm: Option<u32>,
    wide: bool,
) -> Element<'static, Message> {
    let mut r = widget::Row::new().align_y(Alignment::Center).spacing(6);
    if confirm == Some(l.pid) {
        r = r
            .push(text(confirm_label(l, wide), 12, WHITE))
            .push(widget::space().width(Length::Fill))
            .push(btn(&crate::fl!("dev-yes"), Message::DevStopYes(l.pid)))
            .push(btn(&crate::fl!("dev-no"), Message::DevStopNo));
    } else {
        r = r.push(mono(format!(":{}", l.port), 13, PORT)).push(text(
            truncate(&l.name, if wide { 18 } else { 9 }),
            12,
            WHITE,
        ));
        if wide {
            let path = l
                .cwd
                .as_deref()
                .map(|c| truncate(&cwd_label(c, home), 30))
                .unwrap_or_default();
            r = r.push(text(path, 11, DIM));
        }
        r = r
            .push(widget::space().width(Length::Fill))
            .push(btn("↗", Message::DevOpen(l.port)))
            .push(btn("■", Message::DevStopAsk(l.pid)));
    }
    row_box(r)
}

fn terminal_btn() -> Element<'static, Message> {
    btn("› Terminal", Message::Term(crate::app::TermMsg::Toggle))
}

fn servers(c: &DevCtx<'_>, wide: bool) -> Element<'static, Message> {
    let mut col = widget::Column::new().spacing(4).width(Length::Fill);
    if c.layout == DevLayout::Columns {
        col = col.push(header(
            &crate::fl!("dev-servers"),
            widget::Row::new()
                .spacing(6)
                .align_y(Alignment::Center)
                .push(text(c.listeners.len().to_string(), 12, DIM))
                .push(terminal_btn()),
        ));
    }
    if c.listeners.is_empty() {
        col = col.push(text(crate::fl!("dev-no-servers"), 12, DIM));
    }
    for l in c.listeners {
        col = col.push(server_row(l, c.home, c.confirm, wide));
    }
    widget::scrollable(col).height(Length::Fill).into()
}

/// Cápsula de 12 px con las iniciales de la forja (sin logos de marca).
fn provider_badge(forge: Forge) -> Element<'static, Message> {
    let (t, color) = match forge {
        Forge::GitHub => ("GH", Color::from_rgb(0.3, 0.3, 0.34)),
        Forge::GitLab => ("GL", Color::from_rgb(0.55, 0.3, 0.15)),
    };
    widget::container(text(t, 7, WHITE))
        .height(Length::Fixed(12.0))
        .padding([0, 3])
        .align_y(Alignment::Center)
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                background: Some(Background::Color(color)),
                border: Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        }))
        .into()
}

fn clickable_row(
    forge: Forge,
    dot_color: Color,
    label: String,
    sub: Option<String>,
    tag_text: &str,
    url: &str,
) -> Element<'static, Message> {
    let mut r = widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(6)
        .push(provider_badge(forge))
        .push(dot(dot_color))
        .push(text(label, 12, WHITE));
    if let Some(s) = sub {
        r = r.push(text(s, 11, DIM));
    }
    r = r
        .push(widget::space().width(Length::Fill))
        .push(tag(tag_text));
    row_box(
        widget::button::custom(r)
            .padding(0)
            .width(Length::Fill)
            .class(cosmic::theme::Button::Text)
            .on_press(Message::DevOpenUrl(url.to_string())),
    )
}

fn pr_rows(c: &DevCtx<'_>, wide: bool) -> Vec<Element<'static, Message>> {
    if c.prs.is_empty() {
        return vec![text(crate::fl!("dev-no-prs"), 12, DIM).into()];
    }
    c.prs
        .iter()
        .map(|p| {
            let title = format!(
                "#{} {}",
                p.number,
                truncate(&p.title, if wide { 34 } else { 7 })
            );
            let sub = wide.then(|| format!("· {}", truncate(short_project(&p.project), 16)));
            clickable_row(p.forge, pr_dot(p), title, sub, &pr_tag(p, wide), &p.url)
        })
        .collect()
}

fn run_rows(c: &DevCtx<'_>, wide: bool) -> Vec<Element<'static, Message>> {
    if c.runs.is_empty() {
        return vec![text(crate::fl!("dev-no-runs"), 12, DIM).into()];
    }
    c.runs
        .iter()
        .take(10)
        .map(|r| {
            let label = format!(
                "{} · {}",
                truncate(&r.name, if wide { 22 } else { 6 }),
                truncate(&r.branch, if wide { 18 } else { 6 })
            );
            let sub = wide.then(|| format!("· {}", truncate(short_project(&r.project), 16)));
            clickable_row(
                r.forge,
                run_dot(r.status, r.conclusion.as_deref()),
                label,
                sub,
                &run_tag(r.status, r.conclusion.as_deref()),
                &r.url,
            )
        })
        .collect()
}

/// Cabecera de GitHub: "↻ hace N s" y «+ Repos».
fn gh_header(c: &DevCtx<'_>, title: &str) -> Element<'static, Message> {
    let mut right = widget::Row::new().align_y(Alignment::Center).spacing(6);
    if let Some(s) = c.age_secs {
        right = right.push(text(format!("↻ {}", age_label(s)), 11, DIM));
    }
    right = right.push(btn("+ Repos", Message::DevReposOpen));
    header(title, right)
}

/// Mensajes de estado de GitHub; None si hay datos que mostrar.
fn gh_status(c: &DevCtx<'_>) -> Option<Element<'static, Message>> {
    if c.repos == 0 {
        return Some(wtext(crate::fl!("dev-pick-repos"), 12, DIM).into());
    }
    if let Some((t, true)) = no_access_note(c.repos, c.no_access) {
        return Some(wtext(t, 12, NO_ACCESS).into());
    }
    if c.age_secs.is_none() {
        if c.paused {
            return Some(wtext(crate::fl!("dev-rate-paused"), 12, DIM).into());
        }
        let errs: Vec<&String> = c.error.iter().chain(c.gl_error.iter()).collect();
        if errs.is_empty() {
            return Some(wtext(crate::fl!("dev-loading"), 12, DIM).into());
        }
        let mut col = widget::Column::new().spacing(2);
        for e in errs {
            col = col.push(wtext(e.clone(), 12, DIM));
        }
        return Some(col.into());
    }
    None
}

fn github(c: &DevCtx<'_>, wide: bool, only: Option<DevTab>) -> Element<'static, Message> {
    let mut col = widget::Column::new().spacing(4).width(Length::Fill);
    col = col.push(gh_header(c, if only.is_none() { "Repos" } else { "" }));
    if let Some(s) = gh_status(c) {
        col = col.push(s);
    } else {
        for e in c.error.iter().chain(c.gl_error.iter()) {
            col = col.push(wtext(e.clone(), 11, DIM));
        }
        if let Some((t, _)) = no_access_note(c.repos, c.no_access) {
            col = col.push(wtext(t, 11, DIM));
        }
        if only != Some(DevTab::Actions) {
            if only.is_none() {
                col = col.push(text(format!("PRs · {}", c.prs.len()), 12, DIM));
            }
            for r in pr_rows(c, wide) {
                col = col.push(r);
            }
        }
        if only != Some(DevTab::Prs) {
            if only.is_none() {
                col = col.push(text("Actions", 12, DIM));
            }
            for r in run_rows(c, wide) {
                col = col.push(r);
            }
        }
    }
    widget::scrollable(col).height(Length::Fill).into()
}

fn tab_button(label: String, tab: DevTab, active: bool) -> Element<'static, Message> {
    widget::button::custom(boxed(
        text(
            label,
            12,
            if active {
                WHITE
            } else {
                Color::from_rgb(0.67, 0.67, 0.67)
            },
        ),
        if active { TAB_ON } else { TAB_OFF },
        7.0,
        [3, 10],
    ))
    .padding(0)
    .class(cosmic::theme::Button::Text)
    .on_press(Message::DevTab(tab))
    .into()
}

pub fn view(c: &DevCtx<'_>) -> Element<'static, Message> {
    match c.layout {
        DevLayout::Columns => widget::Row::new()
            .spacing(10)
            .push(
                widget::container(servers(c, false))
                    .width(Length::FillPortion(1))
                    .height(Length::Fill),
            )
            .push(
                widget::container(github(c, false, None))
                    .width(Length::FillPortion(1))
                    .height(Length::Fill),
            )
            .into(),
        DevLayout::Tabs => {
            let tabs = widget::Row::new()
                .spacing(4)
                .push(tab_button(
                    crate::fl!("dev-tab-servers", n = c.listeners.len()),
                    DevTab::Servers,
                    c.tab == DevTab::Servers,
                ))
                .push(tab_button(
                    format!("PRs · {}", c.prs.len()),
                    DevTab::Prs,
                    c.tab == DevTab::Prs,
                ))
                .push(tab_button(
                    "Actions".to_string(),
                    DevTab::Actions,
                    c.tab == DevTab::Actions,
                ))
                .push(widget::space().width(Length::Fill))
                .push(terminal_btn());
            let body = match c.tab {
                DevTab::Servers => servers(c, true),
                t => github(c, true, Some(t)),
            };
            widget::Column::new()
                .spacing(8)
                .push(tabs)
                .push(body)
                .into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev::forge::Forge;
    use crate::dev::github::{ChecksState, ReviewState, RunStatus};

    fn pr(f: impl FnOnce(&mut ChangeRequest)) -> ChangeRequest {
        let mut p = ChangeRequest {
            forge: Forge::GitHub,
            updated: String::new(),
            project: "o/r".into(),
            number: 1,
            title: String::new(),
            url: String::new(),
            draft: false,
            review: ReviewState::None,
            checks: ChecksState::None,
            conflicts: false,
            mine: true,
        };
        f(&mut p);
        p
    }

    #[test]
    fn pr_labels() {
        crate::i18n::apply_for_test("es");
        assert_eq!(pr_label(&pr(|p| p.draft = true)), "borrador");
        assert_eq!(pr_label(&pr(|p| p.mine = false)), "revisar tú");
        assert_eq!(pr_label(&pr(|p| p.conflicts = true)), "conflictos");
        assert_eq!(
            pr_label(&pr(|p| p.checks = ChecksState::Failure)),
            "checks ✕"
        );
        assert_eq!(
            pr_label(&pr(|p| p.review = ReviewState::ChangesRequested)),
            "cambios pedidos"
        );
        assert_eq!(
            pr_label(&pr(|p| p.review = ReviewState::Approved)),
            "aprobado"
        );
        assert_eq!(pr_label(&pr(|_| {})), "abierto");
        // Prioridad: borrador gana a todo; conflictos a checks.
        assert_eq!(
            pr_label(&pr(|p| {
                p.draft = true;
                p.conflicts = true
            })),
            "borrador"
        );
        assert_eq!(
            pr_label(&pr(|p| {
                p.conflicts = true;
                p.checks = ChecksState::Failure
            })),
            "conflictos"
        );
    }

    #[test]
    fn pr_dots() {
        assert_eq!(pr_dot(&pr(|p| p.review = ReviewState::Approved)), GREEN);
        assert_eq!(pr_dot(&pr(|p| p.conflicts = true)), RED);
        assert_eq!(pr_dot(&pr(|p| p.checks = ChecksState::Failure)), RED);
        assert_eq!(pr_dot(&pr(|p| p.mine = false)), AMBER);
        assert_eq!(pr_dot(&pr(|p| p.draft = true)), GRAY);
        assert_eq!(pr_dot(&pr(|_| {})), GRAY);
    }

    #[test]
    fn run_dots_and_tags() {
        crate::i18n::apply_for_test("es");
        let ok = Some("success".to_string());
        let bad = Some("failure".to_string());
        assert_eq!(run_dot(RunStatus::Completed, ok.as_deref()), GREEN);
        assert_eq!(run_dot(RunStatus::Completed, bad.as_deref()), RED);
        assert_eq!(run_dot(RunStatus::Completed, Some("cancelled")), GRAY);
        assert_eq!(run_dot(RunStatus::InProgress, None), AMBER);
        assert_eq!(run_dot(RunStatus::Queued, None), GRAY);
        assert_eq!(run_tag(RunStatus::InProgress, None), "en curso");
        assert_eq!(run_tag(RunStatus::Queued, None), "en cola");
        assert_eq!(run_tag(RunStatus::Completed, Some("success")), "éxito");
        assert_eq!(run_tag(RunStatus::Completed, Some("timed_out")), "fallo");
        assert_eq!(
            run_tag(RunStatus::Completed, Some("cancelled")),
            "cancelada"
        );
    }

    #[test]
    fn narrow_tags_and_confirm() {
        crate::i18n::apply_for_test("es");
        let c = pr(|p| p.review = ReviewState::ChangesRequested);
        assert_eq!(pr_tag(&c, true), "cambios pedidos");
        assert_eq!(pr_tag(&c, false), "cambios");
        assert_eq!(pr_tag(&pr(|p| p.draft = true), false), "borrador");
        let l = Listener {
            port: 3000,
            pid: 1,
            start: 0,
            name: "node".into(),
            cwd: None,
        };
        assert_eq!(confirm_label(&l, true), "¿Detener node :3000?");
        assert_eq!(confirm_label(&l, false), "¿Detener :3000?");
    }

    #[test]
    fn ages() {
        crate::i18n::apply_for_test("es");
        assert_eq!(age_label(20), "hace 20 s");
        assert_eq!(age_label(59), "hace 59 s");
        assert_eq!(age_label(180), "hace 3 min");
        assert_eq!(age_label(7300), "hace 2 h");
    }

    fn lr(repo: &str, dir: &str) -> LocalRepo {
        LocalRepo {
            repo: repo.into(),
            dir: dir.into(),
        }
    }

    fn rows(sec: &PickerSection) -> Vec<&str> {
        match &sec.body {
            SectionBody::Rows(r) => r.iter().map(|x| x.repo.as_str()).collect(),
            _ => vec![],
        }
    }

    #[test]
    fn picker_dedupes_and_orders_sections() {
        crate::i18n::apply_for_test("es");
        let mut p = RepoPicker {
            selected: vec!["me/api".into(), "org/x".into()],
            local: Some(vec![
                lr("me/api", "api"),
                lr("me/web", "web"),
                lr("a/b", "b"),
            ]),
            accounts: Some(Ok(vec![
                (
                    "uno".into(),
                    Ok(vec!["me/web".into(), "u/1".into(), "org/x".into()]),
                ),
                ("dos".into(), Ok(vec!["u/1".into(), "d/2".into()])),
                ("tres".into(), Ok(vec!["a/b".into()])),
                ("cuatro".into(), Err("boom".into())),
            ])),
            ..Default::default()
        };
        p.access
            .insert("me/api".into(), Access::Account("uno".into()));
        p.access.insert("org/x".into(), Access::NoAccess);
        let s = picker_sections(&p);
        let titles: Vec<&str> = s.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Elegidos",
                "En tu PC",
                "Cuenta uno",
                "Cuenta dos",
                "Cuenta cuatro"
            ]
        );
        assert_eq!(rows(&s[0]), ["me/api", "org/x"]);
        assert_eq!(rows(&s[1]), ["me/web", "a/b"]);
        assert_eq!(rows(&s[2]), ["u/1"]);
        assert_eq!(rows(&s[3]), ["d/2"]);
        assert_eq!(s[4].body, SectionBody::Error("boom".into()));
        let SectionBody::Rows(chosen) = &s[0].body else {
            panic!()
        };
        assert!(chosen.iter().all(|r| r.on));
        assert_eq!(chosen[0].sub, Some(("uno".to_string(), false)));
        assert_eq!(chosen[1].sub, Some((crate::fl!("dev-no-access"), true)));
        let SectionBody::Rows(local) = &s[1].body else {
            panic!()
        };
        assert_eq!(local[0].sub, Some(("web".to_string(), false)));
    }

    #[test]
    fn no_access_notes() {
        crate::i18n::apply_for_test("es");
        assert_eq!(no_access_note(0, 0), None);
        assert_eq!(no_access_note(3, 0), None);
        assert_eq!(
            no_access_note(3, 2),
            Some(("2 sin acceso".to_string(), false))
        );
        assert_eq!(
            no_access_note(2, 2),
            Some((crate::fl!("dev-no-access-all"), true))
        );
        assert_eq!(no_access_note(0, 1).map(|n| n.1), Some(false));
    }

    #[test]
    fn picker_loading_states() {
        crate::i18n::apply_for_test("es");
        let s = picker_sections(&RepoPicker::default());
        let titles: Vec<&str> = s.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(titles, ["En tu PC", "Cuentas"]);
        assert!(s.iter().all(|x| x.body == SectionBody::Loading));
    }

    #[test]
    fn manual_add_validates_and_dedupes() {
        let mut p = RepoPicker {
            selected: vec!["o/r".into()],
            manual: "nope".into(),
            ..Default::default()
        };
        assert!(!p.add_manual());
        assert!(p.manual_err);
        assert_eq!(p.manual, "nope");
        p.manual = "https://github.com/o/r.git".into();
        assert!(p.add_manual());
        assert!(!p.manual_err && p.manual.is_empty());
        assert_eq!(p.selected, vec!["o/r"]);
        p.manual = "x/y".into();
        assert!(p.add_manual());
        assert_eq!(p.selected, vec!["o/r", "x/y"]);
    }

    #[test]
    fn short_names_and_token_url() {
        assert_eq!(short_project("grupo/sub/proyecto"), "proyecto");
        assert_eq!(short_project("o/r"), "r");
        assert_eq!(short_project("solo"), "solo");
        assert_eq!(
            token_url("gitlab.com"),
            "https://gitlab.com/-/user_settings/personal_access_tokens?name=Morph&scopes=read_api"
        );
    }

    #[test]
    fn gitlab_sections_order_and_dedupe() {
        crate::i18n::apply_for_test("es");
        let p = RepoPicker {
            gl_selected: vec!["g/s/a".into()],
            gl_local: Some(vec![lr("g/s/a", "a"), lr("g/b", "b")]),
            gl_projects: Some(Ok(vec!["g/s/a".into(), "g/b".into(), "h/c".into()])),
            ..Default::default()
        };
        let s = gitlab_sections(&p);
        let titles: Vec<&str> = s.iter().map(|x| x.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Elegidos (GitLab)",
                "En tu PC (GitLab)",
                "Tus proyectos de GitLab"
            ]
        );
        assert_eq!(rows(&s[0]), ["g/s/a"]);
        assert_eq!(rows(&s[1]), ["g/b"]);
        assert_eq!(rows(&s[2]), ["h/c"]);
        let p = RepoPicker::default();
        let s = gitlab_sections(&p);
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| x.body == SectionBody::Loading));
        let p = RepoPicker {
            gl_local: Some(vec![]),
            gl_projects: Some(Err("Token no válido".into())),
            ..Default::default()
        };
        let s = gitlab_sections(&p);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].body, SectionBody::Error("Token no válido".into()));
    }

    #[test]
    fn gitlab_manual_add() {
        let mut p = RepoPicker {
            gl_host: "gitlab.com".into(),
            gl_selected: vec!["g/p".into()],
            gl_manual: "nope".into(),
            ..Default::default()
        };
        assert!(!p.add_manual_gl());
        assert!(p.gl_manual_err);
        p.gl_manual = "https://gitlab.com/g/s/q.git".into();
        assert!(p.add_manual_gl());
        assert!(p.gl_manual.is_empty() && !p.gl_manual_err);
        p.gl_manual = "g/p".into();
        assert!(p.add_manual_gl());
        assert_eq!(p.gl_selected, vec!["g/p", "g/s/q"]);
    }

    #[test]
    fn gitlab_mr_labels_match_github() {
        crate::i18n::apply_for_test("es");
        let mut c = pr(|c| c.forge = Forge::GitLab);
        assert_eq!(pr_label(&c), "abierto");
        c.review = ReviewState::Approved;
        assert_eq!(pr_label(&c), "aprobado");
        c.review = ReviewState::ChangesRequested;
        assert_eq!(pr_label(&c), "cambios pedidos");
        c.checks = ChecksState::Failure;
        assert_eq!(pr_label(&c), "checks ✕");
        c.conflicts = true;
        assert_eq!(pr_label(&c), "conflictos");
        c.mine = false;
        assert_eq!(pr_label(&c), "revisar tú");
        c.draft = true;
        assert_eq!(pr_label(&c), "borrador");
    }

    #[test]
    fn canonical_project_and_case_insensitive_sections() {
        let p = RepoPicker {
            gl_host: "gitlab.com".into(),
            gl_projects: Some(Ok(vec!["Grupo/Sub/Proy".into()])),
            gl_selected: vec!["grupo/sub/proy".into()],
            ..Default::default()
        };
        assert_eq!(canonical_project(&p, "grupo/sub/PROY"), "Grupo/Sub/Proy");
        assert_eq!(canonical_project(&p, "otro/x"), "otro/x");
        let s = gitlab_sections(&p);
        assert_eq!(
            s.len(),
            2,
            "el proyecto elegido no se repite en «Tus proyectos»"
        );
        let mut p = p;
        p.gl_manual = "grupo/sub/proy".into();
        p.gl_selected.clear();
        assert!(p.add_manual_gl());
        assert_eq!(p.gl_selected, vec!["Grupo/Sub/Proy"]);
    }

    #[test]
    fn secrets_never_in_debug() {
        use crate::dev::gitlab::GlToken;
        let p = RepoPicker {
            gl_token_input: GlToken::new("glpat-SAMPLE".into()),
            ..Default::default()
        };
        assert!(!format!("{p:?}").contains("SAMPLE"));
        let m = Message::DevGlTokenInput(GlToken::new("glpat-SAMPLE".into()));
        assert!(!format!("{m:?}").contains("SAMPLE"));
        let m = Message::DevGlTokenSaved(
            "gitlab.com".into(),
            Ok(("yo".into(), GlToken::new("glpat-SAMPLE".into()))),
        );
        assert!(!format!("{m:?}").contains("SAMPLE"));
    }

    #[test]
    fn english_labels() {
        crate::i18n::apply_for_test("en");
        assert_eq!(pr_label(&pr(|p| p.draft = true)), "draft");
        assert_eq!(run_tag(RunStatus::Completed, Some("success")), "success");
        assert_eq!(age_label(180), "3 min ago");
        assert_eq!(
            no_access_note(3, 2),
            Some(("2 without access".to_string(), false))
        );
        let l = Listener {
            port: 3000,
            pid: 1,
            name: "node".into(),
            cwd: None,
            start: 0,
        };
        assert_eq!(confirm_label(&l, true), "Stop node :3000?");
        assert_eq!(confirm_label(&l, false), "Stop :3000?");
    }
}
