//! Terminal desplegable: estado, mensajes, superficie layer-shell y vista.
use std::sync::{Mutex, OnceLock};

use cosmic::cctk::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use cosmic::iced::futures::SinkExt;
use cosmic::iced::keyboard::{self, Key, Modifiers, key::Named};
use cosmic::iced::mouse;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, set_size,
};
use cosmic::iced::{Alignment, Background, Border, Color, Length, Limits, Padding, Subscription};
use cosmic::surface::action::{LiveSettings, simple_layer_shell};
use cosmic::{Element, widget};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::{App, Message, Task, label};
use crate::config::{TermSize, resolve_output};
use crate::term::font;
use crate::term::grid::Snapshot;
use crate::term::pty::{Mode, Session, TermEvent};
use crate::term::surface::{MIN_H, pct_from_height, smallest_monitor, surface_size};
use crate::term::tabs::{self, Tabs};
use crate::term::widget::{Cell, TermView, grid_size};

const TAB_ON: Color = Color::from_rgb(0.176, 0.424, 0.875); // #2d6cdf
const TAB_OFF: Color = Color::from_rgb(0.114, 0.114, 0.114); // #1d1d1d
const GRIP: Color = Color::from_rgb(0.267, 0.267, 0.267); // #444
const BAR_COLOR: Color = Color::from_rgb(0.667, 0.667, 0.667); // #aaa
/// Márgenes de la superficie y alturas fijas (para estimar la rejilla al crear).
const PAD_X: f32 = 14.0;
const PAD_TOP: f32 = 8.0;
const BAR_H: f32 = 26.0;
const GRIP_H: f32 = 14.0;
const BLINK: std::time::Duration = std::time::Duration::from_millis(530);
/// Separación vertical entre barra, terminal y asa.
const SPACING: f32 = 6.0;
const TITLE_MAX: usize = 18;

#[derive(Clone, Debug)]
pub enum TermMsg {
    /// Muestra u oculta la terminal.
    Toggle,
    Event(u64, TermEvent),
    Input(Vec<u8>),
    PasteRequest,
    Paste(Option<String>),
    Copy(String),
    Scroll(i32),
    Resize(u16, u16),
    Select(Cell, Cell),
    SelectClear,
    OpenUrl(String),
    NewTab,
    SelectTab(u64),
    Key(cosmic::iced::window::Id, Key, Modifiers),
    DragStart,
    DragMove(cosmic::iced::window::Id, f32),
    DragEnd,
    Blink,
}

pub struct TermTab {
    id: u64,
    session: Session,
    snap: Snapshot,
    mode: Mode,
    sel: Option<(Cell, Cell)>,
    /// Hubo salida mientras la terminal estaba oculta (o la pestaña no era la visible).
    dirty: bool,
}

pub struct TermState {
    tabs: Tabs,
    /// Sesiones alineadas con `tabs.items`.
    items: Vec<TermTab>,
    /// Superficie layer-shell mientras la terminal está visible.
    pub(super) surface: Option<cosmic::iced::window::Id>,
    cell: (f32, f32),
    cursor_on: bool,
    /// Alto (px) mientras se arrastra el borde inferior.
    live_h: Option<u32>,
    pub(super) dragging: bool,
    grid: (u16, u16),
}

impl TermState {
    pub fn new(font_size: u16) -> Self {
        Self {
            tabs: Tabs::new(),
            items: Vec::new(),
            surface: None,
            cell: font::cell_metrics::<cosmic::Renderer>(f32::from(font_size)),
            cursor_on: true,
            live_h: None,
            dragging: false,
            grid: (0, 0),
        }
    }

    pub fn set_font_size(&mut self, font_size: u16) {
        self.cell = font::cell_metrics::<cosmic::Renderer>(f32::from(font_size));
    }

    pub fn visible(&self) -> bool {
        self.surface.is_some()
    }

    fn tab_mut(&mut self, id: u64) -> Option<&mut TermTab> {
        self.items.iter_mut().find(|t| t.id == id)
    }

    fn active_mut(&mut self) -> Option<&mut TermTab> {
        self.items.get_mut(self.tabs.active)
    }

