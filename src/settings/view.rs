//! Vista del panel de Ajustes: navegación a la izquierda y opciones a la derecha
//! (maqueta «A · Categorías a la izquierda»).

use cosmic::iced::{Alignment, Background, Color, Length};
use cosmic::{Element, widget};

use super::{self as settings, Change, Section, SettingsMsg, normalize_host, page_rows, step_font};
use crate::app::Message;
use crate::config::{
    DesktopsLayout, DevLayout, HudStyle, Language, MorphConfig, ShelfLayout, TermSize,
};
use crate::desktops::apps::AppIndex;
use crate::desktops::view::display_name;
use crate::dev::forge::Forge;
use crate::dev::view::{PickerRow, PickerSection, RepoPicker, SectionBody};

const WHITE: Color = Color::WHITE;
const TEXT: Color = Color::from_rgb(0.933, 0.933, 0.933); // #eee
const NAV_TEXT: Color = Color::from_rgb(0.733, 0.733, 0.733); // #bbb
const DIM: Color = Color::from_rgb(0.54, 0.54, 0.54); // #8a8a8a
const NAV_ON: Color = Color::from_rgb(0.122, 0.165, 0.239); // #1f2a3d
const SEG_ON: Color = Color::from_rgb(0.176, 0.424, 0.875); // #2d6cdf
const SEG_OFF: Color = Color::from_rgb(0.114, 0.114, 0.114); // #1d1d1d
const SEG_TEXT: Color = Color::from_rgb(0.667, 0.667, 0.667); // #aaa
const SEP: Color = Color::from_rgb(0.102, 0.102, 0.102); // #1a1a1a
const OK: Color = Color::from_rgb(0.2, 0.8, 0.2);
const BAD: Color = Color::from_rgb(0.914, 0.4, 0.4); // #e96
const NAV_LINE: Color = Color::from_rgb(0.114, 0.114, 0.114); // #1d1d1d

/// Lo que la vista necesita del estado de la app.
pub struct Ctx<'a> {
    pub section: Section,
    pub config: &'a MorphConfig,
    /// Nombres de los monitores conectados.
    pub outputs: &'a [String],
    pub autostart: bool,
    /// Borrador del servidor de GitLab (`None` = el guardado).
    pub host_draft: Option<&'a str>,
    pub apps: &'a AppIndex,
    /// Estado del selector de repos (GitHub y GitLab); `Some` mientras se está en esas secciones.
    pub picker: Option<&'a RepoPicker>,
}

/// Línea fina horizontal entre filas.
fn separator() -> widget::Container<'static, Message, cosmic::Theme> {
    widget::container(widget::space())
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .class(cosmic::theme::Container::custom(|_| {
            widget::container::Style {
                background: Some(Background::Color(SEP)),
                ..Default::default()
            }
        }))
}

fn text(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

fn flat_style(bg: Option<Color>, fg: Color, radius: f32) -> widget::button::Style {
    widget::button::Style {
        background: bg.map(Background::Color),
        border_radius: radius.into(),
        text_color: Some(fg),
        icon_color: Some(fg),
        ..Default::default()
    }
}

/// Botón plano con colores fijos (sin el estilo del tema).
fn flat_button<'a>(
    content: impl Into<Element<'a, Message>>,
    bg: Option<Color>,
    fg: Color,
    hover_fg: Color,
    radius: f32,
) -> widget::Button<'a, Message> {
    widget::button::custom(content)
        .padding(0)
        .class(cosmic::theme::Button::Custom {
            active: Box::new(move |_, _| flat_style(bg, fg, radius)),
            hovered: Box::new(move |_, _| flat_style(bg, hover_fg, radius)),
            pressed: Box::new(move |_, _| flat_style(bg, hover_fg, radius)),
            disabled: Box::new(move |_| flat_style(bg, fg, radius)),
        })
}

fn centered<'a>(
    e: impl Into<Element<'a, Message>>,
) -> widget::Container<'a, Message, cosmic::Theme> {
    widget::container(e)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
}