    /// Re-lee la instantánea de una pestaña.
    fn refresh(&mut self, index: usize) {
        if let Some(t) = self.items.get_mut(index) {
            t.snap = t.session.snapshot();
            t.mode = t.session.mode();
            t.dirty = false;
        }
    }

    /// Si el historial está desplazado, vuelve al final (al teclear o pegar).
    fn snap_to_bottom(&mut self) {
        let i = self.tabs.active;
        if self.items.get(i).is_some_and(|t| t.snap.display_offset > 0) {
            self.items[i].session.scroll_to_bottom();
            self.refresh(i);
        }
    }

    fn refresh_active_if_dirty(&mut self) {
        let i = self.tabs.active;
        if self.items.get(i).is_some_and(|t| t.dirty) {
            self.refresh(i);
        }
    }
}

// Los eventos de todas las sesiones se funden en un solo canal.
type Hub = (
    UnboundedSender<(u64, TermEvent)>,
    Mutex<Option<UnboundedReceiver<(u64, TermEvent)>>>,
);

fn hub() -> &'static Hub {
    static HUB: OnceLock<Hub> = OnceLock::new();
    HUB.get_or_init(|| {
        let (tx, rx) = unbounded_channel();
        (tx, Mutex::new(Some(rx)))
    })
}

/// Reenvía los eventos de una sesión al canal común (un hilo que duerme en la
/// lectura; termina cuando la sesión se suelta).
fn forward(id: u64, mut rx: UnboundedReceiver<TermEvent>) {
    let tx = hub().0.clone();
    std::thread::spawn(move || {
        while let Some(ev) = rx.blocking_recv() {
            if tx.send((id, ev)).is_err() {
                break;
            }
        }
    });
}

/// Eventos de las sesiones y atajos de pestañas (siempre activa; en reposo no hace nada).
pub fn events() -> Subscription<Message> {
    Subscription::run(|| {
        cosmic::iced::stream::channel(
            16,
            |mut out: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                let rx = hub().1.lock().ok().and_then(|mut g| g.take());
                let Some(mut rx) = rx else {
                    std::future::pending::<()>().await;
                    return;
                };
                while let Some((id, ev)) = rx.recv().await {
                    if out
                        .send(Message::Term(TermMsg::Event(id, ev)))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            },
        )
    })
}

/// Ctrl+Shift+T/W y Ctrl+Tab que el widget no consume.
pub fn shortcuts() -> Subscription<Message> {
    cosmic::iced::event::listen_with(|event, status, id| match event {
        cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if modifiers.control() && status == cosmic::iced::event::Status::Ignored =>
        {
            Some(Message::Term(TermMsg::Key(id, key, modifiers)))
        }
        _ => None,
    })
}

/// Mientras se arrastra el borde: posición del puntero y soltar.
pub fn drag_events() -> Subscription<Message> {
    cosmic::iced::event::listen_with(|event, _status, id| match event {
        cosmic::iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::Term(TermMsg::DragMove(id, position.y)))
        }
        cosmic::iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::Term(TermMsg::DragEnd))
        }
        _ => None,
    })
}

pub fn blink() -> Subscription<Message> {
    cosmic::iced::time::every(BLINK).map(|_| Message::Term(TermMsg::Blink))
}

/// Acción del atajo de teclado de pestañas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKey {
    New,
    Close,
    Next,
    Prev,
}

pub fn tab_key(key: &Key, mods: Modifiers) -> Option<TabKey> {
    if !mods.control() {
        return None;
    }
    match key {
        Key::Named(Named::Tab) => Some(if mods.shift() {
            TabKey::Prev
        } else {
            TabKey::Next
        }),
        Key::Character(c) if mods.shift() => match c.to_ascii_lowercase().as_str() {
            "t" => Some(TabKey::New),
            "w" => Some(TabKey::Close),
            _ => None,
        },
        _ => None,
    }
}

fn shell_name() -> String {
    std::env::var("SHELL")
        .ok()
        .and_then(|s| s.rsplit('/').next().map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "zsh".into())
}