/// Opciones contiguas; la activa en azul. Cada opción lleva su mensaje.
fn segments(options: Vec<(String, bool, SettingsMsg)>) -> Element<'static, Message> {
    let mut row = widget::Row::new().spacing(2);
    for (label, on, msg) in options {
        let (bg, fg) = if on {
            (SEG_ON, WHITE)
        } else {
            (SEG_OFF, SEG_TEXT)
        };
        row = row.push(
            flat_button(
                widget::container(text(label, 11, fg))
                    .padding([0, 9])
                    .height(Length::Fill)
                    .align_y(Alignment::Center),
                Some(bg),
                fg,
                WHITE,
                6.0,
            )
            .height(Length::Fixed(22.0))
            .on_press(Message::Settings(msg)),
        );
    }
    row.into()
}

fn toggle(
    on: bool,
    enabled: bool,
    msg: impl Fn(bool) -> SettingsMsg + 'static,
) -> Element<'static, Message> {
    let t = widget::toggler(on).size(16.0);
    if enabled {
        t.on_toggle(move |v| Message::Settings(msg(v))).into()
    } else {
        t.into()
    }
}

fn set_toggle(on: bool, f: fn(bool) -> Change) -> Element<'static, Message> {
    toggle(on, true, move |v| SettingsMsg::Set(f(v)))
}

/// Fila: etiqueta a la izquierda, control a la derecha y un separador sutil.
fn row(label: impl Into<String>, control: Element<'static, Message>) -> Element<'static, Message> {
    let line = separator();
    widget::Column::new()
        .push(
            widget::Row::new()
                .align_y(Alignment::Center)
                .spacing(8)
                .padding([5, 4])
                .push(text(label, 12, TEXT))
                .push(widget::space().width(Length::Fill))
                .push(control),
        )
        .push(line)
        .into()
}

/// Como `row`, con un texto gris pequeño bajo la etiqueta.
fn row_hint(
    label: &str,
    hint: &str,
    control: Element<'static, Message>,
) -> Element<'static, Message> {
    widget::Column::new()
        .push(
            widget::Row::new()
                .align_y(Alignment::Center)
                .spacing(8)
                .padding([5, 4])
                .push(
                    widget::Column::new()
                        .push(text(label, 12, TEXT))
                        .push(text(hint, 10, DIM)),
                )
                .push(widget::space().width(Length::Fill))
                .push(control),
        )
        .push(separator())
        .into()
}

fn seg_opt<T: PartialEq + Copy>(
    label: &str,
    cur: T,
    value: T,
    f: impl Fn(T) -> Change,
) -> (String, bool, SettingsMsg) {
    (label.to_string(), cur == value, SettingsMsg::Set(f(value)))
}

fn general(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let cfg = c.config;
    let hour = segments(vec![
        seg_opt(
            &crate::fl!("settings-hour-12"),
            cfg.clock_12h,
            true,
            Change::Clock12h,
        ),
        seg_opt(
            &crate::fl!("settings-hour-24"),
            cfg.clock_12h,
            false,
            Change::Clock12h,
        ),
    ]);
    let size = segments(
        [
            (0u16, crate::fl!("settings-size-auto")),
            (115, "115".into()),
            (130, "130".into()),
            (150, "150".into()),
        ]
        .into_iter()
        .map(|(v, l)| seg_opt(&l, cfg.size_percent, v, Change::Size))
        .collect(),
    );
    let mut monitors = vec![(
        crate::fl!("settings-monitor-active"),
        cfg.output.is_none(),
        SettingsMsg::Set(Change::Output(None)),
    )];
    for name in c.outputs {
        monitors.push((
            name.clone(),
            cfg.output.as_deref() == Some(name.as_str()),
            SettingsMsg::Set(Change::Output(Some(name.clone()))),
        ));
    }
    let language = segments(vec![
        seg_opt(
            &crate::fl!("general-language-system"),
            cfg.language,
            Language::System,
            Change::Language,
        ),
        seg_opt(
            &crate::fl!("general-language-en"),
            cfg.language,
            Language::En,
            Change::Language,
        ),
        seg_opt(
            &crate::fl!("general-language-es"),
            cfg.language,
            Language::Es,
            Change::Language,
        ),
    ]);
    vec![
        row(crate::fl!("general-language"), language),
        row(crate::fl!("settings-clock-format"), hour),
        row(crate::fl!("settings-size"), size),
        row(crate::fl!("settings-monitor"), segments(monitors)),
        row(
            crate::fl!("settings-open-on-hover"),
            set_toggle(cfg.open_on_hover, Change::OpenOnHover),
        ),
        row(
            crate::fl!("settings-animations"),
            set_toggle(cfg.animate, Change::Animate),
        ),
        row(
            crate::fl!("settings-alarm-sounds"),
            set_toggle(cfg.sounds, Change::Sounds),
        ),
        row(
            crate::fl!("settings-minimal-pill"),
            set_toggle(cfg.minimal, Change::Minimal),
        ),
        row(
            crate::fl!("settings-hide-line"),
            set_toggle(cfg.hide_line, Change::HideLine),
        ),
        row_hint(
            &crate::fl!("settings-panel-level"),
            &crate::fl!("settings-panel-level-hint"),
            set_toggle(cfg.panel_level, Change::PanelLevel),
        ),
        row(
            crate::fl!("settings-pill-battery"),
            set_toggle(cfg.pill_battery, Change::PillBattery),
        ),
        row(
            crate::fl!("settings-pill-volume"),
            set_toggle(cfg.pill_volume, Change::PillVolume),
        ),
        row(
            crate::fl!("settings-position"),
            small_button(
                crate::fl!("settings-center"),
                settings::can_center(cfg.pill_pos).then_some(SettingsMsg::CenterPill),
            ),
        ),
        row(
            crate::fl!("settings-autostart"),
            toggle(c.autostart, true, SettingsMsg::Autostart),
        ),
    ]
}

fn small_button(label: impl Into<String>, msg: Option<SettingsMsg>) -> Element<'static, Message> {
    let fg = if msg.is_some() {
        SEG_TEXT
    } else {
        Color::from_rgb(0.3, 0.3, 0.3)
    };
    let b = flat_button(centered(text(label, 10, fg)), None, fg, WHITE, 6.0)
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0));
    match msg {
        Some(m) => b.on_press(Message::Settings(m)).into(),
        None => b.into(),
    }
}

fn pages(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let active = c.config.pages.len();
    page_rows(&c.config.pages)
        .into_iter()
        .enumerate()
        .map(|(i, (p, on))| {
            let up = (on && i > 0).then_some(SettingsMsg::PageMove(p, -1));
            let down = (on && i + 1 < active).then_some(SettingsMsg::PageMove(p, 1));
            let controls = widget::Row::new()
                .align_y(Alignment::Center)
                .spacing(2)
                .push(small_button("▲", up))
                .push(small_button("▼", down))
                .push(widget::space().width(Length::Fixed(6.0)))
                .push(text(p.title(), 12, if on { TEXT } else { DIM }));
            let last_active = on && active <= 1;
            let t = toggle(on, !last_active, move |v| SettingsMsg::PageToggle(p, v));
            widget::Column::new()
                .push(
                    widget::Row::new()
                        .align_y(Alignment::Center)
                        .padding([5, 4])
                        .push(controls)
                        .push(widget::space().width(Length::Fill))
                        .push(t),
                )
                .push(separator())
                .into()
        })
        .collect()
}

fn notices(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let cfg = c.config;
    let style = segments(vec![
        seg_opt(
            &crate::fl!("settings-hud-wide"),
            cfg.hud_style,
            HudStyle::Wide,
            Change::HudStyle,
        ),
        seg_opt(
            &crate::fl!("settings-hud-tall"),
            cfg.hud_style,
            HudStyle::Tall,
            Change::HudStyle,
        ),
    ]);
    let head = |s: String| -> Element<'static, Message> {
        widget::container(text(
            s.to_uppercase(),
            11,
            Color::from_rgb(0.435, 0.627, 1.0),
        ))
        .padding([8, 4, 2, 4])
        .into()
    };
    vec![
        head(crate::fl!("settings-hud")),
        row(crate::fl!("settings-hud-style"), style),
        row(
            crate::fl!("settings-hud-volume"),
            set_toggle(cfg.hud_volume, Change::HudVolume),
        ),
        row(
            crate::fl!("settings-hud-brightness"),
            set_toggle(cfg.hud_brightness, Change::HudBrightness),
        ),
        head(crate::fl!("settings-notices")),
        row(
            crate::fl!("settings-notify-battery"),
            set_toggle(cfg.notify_battery, Change::NotifyBattery),
        ),
        row(
            crate::fl!("settings-notify-wifi"),
            set_toggle(cfg.notify_wifi, Change::NotifyWifi),
        ),
        row(
            crate::fl!("settings-notify-bluetooth"),
            set_toggle(cfg.notify_bluetooth, Change::NotifyBluetooth),
        ),
        row(
            crate::fl!("settings-notify-ci"),
            set_toggle(cfg.notify_ci, Change::NotifyCi),
        ),
    ]
}