impl App {
    fn term_monitor(&self) -> (u32, u32) {
        let name = resolve_output(self.config.output.as_deref(), &self.output_names());
        let named = name
            .as_ref()
            .and_then(|n| self.outputs.iter().find(|(_, o)| o == n))
            .and_then(|(wl, _)| self.output_sizes.iter().find(|(o, _)| o == wl));
        match named {
            Some((_, s)) => *s,
            // Salida sin resolver (Active): el menor ancho y alto conocidos.
            None => {
                let all: Vec<(u32, u32)> = self.output_sizes.iter().map(|(_, s)| *s).collect();
                smallest_monitor(&all)
            }
        }
    }

    fn term_dims(&self) -> (u32, u32) {
        let (w, h) = surface_size(
            self.term_monitor(),
            self.config.term_size,
            self.config.term_height_pct,
        );
        (w, self.term.live_h.unwrap_or(h))
    }

    fn term_estimated_grid(&self) -> (u16, u16) {
        let (w, h) = self.term_dims();
        grid_size(
            w as f32 - 2.0 * PAD_X,
            h as f32 - PAD_TOP - BAR_H - GRIP_H - 2.0 * SPACING,
            self.term.cell,
        )
    }

    pub(super) fn toggle_terminal(&mut self) -> Task<Message> {
        if self.term.visible() {
            self.hide_terminal()
        } else {
            self.show_terminal()
        }
    }

    fn spawn_tab(&mut self) -> Result<(), String> {
        let (cols, rows) = self.term.grid_or(self.term_estimated_grid());
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let (session, rx) = Session::spawn(None, &[], &home, cols, rows, self.term.cell)?;
        let title = tabs::tab_title(None, session.cwd().as_deref(), &shell_name());
        let id = self.term.tabs.open(&title);
        if crate::envcompat::var_os("MORPH_DEBUG").is_some() {
            eprintln!("morph: terminal: sesión {id} creada ({cols}x{rows})");
        }
        forward(id, rx);
        let snap = session.snapshot();
        let mode = session.mode();
        self.term.items.push(TermTab {
            id,
            session,
            snap,
            mode,
            sel: None,
            dirty: false,
        });
        // La pestaña nueva queda al final y activa: items sigue alineado.
        debug_assert_eq!(self.term.items.len(), self.term.tabs.items.len());
        Ok(())
    }

    fn show_terminal(&mut self) -> Task<Message> {
        if self.term.items.is_empty()
            && let Err(e) = self.spawn_tab()
        {
            eprintln!("morph: terminal: {e}");
            return Task::none();
        }
        self.term.refresh_active_if_dirty();
        let name = resolve_output(self.config.output.as_deref(), &self.output_names());
        let output = name
            .as_ref()
            .and_then(|n| self.outputs.iter().find(|(_, o)| o == n))
            .map(|(wl, _)| IcedOutput::Output(wl.clone()))
            .unwrap_or(IcedOutput::Active);
        let id = cosmic::iced::window::Id::unique();
        self.term.surface = Some(id);
        self.term.cursor_on = true;
        let (w, h) = self.term_dims();
        if crate::envcompat::var_os("MORPH_DEBUG").is_some() {
            eprintln!(
                "morph: terminal: superficie {w}x{h}, {} pestaña(s)",
                self.term.items.len()
            );
        }
        cosmic::surface::surface_task(simple_layer_shell(
            LiveSettings::default,
            move || SctkLayerSurfaceSettings {
                id,
                layer: Layer::Overlay,
                keyboard_interactivity: KeyboardInteractivity::Exclusive,
                anchor: Anchor::TOP,
                exclusive_zone: 0,
                namespace: "morph-terminal".into(),
                output: output.clone(),
                size: Some((Some(w), Some(h))),
                size_limits: Limits::NONE,
                ..Default::default()
            },
            None::<fn() -> Element<'static, cosmic::Action<Message>>>,
        ))
    }

    /// Destruye la superficie conservando las sesiones.
    fn hide_terminal(&mut self) -> Task<Message> {
        self.term.dragging = false;
        self.term.live_h = None;
        match self.term.surface.take() {
            Some(id) => destroy_layer_surface(id),
            None => Task::none(),
        }
    }

    /// El compositor cerró una superficie: si era la de la terminal, queda oculta.
    pub(super) fn term_surface_closed(&mut self, id: cosmic::iced::window::Id) {
        if self.term.surface == Some(id) {
            self.term.surface = None;
            self.term.dragging = false;
            self.term.live_h = None;
        }
    }