fn shelf(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let cfg = c.config;
    vec![
        row(
            crate::fl!("settings-view"),
            segments(vec![
                seg_opt(
                    &crate::fl!("settings-shelf-grid"),
                    cfg.shelf_layout,
                    ShelfLayout::Grid,
                    Change::ShelfLayout,
                ),
                seg_opt(
                    &crate::fl!("settings-shelf-list"),
                    cfg.shelf_layout,
                    ShelfLayout::List,
                    Change::ShelfLayout,
                ),
            ]),
        ),
        row(
            crate::fl!("settings-shelf-badge"),
            set_toggle(cfg.shelf_badge, Change::ShelfBadge),
        ),
        row(
            crate::fl!("settings-shelf-start-empty"),
            set_toggle(cfg.shelf_start_empty, Change::ShelfStartEmpty),
        ),
        row(
            crate::fl!("settings-shelf-drag"),
            segments(vec![
                seg_opt(
                    &crate::fl!("settings-shelf-move"),
                    cfg.shelf_drag_copy,
                    false,
                    Change::ShelfDragCopy,
                ),
                seg_opt(
                    &crate::fl!("settings-shelf-copy"),
                    cfg.shelf_drag_copy,
                    true,
                    Change::ShelfDragCopy,
                ),
            ]),
        ),
    ]
}

fn desktops(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let cfg = c.config;
    let mut v = vec![
        row(
            crate::fl!("settings-view"),
            segments(vec![
                seg_opt(
                    &crate::fl!("settings-desk-rows"),
                    cfg.desktops_layout,
                    DesktopsLayout::Rows,
                    Change::DesktopsLayout,
                ),
                seg_opt(
                    &crate::fl!("settings-desk-columns"),
                    cfg.desktops_layout,
                    DesktopsLayout::Columns,
                    Change::DesktopsLayout,
                ),
            ]),
        ),
        widget::container(text(
            crate::fl!("settings-pinned-apps"),
            11,
            Color::from_rgb(0.435, 0.627, 1.0),
        ))
        .padding([8, 4, 2, 4])
        .into(),
    ];
    if cfg.pins.is_empty() {
        v.push(
            widget::container(text(crate::fl!("settings-no-pins"), 12, DIM))
                .padding([5, 4])
                .into(),
        );
    }
    for pin in &cfg.pins {
        let label = crate::fl!(
            "settings-pin-label",
            app = display_name(&pin.app_id, c.apps),
            n = pin.workspace
        );
        let pin = pin.clone();
        let del = flat_button(
            centered(
                widget::icon::from_name("user-trash-symbolic")
                    .size(14)
                    .icon(),
            ),
            None,
            SEG_TEXT,
            WHITE,
            6.0,
        )
        .width(Length::Fixed(22.0))
        .height(Length::Fixed(22.0))
        .on_press(Message::Settings(SettingsMsg::Unpin(pin)));
        v.push(row(label, del.into()));
    }
    v.push(
        widget::container(text(crate::fl!("settings-pin-hint"), 11, DIM))
            .padding([6, 4])
            .into(),
    );
    v
}

/// Fila del servidor de GitLab con su validación y borrador.
fn gitlab_host_row(c: &Ctx<'_>) -> Element<'static, Message> {
    let cfg = c.config;
    let shown = c.host_draft.unwrap_or(&cfg.gitlab_host).to_string();
    let invalid = c.host_draft.is_some_and(|d| normalize_host(d).is_none());
    let input = widget::text_input("gitlab.com", shown)
        .on_input(|t| Message::Settings(SettingsMsg::GitlabHostEdit(t)))
        .on_submit(|_| Message::Settings(SettingsMsg::GitlabHostCommit))
        .width(Length::Fixed(180.0));
    // Misma forma de árbol siempre (así el campo no pierde el foco al teclear).
    let border = if invalid {
        Color::from_rgb(0.9, 0.25, 0.25)
    } else {
        Color::TRANSPARENT
    };
    let input = widget::container(input)
        .padding(1)
        .class(cosmic::theme::Container::custom(move |_| {
            widget::container::Style {
                border: cosmic::iced::Border {
                    color: border,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                ..Default::default()
            }
        }));
    let host: Element<'static, Message> = widget::Column::new()
        .align_x(Alignment::End)
        .push(input)
        .push(
            widget::container(text(
                if invalid {
                    crate::fl!("settings-gitlab-invalid")
                } else {
                    String::new()
                },
                10,
                Color::from_rgb(0.9, 0.25, 0.25),
            ))
            .height(Length::Fixed(14.0)),
        )
        .into();
    row(crate::fl!("settings-gitlab-server"), host)
}

fn dev(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let cfg = c.config;
    vec![
        row(
            crate::fl!("settings-view"),
            segments(vec![
                seg_opt(
                    &crate::fl!("settings-dev-columns"),
                    cfg.dev_layout,
                    DevLayout::Columns,
                    Change::DevLayout,
                ),
                seg_opt(
                    &crate::fl!("settings-dev-tabs"),
                    cfg.dev_layout,
                    DevLayout::Tabs,
                    Change::DevLayout,
                ),
            ]),
        ),
        row(
            crate::fl!("settings-repos"),
            text(
                settings::summary(cfg.github_repos.len(), cfg.gitlab_projects.len()),
                11,
                DIM,
            )
            .into(),
        ),
    ]
}

/// Texto que envuelve (no se corta en filas largas).
fn wtext(s: impl Into<String>, size: u16, color: Color) -> widget::Text<'static, cosmic::Theme> {
    widget::text(s.into())
        .size(size)
        .class(cosmic::theme::Text::Color(color))
}

/// Botón de texto pequeño; sin mensaje queda apagado.
fn text_button(label: impl Into<String>, msg: Option<Message>) -> Element<'static, Message> {
    let fg = if msg.is_some() {
        SEG_TEXT
    } else {
        Color::from_rgb(0.3, 0.3, 0.3)
    };
    let b = flat_button(
        widget::container(text(label, 11, fg))
            .padding([0, 9])
            .height(Length::Fill)
            .align_y(Alignment::Center),
        Some(SEG_OFF),
        fg,
        WHITE,
        6.0,
    )
    .height(Length::Fixed(22.0));
    match msg {
        Some(m) => b.on_press(m).into(),
        None => b.into(),
    }
}

/// Fila con casilla de una sección del selector (guarda al momento).
fn picker_row(r: PickerRow, forge: Forge) -> Element<'static, Message> {
    let n = r.repo.clone();
    let mut col = widget::Column::new().width(Length::Fill).push(
        widget::checkbox(r.on)
            .label(r.repo)
            .width(Length::Fill)
            .text_wrapping(cosmic::iced::widget::text::Wrapping::WordOrGlyph)
            .on_toggle(move |_| match forge {
                Forge::GitHub => Message::DevRepoToggle(n.clone()),
                Forge::GitLab => Message::DevGlToggle(n.clone()),
            }),
    );
    if let Some((sub, bad)) = r.sub {
        col = col.push(
            widget::Row::new()
                .push(widget::space().width(Length::Fixed(24.0)))
                .push(wtext(sub, 11, if bad { BAD } else { DIM })),
        );
    }
    col.into()
}

/// Secciones del selector (título, cargando/error/filas) añadidas a la columna.
fn push_sections(
    mut col: widget::Column<'static, Message, cosmic::Theme>,
    sections: Vec<PickerSection>,
    forge: Forge,
) -> widget::Column<'static, Message, cosmic::Theme> {
    for sec in sections {
        col = col.push(widget::container(wtext(sec.title, 12, DIM)).padding([6, 4, 2, 4]));
        match sec.body {
            SectionBody::Loading => col = col.push(wtext(crate::fl!("settings-loading"), 12, DIM)),
            SectionBody::Error(e) => col = col.push(wtext(e, 12, DIM)),
            SectionBody::Rows(rows) => {
                for r in rows {
                    col = col.push(picker_row(r, forge));
                }
            }
        }
    }
    col
}