    /// Cierra una pestaña (sesión incluida). Esconde la terminal si era la última.
    fn close_tab(&mut self, id: u64) -> Task<Message> {
        let Some(c) = self.term.tabs.close(id) else {
            return Task::none();
        };
        if crate::envcompat::var_os("MORPH_DEBUG").is_some() {
            eprintln!("morph: terminal: sesión {id} cerrada");
        }
        // Al soltar la sesión se envía SIGHUP al shell.
        self.term.items.remove(c.index);
        self.term.refresh_active_if_dirty();
        if c.hide {
            self.hide_terminal()
        } else {
            Task::none()
        }
    }

    /// Un monitor cambió de tamaño o escala: reajusta la terminal si está visible.
    pub(super) fn term_monitor_changed(&mut self) -> Task<Message> {
        match self.term.surface {
            Some(id) if !self.term.dragging => {
                let (w, h) = self.term_dims();
                set_size(id, Some(w), Some(h))
            }
            _ => Task::none(),
        }
    }

    /// Cambios de configuración que afectan a la terminal.
    pub(super) fn term_config_changed(
        &mut self,
        old: &crate::config::MorphConfig,
    ) -> Task<Message> {
        if old.term_font_size != self.config.term_font_size {
            self.term.set_font_size(self.config.term_font_size);
        }
        let reshape = old.term_size != self.config.term_size
            || old.term_height_pct != self.config.term_height_pct;
        match self.term.surface {
            Some(id) if reshape && !self.term.dragging => {
                let (w, h) = self.term_dims();
                set_size(id, Some(w), Some(h))
            }
            _ => Task::none(),
        }
    }

    pub(super) fn on_term(&mut self, msg: TermMsg) -> Task<Message> {
        match msg {
            TermMsg::Toggle => return self.toggle_terminal(),
            TermMsg::Event(id, ev) => return self.on_term_event(id, ev),
            TermMsg::Input(bytes) => {
                self.term.snap_to_bottom();
                if let Some(t) = self.term.active_mut() {
                    t.sel = None;
                    t.session.write(&bytes);
                }
            }
            TermMsg::PasteRequest => {
                return cosmic::iced::clipboard::read()
                    .map(|t| cosmic::Action::App(Message::Term(TermMsg::Paste(t))));
            }
            TermMsg::Paste(Some(text)) => {
                self.term.snap_to_bottom();
                if let Some(t) = self.term.active_mut() {
                    t.session.paste(&text);
                }
            }
            TermMsg::Paste(None) => {}
            TermMsg::Copy(text) => return cosmic::iced::clipboard::write(text),
            TermMsg::Scroll(lines) => {
                let i = self.term.tabs.active;
                if let Some(t) = self.term.items.get_mut(i) {
                    t.sel = None;
                    // `scroll` no emite Wakeup: se redibuja aquí.
                    t.session.scroll(lines);
                }
                self.term.refresh(i);
            }
            TermMsg::Resize(cols, rows) => {
                if self.term.grid != (cols, rows) {
                    self.term.grid = (cols, rows);
                    let cell = self.term.cell;
                    let active = self.term.tabs.active;
                    for (i, t) in self.term.items.iter_mut().enumerate() {
                        t.session.resize(cols, rows, cell);
                        t.sel = None;
                        // Las pestañas de fondo se releen al mostrarlas.
                        t.dirty = i != active;
                    }
                    let i = self.term.tabs.active;
                    self.term.refresh(i);
                }
            }
            TermMsg::Select(a, b) => {
                if let Some(t) = self.term.active_mut() {
                    t.sel = Some((a, b));
                }
            }
            TermMsg::SelectClear => {
                if let Some(t) = self.term.active_mut() {
                    t.sel = None;
                }
            }
            TermMsg::OpenUrl(url) => {
                if url.starts_with("https://") || url.starts_with("http://") {
                    super::open_url(&url);
                }
            }
            TermMsg::NewTab => {
                if let Err(e) = self.spawn_tab() {
                    eprintln!("morph: terminal: {e}");
                }
            }
            TermMsg::SelectTab(id) => {
                self.term.tabs.select(id);
                self.term.refresh_active_if_dirty();
            }
            TermMsg::Key(id, key, mods) => {
                if self.term.surface != Some(id) {
                    return Task::none();
                }
                match tab_key(&key, mods) {
                    Some(TabKey::New) => return self.on_term(TermMsg::NewTab),
                    Some(TabKey::Close) => {
                        if let Some(id) = self.term.tabs.active_id() {
                            return self.close_tab(id);
                        }
                    }
                    Some(TabKey::Next) => {
                        self.term.tabs.next();
                        self.term.refresh_active_if_dirty();
                    }
                    Some(TabKey::Prev) => {
                        self.term.tabs.prev();
                        self.term.refresh_active_if_dirty();
                    }
                    None => {}
                }
            }
            TermMsg::DragStart => {
                if self.term.visible() {
                    self.term.dragging = true;
                }
            }
            TermMsg::DragMove(win, y) => {
                let Some(id) = self
                    .term
                    .surface
                    .filter(|s| self.term.dragging && *s == win)
                else {
                    return Task::none();
                };
                let (w, _) = self.term_dims();
                let max = self.term_monitor().1.max(MIN_H);
                let h = ((y + GRIP_H / 2.0).max(0.0) as u32).clamp(MIN_H, max);
                if self.term.live_h != Some(h) {
                    self.term.live_h = Some(h);
                    return set_size(id, Some(w), Some(h));
                }
            }
            TermMsg::DragEnd => {
                self.term.dragging = false;
                if let Some(h) = self.term.live_h.take() {
                    let pct = pct_from_height(h, self.term_monitor().1);
                    match cosmic::cosmic_config::Config::new(super::APP_ID, 1) {
                        Ok(handle) => {
                            if let Err(e) = self.config.set_term_height_pct(&handle, Some(pct)) {
                                eprintln!("morph: no se pudo guardar term_height_pct: {e}");
                            }
                        }
                        Err(e) => {
                            eprintln!("morph: no se pudo guardar term_height_pct: {e}");
                            self.config.term_height_pct = Some(pct);
                        }
                    }
                }
            }
            TermMsg::Blink => self.term.cursor_on = !self.term.cursor_on,
        }
        Task::none()
    }

    fn on_term_event(&mut self, id: u64, ev: TermEvent) -> Task<Message> {
        let visible = self.term.visible();
        let active = self.term.tabs.active_id();
        match ev {
            TermEvent::Wakeup => {
                let Some(index) = self.term.tabs.index_of(id) else {
                    return Task::none();
                };
                // Oculta o en otra pestaña: se marca y se lee al mostrarla.
                if visible && active == Some(id) {
                    self.term.refresh(index);
                    // El contenido cambió: una selección vieja copiaría texto equivocado.
                    if let Some(t) = self.term.tab_mut(id) {
                        t.sel = None;
                    }
                } else if let Some(t) = self.term.tab_mut(id) {
                    t.dirty = true;
                }
            }
            TermEvent::Title(title) => {
                let cwd = self.term.tab_mut(id).and_then(|t| t.session.cwd());
                let t = tabs::tab_title(Some(&title), cwd.as_deref(), &shell_name());
                self.term.tabs.set_title(id, &t);
            }
            TermEvent::Exit => return self.close_tab(id),
            TermEvent::ClipboardStore(text) => return cosmic::iced::clipboard::write(text),
            TermEvent::Bell => {}
        }
        Task::none()
    }

    /// Vista de la superficie de la terminal.
    pub(super) fn term_view(&self) -> Element<'_, Message> {
        let term = &self.term;
        let Some(tab) = term.items.get(term.tabs.active) else {
            return widget::space().into();
        };
        let msg = |m: TermMsg| Message::Term(m);
        let view = TermView::new(
            &tab.snap,
            term.cell,
            f32::from(self.config.term_font_size),
            tab.mode,
        )
        .focused(true)
        .cursor_visible(term.cursor_on)
        .selection(tab.sel)
        .on_input(move |b| msg(TermMsg::Input(b)))
        .on_paste_request(move || msg(TermMsg::PasteRequest))
        .on_copy(move |t| msg(TermMsg::Copy(t)))
        .on_scroll(move |n| msg(TermMsg::Scroll(n)))
        .on_resize(move |c, r| msg(TermMsg::Resize(c, r)))
        .on_select(move |a, b| msg(TermMsg::Select(a, b)))
        .on_select_clear(move || msg(TermMsg::SelectClear))
        .on_open_url(move |u| msg(TermMsg::OpenUrl(u)));