/// Campo para añadir un repo/proyecto a mano.
fn manual_row(
    placeholder: String,
    value: String,
    on_input: fn(String) -> Message,
    on_add: Message,
    err: Option<String>,
) -> Element<'static, Message> {
    let mut col = widget::Column::new().spacing(2).push(
        widget::Row::new()
            .align_y(Alignment::Center)
            .spacing(6)
            .push(
                widget::text_input(placeholder, value)
                    .on_input(on_input)
                    .on_submit({
                        let m = on_add.clone();
                        move |_| m.clone()
                    })
                    .width(Length::Fill),
            )
            .push(text_button(crate::fl!("settings-add"), Some(on_add))),
    );
    if let Some(e) = err {
        col = col.push(wtext(e, 11, BAD));
    }
    col.into()
}

fn github(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let Some(p) = c.picker else {
        return vec![wtext(crate::fl!("settings-loading"), 12, DIM).into()];
    };
    let col = widget::Column::new()
        .spacing(2)
        .width(Length::Fill)
        .push(wtext(crate::fl!("settings-github-hint"), 11, DIM));
    let mut col = push_sections(col, crate::dev::view::picker_sections(p), Forge::GitHub);
    col = col.push(
        widget::container(wtext(crate::fl!("settings-add-manually"), 12, DIM))
            .padding([6, 4, 2, 4]),
    );
    col = col.push(manual_row(
        crate::fl!("settings-github-placeholder"),
        p.manual.clone(),
        Message::DevManualInput,
        Message::DevManualAdd,
        p.manual_err.then(|| crate::fl!("settings-github-format")),
    ));
    col = col.push(
        widget::container(wtext(crate::fl!("settings-github-login"), 11, DIM)).padding([6, 4]),
    );
    vec![col.into()]
}

fn gitlab(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let mut v = vec![gitlab_host_row(c)];
    let Some(p) = c.picker else {
        v.push(wtext(crate::fl!("settings-loading"), 12, DIM).into());
        return v;
    };
    let mut col = widget::Column::new().spacing(4).width(Length::Fill);
    if p.gl_has_token {
        let who = match &p.gl_user {
            Some(u) => crate::fl!("settings-gitlab-connected-as", user = u.as_str()),
            None => crate::fl!("settings-gitlab-connected"),
        };
        col = col.push(
            widget::Row::new()
                .align_y(Alignment::Center)
                .spacing(8)
                .padding([5, 4])
                .push(text(crate::fl!("settings-gitlab-token"), 12, TEXT))
                .push(widget::space().width(Length::Fill))
                .push(text(who, 11, OK))
                .push(text_button(
                    crate::fl!("settings-gitlab-forget-token"),
                    Some(Message::DevGlForget),
                )),
        );
    } else {
        let save = (!p.gl_busy && !p.gl_token_input.as_str().is_empty())
            .then_some(Message::DevGlTokenSave);
        col = col
            .push(
                widget::container(wtext(crate::fl!("settings-gitlab-connect-hint"), 11, DIM))
                    .padding([5, 4, 0, 4]),
            )
            .push(
                widget::Row::new()
                    .align_y(Alignment::Center)
                    .spacing(6)
                    .push(text_button(
                        crate::fl!("settings-gitlab-create-token"),
                        Some(Message::DevGlCreateToken),
                    ))
                    .push(
                        widget::text_input(
                            crate::fl!("settings-gitlab-paste-token"),
                            p.gl_token_input.as_str().to_string(),
                        )
                        .password()
                        .on_input(|t| Message::DevGlTokenInput(crate::dev::gitlab::GlToken::new(t)))
                        .on_submit(|_| Message::DevGlTokenSave)
                        .width(Length::Fill),
                    )
                    .push(text_button(crate::fl!("settings-save"), save)),
            );
    }
    if let Some((m, ok)) = &p.gl_msg {
        col = col.push(wtext(m.clone(), 11, if *ok { OK } else { BAD }));
    }
    col = push_sections(col, crate::dev::view::gitlab_sections(p), Forge::GitLab);
    col = col.push(
        widget::container(wtext(crate::fl!("settings-gitlab-add-manually"), 12, DIM))
            .padding([6, 4, 2, 4]),
    );
    col = col.push(manual_row(
        crate::fl!("settings-gitlab-placeholder"),
        p.gl_manual.clone(),
        Message::DevGlManualInput,
        Message::DevGlManualAdd,
        p.gl_manual_err
            .then(|| crate::fl!("settings-gitlab-format")),
    ));
    v.push(col.into());
    v
}