        let mut bar = widget::Row::new().spacing(8).align_y(Alignment::Center);
        if self.config.term_size == TermSize::Large {
            for (i, t) in term.tabs.items.iter().enumerate() {
                bar = bar.push(chip(
                    tabs::label(i, &t.title, TITLE_MAX),
                    i == term.tabs.active,
                    msg(TermMsg::SelectTab(t.id)),
                ));
            }
            bar = bar.push(chip("+".into(), false, msg(TermMsg::NewTab)));
        } else {
            let t = &term.tabs.items[term.tabs.active.min(term.tabs.items.len() - 1)];
            let n = term.tabs.items.len();
            let text = if n > 1 {
                format!("{}/{n} · {}", term.tabs.active + 1, t.title)
            } else {
                t.title.clone()
            };
            bar = bar.push(label(text, 12, BAR_COLOR));
        }
        bar = bar.push(widget::space().width(Length::Fill)).push(chip(
            "✕".into(),
            false,
            msg(TermMsg::Toggle),
        ));

        let grip = widget::container(widget::space())
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(4.0))
            .class(cosmic::theme::Container::custom(|_| {
                widget::container::Style {
                    background: Some(Background::Color(GRIP)),
                    border: Border {
                        radius: 2.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
        let handle = widget::mouse_area(
            widget::container(grip)
                .width(Length::Fill)
                .height(Length::Fixed(GRIP_H))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center),
        )
        .on_press(msg(TermMsg::DragStart))
        .interaction(mouse::Interaction::ResizingVertically);

        let body = widget::Column::new()
            .spacing(SPACING)
            .push(widget::container(bar).height(Length::Fixed(BAR_H)))
            .push(
                widget::container(view)
                    .width(Length::Fill)
                    .height(Length::Fill),
            )
            .push(handle)
            .padding(Padding {
                top: PAD_TOP,
                right: PAD_X,
                bottom: 0.0,
                left: PAD_X,
            });
        widget::container(body)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::custom(|_| {
                widget::container::Style {
                    background: Some(Background::Color(Color::BLACK)),
                    border: Border {
                        radius: [0.0, 0.0, 16.0, 16.0].into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }))
            .into()
    }
}

impl TermState {
    /// Rejilla conocida (la del widget) o la estimada.
    fn grid_or(&self, estimate: (u16, u16)) -> (u16, u16) {
        if self.grid == (0, 0) {
            estimate
        } else {
            self.grid
        }
    }
}

fn chip(text: String, on: bool, msg: Message) -> Element<'static, Message> {
    let (bg, fg) = if on {
        (TAB_ON, Color::WHITE)
    } else {
        (TAB_OFF, BAR_COLOR)
    };
    widget::button::custom(
        widget::container(label(text, 12, fg))
            .padding([2, 9])
            .class(cosmic::theme::Container::custom(move |_| {
                widget::container::Style {
                    background: Some(Background::Color(bg)),
                    border: Border {
                        radius: 6.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })),
    )
    .padding(0)
    .class(cosmic::theme::Button::Text)
    .on_press(msg)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(s: &str) -> Key {
        Key::Character(s.into())
    }

    #[test]
    fn atajos_de_pestana() {
        let cs = Modifiers::CTRL | Modifiers::SHIFT;
        assert_eq!(tab_key(&ch("T"), cs), Some(TabKey::New));
        assert_eq!(tab_key(&ch("w"), cs), Some(TabKey::Close));
        assert_eq!(
            tab_key(&Key::Named(Named::Tab), Modifiers::CTRL),
            Some(TabKey::Next)
        );
        assert_eq!(tab_key(&Key::Named(Named::Tab), cs), Some(TabKey::Prev));
        assert_eq!(tab_key(&ch("t"), Modifiers::CTRL), None);
        assert_eq!(tab_key(&ch("c"), cs), None);
        assert_eq!(tab_key(&ch("t"), Modifiers::SHIFT), None);
    }
}