fn terminal(c: &Ctx<'_>) -> Vec<Element<'static, Message>> {
    let cfg = c.config;
    let font = widget::Row::new()
        .align_y(Alignment::Center)
        .spacing(6)
        .push(font_step("−", cfg.term_font_size, -1))
        .push(text(cfg.term_font_size.to_string(), 12, TEXT))
        .push(font_step("+", cfg.term_font_size, 1));
    vec![
        row(
            crate::fl!("settings-term-size"),
            segments(vec![
                seg_opt(
                    &crate::fl!("settings-term-large"),
                    cfg.term_size,
                    TermSize::Large,
                    Change::TermSize,
                ),
                seg_opt(
                    &crate::fl!("settings-term-medium"),
                    cfg.term_size,
                    TermSize::Medium,
                    Change::TermSize,
                ),
            ]),
        ),
        row(crate::fl!("settings-term-font"), font.into()),
    ]
}

fn font_step(label: &'static str, size: u16, delta: i32) -> Element<'static, Message> {
    let next = step_font(size, delta);
    small_button(
        label,
        (next != size).then_some(SettingsMsg::Set(Change::TermFont(next))),
    )
}

fn content(c: &Ctx<'_>) -> Element<'static, Message> {
    let items = match c.section {
        Section::General => general(c),
        Section::Pages => pages(c),
        Section::Notices => notices(c),
        Section::Shelf => shelf(c),
        Section::Desktops => desktops(c),
        Section::Dev => dev(c),
        Section::GitHub => github(c),
        Section::GitLab => gitlab(c),
        Section::Terminal => terminal(c),
    };
    let col = items
        .into_iter()
        .fold(widget::Column::new().width(Length::Fill), |col, e| {
            col.push(e)
        });
    widget::scrollable(widget::container(col).padding([0, 10, 0, 0]))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn nav(current: Section) -> Element<'static, Message> {
    let mut col = widget::Column::new().spacing(2).width(Length::Fill);
    for s in Section::ALL {
        let on = s == current;
        let (bg, fg) = if on {
            (Some(NAV_ON), WHITE)
        } else {
            (None, NAV_TEXT)
        };
        col = col.push(
            flat_button(
                widget::container(text(s.title(), 12, fg)).padding([4, 8]),
                bg,
                fg,
                WHITE,
                7.0,
            )
            .width(Length::Fill)
            .on_press(Message::Settings(SettingsMsg::Section(s))),
        );
    }
    widget::container(col)
        .width(Length::Fixed(118.0))
        .padding([0, 8, 0, 0])
        .into()
}

fn header() -> Element<'static, Message> {
    let back = flat_button(
        centered(text("←", 14, SEG_TEXT)),
        Some(SEP),
        SEG_TEXT,
        WHITE,
        7.0,
    )
    .width(Length::Fixed(30.0))
    .height(Length::Fixed(22.0))
    .on_press(Message::Settings(SettingsMsg::Close));
    widget::Row::new()
        .align_y(Alignment::Center)
        .push(back)
        .push(
            widget::container(text(crate::fl!("settings-title"), 15, WHITE))
                .width(Length::Fill)
                .align_x(Alignment::Center),
        )
        .push(widget::space().width(Length::Fixed(30.0)))
        .into()
}

/// Panel completo (reemplaza a la página y a los puntos).
pub fn view(c: &Ctx<'_>) -> Element<'static, Message> {
    widget::Column::new()
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .push(header())
        .push(
            widget::Row::new()
                .spacing(12)
                .height(Length::Fill)
                .push(nav(c.section))
                .push(
                    widget::container(widget::space())
                        .width(Length::Fixed(1.0))
                        .height(Length::Fill)
                        .class(cosmic::theme::Container::custom(|_| {
                            widget::container::Style {
                                background: Some(Background::Color(NAV_LINE)),
                                ..Default::default()
                            }
                        })),
                )
                .push(content(c)),
        )
        .padding(16)
        .into()
}
