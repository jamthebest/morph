use cosmic::app::{Core, CosmicFlags, Task};
use cosmic::cctk::sctk::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use cosmic::cctk::wayland_client::protocol::wl_output::WlOutput;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::iced::event::wayland as wl_event;
use cosmic::iced::platform_specific::runtime::wayland::layer_surface::{
    IcedOutput, SctkLayerSurfaceSettings,
};
use cosmic::iced::window;
use cosmic::iced::{Alignment, Background, Border, Color, Length, Limits, Subscription};
use cosmic::surface::action::{LiveSettings, simple_layer_shell};
use cosmic::{Element, widget};
use std::time::{Duration, Instant};

use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::mouse::{self, ScrollDelta};
use cosmic::iced::platform_specific::shell::commands::layer_surface::{
    destroy_layer_surface, set_anchor, set_exclusive_zone, set_input_zone,
    set_keyboard_interactivity, set_size,
};

mod term_ui;
pub use term_ui::TermMsg;

use crate::agenda::calendar_view::truncate;
use crate::agenda::form::{self, Form, FormKind, Spinner};
use crate::agenda::schedule::{self, DueKind};
use crate::agenda::{Agenda, Kind, calendar_view, form_view, month, store, todo_view};
use crate::anim::SizeAnim;
use crate::cli::CliAction;
use crate::clock;
use crate::config::{Edge, MorphConfig, effective_scale, resolve_output};
use crate::desktops::apps::{self, AppIndex, AppInfo};
use crate::desktops::pins::toggle_pin;
use crate::desktops::view::{display_name, pin_notice};
use crate::desktops::{self, DesktopsCmd, DesktopsEvent};
use crate::dev::ports::{self, Listener};
use crate::dnd;
use crate::drag::{Drag, DragEv, DragOut};
use crate::geometry::{self, Size};
use crate::hud;
use crate::island::{Content, Event, IslandState, KeyboardMode, Page, content_for};
use crate::lyrics::lrc::current_line;
use crate::lyrics::{Lyrics, LyricsState, TrackKey, lrclib};
use crate::media::mpris::{MediaCmd, MediaEvent};
use crate::media::{ArtSource, NowPlaying, Status, art, position_at};
use crate::music::{fmt_time, lyric_window};
use crate::notice::{self, Notice, NoticeAction, NoticeKind, NoticeQueue};
use crate::peek;
use crate::reveal::Reveal;
use crate::scroll::{Delta, ScrollGate};
use crate::settings::{self, Change, Section, SettingsMsg};
use crate::sound::{self, SoundKind};
use crate::sys::power::BatteryReading;
use crate::sys::rules::{
    BatteryRules, BtDevice, BtRules, HudGate, HudValue, NetState, SysNotice, WifiRules,
};
use crate::volume;
use chrono::Datelike;
use cosmic::widget::image::Handle;
use tokio::sync::mpsc::UnboundedSender;

pub const APP_ID: &str = "io.github.jamthebest.Morph";

const PILL_RADIUS: f32 = 18.0;
/// Radio de la línea del modo oculto.
const LINE_RADIUS: f32 = 1.5;
const EXPANDED_RADIUS: f32 = 28.0;
/// Periodo de las barras de la pastilla y del avance de la letra (4 fps).
const TICK: Duration = Duration::from_millis(250);
const SEMIBOLD: cosmic::iced::Font = cosmic::iced::Font {
    weight: cosmic::iced::font::Weight::Semibold,
    ..cosmic::iced::Font::DEFAULT
};
/// Segundos que la barra «Deshacer» sigue visible tras borrar una fila.
const UNDO_SECS: u64 = 5;
const GRAY: Color = Color::from_rgb(0.6, 0.6, 0.6);

/// Una lectura de volumen vale si no es anterior al último `set` y no se está arrastrando.
fn accept_volume_read(read_gen: u64, cur_gen: u64, dragging: bool, pending: bool) -> bool {
    read_gen == cur_gen && !dragging && !pending
}

/// Una respuesta de letras solo vale si es de la canción actual y se la esperaba.
fn accept_lyrics(state: &LyricsState, current: Option<&TrackKey>, incoming: &TrackKey) -> bool {
    current == Some(incoming) && matches!(state, LyricsState::Loading(k) if k == incoming)
}

/// Estado de letras tras una respuesta (sin cambios si es de otra canción).
fn lyrics_after(
    state: &LyricsState,
    current: Option<&TrackKey>,
    incoming: (TrackKey, Result<Lyrics, String>),
) -> LyricsState {
    let (k, res) = incoming;
    if !accept_lyrics(state, current, &k) {
        return state.clone();
    }
    match res {
        Ok(l) => LyricsState::Ready(k, l),
        Err(_) => LyricsState::Failed(k),
    }
}

/// La carátula cargada solo vale si es la de la pista actual.
fn accept_art(current: Option<&ArtSource>, incoming: &ArtSource) -> bool {
    current == Some(incoming)
}

fn label<'a>(text: String, size: u16, color: Color) -> widget::Text<'a, cosmic::Theme> {
    widget::text(text)
        .size(size)
        .wrapping(cosmic::iced::widget::text::Wrapping::None)
        .class(cosmic::theme::Text::Color(color))
}

fn icon_button<'a>(name: &'static str, msg: Message) -> Element<'a, Message> {
    widget::button::icon(widget::icon::from_name(name).size(24).symbolic(true))
        .on_press(msg)
        .into()
}

/// Botón ⚙ de la fila de puntos: gris que se aclara al pasar el puntero.
fn gear_button<'a>() -> Element<'a, Message> {
    let handle = widget::icon::from_name("emblem-system-symbolic")
        .fallback(Some(widget::icon::IconFallback::Names(vec![
            "preferences-system-symbolic".into(),
        ])))
        .size(16)
        .symbolic(true)
        .handle();
    let style = |c: f32| widget::button::Style {
        icon_color: Some(Color::from_rgb(c, c, c)),
        ..Default::default()
    };
    widget::button::custom(widget::icon::icon(handle).size(16))
        .padding(0)
        .class(cosmic::theme::Button::Custom {
            active: Box::new(move |_, _| style(0.55)),
            hovered: Box::new(move |_, _| style(0.9)),
            pressed: Box::new(move |_, _| style(0.9)),
            disabled: Box::new(move |_| style(0.55)),
        })
        .on_press(Message::Settings(SettingsMsg::Open))
        .into()
}

fn interactivity(mode: KeyboardMode) -> KeyboardInteractivity {
    match mode {
        KeyboardMode::None => KeyboardInteractivity::None,
        KeyboardMode::OnDemand => KeyboardInteractivity::OnDemand,
    }
}

/// Un cierre solo exige recrear la superficie si es la vigente (los ids viejos
/// llegan al destruir a propósito en `reconcile_output`).
fn should_recreate(pill_id: window::Id, closed: window::Id, surface_made: bool) -> bool {
    surface_made && closed == pill_id
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub action: CliAction,
    subcommand: Option<String>,
}

impl Flags {
    pub fn new(action: CliAction) -> Self {
        Self {
            action,
            subcommand: action.to_action_string(),
        }
    }
}

impl CosmicFlags for Flags {
    type SubCommand = String;
    type Args = Vec<String>;

    fn action(&self) -> Option<&String> {
        self.subcommand.as_ref()
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Tick,
    Island(Event),
    Key(keyboard::Key),
    /// Datos `text/uri-list` soltados sobre la isla: se añaden al Shelf.
    Dropped(Vec<u8>),
    /// Abre una entrada del Shelf con `xdg-open`.
    ShelfOpen(std::path::PathBuf),
    /// Quita una entrada del Shelf (nunca borra el archivo).
    ShelfRemove(std::path::PathBuf),
    /// Vacía el Shelf (nunca borra archivos).
    ShelfClear,
    /// Miniatura calculada fuera del hilo de la interfaz (None = no se pudo).
    ThumbReady(
        std::path::PathBuf,
        Option<std::time::SystemTime>,
        Option<crate::shelf::thumb::Rgba>,
    ),
    /// El arrastre entrante salió de la isla sin soltar nada.
    DndLeft,
    /// Un arrastre entrante entró en la isla: abre Shelf y anula un cierre pendiente.
    DndEnter,
    /// Arrastre saliente en curso (true) o terminado/cancelado (false).
    Dragging(bool),
    /// Empieza un arrastre saliente del Shelf con estas rutas.
    DragStart(Vec<std::path::PathBuf>),
    /// Vuelve a revisar el Shelf (el destino pudo mover los archivos tras soltar).
    PruneShelf,
    /// Evento del hilo Wayland de escritorios (spike).
    Desktops(DesktopsEvent),
    /// Apps instaladas leídas de los `.desktop` (arranque).
    AppsLoaded(Vec<AppInfo>),
    /// Clic en el número de un escritorio: cambia a él y cierra la isla.
    DeskActivateWorkspace(u32),
    /// Clic en una app abierta: salta a su escritorio, la enfoca y cierra la isla.
    DeskActivateWindow(u32),
    /// Clic en una fijada sin abrir: (app_id, id del escritorio) cambia y la lanza.
    DeskLaunch(String, u32),
    /// Tras dejar que cambie el escritorio: lanza (argv, directorio de trabajo).
    LaunchNow(Vec<String>, Option<String>),
    /// Clic derecho: fija/quita la app en el escritorio (app_id, índice 1-based).
    DeskTogglePin(String, u32),
    /// Configuración cambiada (watch_config).
    Config(MorphConfig, Vec<&'static str>),
    /// Ctrl pulsado/soltado (solo llega con foco de teclado en la isla).
    CtrlHeld(bool),
    /// Panel de Ajustes.
    Settings(SettingsMsg),
    /// Monitor visto o actualizado (nombre + objeto Wayland).
    OutputSeen(WlOutput, String, Option<(i32, i32)>),
    OutputGone(WlOutput),
    /// Terminal desplegable.
    Term(TermMsg),
    /// Crea la superficie tras dar tiempo a conocer los monitores.
    Realize,
    /// El compositor cerró una superficie (p. ej. monitor desconectado).
    SurfaceClosed(window::Id),
    /// Fotograma de la animación de tamaño.
    AnimFrame,
    /// Evento del reproductor MPRIS.
    Media(MediaEvent),
    /// Orden de los botones de la página Música.
    MediaCmd(MediaCmd),
    /// Carátula cargada (o fallida) para esa fuente.
    ArtLoaded(ArtSource, Option<Handle>),
    /// Respuesta de LRCLIB.
    LyricsLoaded((TrackKey, Result<Lyrics, String>)),
    /// Refresco de la letra y el progreso (solo página Música sonando).
    LyricsTick,
    /// Fotograma de las barras de la pastilla (solo cerrada y sonando).
    BarsTick,
    /// Evento de rueda o panel táctil sobre la isla.
    Wheel(Delta),
    /// Temporizador de relectura del volumen (solo página Música abierta).
    VolumePoll,
    /// Volumen del sistema leído (None si falló).
    VolumeRead(u64, Option<(f32, bool)>),
    /// El deslizador de volumen se movió (0..=100).
    VolumeChanged(f32),
    /// Se soltó el deslizador de volumen.
    VolumeReleased,
    /// Acción del usuario sobre el aviso actual.
    Notice(NoticeAction),
    /// Cierre automático de un aviso no fijo (generación del aviso mostrado).
    NoticeTimeout(u64),
    /// Lectura de volumen o brillo de un vigilante del sistema (la primera es la línea base).
    HudRaw(HudValue),
    /// El puntero entró en la pastilla (vista asomada).
    PeekEnter,
    /// Pasó el retardo de salida: el modo oculto vuelve a la línea.
    LineHide,
    /// Botón pulsado sobre la pastilla cerrada (clic o inicio de arrastre).
    PillPress,
    /// Puntero movido sobre la caja ((x, y) relativas a la caja); recuerda dónde se pulsará.
    PillHover(f32, f32),
    /// Puntero movido durante la pulsación: ventana y (x, y) en la superficie.
    PillDragMove(window::Id, f32, f32),
    /// Botón soltado (o dedo levantado) tras pulsar la pastilla.
    PillDragEnd,
    /// Configure de una superficie: ancho real (todo el del monitor) y alto.
    SurfaceResized(window::Id, u32, u32),
    /// Muestra de CPU/RAM de la vista asomada.
    PeekSample(crate::sys::stats::Sample),
    /// Fin del HUD (generación del HUD mostrado).
    HudExpire(u64),
    /// Lectura de la batería (UPower).
    SysBattery(BatteryReading),
    /// Estado de la red (NetworkManager).
    SysNet(NetState),
    /// Dispositivo Bluetooth leído (BlueZ).
    SysBt(BtDevice),
    /// Confirma una desconexión de Wi-Fi pendiente (generación del temporizador).
    WifiTick(u64),
    /// Abre el formulario de tarea/alarma (alta o edición).
    FormOpen(Form),
    /// Calendario: cambia de mes (±1).
    CalShift(i32),
    /// Calendario: selecciona un día.
    CalSelect(chrono::NaiveDate),
    /// To-do: texto del campo de alta rápida.
    TodoDraft(String),
    /// To-do: agrega una tarea sin fecha con el texto del campo.
    TodoAdd,
    /// Alterna hecha/pendiente de una tarea.
    TaskToggle(crate::agenda::ItemId),
    FormTitle(String),
    /// Selector de hora: horas ±1 (con vuelta).
    FormHour(i32),
    /// Selector de hora: minutos al múltiplo de 5 siguiente/anterior.
    FormMinute(i32),
    FormHourText(String),
    FormMinuteText(String),
    FormPm(bool),
    /// Rueda sobre un selector de hora.
    FormWheel(Spinner, Delta),
    /// Pone la hora de una tarea sin hora (próxima hora en punto).
    FormAddTime,
    /// Quita la hora (y la duración) de una tarea.
    FormRemoveTime,
    FormDuration(String),
    FormRemindStart(bool),
    FormRemindEnd(bool),
    /// Marca/desmarca un día de la semana (lunes = 0).
    FormDay(usize),
    FormKind(FormKind),
    FormSave,
    FormCancel,
    FormDelete,
    /// Borra una fila de Calendario/To-do (queda para deshacer).
    RowDelete(crate::agenda::ItemId),
    /// Deshace el último borrado de fila.
    UndoDelete,
    /// Cierra la barra de deshacer (generación del borrado).
    UndoTimeout(u64),
    /// Activa/desactiva una alarma desde su fila.
    AlarmToggle(crate::agenda::ItemId, bool),
    /// Sin efecto (respuesta de tareas que no devuelven nada).
    Nop,
    /// Sondeo de puertos (cada 3 s, solo con la isla abierta en Dev).
    DevPoll,
    /// Resultado de un escaneo de puertos.
    DevPorts(Vec<Listener>),
    /// Abre `http://localhost:PUERTO` en el navegador.
    DevOpen(u16),
    /// Sondeo de GitHub (cada 60 s con repos elegidos).
    GhTick,
    GlTokenLoaded(
        String,
        Result<Option<crate::dev::gitlab::GlToken>, crate::dev::gitlab::GlError>,
    ),
    GlFetched(u64, crate::dev::gitlab::GlRefresh),
    /// Lectura de GitHub terminada (generación de repos en que se pidió).
    GhFetched(u64, crate::dev::github::Refresh),
    /// Pide confirmación para detener el proceso (pid).
    DevStopAsk(u32),
    DevStopNo,
    /// Confirmado: SIGTERM al proceso (pid).
    DevStopYes(u32),
    /// Pestaña de la página Dev (disposición por pestañas).
    DevTab(crate::dev::view::DevTab),
    /// Abre una URL de PR o ejecución en el navegador.
    DevOpenUrl(String),
    /// «+ Repos»: abre Ajustes en la sección GitHub.
    DevReposOpen,
    /// Repos de cada cuenta de gh para Ajustes (o el error ya en texto).
    DevReposLoaded(Result<crate::dev::github::AccountRepos, String>),
    /// Repos de GitHub hallados en clones locales.
    DevLocalLoaded(Vec<crate::dev::accounts::LocalRepo>),
    /// Texto de la entrada manual del selector.
    DevManualInput(String),
    DevManualAdd,
    DevRepoToggle(String),
    /// GitLab (selector): token pegado (redactado en `Debug`), guardar, olvidar y crear.
    DevGlTokenInput(crate::dev::gitlab::GlToken),
    DevGlTokenSave,
    DevGlTokenSaved(
        String,
        Result<(String, crate::dev::gitlab::GlToken), crate::dev::gitlab::GlError>,
    ),
    DevGlForget,
    DevGlForgot(Result<(), crate::dev::gitlab::GlError>),
    DevGlCreateToken,
    /// Resultado de consultar el llavero al entrar en GitLab (con su servidor).
    DevGlKeyringLoaded(
        String,
        Result<Option<crate::dev::gitlab::GlToken>, crate::dev::gitlab::GlError>,
    ),
    /// Usuario del token ya guardado (para «Conectado como»).
    DevGlWho(String, Result<String, crate::dev::gitlab::GlError>),
    DevGlProjectsLoaded(String, Result<Vec<String>, crate::dev::gitlab::GlError>),
    DevGlLocalLoaded(String, Vec<crate::dev::accounts::LocalRepo>),
    /// Pasó el aplazamiento de la lectura tras marcar repos en Ajustes (generación).
    PollDebounce(u64),
    DevGlManualInput(String),
    DevGlManualAdd,
    DevGlToggle(String),
}

pub struct App {
    core: Core,
    now: chrono::DateTime<chrono::Local>,
    state: IslandState,
    shelf: crate::shelf::Shelf,
    /// El Shelf en disco no se pudo leer/respaldar: no guardar para no pisarlo.
    shelf_save_blocked: bool,
    shelf_save_failed: bool,
    thumbs: crate::shelf::view::Thumbs,
    thumb_pending: std::collections::HashSet<std::path::PathBuf>,
    /// El puntero salió de la isla durante un arrastre saliente: al terminar, cerrarla.
    left_while_dragging: bool,
    /// Fin del último arrastre saliente (para ignorar un clic fantasma).
    last_drag_end: Option<Instant>,
    /// Última vez que se soltó algo sobre la isla (ignora un `leave` posterior al drop).
    last_drop: Option<Instant>,
    /// Hay un arrastre saliente en curso: no cerrar la isla al salir el puntero.
    dragging: bool,
    /// Rutas del arrastre saliente en curso (para atenuar el origen).
    drag_paths: Vec<std::path::PathBuf>,
    /// Un arrastre externo está sobre la isla (entre DndEnter y DndLeft/soltar).
    drop_hover: bool,
    /// Ctrl pulsado según los eventos de teclado (limitación: solo con foco).
    ctrl_held: bool,
    /// Última instantánea cruda; se filtra por monitor al dibujar.
    desk_snap: desktops::Snapshot,
    /// Apps instaladas (vacío hasta `AppsLoaded`: íconos genéricos).
    apps: AppIndex,
    /// Título del último aviso de fijación (para reemplazarlo si aún no se vio).
    last_pin_title: Option<String>,
    config: MorphConfig,
    outputs: Vec<(WlOutput, String)>,
    /// Tamaño lógico de cada monitor (para dimensionar la terminal).
    output_sizes: Vec<(WlOutput, (u32, u32))>,
    term: term_ui::TermState,
    pill_id: window::Id,
    /// Monitor de la superficie actual (None = activo); solo válido si `surface_made`.
    surface_output: Option<String>,
    surface_made: bool,
    /// Tamaño de la superficie: alto fijo (el máximo para la escala); el ancho lo da el monitor.
    surface: Size,
    /// Tamaño actual de la caja negra visible (punto de partida de una animación).
    visible: Size,
    /// Ancho real de la superficie (todo el ancho del monitor), según su configure.
    surface_w: Option<u32>,
    /// Alto real de la superficie según su configure (el normal, o la pantalla entera
    /// durante un arrastre que cambia de borde).
    surface_h: Option<u32>,
    /// Pulsación en curso sobre la pastilla cerrada (clic o arrastre por decidir).
    pill_drag: Option<Drag>,
    /// Posición provisional durante el arrastre (la guardada va en la config al soltar).
    drag_pos: Option<f32>,
    /// Pulsación sin x conocida: el arrastre arranca con el primer movimiento.
    pill_press_pending: bool,
    /// Último (x, y) del puntero dentro de la superficie (para ubicar la pulsación).
    hover: Option<(f32, f32)>,
    /// Última y del puntero en el marco de la superficie de reposo (el agarre implícito la
    /// sigue enviando aunque el puntero salga). Decide el borde de destino.
    drag_last_raw_y: Option<f32>,
    /// Cerrando con animación: se sigue mostrando el contenido expandido hasta terminar.
    closing: bool,
    /// Última página mostrada: se retoma al reabrir.
    last_page: Page,
    /// Panel de Ajustes abierto (sección mostrada). Al cerrar la isla queda hasta la
    /// próxima apertura para que la animación de cierre siga dibujándolo.
    settings: Option<Section>,
    /// Última sección usada en esta ejecución.
    settings_section: Section,
    /// El archivo de autostart existe (se lee al abrir el panel).
    autostart: bool,
    /// Borrador del servidor de GitLab del panel (`None` = mostrar `config.gitlab_host`).
    settings_host_draft: Option<String>,
    anim: Option<SizeAnim>,
    /// Instante del último fotograma (o del inicio) de la animación.
    anim_last: Instant,
    media_tx: Option<UnboundedSender<MediaCmd>>,
    np: Option<NowPlaying>,
    art: Option<Handle>,
    lyrics: LyricsState,
    bars_phase: usize,
    scroll_gate: ScrollGate,
    /// Rueda sobre los selectores de hora (aparte de la de página).
    form_gate: ScrollGate,
    /// Volumen del sistema (0..=1.5) y si está silenciado.
    volume: f32,
    muted: bool,
    /// Instante del último `set` enviado, y si hay un cambio sin enviar.
    vol_last_set: Instant,
    vol_pending: bool,
    vol_dragging: bool,
    /// Se incrementa en cada `set`: las lecturas iniciadas antes son obsoletas.
    vol_gen: u64,
    agenda: Agenda,
    data_dir: std::path::PathBuf,
    notices: NoticeQueue,
    /// Se incrementa cada vez que cambia el aviso mostrado: los cierres viejos no valen.
    notice_gen: u64,
    /// Cierre previsto del aviso no fijo visible (para pausarlo durante un arrastre).
    notice_deadline: Option<Instant>,
    /// Instante en que un arrastre de la pastilla pausó la cuenta del aviso.
    notice_paused: Option<Instant>,
    /// HUD de volumen/brillo mostrado y su generación (el último cambio manda).
    hud: Option<HudValue>,
    hud_gen: u64,
    hud_gate: HudGate,
    /// Vista asomada: el puntero está sobre la pastilla; datos de la última lectura.
    peek: bool,
    /// El puntero está sobre la pastilla (según su mouse_area), con o sin vista asomada.
    pointer_inside: bool,
    /// Modo oculto: la pastilla se muestra porque el puntero está (o estuvo hace < 1 s) encima.
    line_hover: bool,
    peek_stats: peek::Stats,
    sys_battery: BatteryRules,
    sys_wifi: WifiRules,
    sys_bt: BtRules,
    wifi_gen: u64,
    wifi_armed: Option<Instant>,
    /// Último elemento borrado desde una fila (para «Deshacer»).
    last_deleted: Option<crate::agenda::Item>,
    /// Se incrementa en cada borrado: los cierres viejos de la barra no valen.
    undo_gen: u64,
    /// Formulario abierto (reemplaza el contenido de la página actual).
    form: Option<Form>,
    /// Calendario: mes mostrado (año, mes) y día seleccionado.
    cal_view: (i32, u32),
    cal_selected: chrono::NaiveDate,
    /// Ya se fijó el día seleccionado a «hoy» al abrir la isla por primera vez.
    cal_inited: bool,
    /// To-do: borrador del campo de alta rápida.
    todo_draft: String,
    /// La agenda en disco no se pudo leer/respaldar: no guardar para no pisarla.
    save_blocked: bool,
    /// Ya se avisó de un fallo de guardado (se rearma al guardar bien).
    save_failed: bool,
    /// Dev: servidores locales detectados y fila pendiente de confirmar (pid).
    dev_ports: Vec<Listener>,
    dev_confirm: Option<u32>,
    dev_tab: crate::dev::view::DevTab,
    dev_picker: Option<crate::dev::view::RepoPicker>,
    /// GitHub: token en memoria, último dato bueno, error vigente y control del sondeo.
    gh_state: crate::dev::github::GhState,
    /// Repos elegidos que ninguna cuenta ve (última lectura).
    gh_no_access: usize,
    /// Escaneo de clones locales: resultado de la sesión y si hay uno en curso.
    local_scan: Option<Vec<crate::dev::accounts::LocalRepo>>,
    local_scan_busy: bool,
    local_gl_scan: Option<Vec<crate::dev::accounts::LocalRepo>>,
    local_gl_busy: bool,
    gl_kr_busy: bool,
    /// Lecturas de GitHub/GitLab aplazadas tras marcar repos en Ajustes.
    poll_gen: u64,
    pending_gh: bool,
    pending_gl: bool,
    /// Usuario del token de GitLab (solo se averigua al abrir el selector).
    gl_user: Option<String>,
    gh_prs: Vec<crate::dev::github::Pr>,
    gh_runs: Vec<crate::dev::github::Run>,
    gh_last_ok: Option<Instant>,
    /// Última lectura buena de PRs (puede diferir de la de ejecuciones).
    gh_prs_ok: Option<Instant>,
    gh_error: Option<crate::dev::github::GhError>,
    gh_busy: bool,
    /// Se incrementa al cambiar los repos: las lecturas viejas no valen.
    gh_gen: u64,
    /// Límite de uso alcanzado: no sondear hasta entonces.
    gh_pause_until: Option<Instant>,
    /// GitLab (paralelo a GitHub): token del llavero (solo en memoria) y su carga única.
    gl_token: Option<crate::dev::gitlab::GlToken>,
    gl_mrs: Vec<crate::dev::forge::ChangeRequest>,
    gl_pipelines: Vec<crate::dev::forge::Pipeline>,
    gl_last_ok: Option<Instant>,
    gl_error: Option<crate::dev::gitlab::GlError>,
    gl_busy: bool,
    gl_gen: u64,
    gl_pause_until: Option<Instant>,
    /// Proyectos elegidos sin acceso (última lectura).
    gl_no_access: usize,
}

/// Un clic abre el archivo solo si no hay arrastre y no acaba de terminar uno.
fn click_allowed(dragging: bool, since_drag_end: Option<Duration>) -> bool {
    !dragging && since_drag_end.is_none_or(|d| d >= Duration::from_millis(300))
}

/// Abre la ruta con `xdg-open` en un proceso aparte; un hilo lo recoge sin bloquear la UI.
fn open_path(path: &std::path::Path) {
    use std::process::{Command, Stdio};
    match Command::new("xdg-open")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("morph: no se pudo abrir {}: {e}", path.display()),
    }
}

/// Solo URLs `https://` (las de los avisos vienen de la API).
fn is_https_url(u: &str) -> bool {
    u.starts_with("https://")
}

/// Abre una URL con `xdg-open` (mismo patrón que `open_path`).
fn open_url(url: &str) {
    open_path(std::path::Path::new(url));
}

/// Lanza una app (argv ya sin códigos de campo) sin shell y con stdio nulo; un hilo
/// recoge el proceso. Un fallo solo se registra.
fn launch_app(argv: &[String], dir: Option<&str>) {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let Some((prog, args)) = argv.split_first() else {
        return;
    };
    let cwd = dir
        .map(std::path::PathBuf::from)
        .filter(|d| d.is_dir())
        .or_else(|| std::env::var_os("HOME").map(std::path::PathBuf::from));
    let mut cmd = Command::new(prog);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    match cmd
        // Grupo de procesos propio: no muere ni recibe señales con la isla.
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("morph: no se pudo lanzar {prog}: {e}"),
    }
}

fn gl_needs_token() -> String {
    crate::fl!("gl-needs-token")
}

fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

/// Anclaje de la superficie en reposo: ancho completo, pegada al borde elegido.
fn rest_anchor(edge: Edge) -> Anchor {
    match edge {
        Edge::Top => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        Edge::Bottom => Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
    }
}

/// Zona exclusiva en reposo: 0 respeta los paneles; -1 los ignora.
fn rest_zone(panel_level: bool) -> i32 {
    if panel_level { -1 } else { 0 }
}

impl App {
    fn playing(&self) -> bool {
        self.np
            .as_ref()
            .is_some_and(|n| n.status == Status::Playing)
    }

    /// ¿Hay un aviso fijo al frente? El HUD nunca lo tapa.
    fn sticky_notice(&self) -> bool {
        self.notices.current().is_some_and(|n| n.sticky)
    }

    /// HUD visible ahora: isla cerrada y sin aviso fijo. Tapa avisos no fijos y el contador.
    fn hud_active(&self) -> bool {
        self.hud.as_ref().is_some_and(|v| {
            hud::should_show(
                v,
                self.state == IslandState::Collapsed,
                self.sticky_notice(),
                &self.config,
            )
        })
    }

    /// Pastilla cerrada sin los datos de los lados (reloj, música o cápsula del Shelf).
    fn center_collapsed_size(&self) -> Size {
        let base = geometry::collapsed_size(self.config.minimal, self.playing(), self.scale());
        if self.show_badge() {
            geometry::with_badge(base, self.scale())
        } else {
            base
        }
    }

    /// Datos que lleva la pastilla cerrada: (batería, volumen).
    fn pill_extras(&self) -> (bool, bool) {
        geometry::pill_extras(
            self.config.pill_battery,
            self.peek_stats.battery.is_some(),
            self.config.pill_volume,
            self.config.minimal,
        )
    }

    /// Pastilla cerrada normal: lo de siempre más batería y volumen si están activos.
    fn base_collapsed_size(&self) -> Size {
        let (bat, vol) = self.pill_extras();
        geometry::with_extras(
            self.center_collapsed_size(),
            u32::from(bat) + u32::from(vol),
            self.scale(),
        )
    }

    /// Vista asomada visible ahora (con `open_on_hover` no hay vista asomada).
    fn peek_active(&self) -> bool {
        peek::should_peek(&peek::PeekGate {
            hovered: self.peek,
            collapsed: self.state == IslandState::Collapsed,
            dragging: self.dragging || self.pill_dragging(),
            notice: self.notices.current().is_some(),
            hud: self.hud_active(),
            // En modo oculto no hay vista asomada.
            open_on_hover: self.config.open_on_hover || self.config.hide_line,
        })
    }

    /// Solo la línea visible ahora (modo «Ocultar en una línea»).
    fn line_shown(&self) -> bool {
        peek::show_line(
            self.config.hide_line,
            self.state == IslandState::Collapsed,
            self.line_hover,
            self.notices.current().is_some(),
            self.hud_active(),
            self.dragging || self.pill_dragging(),
        )
    }

    /// El puntero entra o sale de la pastilla: crece o vuelve al tamaño normal.
    fn set_peek(&mut self, hovered: bool) -> Task<Message> {
        if self.peek == hovered {
            return Task::none();
        }
        let before = (self.peek_active(), self.line_shown());
        self.peek = hovered;
        // Al salir, `line_hover` se mantiene hasta que venza el retardo de 1 s.
        if hovered && self.config.hide_line {
            self.line_hover = true;
        }
        if hovered {
            // Una nueva asomada muestra «—» hasta la primera muestra.
            self.peek_stats.cpu = None;
            self.peek_stats.ram = None;
        }
        if before == (self.peek_active(), self.line_shown()) {
            return Task::none();
        }
        self.retarget()
    }

    /// Apaga la vista asomada sin animar (el llamador reajusta el tamaño si hace falta).
    fn clear_peek(&mut self) {
        self.peek = false;
        self.line_hover = false;
        self.peek_stats.cpu = None;
        self.peek_stats.ram = None;
    }

    /// Si el puntero sigue sobre la pastilla cerrada, vuelve a asomarse.
    fn resume_peek(&mut self) -> Task<Message> {
        if self.pointer_inside && self.state == IslandState::Collapsed && !self.peek {
            self.set_peek(true)
        } else {
            Task::none()
        }
    }

    /// Único origen del tamaño objetivo de un estado (la animación y `reshape` lo usan).
    fn target_size(&self, state: &IslandState) -> Size {
        match state {
            // Durante el arrastre de la pastilla el HUD y los avisos esperan.
            IslandState::Collapsed if self.pill_dragging() => self.base_collapsed_size(),
            IslandState::Collapsed if self.hud_active() => {
                geometry::hud_size(self.config.hud_style, self.scale())
            }
            IslandState::Collapsed if self.notices.current().is_some() => {
                geometry::notice_size(self.scale())
            }
            IslandState::Collapsed if self.peek_active() => geometry::peek_size(
                self.center_collapsed_size(),
                self.peek_stats.battery.is_some(),
                self.scale(),
            ),
            IslandState::Collapsed if self.line_shown() => geometry::line_size(self.scale()),
            IslandState::Collapsed => self.base_collapsed_size(),
            IslandState::Expanded { .. } if self.settings.is_some() => {
                geometry::settings_size(self.scale())
            }
            IslandState::Expanded { .. } => geometry::size_for(state, self.scale()),
        }
    }

    /// Fija el tamaño de la caja visible y mueve la zona de entrada a su rectángulo.
    /// La superficie no cambia de tamaño.
    fn set_visible(&mut self, size: Size) -> Task<Message> {
        self.visible = size;
        self.sync_place()
    }

    /// Posición efectiva de la pastilla: la provisional del arrastre o la guardada.
    fn pill_pos(&self) -> f32 {
        self.drag_pos.unwrap_or(self.config.pill_pos)
    }

    /// Hay un arrastre de la pastilla en curso (ya pasó del umbral).
    fn pill_dragging(&self) -> bool {
        self.pill_drag.as_ref().is_some_and(Drag::is_active)
    }

    /// Borde al que iría la pastilla si se soltara ahora (el actual fuera de un arrastre).
    fn target_edge(&self) -> Edge {
        match (self.pill_dragging(), self.drag_last_raw_y) {
            (true, Some(y)) => {
                geometry::target_edge(self.config.pill_edge, y, self.surface.h, self.output_h())
            }
            _ => self.config.pill_edge,
        }
    }

    /// y de una caja de alto `h` en la superficie de reposo, pegada al borde.
    fn box_top(&self, h: u32) -> u32 {
        geometry::box_y(self.config.pill_edge, self.surface.h, h)
    }

    /// Alto de la zona de entrada (y del área que recibe el puntero): la línea del modo
    /// oculto es más alta que lo dibujado.
    fn hit_h(&self) -> u32 {
        geometry::zone_height(
            self.config.hide_line && self.state == IslandState::Collapsed,
            self.visible.h,
            self.scale(),
        )
    }

    /// Anclaje de la superficie en reposo: ancho completo, pegada al borde elegido.
    fn anchor(&self) -> Anchor {
        rest_anchor(self.config.pill_edge)
    }

    /// Zona exclusiva: 0 respeta los paneles; -1 los ignora (al nivel del panel).
    fn exclusive_zone(&self) -> i32 {
        rest_zone(self.config.panel_level)
    }

    /// Alto lógico del monitor de la superficie: el de nombre elegido o, con «monitor
    /// activo», el de un monitor del mismo ancho que la superficie.
    fn output_h(&self) -> Option<u32> {
        let w = self.surface_w?;
        let wanted = self.surface_output.as_deref();
        let by_name = wanted.and_then(|n| {
            let (wl, _) = self.outputs.iter().find(|(_, o)| o == n)?;
            self.output_sizes.iter().find(|(o, _)| o == wl)
        });
        by_name
            .or_else(|| self.output_sizes.iter().find(|(_, (ow, _))| *ow == w))
            .map(|(_, (_, h))| *h)
    }

    /// x de la caja en la superficie; None mientras no se conoce su ancho (centrada).
    fn placement(&self) -> Option<u32> {
        let w = self.surface_w?;
        Some(geometry::box_x(w, self.visible.w, self.pill_pos()))
    }

    /// Zona de entrada: solo la caja visible en su `box_x`. Hasta conocer el ancho real
    /// de la superficie queda vacía (no se sabe dónde se dibuja); el primer configure
    /// la coloca.
    fn input_zone(&self) -> Vec<cosmic::iced::Rectangle> {
        match self.placement() {
            Some(x) => {
                let mut zone = self.visible;
                // La línea es muy fina: en modo oculto su zona es más alta que lo dibujado.
                zone.h = self.hit_h();
                vec![geometry::box_rect_at(x, self.box_top(zone.h), zone)]
            }
            None => Vec::new(),
        }
    }

    /// Reaplica la zona de entrada según el tamaño y la posición actuales.
    fn sync_place(&mut self) -> Task<Message> {
        set_input_zone(self.pill_id, Some(self.input_zone()))
    }

    /// Fin de la pulsación: restablece el estado y devuelve el tamaño al que toque.
    fn end_pill_drag(&mut self) -> Task<Message> {
        self.pill_drag = None;
        self.drag_pos = None;
        self.drag_last_raw_y = None;
        Task::batch([self.retarget(), self.resume_peek(), self.resume_notice()])
    }

    /// Reanuda la cuenta del aviso pausada por el arrastre, con el tiempo que le quedaba.
    fn resume_notice(&mut self) -> Task<Message> {
        let (Some(paused), Some(deadline)) = (self.notice_paused.take(), self.notice_deadline)
        else {
            return Task::none();
        };
        let live = self.state == IslandState::Collapsed
            && !self.hud_active()
            && self.notices.current().is_some_and(|n| !n.sticky);
        if !live {
            return Task::none();
        }
        let left = peek::remaining_after_pause(deadline, paused);
        self.notice_deadline = Some(Instant::now() + left);
        self.notice_gen += 1;
        let gen_ = self.notice_gen;
        cosmic::task::future(async move {
            tokio::time::sleep(left).await;
            Message::NoticeTimeout(gen_)
        })
    }

    /// Guarda la posición de la pastilla (solo al soltar).
    fn save_pill_pos(&mut self, pos: f32) {
        match cosmic::cosmic_config::Config::new(APP_ID, 1) {
            Ok(h) => {
                // El setter actualiza `self.config` antes de escribir: el eco del
                // observador llega con el mismo valor y no mueve la superficie.
                if let Err(e) = self.config.set_pill_pos(&h, pos) {
                    eprintln!("morph: no se pudo guardar pill_pos: {e}");
                }
            }
            Err(e) => {
                eprintln!("morph: no se pudo abrir la config: {e}");
                self.config.pill_pos = pos;
            }
        }
    }

    /// Guarda el borde de la isla (solo al soltar).
    fn save_pill_edge(&mut self, edge: Edge) {
        match cosmic::cosmic_config::Config::new(APP_ID, 1) {
            Ok(h) => {
                if let Err(e) = self.config.set_pill_edge(&h, edge) {
                    eprintln!("morph: no se pudo guardar pill_edge: {e}");
                }
            }
            Err(e) => {
                eprintln!("morph: no se pudo abrir la config: {e}");
                self.config.pill_edge = edge;
            }
        }
    }

    /// Escanea los puertos en un hilo aparte.
    fn scan_ports() -> Task<Message> {
        cosmic::task::future(async {
            let uid = ports::my_uid();
            let v = tokio::task::spawn_blocking(move || ports::scan(uid))
                .await
                .unwrap_or_default();
            Message::DevPorts(v)
        })
    }

    fn read_volume(gen_: u64) -> Task<Message> {
        Task::perform(volume::get(), move |v| {
            cosmic::Action::App(Message::VolumeRead(gen_, v))
        })
    }

    fn send_volume(&mut self) -> Task<Message> {
        self.vol_last_set = Instant::now();
        self.vol_pending = false;
        self.vol_gen += 1;
        Task::perform(volume::set(self.volume), |()| {
            cosmic::Action::App(Message::Nop)
        })
    }

    /// Aplica un evento; al abrir la página Música se lee el volumen del sistema.
    fn apply(&mut self, e: Event) -> Task<Message> {
        let entering_shelf = self.state != (IslandState::Expanded { page: Page::Shelf });
        let dev = IslandState::Expanded { page: Page::Dev };
        let entering_dev = self.state != dev;
        let task = self.apply_music(e);
        let mut tasks = vec![task];
        if entering_shelf && self.state == (IslandState::Expanded { page: Page::Shelf }) {
            tasks.push(self.request_thumbs());
        }
        if entering_dev && self.state == dev {
            tasks.push(App::scan_ports());
            if self
                .gh_last_ok
                .is_none_or(|t| t.elapsed() > Duration::from_secs(20))
            {
                tasks.push(self.gh_refresh());
            }
            if !self.gl_error.as_ref().is_some_and(|e| e.is_fatal())
                && self
                    .gl_last_ok
                    .is_none_or(|t| t.elapsed() > Duration::from_secs(20))
            {
                tasks.push(self.gl_refresh());
            }
        }
        if self.state != IslandState::Collapsed && self.peek {
            self.clear_peek();
        }
        // Si la isla deja de estar cerrada con el botón pulsado, no hay arrastre ni clic pendientes.
        if self.state != IslandState::Collapsed {
            self.pill_drag = None;
            self.pill_press_pending = false;
            self.drag_pos = None;
            self.drag_last_raw_y = None;
        }
        tasks.push(self.resume_peek());
        Task::batch(tasks)
    }

    /// La página Dev está activa: sin ella no se consulta GitHub/GitLab ni el llavero.
    fn dev_enabled(&self) -> bool {
        self.config.pages.contains(&Page::Dev)
    }

    /// Lee GitHub (si hay repos, no hay otra lectura en curso ni pausa por límite).
    fn gh_refresh(&mut self) -> Task<Message> {
        if !self.dev_enabled() || self.config.github_repos.is_empty() || self.gh_busy {
            return Task::none();
        }
        if self.gh_pause_until.is_some_and(|t| Instant::now() < t) {
            return Task::none();
        }
        self.gh_busy = true;
        let gen_ = self.gh_gen;
        let state = self.gh_state.clone();
        let repos = self.config.github_repos.clone();
        let prev = self.gh_runs.clone();
        cosmic::task::future(async move {
            Message::GhFetched(gen_, crate::dev::github::refresh(state, repos, prev).await)
        })
    }

    /// Antigüedad del último dato bueno de GitHub (para su mensaje de «sin conexión»).
    fn gh_own_age(&self) -> Option<u64> {
        self.gh_last_ok
            .into_iter()
            .chain(self.gh_prs_ok)
            .max()
            .map(|t| t.elapsed().as_secs())
    }

    /// Antigüedad combinada (solo para «↻ hace N s»).
    fn gh_data_age(&self) -> Option<u64> {
        self.gh_last_ok
            .into_iter()
            .chain(self.gh_prs_ok)
            .chain(self.gl_last_ok)
            .max()
            .map(|t| t.elapsed().as_secs())
    }

    /// Cambiaron los repos: descarta datos (la primera carga nueva no avisa) y relee.
    fn gh_reset(&mut self) -> Task<Message> {
        self.gh_gen += 1;
        self.gh_busy = false;
        self.gh_prs.clear();
        self.gh_runs.clear();
        self.gh_last_ok = None;
        self.gh_prs_ok = None;
        self.gh_error = None;
        self.gh_no_access = 0;
        // Cambió la selección: se conservan las cuentas ya resueltas de repos que siguen
        // elegidos y se vuelve a intentar el resto (incluidos los «sin acceso»).
        let repos = &self.config.github_repos;
        self.gh_state.access.retain(|r, a| {
            repos.contains(r) && matches!(a, crate::dev::accounts::Access::Account(_))
        });
        if self.config.github_repos.is_empty() && debug() {
            eprintln!("morph: github: sin repos, sin sondeo");
        }
        self.gh_refresh()
    }

    fn on_gh_fetched(&mut self, r: crate::dev::github::Refresh) -> Task<Message> {
        use crate::dev::github::GhError;
        self.gh_busy = false;
        // Solo cuando cambia: cuentas y qué cuenta ve cada repo (nunca tokens).
        if debug()
            && (r.state.accounts != self.gh_state.accounts
                || r.state.access != self.gh_state.access)
        {
            eprintln!(
                "morph: github: cuentas {:?}, repos {:?}",
                r.state.accounts, r.state.access
            );
        }
        self.gh_state = r.state;
        self.gh_no_access = r.no_access;
        let limited = |e: Option<&GhError>| match e {
            Some(GhError::RateLimited { until }) => Some(*until),
            _ => None,
        };
        let rate = limited(r.runs.as_ref().err()).or(limited(r.prs.as_ref().err()));
        if let Some(until) = rate {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let wait = until.map_or(300, |u| u.saturating_sub(now)).clamp(60, 900);
            self.gh_pause_until = Some(Instant::now() + Duration::from_secs(wait));
        }
        let mut error = None;
        let mut task = Task::none();
        match r.runs {
            Ok(runs) => {
                let was_empty = self.notices.current().is_none();
                // La primera carga (sin dato previo) nunca avisa.
                if self.gh_last_ok.is_some() {
                    let mut failed = false;
                    let as_pipes =
                        |v: &[crate::dev::github::Run]| -> Vec<crate::dev::forge::Pipeline> {
                            v.iter().cloned().map(Into::into).collect()
                        };
                    for ev in
                        crate::dev::runs::transitions(&as_pipes(&self.gh_runs), &as_pipes(&runs))
                    {
                        if debug() {
                            eprintln!("morph: github: {ev:?}");
                        }
                        failed |= matches!(ev, crate::dev::runs::CiEvent::Failed(_));
                        if self.config.notify_ci {
                            self.notices.push(notice::ci_notice(&ev));
                        }
                    }
                    if failed && self.config.notify_ci && self.config.sounds {
                        sound::play(SoundKind::Reminder);
                    }
                }
                if debug() {
                    eprintln!("morph: github: {} ejecuciones", runs.len());
                }
                self.gh_runs = runs;
                self.gh_last_ok = Some(Instant::now());
                if rate.is_none() {
                    self.gh_pause_until = None;
                }
                if was_empty && self.notices.current().is_some() {
                    task = self.notice_changed();
                }
            }
            Err(e) => error = Some(e),
        }
        match r.prs {
            Ok(prs) => {
                if debug() {
                    eprintln!("morph: github: {} PRs", prs.len());
                }
                self.gh_prs = prs;
                self.gh_prs_ok = Some(Instant::now());
            }
            Err(e) => error = error.or(Some(e)),
        }
        if debug()
            && let Some(e) = &error
        {
            eprintln!("morph: github: error {e:?}");
        }
        self.gh_error = error;
        task
    }

    /// Al entrar en la sección GitHub de Ajustes: cuentas, clones locales y repos de cada
    /// cuenta (lo que antes cargaba el selector «+ Repos»).
    fn gh_picker_open(&mut self) -> Task<Message> {
        let p = self.dev_picker.get_or_insert_with(Default::default);
        p.selected = self.config.github_repos.clone();
        p.accounts = None;
        p.local = None;
        p.manual.clear();
        p.manual_err = false;
        p.access = self.gh_state.access.clone();
        // Cuentas y «sin acceso» se vuelven a averiguar (por si hubo un `gh auth login` nuevo).
        self.gh_state.accounts.clear();
        crate::dev::accounts::drop_no_access(&mut self.gh_state.access);
        if let Some(p) = &mut self.dev_picker {
            p.access = self.gh_state.access.clone();
            p.local = self.local_scan.clone();
        }
        let state = self.gh_state.clone();
        let mut tasks = vec![Task::perform(
            crate::dev::github::picker_accounts(state),
            |r| cosmic::Action::App(Message::DevReposLoaded(r)),
        )];
        // El escaneo del disco se hace una vez por sesión y nunca dos a la vez.
        if self.local_scan.is_none() && !self.local_scan_busy {
            self.local_scan_busy = true;
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            tasks.push(Task::perform(
                async move {
                    match home {
                        Some(h) => tokio::task::spawn_blocking(move || {
                            crate::dev::accounts::scan_local_repos(&h)
                        })
                        .await
                        .unwrap_or_default(),
                        None => Vec::new(),
                    }
                },
                |r| cosmic::Action::App(Message::DevLocalLoaded(r)),
            ));
        }
        Task::batch(tasks)
    }

    /// Al entrar en una sección de Ajustes que necesita cargar datos (el llavero solo se
    /// toca al entrar en GitLab sin token en memoria).
    fn section_entered(&mut self, s: Section) -> Task<Message> {
        match s {
            Section::GitHub => self.gh_picker_open(),
            Section::GitLab => {
                self.dev_picker.get_or_insert_with(Default::default);
                self.gl_picker_open()
            }
            _ => Task::none(),
        }
    }

    /// Abre Ajustes en `section` (la isla debe estar expandida y sin formulario).
    fn open_settings(&mut self, section: Section) -> Task<Message> {
        if self.settings.is_none()
            && self.form.is_none()
            && matches!(self.state, IslandState::Expanded { .. })
        {
            self.autostart = settings::autostart_enabled(&settings::autostart_dir());
            self.settings = Some(section);
            self.settings_section = section;
            return Task::batch([self.section_entered(section), self.retarget()]);
        }
        Task::none()
    }

    /// Al entrar en GitLab: datos de GitLab (clones locales una vez por sesión; proyectos y
    /// usuario si hay token; sin token en memoria se busca en el llavero, porque entrar en la
    /// sección es una acción del usuario y un posible aviso de desbloqueo es aceptable).
    fn gl_picker_open(&mut self) -> Task<Message> {
        let host = self.config.gitlab_host.clone();
        let has_token = self.gl_token.is_some();
        if let Some(p) = &mut self.dev_picker {
            p.gl_host = host.clone();
            p.gl_selected = self.config.gitlab_projects.clone();
            p.gl_local = self.local_gl_scan.clone();
            p.gl_has_token = has_token;
            p.gl_user = self.gl_user.clone();
            if !has_token {
                p.gl_projects = Some(Err(gl_needs_token()));
            }
        }
        let mut tasks = Vec::new();
        if self.local_gl_scan.is_none() && !self.local_gl_busy {
            self.local_gl_busy = true;
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
            let h = host.clone();
            let h2 = host.clone();
            tasks.push(Task::perform(
                async move {
                    match home {
                        Some(d) => tokio::task::spawn_blocking(move || {
                            crate::dev::accounts::scan_local_gitlab(&d, &h)
                        })
                        .await
                        .unwrap_or_default(),
                        None => Vec::new(),
                    }
                },
                move |r| cosmic::Action::App(Message::DevGlLocalLoaded(h2, r)),
            ));
        }
        if let Some(t) = self.gl_token.clone() {
            tasks.push(self.gl_connected_tasks(t));
        } else if !self.gl_kr_busy {
            // Acción del usuario: se consulta el llavero (también reintenta tras un error
            // al arrancar). Puede pedir desbloquearlo.
            self.gl_kr_busy = true;
            let h2 = host.clone();
            tasks.push(Task::perform(
                async move { crate::dev::keyring::load(&host).await },
                move |r| cosmic::Action::App(Message::DevGlKeyringLoaded(h2, r)),
            ));
        }
        Task::batch(tasks)
    }

    /// Proyectos del usuario y su nombre para el selector, con un token ya en memoria.
    fn gl_connected_tasks(&self, t: crate::dev::gitlab::GlToken) -> Task<Message> {
        let host = self.config.gitlab_host.clone();
        let mut tasks = Vec::new();
        if self.gl_user.is_none() {
            let (h, t2, h2) = (host.clone(), t.clone(), host.clone());
            tasks.push(Task::perform(
                async move { crate::dev::gitlab::whoami(&h, &t2).await },
                move |r| cosmic::Action::App(Message::DevGlWho(h2, r)),
            ));
        }
        let h3 = host.clone();
        tasks.push(Task::perform(
            async move { crate::dev::gitlab::my_projects(&host, &t).await },
            move |r| cosmic::Action::App(Message::DevGlProjectsLoaded(h3, r)),
        ));
        Task::batch(tasks)
    }

    /// Lee GitLab (si hay proyectos y token, sin otra lectura en curso ni pausa por límite).
    fn gl_refresh(&mut self) -> Task<Message> {
        if !self.dev_enabled() || self.config.gitlab_projects.is_empty() || self.gl_busy {
            return Task::none();
        }
        if self.gl_pause_until.is_some_and(|t| Instant::now() < t) {
            return Task::none();
        }
        let Some(token) = self.gl_token.clone() else {
            return Task::none();
        };
        self.gl_busy = true;
        let gen_ = self.gl_gen;
        let host = self.config.gitlab_host.clone();
        let projects = self.config.gitlab_projects.clone();
        cosmic::task::future(async move {
            Message::GlFetched(
                gen_,
                crate::dev::gitlab::fetch(&host, &token, &projects).await,
            )
        })
    }

    /// Cambiaron host o proyectos (o arranque): descarta datos y recarga el token del llavero
    /// solo si hay proyectos de GitLab (sin ellos nunca se toca el llavero).
    fn gl_reset(&mut self, keep_token: bool) -> Task<Message> {
        self.gl_gen += 1;
        self.gl_busy = false;
        self.gl_mrs.clear();
        self.gl_pipelines.clear();
        self.gl_last_ok = None;
        self.gl_error = None;
        self.gl_no_access = 0;
        self.gl_pause_until = None;
        if !keep_token {
            self.gl_token = None;
            self.gl_user = None;
        }
        if self.config.gitlab_projects.is_empty() || !self.dev_enabled() {
            if debug() {
                eprintln!("morph: gitlab: sin proyectos o sin página Dev, sin sondeo ni llavero");
            }
            return Task::none();
        }
        if self.gl_token.is_some() {
            return self.gl_refresh();
        }
        let host = self.config.gitlab_host.clone();
        let h2 = host.clone();
        cosmic::task::future(async move {
            Message::GlTokenLoaded(h2, crate::dev::keyring::load(&host).await)
        })
    }

    fn on_gl_token_loaded(
        &mut self,
        r: Result<Option<crate::dev::gitlab::GlToken>, crate::dev::gitlab::GlError>,
    ) -> Task<Message> {
        use crate::dev::gitlab::GlError;
        match r {
            Ok(Some(t)) => {
                if debug() {
                    eprintln!("morph: gitlab: token cargado del llavero");
                }
                self.gl_token = Some(t);
                self.gl_error = None;
                self.gl_refresh()
            }
            Ok(None) => {
                self.gl_error = Some(GlError::NoToken);
                Task::none()
            }
            Err(e) => {
                if debug() {
                    eprintln!("morph: gitlab: error {e:?}");
                }
                self.gl_error = Some(e);
                Task::none()
            }
        }
    }

    fn on_gl_fetched(&mut self, r: crate::dev::gitlab::GlRefresh) -> Task<Message> {
        use crate::dev::gitlab::GlError;
        self.gl_busy = false;
        self.gl_no_access = r.no_access;
        let limited = |e: Option<&GlError>| match e {
            Some(GlError::RateLimited { until }) => Some(*until),
            _ => None,
        };
        let rate = limited(r.pipelines.as_ref().err()).or(limited(r.mrs.as_ref().err()));
        if let Some(until) = rate {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let wait = until.map_or(300, |u| u.saturating_sub(now)).clamp(60, 900);
            self.gl_pause_until = Some(Instant::now() + Duration::from_secs(wait));
        }
        let mut error = None;
        let mut task = Task::none();
        match r.pipelines {
            Ok(fetched) => {
                let pipes = crate::dev::gitlab::keep_failed(&self.gl_pipelines, &r.failed, fetched);
                let was_empty = self.notices.current().is_none();
                // La primera carga (sin dato previo) nunca avisa.
                if self.gl_last_ok.is_some() {
                    let mut failed = false;
                    for ev in crate::dev::runs::transitions(&self.gl_pipelines, &pipes) {
                        if debug() {
                            eprintln!("morph: gitlab: {ev:?}");
                        }
                        failed |= matches!(ev, crate::dev::runs::CiEvent::Failed(_));
                        if self.config.notify_ci {
                            self.notices.push(notice::ci_notice(&ev));
                        }
                    }
                    if failed && self.config.notify_ci && self.config.sounds {
                        sound::play(SoundKind::Reminder);
                    }
                }
                if debug() {
                    eprintln!("morph: gitlab: {} pipelines", pipes.len());
                }
                self.gl_pipelines = pipes;
                self.gl_last_ok = Some(Instant::now());
                if rate.is_none() {
                    self.gl_pause_until = None;
                }
                if was_empty && self.notices.current().is_some() {
                    task = self.notice_changed();
                }
            }
            Err(e) => error = Some(e),
        }
        match r.mrs {
            Ok(mrs) => {
                if debug() {
                    eprintln!("morph: gitlab: {} MRs", mrs.len());
                }
                self.gl_mrs = mrs;
            }
            Err(e) => error = error.or(Some(e)),
        }
        if debug()
            && let Some(e) = &error
        {
            eprintln!("morph: gitlab: error {e:?}");
        }
        self.gl_error = error;
        task
    }

    fn apply_music(&mut self, e: Event) -> Task<Message> {
        let was_music = self.state == (IslandState::Expanded { page: Page::Musica });
        let was_collapsed = self.state == IslandState::Collapsed;
        let before = self.state;
        let task = self.apply_inner(e);
        // El formulario pertenece a la página donde se abrió: cualquier cambio de estado
        // (rueda, punto, CLI --toggle/--page, cierre) lo descarta.
        if self.state != before {
            self.ctrl_held = false;
            self.drop_hover = false;
            self.form = None;
            self.dev_confirm = None;
            self.dev_picker = None;
            self.todo_draft.clear();
            self.last_deleted = None;
            if self.state == (IslandState::Expanded { page: Page::Shelf }) {
                self.prune_shelf();
            }
        }
        // Primera apertura de la isla: el calendario arranca en hoy.
        if !self.cal_inited && matches!(self.state, IslandState::Expanded { .. }) {
            self.cal_inited = true;
            let today = self.now.date_naive();
            self.cal_selected = today;
            self.cal_view = (today.year(), today.month());
        }
        // Al cerrarse la isla, un aviso no fijo en espera empieza a contar.
        let task = if was_collapsed || self.state != IslandState::Collapsed {
            task
        } else {
            Task::batch([task, self.notice_timer()])
        };
        if self.state != (IslandState::Expanded { page: Page::Musica }) || !was_music {
            // Fuera de Música (o al reabrirla) no puede quedar un arrastre pendiente.
            self.vol_dragging = false;
            self.vol_pending = false;
        }
        if !was_music && self.state == (IslandState::Expanded { page: Page::Musica }) {
            Task::batch([task, App::read_volume(self.vol_gen)])
        } else {
            task
        }
    }

    /// Los títulos solo se publican con la isla abierta en Escritorios.
    fn sync_titles_wanted(&mut self) {
        use std::sync::atomic::Ordering::Relaxed;
        let want = self.state
            == (IslandState::Expanded {
                page: Page::Escritorios,
            });
        if desktops::TITLES_WANTED.swap(want, Relaxed) != want && want {
            desktops::send(DesktopsCmd::Refresh);
        }
    }

    /// Aplica un evento; si el estado cambia, ajusta tamaño, zona de entrada y teclado.
    fn apply_inner(&mut self, e: Event) -> Task<Message> {
        // Salir del panel por otra vía (isla que se cierra, otra página) resuelve el borrador.
        let leaves = self.settings.is_some()
            && self.state != IslandState::Collapsed
            && (matches!(e, Event::OpenPage(_))
                || self.state.on(e, &self.config.pages, self.last_page) == IslandState::Collapsed);
        let commit = if leaves {
            let from = self.settings_section;
            self.settings = None;
            self.dev_picker = None;
            let c = self.commit_host_draft();
            Task::batch([c, self.leave_section(from)])
        } else {
            Task::none()
        };
        Task::batch([commit, self.apply_inner2(e)])
    }

    fn apply_inner2(&mut self, e: Event) -> Task<Message> {
        let old_size = self.target_size(&self.state);
        let was_collapsed = self.state == IslandState::Collapsed;
        if self.settings.is_some() && !was_collapsed {
            match e {
                // La rueda no cambia de página debajo del panel.
                Event::Scroll(_) => return Task::none(),
                // Abrir una página (p. ej. desde un aviso) cierra el panel.
                Event::OpenPage(_) => self.settings = None,
                _ => {}
            }
        }
        let next = self.state.on(e, &self.config.pages, self.last_page);
        if was_collapsed && next != IslandState::Collapsed {
            self.settings = None;
        }
        if let IslandState::Expanded { page } = next {
            self.last_page = page;
        }
        let keyboard_changed = next.keyboard() != self.state.keyboard();
        let size_changed = self.target_size(&next) != old_size;
        self.state = next;
        self.sync_titles_wanted();
        if !keyboard_changed && !size_changed {
            return Task::none();
        }
        if !(self.config.animate && size_changed && self.surface_made) {
            return self.reshape();
        }
        // El teclado cambia al inicio (abrir: OnDemand, cerrar: None); el tamaño se anima.
        self.closing = self.state == IslandState::Collapsed;
        self.anim = Some(SizeAnim::new(self.visible, self.target_size(&self.state)));
        self.anim_last = Instant::now();
        set_keyboard_interactivity(self.pill_id, interactivity(self.state.keyboard()))
    }

    /// Lleva la superficie al tamaño objetivo actual (animado si procede).
    fn retarget(&mut self) -> Task<Message> {
        let target = self.target_size(&self.state);
        if !self.surface_made || target == self.visible && self.anim.is_none() {
            return Task::none();
        }
        if self.config.animate {
            self.anim = Some(SizeAnim::new(self.visible, target));
            self.anim_last = Instant::now();
            Task::none()
        } else {
            self.reshape()
        }
    }

    /// Reinicia la cuenta del aviso mostrado; si es no fijo y la isla está cerrada,
    /// programa su cierre automático.
    fn notice_timer(&mut self) -> Task<Message> {
        self.notice_gen += 1;
        let gen_ = self.notice_gen;
        match self.notices.current() {
            Some(n) if !n.sticky && self.state == IslandState::Collapsed && !self.hud_active() => {
                let secs = n.kind.secs();
                self.notice_deadline = Some(Instant::now() + Duration::from_secs(secs));
                if self.pill_dragging() {
                    // Durante el arrastre la cuenta espera; al soltar corre completa.
                    self.notice_paused = Some(Instant::now());
                    return Task::none();
                }
                cosmic::task::future(async move {
                    tokio::time::sleep(Duration::from_secs(secs)).await;
                    Message::NoticeTimeout(gen_)
                })
            }
            _ => {
                self.notice_deadline = None;
                Task::none()
            }
        }
    }

    /// Encola avisos del sistema (respeta `notify_*`; sonido solo en batería crítica).
    fn push_sys(&mut self, list: Vec<SysNotice>) -> Task<Message> {
        let was_empty = self.notices.current().is_none();
        let mut pushed = false;
        for n in list {
            let enabled = match n {
                SysNotice::BatteryLow { .. }
                | SysNotice::BatteryCritical { .. }
                | SysNotice::BatteryFull => self.config.notify_battery,
                SysNotice::WifiConnected(_)
                | SysNotice::WifiDisconnected
                | SysNotice::NoInternet { .. } => self.config.notify_wifi,
                SysNotice::BtConnected { .. } | SysNotice::BtDisconnected { .. } => {
                    self.config.notify_bluetooth
                }
            };
            if debug() {
                eprintln!("morph: aviso del sistema {n:?} (activo: {enabled})");
            }
            if !enabled {
                continue;
            }
            if matches!(n, SysNotice::BatteryCritical { .. })
                && self.config.sounds
                && !self.notices.has_alarm()
            {
                sound::play(SoundKind::Reminder);
            }
            self.notices.push(notice::notice_from(&n));
            pushed = true;
        }
        if pushed && was_empty && self.notices.current().is_some() {
            self.notice_changed()
        } else {
            Task::none()
        }
    }

    /// Arma el temporizador de un solo uso hasta el próximo vencimiento de las reglas de Wi-Fi
    /// (solo si hay algo pendiente y no está ya armado para ese instante).
    fn arm_wifi(&mut self) -> Task<Message> {
        let Some(at) = self.sys_wifi.deadline() else {
            return Task::none();
        };
        if self.wifi_armed == Some(at) {
            return Task::none();
        }
        self.wifi_armed = Some(at);
        self.wifi_gen += 1;
        let gen_ = self.wifi_gen;
        let wait = at.saturating_duration_since(Instant::now());
        cosmic::task::future(async move {
            tokio::time::sleep(wait).await;
            Message::WifiTick(gen_)
        })
    }

    /// Cambió el aviso mostrado (cola vacía → con aviso, o tras sacar uno).
    fn notice_changed(&mut self) -> Task<Message> {
        Task::batch([self.retarget(), self.notice_timer(), self.resume_peek()])
    }

    /// Chequeo del planificador: avisos vencidos desde el último chequeo hasta ahora.
    fn run_scheduler(&mut self) {
        // Ventana hasta el minuto que acaba de empezar: un despertar de hasta 2 s
        // antes del límite igualmente lo cubre.
        let real_now = chrono::Local::now().naive_local();
        let now = schedule::floor_to_minute(real_now + chrono::Duration::seconds(2));
        let from = self.agenda.last_check.unwrap_or(now);
        let dues = schedule::due(&self.agenda, from, now);
        let (ring, missed) = schedule::classify_missed(dues, now);
        for d in ring {
            if debug() {
                eprintln!("morph: vence {:?} id={} a las {}", d.kind, d.id, d.at);
            }
            if let Some(item) = self.agenda.items.iter().find(|i| i.id == d.id) {
                self.notices
                    .push(notice::notice_for(item, &d, self.config.clock_12h));
            }
            schedule::on_fired(&mut self.agenda, &d);
            if self.config.sounds {
                sound::play(match d.kind {
                    DueKind::Alarm => SoundKind::Alarm,
                    DueKind::TaskStart | DueKind::TaskEnd => SoundKind::Reminder,
                });
            }
        }
        for d in missed.alarms.iter().chain(&missed.tasks) {
            schedule::on_fired(&mut self.agenda, d);
        }
        if let Some(n) = notice::missed_notice(&missed) {
            if debug() {
                eprintln!(
                    "morph: resumen de perdidas: {} alarmas, {} tareas",
                    missed.alarms.len(),
                    missed.tasks.len()
                );
            }
            self.notices.push(n);
        }
        self.agenda.last_check = Some(now);
        self.save_agenda();
    }

    fn form_edit(&mut self, f: impl FnOnce(&mut Form)) {
        if let Some(form) = &mut self.form {
            f(form);
            form.error = None;
        }
    }

    fn form_save(&mut self) {
        let today = self.now.date_naive();
        let Some(form) = &mut self.form else { return };
        match form.validate(today) {
            Err(e) => form.error = Some(e),
            Ok((title, mut kind)) => {
                let editing = form.editing;
                self.form = None;
                match editing {
                    Some(id) => {
                        // Editar no cambia si está hecha / activada.
                        if let Some(old) = self.agenda.items.iter().find(|i| i.id == id) {
                            match (&old.kind, &mut kind) {
                                (Kind::Task { done: o, .. }, Kind::Task { done, .. }) => *done = *o,
                                (
                                    Kind::Alarm {
                                        enabled: o,
                                        time: ot,
                                        once_on: od,
                                        ..
                                    },
                                    Kind::Alarm {
                                        enabled,
                                        time,
                                        once_on,
                                        days,
                                    },
                                ) => {
                                    *enabled = *o;
                                    // Una alarma de una vez ya disparada se reactiva al reprogramarla.
                                    if !days.iter().any(|&d| d) && (ot != time || od != once_on) {
                                        *enabled = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.agenda.update(id, title, kind);
                        // Los avisos pendientes pueden ser de la versión anterior.
                        self.notices.remove_item(id);
                    }
                    None => {
                        self.agenda.add(title, kind);
                    }
                }
                self.save_agenda();
            }
        }
    }

    /// Aviso breve (no fijo): se muestra al colapsar la isla.
    fn info_notice(&mut self, title: String) {
        self.notices.push(Notice {
            kind: NoticeKind::Info,
            title,
            subtitle: String::new(),
            sticky: false,
        });
    }

    fn save_agenda(&mut self) {
        if self.save_blocked {
            return;
        }
        match store::save(&self.data_dir, &self.agenda) {
            Ok(()) => self.save_failed = false,
            Err(e) => {
                eprintln!("morph: no se pudo guardar la agenda: {e}");
                if !self.save_failed {
                    self.save_failed = true;
                    self.notices.push(Notice {
                        kind: NoticeKind::Info,
                        title: crate::fl!("cal-save-failed"),
                        subtitle: e,
                        sticky: true,
                    });
                }
            }
        }
    }

    /// Quita del Shelf lo que ya no existe en disco y guarda si cambió algo.
    fn prune_shelf(&mut self) -> bool {
        if self.shelf.prune_missing() == 0 {
            return false;
        }
        let shelf = &self.shelf;
        self.thumbs
            .retain(|p, _| shelf.entries.iter().any(|e| &e.path == p));
        self.save_shelf();
        true
    }

    /// Poda y, si quitó algo con la isla cerrada, reajusta la pastilla (cápsula 📎 N).
    fn prune_and_retarget(&mut self) -> Task<Message> {
        if self.prune_shelf() && self.state == IslandState::Collapsed {
            self.retarget()
        } else {
            Task::none()
        }
    }

    fn save_shelf(&mut self) {
        if self.shelf_save_blocked {
            return;
        }
        match crate::shelf::save(&self.data_dir, &self.shelf) {
            Ok(()) => self.shelf_save_failed = false,
            Err(e) => {
                eprintln!("morph: no se pudo guardar el Shelf: {e}");
                if !self.shelf_save_failed {
                    self.shelf_save_failed = true;
                    self.notices.push(Notice {
                        kind: NoticeKind::Info,
                        title: crate::fl!("shelf-save-failed"),
                        subtitle: e,
                        sticky: true,
                    });
                }
            }
        }
    }

    /// Lanza (fuera del hilo de la interfaz) las miniaturas que falten o estén viejas.
    fn request_thumbs(&mut self) -> Task<Message> {
        let mut tasks = Vec::new();
        for e in &self.shelf.entries {
            if !e.info.thumbnailable || self.thumb_pending.contains(&e.path) {
                continue;
            }
            if self
                .thumbs
                .get(&e.path)
                .is_some_and(|(m, _)| *m == e.info.mtime)
            {
                continue;
            }
            self.thumb_pending.insert(e.path.clone());
            let (path, mtime) = (e.path.clone(), e.info.mtime);
            tasks.push(cosmic::task::future(async move {
                let p = path.clone();
                let rgba =
                    tokio::task::spawn_blocking(move || crate::shelf::thumb::load(&p, mtime))
                        .await
                        .ok()
                        .flatten();
                Message::ThumbReady(path, mtime, rgba)
            }));
        }
        Task::batch(tasks)
    }

    /// El puntero (o un arrastre entrante) dejó la isla: cierra salvo arrastre propio,
    /// Ajustes o formulario abiertos o borrador en el To-do.
    fn pointer_left_by_drag(&mut self) -> Task<Message> {
        if self.dragging {
            self.left_while_dragging = true;
            return Task::none();
        }
        if settings::keep_open_on_leave(
            self.settings_open(),
            self.form.is_some(),
            !self.todo_draft.trim().is_empty(),
        ) {
            return Task::none();
        }
        self.apply(Event::PointerLeft)
    }

    /// Cápsula del Shelf en la pastilla: cerrada, sin aviso ni música y con elementos.
    fn show_badge(&self) -> bool {
        self.config.shelf_badge
            && !self.shelf.entries.is_empty()
            && self.notices.current().is_none()
            && !self.playing()
    }

    /// Chequeo + reacción en la interfaz si apareció el primer aviso.
    fn scheduler_tick(&mut self) -> Task<Message> {
        let before = self.notices.current().cloned();
        self.run_scheduler();
        if self.notices.current().is_some() && before.as_ref() != self.notices.current() {
            self.notice_changed()
        } else {
            Task::none()
        }
    }

    fn on_notice_action(&mut self, a: NoticeAction) -> Task<Message> {
        let Some(n) = self.notices.pop() else {
            return Task::none();
        };
        if n.kind.owns_sound() {
            sound::stop();
        }
        match a {
            NoticeAction::Dismiss => {}
            NoticeAction::Snooze(m) => {
                if let Some(id) = n.kind.item_id() {
                    schedule::snooze(&mut self.agenda, id, m, chrono::Local::now().naive_local());
                    self.save_agenda();
                }
            }
            NoticeAction::View => {
                if let NoticeKind::Ci { url, .. } = &n.kind
                    && is_https_url(url)
                {
                    open_url(url);
                }
            }
            NoticeAction::Open if matches!(n.kind, NoticeKind::Sys { .. }) => {}
            NoticeAction::Open => {
                let page = n.kind.target_page();
                let t = self.apply_and_revive(Event::OpenPage(page));
                return Task::batch([t, self.notice_timer()]);
            }
        }
        self.notice_changed()
    }

    /// Nueva reproducción: pide letra y carátula si cambiaron y ajusta la pastilla.
    fn on_media(&mut self, new: Option<NowPlaying>) -> Task<Message> {
        let old_key = self.np.as_ref().map(NowPlaying::track_key);
        let old_art = self.np.as_ref().and_then(|n| n.art.clone());
        let was_playing = self.playing();
        let new_key = new.as_ref().map(NowPlaying::track_key);
        let new_art = new.as_ref().and_then(|n| n.art.clone());
        self.np = new;
        let mut tasks = Vec::new();
        if new_key != old_key {
            self.lyrics = match &new_key {
                None => LyricsState::Idle,
                Some(k) if k.is_searchable() => {
                    let fetch_key = k.clone();
                    tasks.push(cosmic::task::future(async move {
                        Message::LyricsLoaded(lrclib::fetch(fetch_key).await)
                    }));
                    LyricsState::Loading(k.clone())
                }
                Some(k) => LyricsState::Ready(k.clone(), Lyrics::NotFound),
            };
        }
        if new_art != old_art {
            self.art = None;
            if let Some(src) = new_art {
                tasks.push(cosmic::task::future(async move {
                    let h = art::load(src.clone()).await;
                    Message::ArtLoaded(src, h)
                }));
            }
        }
        if was_playing != self.playing() && self.state == IslandState::Collapsed {
            tasks.push(self.retarget());
        }
        Task::batch(tasks)
    }

    /// Un fotograma: muestrea y aplica el tamaño; al terminar retira la animación.
    fn anim_frame(&mut self) -> Task<Message> {
        let now = Instant::now();
        let dt = now.saturating_duration_since(self.anim_last);
        self.anim_last = now;
        let Some(anim) = self.anim.as_mut() else {
            return Task::none();
        };
        // `advance` limita dt: un atasco pausa la animación en lugar de saltarla.
        let (size, done) = anim.advance(dt);
        if done {
            self.anim = None;
            self.closing = false;
        }
        if !self.surface_made || size == self.visible {
            return Task::none();
        }
        self.set_visible(size)
    }

    /// Abre la isla (si estaba cerrada) directamente en el panel de Ajustes.
    fn open_settings_cli(&mut self) -> Task<Message> {
        let mut tasks = vec![self.apply(Event::Click)];
        tasks.push(self.settings_update(SettingsMsg::Open));
        if !self.surface_made {
            tasks.push(self.create_surface());
        }
        Task::batch(tasks)
    }

    /// Como `apply`, y si la superficie se perdió la recrea (la isla vuelve con `morph --toggle`).
    fn apply_and_revive(&mut self, e: Event) -> Task<Message> {
        let t = self.apply(e);
        if self.surface_made {
            t
        } else {
            Task::batch([t, self.create_surface()])
        }
    }

    fn scale(&self) -> u16 {
        effective_scale(self.config.size_percent)
    }

    /// Reaplica superficie (solo si cambió la escala), caja visible, zona de entrada y
    /// teclado según el estado y la escala actuales. También cancela la animación en curso.
    fn reshape(&mut self) -> Task<Message> {
        self.anim = None;
        self.closing = false;
        if !self.surface_made {
            return Task::none();
        }
        let size = self.target_size(&self.state);
        let mode = interactivity(self.state.keyboard());
        let surface = geometry::surface_size(self.scale());
        let mut tasks = Vec::new();
        if surface != self.surface {
            self.surface = surface;
            tasks.push(set_size(self.pill_id, None, Some(surface.h)));
        }
        tasks.push(self.set_visible(size));
        tasks.push(set_keyboard_interactivity(self.pill_id, mode));
        Task::batch(tasks)
    }

    /// Pide crear la superficie tras una breve espera para que los monitores se asienten.
    fn realize_later() -> Task<Message> {
        cosmic::task::future(async {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            Message::Realize
        })
    }

    fn output_names(&self) -> Vec<String> {
        self.outputs.iter().map(|(_, n)| n.clone()).collect()
    }

    /// Crea la superficie en el monitor resuelto (o el activo) con un id nuevo.
    fn create_surface(&mut self) -> Task<Message> {
        let name = resolve_output(self.config.output.as_deref(), &self.output_names());
        let output = name
            .as_ref()
            .and_then(|n| self.outputs.iter().find(|(_, o)| o == n))
            .map(|(wl, _)| IcedOutput::Output(wl.clone()))
            .unwrap_or(IcedOutput::Active);
        let id = window::Id::unique();
        self.pill_id = id;
        self.surface_output = name;
        self.surface_made = true;
        self.pointer_inside = false;
        self.clear_peek();
        let visible = self.target_size(&self.state);
        let size = geometry::surface_size(self.scale());
        self.surface = size;
        self.visible = visible;
        self.closing = false;
        self.anim = None;
        self.surface_w = None;
        self.surface_h = None;
        self.drag_pos = None;
        self.drag_last_raw_y = None;
        self.pill_drag = None;
        self.hover = None;
        self.notice_paused = None;
        let zone = self.input_zone();
        let anchor = self.anchor();
        let exclusive_zone = self.exclusive_zone();
        let keyboard = interactivity(self.state.keyboard());
        cosmic::surface::surface_task(simple_layer_shell(
            LiveSettings::default,
            move || SctkLayerSurfaceSettings {
                id,
                layer: Layer::Overlay,
                keyboard_interactivity: keyboard,
                input_zone: Some(zone.clone()),
                // Todo el ancho del monitor (ancho pedido 0), pegada al borde elegido; la zona
                // de entrada es solo la caja.
                anchor,
                exclusive_zone,
                namespace: "morph".into(),
                output: output.clone(),
                size: Some((None, Some(size.h))),
                size_limits: Limits::NONE,
                ..Default::default()
            },
            None::<fn() -> Element<'static, cosmic::Action<Message>>>,
        ))
    }

    /// Si el monitor deseado cambió respecto al de la superficie, la recrea allí.
    fn reconcile_output(&mut self) -> Task<Message> {
        if !self.surface_made {
            return Task::none();
        }
        let desired = resolve_output(self.config.output.as_deref(), &self.output_names());
        if desired == self.surface_output {
            return Task::none();
        }
        let old = self.pill_id;
        Task::batch([destroy_layer_surface(old), self.create_surface()])
    }

    /// Reacciona a que `self.config` cambió respecto de `old` (ya aplicado en memoria).
    fn config_applied(&mut self, old: &MorphConfig) -> Task<Message> {
        if old.language != self.config.language {
            crate::i18n::apply(self.config.language);
        }
        if old.clock_12h != self.config.clock_12h {
            let on = self.config.clock_12h;
            if let Some(f) = &mut self.form {
                f.set_h12(on);
            }
        }
        let before = self.pill_id;
        let mut tasks = vec![self.reconcile_output()];
        // Si se recreó la superficie ya nace con el tamaño correcto.
        if self.pill_id == before {
            if old.pill_pos != self.config.pill_pos && self.surface_made {
                tasks.push(self.sync_place());
            }
            if self.surface_made {
                if old.pill_edge != self.config.pill_edge {
                    tasks.push(set_anchor(self.pill_id, self.anchor()));
                    tasks.push(self.sync_place());
                }
                if old.panel_level != self.config.panel_level {
                    tasks.push(set_exclusive_zone(self.pill_id, self.exclusive_zone()));
                }
            }
            if old.hide_line != self.config.hide_line {
                // Con el puntero encima, la pastilla se queda mostrada; si no, vuelve a la línea.
                self.line_hover = self.config.hide_line && self.pointer_inside;
            }
            if old.size_percent != self.config.size_percent {
                tasks.push(self.reshape());
            } else if (old.minimal != self.config.minimal
                || old.shelf_badge != self.config.shelf_badge
                || old.pill_battery != self.config.pill_battery
                || old.pill_volume != self.config.pill_volume
                || old.hide_line != self.config.hide_line
                || old.hud_style != self.config.hud_style)
                && self.state == IslandState::Collapsed
            {
                tasks.push(self.retarget());
            }
        }
        // El selector de Ajustes refleja siempre lo guardado.
        if let Some(p) = &mut self.dev_picker {
            p.selected = self.config.github_repos.clone();
            p.gl_selected = self.config.gitlab_projects.clone();
        }
        // Al activar la página Dev empiezan las lecturas (y el token de GitLab, si hay proyectos).
        let dev_was = old.pages.contains(&Page::Dev);
        if !dev_was && self.dev_enabled() {
            tasks.push(self.gh_reset());
            tasks.push(self.gl_reset(true));
        }
        // Con Ajustes abierto las lecturas se aplazan (varias casillas seguidas = una lectura).
        if old.github_repos != self.config.github_repos {
            if self.settings_open() {
                tasks.push(self.schedule_poll(true, false));
            } else {
                tasks.push(self.gh_reset());
            }
        }
        let host_changed = old.gitlab_host != self.config.gitlab_host;
        if host_changed {
            // Los clones locales dependen del servidor: se vuelven a buscar.
            self.local_gl_scan = None;
            self.local_gl_busy = false;
            tasks.push(self.gl_reset(false));
            // Otro servidor: el selector de GitLab se vuelve a cargar (token y proyectos de ese host).
            if self.settings_open() && self.settings == Some(Section::GitLab) {
                tasks.push(self.gl_picker_open());
            }
        } else if old.gitlab_projects != self.config.gitlab_projects {
            if self.settings_open() {
                tasks.push(self.schedule_poll(false, true));
            } else {
                tasks.push(self.gl_reset(true));
            }
        }
        tasks.push(self.term_config_changed(old));
        Task::batch(tasks)
    }

    /// Aplaza la lectura de GitHub/GitLab ~1,5 s; otro cambio antes reinicia la espera.
    fn schedule_poll(&mut self, gh: bool, gl: bool) -> Task<Message> {
        self.pending_gh |= gh;
        self.pending_gl |= gl;
        self.poll_gen += 1;
        let g = self.poll_gen;
        Task::perform(
            async {
                tokio::time::sleep(Duration::from_millis(1500)).await;
            },
            move |()| cosmic::Action::App(Message::PollDebounce(g)),
        )
    }

    /// Lanza ya las lecturas aplazadas; devuelve (tarea, GitHub lanzada, GitLab lanzada).
    fn flush_polls(&mut self) -> (Task<Message>, bool, bool) {
        let (gh, gl) = (self.pending_gh, self.pending_gl);
        self.pending_gh = false;
        self.pending_gl = false;
        let mut tasks = Vec::new();
        if gh {
            tasks.push(self.gh_reset());
        }
        if gl {
            tasks.push(self.gl_reset(true));
        }
        (Task::batch(tasks), gh, gl)
    }

    /// Se sale de la sección `from` (o del panel): lanza lo aplazado y, si no hubo cambios,
    /// reintenta los «sin acceso» como hacía «Listo».
    fn leave_section(&mut self, from: Section) -> Task<Message> {
        let (t, gh, gl) = self.flush_polls();
        let mut tasks = vec![t];
        if from == Section::GitHub && !gh {
            crate::dev::accounts::drop_no_access(&mut self.gh_state.access);
            tasks.push(self.gh_refresh());
        }
        if from == Section::GitLab && !gl {
            tasks.push(self.gl_refresh());
        }
        Task::batch(tasks)
    }

    /// Antes de guardar o crear un token: resuelve el borrador del servidor. `false` si el
    /// borrador no es válido (se queda el error en pantalla y no se hace nada).
    fn commit_host_for_token(&mut self) -> (bool, Task<Message>) {
        let Some(draft) = self.settings_host_draft.clone() else {
            return (true, Task::none());
        };
        if matches!(
            settings::decide_host(&draft, &self.config.gitlab_host, false),
            settings::HostDecision::KeepInvalid
        ) {
            return (false, Task::none());
        }
        (true, self.commit_host_draft_with(false))
    }

    fn settings_open(&self) -> bool {
        self.settings.is_some() && matches!(self.state, IslandState::Expanded { .. })
    }

    /// Cierra el panel y vuelve a la página (la primera activa si la actual se ocultó).
    fn close_settings(&mut self) -> Task<Message> {
        let from = self.settings_section;
        // Panel cerrado antes de guardar el borrador: el servidor nuevo no recarga el selector.
        self.settings = None;
        self.dev_picker = None;
        let commit = self.commit_host_draft();
        let leave = self.leave_section(from);
        if let IslandState::Expanded { page } = self.state {
            let page = settings::effective_page(&self.config.pages, page);
            self.state = IslandState::Expanded { page };
            self.last_page = page;
        }
        self.sync_titles_wanted();
        Task::batch([commit, leave, self.retarget()])
    }

    /// Resuelve el borrador del servidor de GitLab. `last`: se sale del panel o de la
    /// sección (el borrador siempre se descarta); si no (Enter), uno no válido se conserva.
    fn commit_host_draft_with(&mut self, last: bool) -> Task<Message> {
        let Some(draft) = self.settings_host_draft.clone() else {
            return Task::none();
        };
        match settings::decide_host(&draft, &self.config.gitlab_host, last) {
            settings::HostDecision::Save(h) => {
                self.settings_host_draft = None;
                self.apply_change(Change::GitlabHost(h))
            }
            settings::HostDecision::Drop => {
                self.settings_host_draft = None;
                Task::none()
            }
            settings::HostDecision::KeepInvalid => Task::none(),
        }
    }

    fn commit_host_draft(&mut self) -> Task<Message> {
        self.commit_host_draft_with(true)
    }

    fn settings_update(&mut self, m: SettingsMsg) -> Task<Message> {
        match m {
            SettingsMsg::Open => {
                // Abrir Ajustes nunca toca el llavero: si se quedó en GitLab, abre en General.
                let s = match self.settings_section {
                    Section::GitLab => Section::General,
                    s => s,
                };
                return self.open_settings(s);
            }
            SettingsMsg::Close => {
                if self.settings_open() {
                    return self.close_settings();
                }
            }
            SettingsMsg::Section(s) => {
                if let Some(cur) = self.settings
                    && settings::section_switch(cur, s)
                {
                    let leave = self.leave_section(cur);
                    // La sección nueva ya está activa al guardar el borrador del servidor.
                    self.settings = Some(s);
                    self.settings_section = s;
                    let host = self.config.gitlab_host.clone();
                    let t = self.commit_host_draft();
                    // Si el servidor cambió y se entra en GitLab, el guardado ya recargó.
                    let entered = if host != self.config.gitlab_host && s == Section::GitLab {
                        Task::none()
                    } else {
                        self.section_entered(s)
                    };
                    return Task::batch([t, leave, entered]);
                }
            }
            SettingsMsg::Unpin(p) => {
                let pins = settings::unpin(&self.config.pins, &p);
                if pins != self.config.pins {
                    return self.apply_change(Change::Pins(pins));
                }
            }
            SettingsMsg::GitlabHostEdit(t) => self.settings_host_draft = Some(t),
            SettingsMsg::GitlabHostCommit => return self.commit_host_draft_with(false),
            SettingsMsg::Set(c) => return self.apply_change(c),
            SettingsMsg::PageMove(p, d) => {
                let pages = settings::move_page(&self.config.pages, p, d);
                self.save_pages(pages);
            }
            SettingsMsg::PageToggle(p, on) => {
                let pages = settings::toggle_page(&self.config.pages, p, on);
                self.save_pages(pages);
            }
            SettingsMsg::CenterPill => {
                if settings::can_center(self.config.pill_pos) {
                    self.save_pill_pos(settings::PILL_CENTER);
                    return self.sync_place();
                }
            }
            SettingsMsg::Autostart(on) => {
                match settings::set_autostart(&settings::autostart_dir(), on) {
                    Ok(()) => self.autostart = on,
                    Err(e) => {
                        eprintln!("morph: no se pudo cambiar el inicio con la sesión: {e}");
                        self.autostart = settings::autostart_enabled(&settings::autostart_dir());
                    }
                }
            }
        }
        Task::none()
    }

    fn save_pages(&mut self, pages: Vec<Page>) {
        if pages == self.config.pages {
            return;
        }
        match cosmic::cosmic_config::Config::new(APP_ID, 1) {
            Ok(h) => {
                if let Err(e) = self.config.set_pages(&h, pages) {
                    eprintln!("morph: no se pudo guardar pages: {e}");
                }
            }
            Err(e) => {
                eprintln!("morph: no se pudo abrir la config: {e}");
                self.config.pages = pages;
            }
        }
    }

    /// Guarda un cambio del panel (se aplica al momento en memoria; el observador de
    /// la config lo confirma y dispara el resto de reacciones).
    fn apply_change(&mut self, c: Change) -> Task<Message> {
        let handle = cosmic::cosmic_config::Config::new(APP_ID, 1)
            .map_err(|e| eprintln!("morph: no se pudo abrir la config: {e}"))
            .ok();
        let old = self.config.clone();
        macro_rules! save {
            ($set:ident, $field:ident, $v:expr) => {{
                let v = $v;
                match &handle {
                    Some(h) => {
                        // El setter ya actualiza `self.config` antes de escribir.
                        if let Err(e) = self.config.$set(h, v) {
                            eprintln!("morph: no se pudo guardar {}: {e}", stringify!($field));
                        }
                    }
                    None => self.config.$field = v,
                }
            }};
        }
        match c {
            Change::Clock12h(v) => save!(set_clock_12h, clock_12h, v),
            Change::Size(v) => save!(set_size_percent, size_percent, v),
            Change::Output(v) => save!(set_output, output, v),
            Change::OpenOnHover(v) => save!(set_open_on_hover, open_on_hover, v),
            Change::Animate(v) => save!(set_animate, animate, v),
            Change::Sounds(v) => save!(set_sounds, sounds, v),
            Change::Minimal(v) => save!(set_minimal, minimal, v),
            Change::HudStyle(v) => save!(set_hud_style, hud_style, v),
            Change::HudVolume(v) => save!(set_hud_volume, hud_volume, v),
            Change::HudBrightness(v) => save!(set_hud_brightness, hud_brightness, v),
            Change::NotifyBattery(v) => save!(set_notify_battery, notify_battery, v),
            Change::NotifyWifi(v) => save!(set_notify_wifi, notify_wifi, v),
            Change::NotifyBluetooth(v) => save!(set_notify_bluetooth, notify_bluetooth, v),
            Change::NotifyCi(v) => save!(set_notify_ci, notify_ci, v),
            Change::ShelfLayout(v) => save!(set_shelf_layout, shelf_layout, v),
            Change::ShelfBadge(v) => save!(set_shelf_badge, shelf_badge, v),
            Change::ShelfStartEmpty(v) => save!(set_shelf_start_empty, shelf_start_empty, v),
            Change::ShelfDragCopy(v) => save!(set_shelf_drag_copy, shelf_drag_copy, v),
            Change::DesktopsLayout(v) => save!(set_desktops_layout, desktops_layout, v),
            Change::DevLayout(v) => save!(set_dev_layout, dev_layout, v),
            Change::TermSize(v) => save!(set_term_size, term_size, v),
            Change::TermFont(v) => save!(set_term_font_size, term_font_size, v),
            Change::GitlabHost(v) => save!(set_gitlab_host, gitlab_host, v),
            Change::GithubRepos(v) => save!(set_github_repos, github_repos, v),
            Change::GitlabProjects(v) => save!(set_gitlab_projects, gitlab_projects, v),
            Change::Pins(v) => save!(set_pins, pins, v),
            Change::HideLine(v) => save!(set_hide_line, hide_line, v),
            Change::PanelLevel(v) => save!(set_panel_level, panel_level, v),
            Change::PillBattery(v) => save!(set_pill_battery, pill_battery, v),
            Change::PillVolume(v) => save!(set_pill_volume, pill_volume, v),
            Change::Language(v) => save!(set_language, language, v),
        }
        // Lo mismo que hace `Message::Config` al llegar el cambio por el observador.
        self.config_applied(&old)
    }

    /// Página del contenido expandido: la actual, o la última mostrada al cerrar.
    fn shown_page(&self) -> Page {
        match self.state {
            IslandState::Expanded { page } => page,
            IslandState::Collapsed => self.last_page,
        }
    }

    fn expanded_view(&self, page: Page) -> Element<'_, Message> {
        if let Some(section) = self.settings {
            let outputs = self.output_names();
            return crate::settings::view::view(&crate::settings::view::Ctx {
                section,
                config: &self.config,
                outputs: &outputs,
                autostart: self.autostart,
                host_draft: self.settings_host_draft.as_deref(),
                apps: &self.apps,
                picker: self.dev_picker.as_ref(),
            });
        }
        let title = widget::text(page.title())
            .size(18)
            .class(cosmic::theme::Text::Color(Color::WHITE));
        let dots = self
            .config
            .pages
            .iter()
            .fold(widget::Row::new().spacing(8), |row, p| {
                let color = if *p == page {
                    Color::WHITE
                } else {
                    Color::from_rgb(0.4, 0.4, 0.4)
                };
                let dot = widget::container(widget::space())
                    .width(Length::Fixed(10.0))
                    .height(Length::Fixed(10.0))
                    .class(cosmic::theme::Container::custom(move |_| {
                        widget::container::Style {
                            background: Some(Background::Color(color)),
                            border: Border {
                                radius: 5.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        }
                    }));
                row.push(widget::mouse_area(dot).on_press(Message::Island(Event::OpenPage(*p))))
            });
        // Los puntos quedan centrados; el ⚙ a la derecha (oculto con un formulario abierto).
        let gear: Element<'_, Message> = if self.form.is_some() {
            widget::space().into()
        } else {
            gear_button()
        };
        // Altura fija (la del ⚙ de 16 px): no cambia con un formulario abierto.
        let dots_row = widget::Row::new()
            .align_y(Alignment::Center)
            .width(Length::Fill)
            .height(Length::Fixed(16.0))
            .push(widget::space().width(Length::Fill))
            .push(dots)
            .push(
                widget::container(gear)
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            );
        let middle: Element<'_, Message> = if let Some(f) = &self.form {
            form_view::view(f)
        } else if page == Page::Calendario {
            calendar_view::view(
                &self.agenda,
                self.now,
                self.cal_view,
                self.cal_selected,
                self.last_deleted.as_ref().map(|i| i.title.as_str()),
                self.config.clock_12h,
            )
        } else if page == Page::Todo {
            todo_view::view(
                &self.agenda,
                self.now.date_naive(),
                &self.todo_draft,
                self.last_deleted.as_ref().map(|i| i.title.as_str()),
                self.config.clock_12h,
            )
        } else if page == Page::Musica {
            self.music_view()
        } else if page == Page::Shelf {
            crate::shelf::view::view(
                &self.shelf,
                self.config.shelf_layout,
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .as_deref(),
                &self.thumbs,
                &self.drag_paths,
                self.drop_hover,
                (self.config.shelf_drag_copy, self.ctrl_held),
            )
        } else if page == Page::Escritorios {
            crate::desktops::view::view(
                &self.desk_snap,
                self.surface_output.as_deref(),
                self.config.desktops_layout,
                &self.config.pins,
                &self.apps,
            )
        } else if page == Page::Dev {
            let prs = crate::dev::forge::merge_change_requests(
                self.gh_prs
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .chain(self.gl_mrs.iter().cloned())
                    .collect(),
            );
            let runs = crate::dev::forge::merge_pipelines(
                self.gh_runs
                    .iter()
                    .cloned()
                    .map(Into::into)
                    .chain(self.gl_pipelines.iter().cloned())
                    .collect(),
            );
            let gl_msg = self
                .gl_error
                .as_ref()
                .map(|e| e.message(self.gl_last_ok.map(|t| t.elapsed().as_secs() / 60)));
            crate::dev::view::view(&crate::dev::view::DevCtx {
                listeners: &self.dev_ports,
                home: std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .as_deref()
                    .unwrap_or(std::path::Path::new("/")),
                confirm: self.dev_confirm,
                layout: self.config.dev_layout,
                tab: self.dev_tab,
                repos: self.config.github_repos.len() + self.config.gitlab_projects.len(),
                no_access: self.gh_no_access + self.gl_no_access,
                prs: &prs,
                runs: &runs,
                error: self
                    .gh_error
                    .as_ref()
                    .map(|e| e.message(self.gh_own_age().map(|s| s / 60))),
                gl_error: gl_msg,
                age_secs: self.gh_data_age(),
                paused: self.gh_pause_until.is_some_and(|t| Instant::now() < t)
                    || self.gl_pause_until.is_some_and(|t| Instant::now() < t),
            })
        } else {
            widget::space().height(Length::Fill).into()
        };
        let mut col = widget::Column::new();
        if page != Page::Musica
            && page != Page::Calendario
            && page != Page::Todo
            && page != Page::Shelf
            && page != Page::Dev
            && self.form.is_none()
        {
            col = col.push(title);
        }
        // Todas las páginas llenan la altura: los puntos quedan siempre abajo.
        col.push(
            widget::container(middle)
                .width(Length::Fill)
                .height(Length::Fill)
                .clip(true),
        )
        // Calendario, To-do y Shelf: el último control queda a ≥ 12 px de los puntos.
        .push(widget::space().height(Length::Fixed(
            if page == Page::Calendario
                || page == Page::Todo
                || page == Page::Shelf
                || page == Page::Dev
            {
                12.0
            } else {
                0.0
            },
        )))
        .push(dots_row)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .padding(16)
        .into()
    }

    /// Pastilla cerrada: batería a la izquierda y volumen a la derecha (si están activos),
    /// con lo de siempre en el centro.
    fn collapsed_view(&self) -> Element<'_, Message> {
        let (bat, vol) = self.pill_extras();
        let center = self.collapsed_center();
        if !bat && !vol {
            return center;
        }
        let scale = self.scale();
        fn side<'a>(
            item: Option<Element<'a, Message>>,
            left: bool,
            scale: u16,
        ) -> Element<'a, Message> {
            let w = if item.is_some() {
                peek::px(geometry::PEEK_ITEM + geometry::EXTRA_MARGIN, scale)
            } else {
                0.0
            };
            // Margen interior frente a la esquina redondeada, por el lado de fuera.
            let m = peek::px(geometry::EXTRA_MARGIN, scale);
            let pad = if left {
                cosmic::iced::Padding::new(0.0).left(m)
            } else {
                cosmic::iced::Padding::new(0.0).right(m)
            };
            widget::container(item.unwrap_or_else(|| widget::space().into()))
                .padding(pad)
                .width(Length::Fixed(w))
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .into()
        }
        let left = self.peek_stats.battery.filter(|_| bat).map(|(pct, ch)| {
            peek::item(
                peek::named(&peek::battery_icon_name(pct, ch), &["battery-symbolic"]),
                peek::battery_label(pct),
                false,
                scale,
            )
        });
        let right = vol.then(|| {
            let v = self.peek_stats.volume;
            peek::item(
                peek::named(peek::volume_icon(v), &["audio-volume-medium-symbolic"]),
                peek::volume_label(v),
                v.is_none_or(|v| v.muted),
                scale,
            )
        });
        widget::row::with_children(vec![
            side(left, true, scale),
            widget::container(center)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into(),
            side(right, false, scale),
        ])
        .align_y(Alignment::Center)
        .into()
    }

    /// Aviso durante el arrastre: al soltar la pastilla cambia de borde.
    fn edge_hint(&self) -> Element<'_, Message> {
        let (icon, text) = match self.target_edge() {
            Edge::Top => ("go-up-symbolic", crate::fl!("island-move-up")),
            Edge::Bottom => ("go-down-symbolic", crate::fl!("island-move-down")),
        };
        widget::Row::new()
            .spacing(6)
            .align_y(Alignment::Center)
            .push(widget::icon::from_name(icon).size(14).symbolic(true))
            .push(label(text, 14, Color::WHITE))
            .into()
    }

    /// Centro de la pastilla cerrada: hora, música o contador del Shelf.
    fn collapsed_center(&self) -> Element<'_, Message> {
        let playing = self.np.is_some() && self.playing();
        let text = if playing || self.config.minimal {
            clock::format_short(self.now, self.config.clock_12h)
        } else {
            clock::format_pill(self.now, self.config.clock_12h)
        };
        let time = widget::text(text)
            .size(14)
            .class(cosmic::theme::Text::Color(Color::WHITE));
        let Some(np) = self.np.as_ref().filter(|_| self.playing()) else {
            return match self
                .show_badge()
                .then(|| crate::shelf::view::badge(self.shelf.entries.len()))
                .flatten()
            {
                Some(b) => widget::Row::new()
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .push(time)
                    .push(b)
                    .into(),
                None => time.into(),
            };
        };
        let cover: Element<'_, Message> = match &self.art {
            Some(h) => widget::image(h.clone())
                .width(Length::Fixed(18.0))
                .height(Length::Fixed(18.0))
                .content_fit(cosmic::iced::ContentFit::Cover)
                .into(),
            None => widget::icon::from_name("audio-x-generic-symbolic")
                .size(18)
                .symbolic(true)
                .into(),
        };
        const PHASES: [[f32; 3]; 4] = [
            [4.0, 10.0, 6.0],
            [8.0, 5.0, 10.0],
            [10.0, 8.0, 4.0],
            [6.0, 10.0, 8.0],
        ];
        let bars = PHASES[self.bars_phase % PHASES.len()].iter().fold(
            widget::Row::new().spacing(2).align_y(Alignment::Center),
            |row, h| {
                row.push(
                    widget::container(widget::space())
                        .width(Length::Fixed(3.0))
                        .height(Length::Fixed(*h))
                        .class(cosmic::theme::Container::custom(|_| {
                            widget::container::Style {
                                background: Some(Background::Color(Color::WHITE)),
                                ..Default::default()
                            }
                        })),
                )
            },
        );
        widget::Row::new()
            .spacing(6)
            .align_y(Alignment::Center)
            .padding([0, 10])
            .push(cover)
            .push(
                widget::container(label(truncate(&np.display_title(), 14), 11, Color::WHITE))
                    .width(Length::Fill)
                    .clip(true),
            )
            .push(bars)
            .push(time)
            .into()
    }

    /// Página Música (diseño "Columna llena"): columna izquierda con carátula,
    /// controles y volumen; columna derecha con datos, letra y progreso.
    fn music_view(&self) -> Element<'_, Message> {
        let Some(np) = self.np.as_ref() else {
            return widget::container(label(crate::fl!("music-idle"), 14, GRAY))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .align_x(Alignment::Center)
                .into();
        };
        let cover: Element<'_, Message> = match &self.art {
            Some(h) => widget::image(h.clone())
                .width(Length::Fixed(150.0))
                .height(Length::Fixed(150.0))
                .content_fit(cosmic::iced::ContentFit::Cover)
                .border_radius(10.0)
                .into(),
            None => widget::container(
                widget::icon::from_name("audio-x-generic-symbolic")
                    .size(64)
                    .symbolic(true),
            )
            .width(Length::Fixed(150.0))
            .height(Length::Fixed(150.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .class(cosmic::theme::Container::custom(|_| {
                widget::container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.17, 0.17, 0.17))),
                    border: Border {
                        radius: 10.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }))
            .into(),
        };
        let playing = np.status == Status::Playing;
        let play_icon = if playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };
        let play = widget::button::custom(
            widget::container(
                widget::icon::from_name(play_icon)
                    .size(18)
                    .symbolic(true)
                    .icon()
                    .class(cosmic::theme::Svg::custom(|_| {
                        cosmic::iced::widget::svg::Style {
                            color: Some(Color::BLACK),
                        }
                    })),
            )
            .width(Length::Fixed(34.0))
            .height(Length::Fixed(34.0))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .class(cosmic::theme::Container::custom(|_| {
                widget::container::Style {
                    background: Some(Background::Color(Color::WHITE)),
                    border: Border {
                        radius: 17.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            })),
        )
        .padding(0)
        .on_press(Message::MediaCmd(MediaCmd::PlayPause));
        let controls = widget::Row::new()
            .spacing(14)
            .align_y(Alignment::Center)
            .push(icon_button(
                "media-skip-backward-symbolic",
                Message::MediaCmd(MediaCmd::Previous),
            ))
            .push(play)
            .push(icon_button(
                "media-skip-forward-symbolic",
                Message::MediaCmd(MediaCmd::Next),
            ));
        let vol_icon = if self.muted || self.volume <= 0.0 {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        let volume = widget::Row::new()
            .width(Length::Fixed(150.0))
            .spacing(8)
            .align_y(Alignment::Center)
            .push(widget::icon::from_name(vol_icon).size(18).symbolic(true))
            .push(
                widget::slider(
                    0.0f32..=100.0,
                    (self.volume.min(1.0) * 100.0).round(),
                    Message::VolumeChanged,
                )
                .step(1.0_f32)
                .on_release(Message::VolumeReleased)
                .width(Length::Fill),
            );
        // Equivalente a justify-content: space-between en toda la altura.
        let left = widget::Column::new()
            .width(Length::Fixed(160.0))
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .push(cover)
            .push(widget::space().height(Length::Fill))
            .push(controls)
            .push(widget::space().height(Length::Fill))
            .push(volume);

        let pos = position_at(np, Instant::now());
        let artist = np.artists.join(", ");
        let sub = if np.album.trim().is_empty() {
            artist
        } else {
            format!("{artist} · {}", np.album)
        };
        let mut right = widget::Column::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(2)
            .push(label(truncate(&np.display_title(), 34), 16, Color::WHITE).font(SEMIBOLD))
            .push(label(truncate(&sub, 46), 13, GRAY))
            .push(
                widget::container(self.lyrics_view(pos))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::Center)
                    .clip(true),
            );
        if let Some(len) = np.length_ms.filter(|l| *l > 0) {
            let part = ((pos.min(len) * 1000) / len) as u16;
            let bar = |c: Color| {
                cosmic::theme::Container::custom(move |_| widget::container::Style {
                    background: Some(Background::Color(c)),
                    border: Border {
                        radius: 2.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
            };
            let progress = widget::Row::new()
                .push(
                    widget::container(widget::space())
                        .width(Length::FillPortion(part.max(1)))
                        .height(Length::Fixed(4.0))
                        .class(bar(Color::WHITE)),
                )
                .push(
                    widget::container(widget::space())
                        .width(Length::FillPortion((1000 - part).max(1)))
                        .height(Length::Fixed(4.0))
                        .class(bar(Color::from_rgb(0.3, 0.3, 0.3))),
                );
            let times = widget::Row::new()
                .push(label(fmt_time(pos.min(len)), 11, GRAY))
                .push(widget::space().width(Length::Fill))
                .push(label(fmt_time(len), 11, GRAY));
            right = right.push(progress).push(times);
        }
        widget::Row::new()
            .spacing(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .push(left)
            .push(right)
            .into()
    }

    /// Bloque de letra: línea anterior, actual y siguiente, más el crédito.
    fn lyrics_view(&self, pos: u64) -> Element<'_, Message> {
        const PREV: Color = Color::from_rgb(0.333, 0.333, 0.333);
        const NEXT: Color = Color::from_rgb(0.467, 0.467, 0.467);
        fn nonempty(t: Option<&str>) -> Option<&str> {
            t.filter(|t| !t.trim().is_empty())
        }
        let loading = crate::fl!("music-lyrics-loading");
        let unavailable = crate::fl!("music-lyrics-unavailable");
        let instrumental = crate::fl!("music-lyrics-instrumental");
        let (prev, cur, next, credit): (Option<&str>, Option<&str>, Option<&str>, bool) =
            match &self.lyrics {
                LyricsState::Idle => (None, None, None, false),
                LyricsState::Loading(_) => (None, Some(loading.as_str()), None, false),
                LyricsState::Failed(_) | LyricsState::Ready(_, Lyrics::NotFound) => {
                    (None, Some(unavailable.as_str()), None, false)
                }
                LyricsState::Ready(_, Lyrics::Instrumental) => {
                    (None, Some(instrumental.as_str()), None, false)
                }
                LyricsState::Ready(_, Lyrics::Plain(text)) => {
                    let mut it = text.lines().filter(|l| !l.trim().is_empty());
                    (None, it.next(), it.next(), true)
                }
                LyricsState::Ready(_, Lyrics::Synced(lines)) => {
                    let (p, c, n) = lyric_window(lines, current_line(lines, pos));
                    (
                        nonempty(p),
                        Some(nonempty(c).unwrap_or("♪")),
                        nonempty(n),
                        true,
                    )
                }
            };
        // Siempre tres huecos: la línea actual no se mueve al principio ni al final.
        let slot =
            |t: Option<&str>, size: u16, c: Color| label(truncate(t.unwrap_or(" "), 38), size, c);
        let mut col = widget::Column::new()
            .spacing(3)
            .width(Length::Fill)
            .push(slot(prev, 14, PREV))
            .push(slot(cur, 16, Color::WHITE).font(SEMIBOLD))
            .push(slot(next, 14, NEXT));
        if credit {
            col = col.push(label(crate::fl!("music-lyrics-credit"), 10, GRAY));
        }
        col.into()
    }
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Flags) -> (Self, Task<Message>) {
        let config = cosmic::cosmic_config::Config::new(APP_ID, 1)
            .map(|c| MorphConfig::get_entry(&c).unwrap_or_else(|(_errs, cfg)| cfg))
            .unwrap_or_default();
        crate::i18n::apply(config.language);
        let config_font = config.term_font_size;
        let now_local = chrono::Local::now();
        let now0 = now_local.date_naive();
        let mut app = App {
            core,
            now: now_local,
            state: IslandState::Collapsed,
            config,
            outputs: Vec::new(),
            output_sizes: Vec::new(),
            term: term_ui::TermState::new(config_font),
            pill_id: window::Id::unique(),
            surface_output: None,
            surface_made: false,
            surface: Size { w: 0, h: 0 },
            visible: Size { w: 0, h: 0 },
            closing: false,
            last_page: Page::Musica,
            settings: None,
            settings_section: Section::General,
            settings_host_draft: None,
            autostart: false,
            anim: None,
            anim_last: Instant::now(),
            media_tx: None,
            np: None,
            art: None,
            lyrics: LyricsState::Idle,
            bars_phase: 0,
            scroll_gate: ScrollGate::default(),
            form_gate: ScrollGate::with_cooldown(Duration::from_millis(40)),
            last_deleted: None,
            undo_gen: 0,
            volume: 0.0,
            muted: false,
            vol_last_set: Instant::now(),
            vol_pending: false,
            vol_dragging: false,
            vol_gen: 0,
            agenda: Agenda::default(),
            data_dir: store::data_dir(),
            notices: NoticeQueue::default(),
            form: None,
            cal_view: (now0.year(), now0.month()),
            cal_selected: now0,
            cal_inited: false,
            todo_draft: String::new(),
            save_blocked: false,
            save_failed: false,
            dev_ports: Vec::new(),
            dev_confirm: None,
            dev_tab: crate::dev::view::DevTab::default(),
            dev_picker: None,
            gh_state: Default::default(),
            gh_no_access: 0,
            local_scan: None,
            local_scan_busy: false,
            local_gl_scan: None,
            local_gl_busy: false,
            gl_kr_busy: false,
            poll_gen: 0,
            pending_gh: false,
            pending_gl: false,
            gl_user: None,
            gh_prs: Vec::new(),
            gh_runs: Vec::new(),
            gh_last_ok: None,
            gh_prs_ok: None,
            gh_error: None,
            gh_busy: false,
            gh_gen: 0,
            gh_pause_until: None,
            gl_token: None,
            gl_mrs: Vec::new(),
            gl_pipelines: Vec::new(),
            gl_last_ok: None,
            gl_error: None,
            gl_busy: false,
            gl_gen: 0,
            gl_pause_until: None,
            gl_no_access: 0,
            notice_gen: 0,
            notice_deadline: None,
            notice_paused: None,
            hud: None,
            hud_gen: 0,
            hud_gate: HudGate::default(),
            peek: false,
            pointer_inside: false,
            line_hover: false,
            peek_stats: peek::Stats::default(),
            sys_battery: BatteryRules::default(),
            sys_wifi: WifiRules::default(),
            sys_bt: BtRules::default(),
            wifi_gen: 0,
            wifi_armed: None,
            shelf: crate::shelf::Shelf::default(),
            shelf_save_blocked: false,
            shelf_save_failed: false,
            thumbs: Default::default(),
            thumb_pending: Default::default(),
            surface_w: None,
            pill_drag: None,
            drag_pos: None,
            hover: None,
            drag_last_raw_y: None,
            surface_h: None,
            pill_press_pending: false,
            left_while_dragging: false,
            last_drag_end: None,
            last_drop: None,
            dragging: false,
            drag_paths: Vec::new(),
            drop_hover: false,
            ctrl_held: false,
            desk_snap: desktops::Snapshot::default(),
            apps: AppIndex::default(),
            last_pin_title: None,
        };
        // Sin instancia previa: la acción de la línea de comandos se aplica al arrancar.
        match flags.action {
            CliAction::Toggle => {
                app.state = app
                    .state
                    .on(Event::Toggle, &app.config.pages, app.last_page)
            }
            CliAction::OpenPage(p) => {
                app.state = app
                    .state
                    .on(Event::OpenPage(p), &app.config.pages, app.last_page)
            }
            CliAction::Settings => {
                app.state = app.state.on(Event::Click, &app.config.pages, app.last_page);
                if matches!(app.state, IslandState::Expanded { .. }) {
                    app.autostart = settings::autostart_enabled(&settings::autostart_dir());
                    app.settings = Some(app.settings_section);
                }
            }
            CliAction::Terminal | CliAction::Run => {}
        }
        if let IslandState::Expanded { page } = app.state {
            app.last_page = page;
        }
        app.sync_titles_wanted();
        let (agenda, warning) = store::load(&app.data_dir);
        app.agenda = agenda;
        if let Some(w) = warning {
            eprintln!("morph: {}", w.log);
            app.save_blocked = !w.writable;
            app.notices.push(Notice {
                kind: NoticeKind::Info,
                title: crate::fl!("cal-notice-damaged"),
                subtitle: w.subtitle,
                sticky: true,
            });
        }
        let (shelf, warning) = crate::shelf::load(&app.data_dir);
        app.shelf = shelf;
        if let Some(w) = warning {
            eprintln!("morph: {}", w.log);
            app.shelf_save_blocked = !w.writable;
            app.notices.push(Notice {
                kind: NoticeKind::Info,
                title: crate::fl!("shelf-notice-damaged"),
                subtitle: w.subtitle,
                sticky: true,
            });
        }
        if app.config.shelf_start_empty {
            app.shelf.clear();
            app.save_shelf();
        } else {
            app.prune_shelf();
        }
        app.run_scheduler();
        let notice_task = app.notice_timer();
        app.visible = app.target_size(&app.state);
        app.surface = geometry::surface_size(app.scale());
        let task = if app.config.output.is_none() {
            app.create_surface()
        } else {
            // Los monitores llegan por eventos Wayland: darles un momento antes de elegir.
            App::realize_later()
        };
        let task = if app.state == (IslandState::Expanded { page: Page::Musica }) {
            Task::batch([task, App::read_volume(0)])
        } else {
            task
        };
        let thumb_task = app.request_thumbs();
        let gh_task = app.gh_reset();
        let gl_task = app.gl_reset(false);
        let apps_task = cosmic::task::future(async {
            let list = tokio::task::spawn_blocking(apps::load_entries)
                .await
                .unwrap_or_default();
            Message::AppsLoaded(list)
        });
        let term_task = if flags.action == CliAction::Terminal {
            app.toggle_terminal()
        } else {
            Task::none()
        };
        (
            app,
            Task::batch([
                task,
                notice_task,
                thumb_task,
                apps_task,
                gh_task,
                gl_task,
                term_task,
            ]),
        )
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::SurfaceClosed(id))
    }

    fn dbus_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<Message> {
        if let cosmic::dbus_activation::Details::ActivateAction { action, .. } = msg.msg {
            match CliAction::from_action_string(&action) {
                Some(CliAction::Toggle) => return self.apply_and_revive(Event::Toggle),
                Some(CliAction::OpenPage(p)) => return self.apply_and_revive(Event::OpenPage(p)),
                Some(CliAction::Settings) => return self.open_settings_cli(),
                Some(CliAction::Terminal) => return self.toggle_terminal(),
                Some(CliAction::Run) | None => eprintln!("acción desconocida: {action}"),
            }
        }
        Task::none()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.now = chrono::Local::now();
                return self.scheduler_tick();
            }
            Message::Notice(a) => return self.on_notice_action(a),
            Message::HudRaw(v) => {
                if debug() {
                    eprintln!("morph: HUD lectura {v:?}");
                }
                if v.kind == crate::sys::rules::HudKind::Volume {
                    self.peek_stats.volume = Some(v);
                }
                let Some(v) = self.hud_gate.feed(v) else {
                    return Task::none();
                };
                if !hud::should_show(
                    &v,
                    self.state == IslandState::Collapsed,
                    self.sticky_notice(),
                    &self.config,
                ) {
                    return Task::none();
                }
                self.hud = Some(v);
                self.hud_gen += 1;
                let gen_ = self.hud_gen;
                // Aplaza el aviso no fijo visible: su cuenta se reinicia al terminar el HUD.
                let notice = self.notice_timer();
                let expire = cosmic::task::future(async move {
                    tokio::time::sleep(Duration::from_millis(hud::HUD_MILLIS)).await;
                    Message::HudExpire(gen_)
                });
                return Task::batch([self.retarget(), notice, expire]);
            }
            Message::PeekEnter => {
                self.pointer_inside = true;
                self.hover = None;
                return self.resume_peek();
            }
            Message::LineHide => {
                if !self.pointer_inside && self.line_hover {
                    self.line_hover = false;
                    return self.retarget();
                }
            }
            Message::PillPress => {
                // Si se perdió la suelta de un arrastre anterior, se cierra en su última posición.
                let stale = if self.pill_drag.is_some() {
                    self.update(Message::PillDragEnd)
                } else {
                    Task::none()
                };
                // Sin ancho de monitor no se puede mover: el clic abre como siempre.
                if self.placement().is_none() {
                    return Task::batch([stale, self.apply(Event::Click)]);
                }
                let ow = self.surface_w.unwrap_or(0);
                // El centro de la propia pastilla, no el de la caja dibujada (asomada, aviso...).
                let base = self.base_collapsed_size();
                let center_x = geometry::box_center(ow, base.w, self.pill_pos());
                let center_y = self.box_top(base.h) as f32 + base.h as f32 / 2.0;
                let mut drag = Drag::new(center_x, center_y);
                self.drag_last_raw_y = self.hover.map(|(_, y)| y);
                // Sin posición previa del puntero, la pulsación se fija con el primer
                // movimiento (así la pastilla no salta).
                self.pill_press_pending = self.hover.is_none();
                if let Some((x, y)) = self.hover {
                    drag.on(DragEv::Press(x, y));
                }
                self.pill_drag = Some(drag);
                return stale;
            }
            Message::PillHover(x, y) => {
                let box_x = self.placement().unwrap_or(0);
                let box_y = self.box_top(self.hit_h());
                self.hover = Some((box_x as f32 + x, box_y as f32 + y));
            }
            Message::PillDragMove(id, x, y) if id == self.pill_id => {
                let was_active = self.pill_dragging();
                let base_w = self.base_collapsed_size().w;
                self.drag_last_raw_y = Some(y);
                let (Some(drag), Some(ow)) = (&mut self.pill_drag, self.surface_w) else {
                    return Task::none();
                };
                if std::mem::take(&mut self.pill_press_pending) {
                    drag.on(DragEv::Press(x, y));
                    return Task::none();
                }
                if let DragOut::Moved(c, _) = drag.on(DragEv::Move(x, y)) {
                    let pos = geometry::snap(ow, base_w, c / ow as f32).clamp(0.0, 1.0);
                    self.drag_pos = Some(pos);
                    let place = self.sync_place();
                    // Al empezar, la vista asomada y los avisos se apartan.
                    let shrink = if was_active {
                        Task::none()
                    } else {
                        // Los avisos esperan: se anota cuánto les quedaba.
                        if self.notice_paused.is_none() {
                            self.notice_paused = Some(Instant::now());
                        }
                        self.retarget()
                    };
                    return Task::batch([place, shrink]);
                }
            }
            Message::PillDragMove(..) => {}
            Message::SurfaceResized(id, w, h) => {
                if id == self.pill_id && w > 0 && h > 0 {
                    let changed = self.surface_w != Some(w) || self.surface_h != Some(h);
                    self.surface_w = Some(w);
                    self.surface_h = Some(h);
                    if changed {
                        return self.sync_place();
                    }
                }
            }
            Message::PillDragEnd => {
                let Some(mut drag) = self.pill_drag.take() else {
                    return Task::none();
                };
                self.hover = None;
                let pending = std::mem::take(&mut self.pill_press_pending);
                let out = drag.on(DragEv::Release);
                // Sin movimientos ni posición de pulsación: fue un clic.
                let out = if pending { DragOut::Click } else { out };
                match out {
                    DragOut::Click if self.state != IslandState::Collapsed => {
                        self.drag_pos = None;
                        return Task::none();
                    }
                    DragOut::Click => {
                        self.drag_pos = None;
                        return self.apply(Event::Click);
                    }
                    DragOut::Done(c, _) => {
                        if let Some(ow) = self.surface_w {
                            let pos =
                                geometry::snap(ow, self.base_collapsed_size().w, c / ow as f32);
                            self.save_pill_pos(pos.clamp(0.0, 1.0));
                        }
                        // Borde: el de destino según dónde quedó el puntero al soltar.
                        let edge = geometry::target_edge(
                            self.config.pill_edge,
                            self.drag_last_raw_y.unwrap_or(0.0),
                            self.surface.h,
                            self.output_h(),
                        );
                        // `set_pill_edge` ya cambia `self.config`, así que el eco de la
                        // config no verá diferencia: el anclaje se pide aquí.
                        let anchor = if edge != self.config.pill_edge {
                            self.save_pill_edge(edge);
                            set_anchor(self.pill_id, self.anchor())
                        } else {
                            Task::none()
                        };
                        let end = self.end_pill_drag();
                        return Task::batch([anchor, end, self.sync_place()]);
                    }
                    DragOut::None | DragOut::Moved(..) => {}
                }
                return self.end_pill_drag();
            }
            Message::PeekSample(s) => {
                self.peek_stats.ram = s.ram;
                self.peek_stats.cpu = s.cpu;
            }
            Message::Island(Event::PointerLeft) if self.peek || self.pointer_inside => {
                self.pointer_inside = false;
                self.hover = None;
                let shrink = self.set_peek(false);
                return Task::batch([shrink, self.update(Message::Island(Event::PointerLeft))]);
            }
            Message::SysBattery(r) => {
                let had_battery = self.peek_stats.battery.is_some();
                self.peek_stats.battery = Some((r.percent, r.state.plugged()));
                let resize = if !had_battery
                    && (self.peek_active() || self.config.pill_battery)
                    && self.state == IslandState::Collapsed
                {
                    // Aparece la batería con la pastilla asomada o con el dato fijo: cambia el ancho.
                    self.retarget()
                } else {
                    Task::none()
                };
                let mut task = Task::none();
                if r.state.plugged() {
                    // Al enchufar, la advertencia fija de batería crítica ya no aplica.
                    let before = self.notices.current().cloned();
                    self.notices
                        .remove_where(|n| matches!(n.kind, NoticeKind::Sys { critical: true, .. }));
                    if before.as_ref() != self.notices.current() {
                        // El sonido de la crítica solo se corta si no suena una alarma.
                        if !self.notices.has_alarm() {
                            sound::stop();
                        }
                        task = self.notice_changed();
                    }
                }
                let n = self.sys_battery.feed(r.percent, r.state, r.minutes_left);
                return Task::batch([task, resize, self.push_sys(n.into_iter().collect())]);
            }
            Message::SysNet(ev) => {
                let list = self.sys_wifi.feed(ev, Instant::now());
                return Task::batch([self.push_sys(list), self.arm_wifi()]);
            }
            Message::WifiTick(g) => {
                if g == self.wifi_gen {
                    self.wifi_armed = None;
                    let list = self.sys_wifi.tick(Instant::now());
                    return Task::batch([self.push_sys(list), self.arm_wifi()]);
                }
            }
            Message::SysBt(dev) => {
                let n = self.sys_bt.feed(dev);
                return self.push_sys(n.into_iter().collect());
            }
            Message::HudExpire(g) => {
                if g == self.hud_gen && self.hud.take().is_some() {
                    // Si había un aviso no fijo debajo, vuelve con su cuenta de 4 s completa.
                    if self.state == IslandState::Collapsed {
                        return self.notice_changed();
                    }
                }
            }
            Message::NoticeTimeout(g) => {
                if g == self.notice_gen
                    && !self.pill_dragging()
                    && !self.hud_active()
                    && self.state == IslandState::Collapsed
                    && self.notices.current().is_some_and(|n| !n.sticky)
                {
                    if self.notices.pop().is_some_and(|n| n.kind.owns_sound()) {
                        sound::stop();
                    }
                    return self.notice_changed();
                }
            }
            Message::CtrlHeld(on) => {
                if self.ctrl_held != on {
                    self.ctrl_held = on;
                }
            }
            Message::Settings(m) => return self.settings_update(m),
            Message::Key(key) => {
                if key == keyboard::Key::Named(Named::Escape) {
                    if self.settings_open() {
                        if self.settings_host_draft.take().is_some() {
                            return Task::none();
                        }
                        let close = self.close_settings();
                        // Con el puntero ya fuera, cierra la isla como al salir.
                        if !self.pointer_inside {
                            return Task::batch([close, self.pointer_left_by_drag()]);
                        }
                        return close;
                    }
                    if self.form.take().is_some() {
                        return Task::none();
                    }
                    if !self.todo_draft.trim().is_empty() {
                        self.todo_draft.clear();
                        return Task::none();
                    }
                    self.dragging = false;
                    self.drag_paths.clear();
                    self.drop_hover = false;
                    self.left_while_dragging = false;
                    return self.apply(Event::Escape);
                }
            }
            Message::Dropped(data) => {
                self.drop_hover = false;
                self.last_drop = Some(Instant::now());
                let paths = dnd::parse_uri_list(&data);
                if !paths.is_empty() {
                    let now = chrono::Local::now().naive_local();
                    if self.shelf.add_paths(paths, now) > 0 {
                        // prune_shelf ya guarda si quitó algo.
                        if !self.prune_shelf() {
                            self.save_shelf();
                        }
                        return Task::batch([self.retarget(), self.request_thumbs()]);
                    }
                }
            }
            Message::ThumbReady(path, mtime, rgba) => {
                self.thumb_pending.remove(&path);
                if let Some((w, h, px)) = rgba
                    && self.shelf.entries.iter().any(|e| e.path == path)
                {
                    self.thumbs
                        .insert(path, (mtime, Handle::from_rgba(w, h, px)));
                }
            }
            Message::DndLeft => {
                self.drop_hover = false;
                if self
                    .last_drop
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(300))
                {
                    return Task::none();
                }
                return self.pointer_left_by_drag();
            }
            Message::DndEnter => {
                self.drop_hover = !self.dragging;
                self.left_while_dragging = false;
                self.clear_peek();
                return self.apply(Event::OpenPage(Page::Shelf));
            }
            Message::ShelfOpen(path) => {
                let since = self.last_drag_end.map(|t| t.elapsed());
                if click_allowed(self.dragging, since) && path.exists() {
                    open_path(&path);
                }
            }
            Message::ShelfRemove(path) => {
                if self.shelf.remove(&path) {
                    self.thumbs.remove(&path);
                    self.save_shelf();
                }
            }
            Message::ShelfClear => {
                self.thumbs.clear();
                self.shelf.clear();
                self.save_shelf();
            }
            Message::DragStart(paths) => {
                self.dragging = true;
                self.drag_paths = paths;
                self.drop_hover = false;
                // La acción ya quedó fijada al empezar; Ctrl no debe quedarse pegado.
                self.ctrl_held = false;
            }
            Message::PruneShelf => {
                if !self.dragging {
                    return self.prune_and_retarget();
                }
            }
            Message::Dragging(on) => {
                self.dragging = on;
                if !on {
                    self.last_drag_end = Some(Instant::now());
                    self.drag_paths.clear();
                    self.drop_hover = false;
                    self.ctrl_held = false;
                    // El destino mueve el archivo después de soltar: revisar ya y de nuevo
                    // tras una breve espera.
                    let pruned = self.prune_and_retarget();
                    let recheck = cosmic::task::future(async {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        Message::PruneShelf
                    });
                    // El puntero salió durante el arrastre: ahora que terminó, cerrar.
                    if std::mem::take(&mut self.left_while_dragging) {
                        return Task::batch([pruned, recheck, self.pointer_left_by_drag()]);
                    }
                    return Task::batch([pruned, recheck]);
                }
            }
            Message::Term(m) => return self.on_term(m),
            Message::SurfaceClosed(id) => {
                self.term_surface_closed(id);
                if should_recreate(self.pill_id, id, self.surface_made) {
                    self.surface_made = false;
                    self.anim = None;
                    return App::realize_later();
                }
            }
            Message::AnimFrame => return self.anim_frame(),
            Message::Media(MediaEvent::Ready(tx)) => self.media_tx = Some(tx),
            Message::Media(MediaEvent::Changed(np)) => return self.on_media(np),
            Message::MediaCmd(cmd) => {
                if let Some(tx) = &self.media_tx {
                    let _ = tx.send(cmd);
                }
            }
            Message::ArtLoaded(src, handle) => {
                // Solo vale si sigue siendo la carátula de la pista actual.
                if accept_art(self.np.as_ref().and_then(|n| n.art.as_ref()), &src) {
                    self.art = handle;
                }
            }
            Message::LyricsLoaded(resp) => {
                let cur = self.np.as_ref().map(NowPlaying::track_key);
                self.lyrics = lyrics_after(&self.lyrics, cur.as_ref(), resp);
            }
            Message::LyricsTick => {}
            Message::Wheel(_) if self.form.is_some() || !self.todo_draft.trim().is_empty() => {}
            Message::Wheel(d) => {
                let step = self.scroll_gate.feed(d, Instant::now());
                if step != 0 {
                    return self.apply(Event::Scroll(step));
                }
            }
            Message::VolumePoll => return App::read_volume(self.vol_gen),
            Message::VolumeRead(g, v) => {
                // Mientras se arrastra el deslizador manda el valor local.
                if let Some((vol, muted)) = v.filter(|_| {
                    accept_volume_read(g, self.vol_gen, self.vol_dragging, self.vol_pending)
                }) {
                    self.volume = vol;
                    self.muted = muted;
                }
            }
            Message::VolumeChanged(pct) => {
                self.vol_dragging = true;
                self.volume = (pct / 100.0).clamp(0.0, 1.0);
                if self.vol_last_set.elapsed() >= Duration::from_millis(50) {
                    return self.send_volume();
                }
                self.vol_pending = true;
            }
            Message::VolumeReleased => {
                self.vol_dragging = false;
                return self.send_volume();
            }
            Message::Nop => {}
            Message::DevPoll => {
                if self.state == (IslandState::Expanded { page: Page::Dev }) {
                    return App::scan_ports();
                }
            }
            Message::DevPorts(v) => {
                if debug() {
                    eprintln!("morph: dev: {} servidores locales", v.len());
                }
                // Si el proceso de la confirmación ya no está, se descarta.
                if self
                    .dev_confirm
                    .is_some_and(|p| !v.iter().any(|l| l.pid == p))
                {
                    self.dev_confirm = None;
                }
                if v != self.dev_ports {
                    self.dev_ports = v;
                }
            }
            Message::GhTick => {
                use crate::dev::github::GhError;
                let mut tasks = Vec::new();
                if !matches!(self.gh_error, Some(GhError::NoGh | GhError::NotLoggedIn)) {
                    tasks.push(self.gh_refresh());
                }
                if !self.gl_error.as_ref().is_some_and(|e| e.is_fatal()) {
                    tasks.push(self.gl_refresh());
                }
                return Task::batch(tasks);
            }
            Message::GlTokenLoaded(h, r) => {
                if settings::host_is_current(&h, &self.config.gitlab_host) {
                    return self.on_gl_token_loaded(r);
                }
            }
            Message::GlFetched(g, r) => {
                if g == self.gl_gen {
                    return self.on_gl_fetched(r);
                }
            }
            Message::GhFetched(g, r) => {
                if g == self.gh_gen {
                    return self.on_gh_fetched(r);
                }
            }
            Message::DevOpen(port) => open_url(&format!("http://localhost:{port}")),
            Message::DevStopAsk(pid) => self.dev_confirm = Some(pid),
            Message::DevStopNo => self.dev_confirm = None,
            Message::DevStopYes(pid) => {
                self.dev_confirm = None;
                // Solo procesos listados, del mismo uid y nunca la propia isla.
                let listed = self
                    .dev_ports
                    .iter()
                    .find(|l| l.pid == pid)
                    .map(|l| l.start);
                let Some(start) = listed else {
                    self.info_notice(crate::fl!("dev-cannot-stop"));
                    return App::scan_ports();
                };
                if pid == std::process::id() || !ports::can_stop(pid, ports::my_uid()) {
                    self.info_notice(crate::fl!("dev-cannot-stop"));
                } else if let Err(e) = ports::stop(pid, start) {
                    if debug() {
                        eprintln!("morph: dev: no se pudo detener {pid}: {e}");
                    }
                    self.info_notice(crate::fl!("dev-stop-failed"));
                }
                return App::scan_ports();
            }
            Message::DevTab(t) => {
                self.dev_tab = t;
            }
            Message::DevOpenUrl(url) => open_url(&url),
            // «+ Repos» de la página Dev: abre Ajustes en la sección GitHub.
            Message::DevReposOpen => return self.open_settings(Section::GitHub),
            Message::DevReposLoaded(r) => {
                if let Some(p) = &mut self.dev_picker {
                    p.accounts = Some(r);
                }
            }
            Message::DevLocalLoaded(v) => {
                self.local_scan_busy = false;
                if let Some(p) = &mut self.dev_picker {
                    p.local = Some(v.clone());
                }
                self.local_scan = Some(v);
            }
            Message::DevManualInput(t) => {
                if let Some(p) = &mut self.dev_picker {
                    p.manual = t;
                    p.manual_err = false;
                }
            }
            Message::DevManualAdd => {
                let added = self
                    .dev_picker
                    .as_mut()
                    .is_some_and(|p| p.add_manual())
                    .then(|| self.dev_picker.as_ref().map(|p| p.selected.clone()))
                    .flatten();
                if let Some(v) = added {
                    return self.apply_change(Change::GithubRepos(v));
                }
            }
            Message::DevRepoToggle(name) => {
                let v = settings::toggle_repo(&self.config.github_repos, &name);
                return self.apply_change(Change::GithubRepos(v));
            }
            Message::DevGlTokenInput(t) => {
                if let Some(p) = &mut self.dev_picker {
                    p.gl_token_input = t;
                    p.gl_msg = None;
                }
            }
            Message::DevGlTokenSave => {
                // El token va al servidor que se ve en pantalla: primero se resuelve el borrador.
                let (ok, commit) = self.commit_host_for_token();
                if !ok {
                    return Task::none();
                }
                let Some(p) = &mut self.dev_picker else {
                    return Task::none();
                };
                if p.gl_busy {
                    return Task::none();
                }
                let Some(token) = crate::dev::gitlab::clean_token(p.gl_token_input.as_str()) else {
                    return Task::none();
                };
                p.gl_token_input = Default::default();
                p.gl_busy = true;
                p.gl_msg = None;
                let host = self.config.gitlab_host.clone();
                let h2 = host.clone();
                return Task::batch([
                    commit,
                    Task::perform(
                        async move {
                            let user = crate::dev::gitlab::whoami(&host, &token).await?;
                            crate::dev::keyring::store(&host, token.as_str()).await?;
                            Ok((user, token))
                        },
                        move |r| cosmic::Action::App(Message::DevGlTokenSaved(h2, r)),
                    ),
                ]);
            }
            Message::DevGlTokenSaved(h, _)
                if !settings::host_is_current(&h, &self.config.gitlab_host) =>
            {
                // Cambió el servidor mientras se guardaba: el resultado ya no aplica.
                if let Some(p) = &mut self.dev_picker {
                    p.gl_busy = false;
                }
            }
            Message::DevGlTokenSaved(_, r) => match r {
                Ok((user, token)) => {
                    self.gl_token = Some(token);
                    self.gl_user = Some(user.clone());
                    self.gl_error = None;
                    self.gl_pause_until = None;
                    if let Some(p) = &mut self.dev_picker {
                        p.gl_busy = false;
                        p.gl_has_token = true;
                        p.gl_user = Some(user.clone());
                        p.gl_msg =
                            Some((crate::fl!("gl-connected-as", user = user.as_str()), true));
                        p.gl_projects = None;
                    }
                    let host = self.config.gitlab_host.clone();
                    let token = self.gl_token.clone();
                    let mut tasks = vec![self.gl_refresh()];
                    if let Some(t) = token {
                        tasks.push(Task::perform(
                            async move {
                                (
                                    host.clone(),
                                    crate::dev::gitlab::my_projects(&host, &t).await,
                                )
                            },
                            |(h, r)| cosmic::Action::App(Message::DevGlProjectsLoaded(h, r)),
                        ));
                    }
                    return Task::batch(tasks);
                }
                Err(e) => {
                    if debug() {
                        eprintln!("morph: gitlab: token no guardado: {e:?}");
                    }
                    if let Some(p) = &mut self.dev_picker {
                        p.gl_busy = false;
                        p.gl_msg = Some((e.message(None), false));
                    }
                }
            },
            Message::DevGlForget => {
                let host = self.config.gitlab_host.clone();
                return Task::perform(
                    async move { crate::dev::keyring::forget(&host).await },
                    |r| cosmic::Action::App(Message::DevGlForgot(r)),
                );
            }
            Message::DevGlForgot(r) => match r {
                Ok(()) => {
                    self.gl_token = None;
                    self.gl_user = None;
                    self.gl_gen += 1;
                    self.gl_busy = false;
                    self.gl_no_access = 0;
                    self.gl_pause_until = None;
                    self.gl_mrs.clear();
                    self.gl_pipelines.clear();
                    self.gl_last_ok = None;
                    self.gl_error = (!self.config.gitlab_projects.is_empty())
                        .then_some(crate::dev::gitlab::GlError::NoToken);
                    if let Some(p) = &mut self.dev_picker {
                        p.gl_has_token = false;
                        p.gl_user = None;
                        p.gl_msg = Some((crate::fl!("gl-token-forgotten"), true));
                        p.gl_projects = Some(Err(gl_needs_token()));
                    }
                }
                Err(e) => {
                    if let Some(p) = &mut self.dev_picker {
                        p.gl_msg = Some((e.message(None), false));
                    }
                }
            },
            Message::DevGlCreateToken => {
                let (_, commit) = self.commit_host_for_token();
                open_url(&crate::dev::view::token_url(&self.config.gitlab_host));
                return commit;
            }
            Message::DevGlKeyringLoaded(h, _)
                if !settings::host_is_current(&h, &self.config.gitlab_host) =>
            {
                self.gl_kr_busy = false;
                // Seguimos en GitLab con otro servidor: se consulta el de ahora.
                if self.settings_open() && self.settings == Some(Section::GitLab) {
                    return self.gl_picker_open();
                }
            }
            Message::DevGlKeyringLoaded(_, r) => {
                self.gl_kr_busy = false;
                match r {
                    Ok(Some(t)) => {
                        self.gl_token = Some(t.clone());
                        self.gl_error = None;
                        if let Some(p) = &mut self.dev_picker {
                            p.gl_has_token = true;
                            p.gl_projects = None;
                            p.gl_msg = None;
                        }
                        return Task::batch([self.gl_connected_tasks(t), self.gl_refresh()]);
                    }
                    Ok(None) => {
                        if let Some(p) = &mut self.dev_picker {
                            p.gl_projects = Some(Err(gl_needs_token()));
                        }
                    }
                    Err(e) => {
                        if let Some(p) = &mut self.dev_picker {
                            p.gl_msg = Some((e.message(None), false));
                            p.gl_projects = Some(Err(gl_needs_token()));
                        }
                    }
                }
            }
            Message::DevGlWho(h, _) if !settings::host_is_current(&h, &self.config.gitlab_host) => {
            }
            Message::DevGlWho(_, r) => {
                if let Ok(u) = r {
                    self.gl_user = Some(u.clone());
                    if let Some(p) = &mut self.dev_picker {
                        p.gl_user = Some(u);
                    }
                }
            }
            Message::DevGlProjectsLoaded(h, _)
                if !settings::host_is_current(&h, &self.config.gitlab_host) => {}
            Message::DevGlProjectsLoaded(_, r) => {
                if let Some(p) = &mut self.dev_picker {
                    p.gl_projects = Some(r.map_err(|e| e.message(None)));
                }
            }
            Message::DevGlLocalLoaded(h, _)
                if !settings::host_is_current(&h, &self.config.gitlab_host) =>
            {
                self.local_gl_busy = false;
            }
            Message::DevGlLocalLoaded(_, v) => {
                self.local_gl_busy = false;
                if let Some(p) = &mut self.dev_picker {
                    p.gl_local = Some(v.clone());
                }
                self.local_gl_scan = Some(v);
            }
            Message::DevGlManualInput(t) => {
                if let Some(p) = &mut self.dev_picker {
                    p.gl_manual = t;
                    p.gl_manual_err = false;
                }
            }
            Message::DevGlManualAdd => {
                let added = self
                    .dev_picker
                    .as_mut()
                    .is_some_and(|p| p.add_manual_gl())
                    .then(|| self.dev_picker.as_ref().map(|p| p.gl_selected.clone()))
                    .flatten();
                if let Some(v) = added {
                    return self.apply_change(Change::GitlabProjects(v));
                }
            }
            Message::DevGlToggle(name) => {
                let Some(p) = &self.dev_picker else {
                    return Task::none();
                };
                // Si ya está elegido se quitan todas sus grafías; si no, se añade el canónico.
                let on = self
                    .config
                    .gitlab_projects
                    .iter()
                    .any(|s| crate::dev::gitlab::same_project(s, &name));
                let name = if on {
                    name
                } else {
                    crate::dev::view::canonical_project(p, &name)
                };
                let v = settings::toggle_project(&self.config.gitlab_projects, &name);
                return self.apply_change(Change::GitlabProjects(v));
            }
            Message::PollDebounce(g) => {
                if settings::debounce_fire(g, self.poll_gen, self.pending_gh || self.pending_gl) {
                    return self.flush_polls().0;
                }
            }
            Message::BarsTick => self.bars_phase = self.bars_phase.wrapping_add(1),
            Message::Realize => {
                if !self.surface_made {
                    return self.create_surface();
                }
            }
            Message::OutputSeen(wl, name, size) => {
                if let Some((w, h)) = size.filter(|(w, h)| *w > 0 && *h > 0) {
                    let s = (w as u32, h as u32);
                    match self.output_sizes.iter_mut().find(|(o, _)| *o == wl) {
                        Some(e) => e.1 = s,
                        None => self.output_sizes.push((wl.clone(), s)),
                    }
                }
                match self.outputs.iter_mut().find(|(o, _)| *o == wl) {
                    Some(entry) => entry.1 = name,
                    None => self.outputs.push((wl, name)),
                }
                return Task::batch([self.reconcile_output(), self.term_monitor_changed()]);
            }
            Message::OutputGone(wl) => {
                self.outputs.retain(|(o, _)| *o != wl);
                self.output_sizes.retain(|(o, _)| *o != wl);
                return Task::batch([self.reconcile_output(), self.term_monitor_changed()]);
            }
            Message::Config(new, keys) => {
                let new = crate::config::merge_update(&self.config, new, &keys);
                let old = std::mem::replace(&mut self.config, new);
                return self.config_applied(&old);
            }
            Message::Desktops(DesktopsEvent::Snapshot(snap)) => self.desk_snap = snap,
            Message::AppsLoaded(list) => self.apps = AppIndex::new(list),
            Message::DeskActivateWorkspace(id) => {
                desktops::send(DesktopsCmd::ActivateWorkspace(id));
                return self.apply(Event::Escape);
            }
            Message::DeskActivateWindow(id) => {
                desktops::send(DesktopsCmd::ActivateWindow(id));
                return self.apply(Event::Escape);
            }
            Message::DeskLaunch(app_id, ws_id) => {
                desktops::send(DesktopsCmd::ActivateWorkspace(ws_id));
                let info = self.apps.get(&app_id);
                let mut tasks = Vec::new();
                if info.is_some_and(|a| a.terminal) {
                    let name = display_name(&app_id, &self.apps);
                    self.info_notice(crate::fl!("desk-terminal-app", name = name.as_str()));
                } else if let Some(argv) = info
                    .and_then(|a| a.exec.as_deref())
                    .map(apps::exec_argv)
                    .filter(|v| !v.is_empty())
                {
                    let dir = info.and_then(|a| a.path.clone());
                    // Un momento para que el compositor cambie de escritorio antes de abrir.
                    tasks.push(cosmic::task::future(async move {
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        Message::LaunchNow(argv, dir)
                    }));
                } else {
                    self.info_notice(crate::fl!("desk-cannot-open", app = app_id.as_str()));
                }
                tasks.push(self.apply(Event::Escape));
                return Task::batch(tasks);
            }
            Message::LaunchNow(argv, dir) => launch_app(&argv, dir.as_deref()),
            Message::DeskTogglePin(app_id, index) => {
                let mut pins = self.config.pins.clone();
                toggle_pin(&mut pins, &app_id, index);
                let now = pins
                    .iter()
                    .find(|p| p.app_id.eq_ignore_ascii_case(&app_id))
                    .map(|p| p.workspace);
                match cosmic::cosmic_config::Config::new(APP_ID, 1) {
                    Ok(h) => {
                        if let Err(e) = self.config.set_pins(&h, pins) {
                            eprintln!("morph: no se pudo guardar pins: {e}");
                        }
                    }
                    Err(e) => {
                        eprintln!("morph: no se pudo abrir la config: {e}");
                        self.config.pins = pins;
                    }
                }
                let name = display_name(&app_id, &self.apps);
                let title = pin_notice(&name, now);
                // Un aviso de fijación que aún no se vio se reemplaza por el nuevo.
                if self.state != IslandState::Collapsed
                    && let Some(old) = self.last_pin_title.take()
                {
                    self.notices.remove_where(|n| n.title == old);
                }
                self.last_pin_title = Some(title.clone());
                self.info_notice(title);
            }
            Message::Island(Event::PointerLeft) if self.dragging => {
                self.left_while_dragging = true;
            }
            Message::Island(Event::PointerLeft)
                if settings::keep_open_on_leave(
                    self.settings_open(),
                    self.form.is_some(),
                    !self.todo_draft.trim().is_empty(),
                ) => {}
            Message::Island(e) => {
                return self.apply(e);
            }
            Message::FormOpen(mut f) => {
                if matches!(self.state, IslandState::Expanded { .. }) {
                    f.set_h12(self.config.clock_12h);
                    self.form = Some(f);
                    return widget::text_input::focus(form_view::title_id());
                }
            }
            Message::CalShift(d) => {
                self.cal_view = month::shift_month(self.cal_view.0, self.cal_view.1, d);
            }
            Message::CalSelect(d) => {
                self.cal_selected = d;
                // Elegir un día de otro mes lleva la vista a ese mes.
                self.cal_view = (d.year(), d.month());
            }
            Message::TodoDraft(v) => self.todo_draft = v,
            Message::TodoAdd => {
                let title = self.todo_draft.trim().to_string();
                if !title.is_empty() {
                    self.agenda.add(
                        title,
                        Kind::Task {
                            date: None,
                            time: None,
                            duration_min: None,
                            remind_start: true,
                            remind_end: false,
                            done: false,
                        },
                    );
                    self.todo_draft.clear();
                    self.save_agenda();
                }
            }
            Message::TaskToggle(id) => {
                if self.agenda.toggle_done(id) {
                    self.save_agenda();
                }
            }
            Message::RowDelete(id) => {
                if let Some(item) = self.agenda.items.iter().find(|i| i.id == id).cloned() {
                    self.agenda.remove(id);
                    self.notices.remove_item(id);
                    sound::stop();
                    self.last_deleted = Some(item);
                    self.undo_gen += 1;
                    let gen_ = self.undo_gen;
                    self.save_agenda();
                    return cosmic::task::future(async move {
                        tokio::time::sleep(Duration::from_secs(UNDO_SECS)).await;
                        Message::UndoTimeout(gen_)
                    });
                }
            }
            Message::UndoDelete => {
                if let Some(item) = self.last_deleted.take() {
                    self.agenda.restore(item);
                    self.save_agenda();
                }
            }
            Message::UndoTimeout(g) => {
                if g == self.undo_gen {
                    self.last_deleted = None;
                }
            }
            Message::AlarmToggle(id, on) => {
                if self.agenda.set_enabled(id, on) {
                    if !on {
                        // Un aviso viejo en cola no debe reactivar la posposición.
                        self.notices.remove_item(id);
                        sound::stop();
                        self.save_agenda();
                        return self.notice_changed();
                    }
                    self.save_agenda();
                }
            }
            Message::FormTitle(v) => self.form_edit(|f| f.title = v),
            Message::FormHour(d) => self.form_edit(|f| f.step(Spinner::Hour, d)),
            Message::FormMinute(d) => self.form_edit(|f| f.step(Spinner::Minute, d)),
            Message::FormHourText(v) => self.form_edit(|f| f.type_hour(&v)),
            Message::FormMinuteText(v) => self.form_edit(|f| f.type_minute(&v)),
            Message::FormPm(pm) => self.form_edit(|f| f.set_pm(pm)),
            Message::FormWheel(which, d) => {
                // Rueda hacia arriba (`feed` < 0 = abajo) suma.
                let step = -self.form_gate.feed(d, Instant::now());
                if step != 0 {
                    self.form_edit(|f| f.step(which, step));
                }
            }
            Message::FormAddTime => {
                let now = self.now.time();
                self.form_edit(|f| f.set_time(Some(form::next_full_hour(now))));
            }
            Message::FormRemoveTime => self.form_edit(|f| {
                f.set_time(None);
                f.duration.clear();
            }),
            Message::FormDuration(v) => self.form_edit(|f| f.duration = v),
            Message::FormRemindStart(v) => self.form_edit(|f| f.remind_start = v),
            Message::FormRemindEnd(v) => self.form_edit(|f| f.remind_end = v),
            Message::FormDay(i) => self.form_edit(|f| f.days[i % 7] = !f.days[i % 7]),
            Message::FormKind(k) => {
                let today = self.now.date_naive();
                let now = self.now.time();
                self.form_edit(|f| f.set_kind(k, today, now))
            }
            Message::FormCancel => self.form = None,
            Message::FormSave => self.form_save(),
            Message::FormDelete => {
                if let Some(id) = self.form.take().and_then(|f| f.editing) {
                    self.agenda.remove(id);
                    self.notices.remove_item(id);
                    sound::stop();
                    self.save_agenda();
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        // Sin ventana principal: solo existe la superficie de la pastilla.
        widget::space().into()
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.term.surface == Some(id) {
            return self.term_view();
        }
        if id != self.pill_id {
            return widget::space().into();
        }
        let full = |size: Size| cosmic::iced::Size::new(size.w as f32, size.h as f32);
        let line = self.line_shown();
        let (content, radius, final_size) = match content_for(self.state, self.closing) {
            Content::Collapsed if line => (
                widget::space().into(),
                LINE_RADIUS,
                self.target_size(&IslandState::Collapsed),
            ),
            Content::Collapsed => (
                if self.target_edge() != self.config.pill_edge {
                    self.edge_hint()
                } else {
                    match (
                        self.hud
                            .as_ref()
                            .filter(|_| self.hud_active() && !self.pill_dragging()),
                        self.notices.current().filter(|_| !self.pill_dragging()),
                    ) {
                        (Some(v), _) => hud::view(v, self.config.hud_style),
                        (None, Some(n)) => notice::view(n, Message::Notice),
                        (None, None) if self.peek_active() => peek::view(
                            &self.peek_stats,
                            self.collapsed_center(),
                            self.center_collapsed_size(),
                            self.scale(),
                        ),
                        (None, None) => self.collapsed_view(),
                    }
                },
                PILL_RADIUS,
                self.target_size(&IslandState::Collapsed),
            ),
            Content::Expanded => (
                self.expanded_view(self.shown_page()),
                EXPANDED_RADIUS,
                geometry::expanded_content_size(self.settings.is_some(), self.scale()),
            ),
        };
        // El contenido se maqueta a su tamaño final y se recorta a la caja animada.
        let fixed = widget::container(content)
            .width(Length::Fixed(final_size.w as f32))
            .height(Length::Fixed(final_size.h as f32))
            .align_x(Alignment::Center)
            .align_y(Alignment::Center);
        let clipped = Reveal::new(
            fixed,
            full(final_size),
            full(self.visible),
            self.config.pill_edge,
        );
        let all_round = line;
        let edge = self.config.pill_edge;
        let body = widget::container(clipped)
            .width(Length::Fixed(self.visible.w as f32))
            .height(Length::Fixed(self.visible.h as f32))
            .class(cosmic::theme::Container::custom(move |_| {
                widget::container::Style {
                    background: Some(Background::Color(Color::BLACK)),
                    border: Border {
                        radius: if all_round {
                            radius.into()
                        } else {
                            geometry::radii(edge, radius).into()
                        },
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }));
        // La línea se dibuja pegada al borde, en una zona más alta, que es la que recibe el puntero.
        let hit_h = self.hit_h();
        let body = widget::container(body)
            .width(Length::Fixed(self.visible.w as f32))
            .height(Length::Fixed(hit_h as f32))
            .align_y(match edge {
                Edge::Top => Alignment::Start,
                Edge::Bottom => Alignment::End,
            });
        let area = widget::mouse_area(body);
        // Cerrada, la pulsación decide entre clic (al soltar) y arrastre; abierta, clic al pulsar.
        let area = if self.state == IslandState::Collapsed {
            area.on_press(Message::PillPress)
                .on_release(Message::PillDragEnd)
                .on_move(|p| Message::PillHover(p.x, p.y))
        } else {
            area.on_press(Message::Island(Event::Click))
        };
        let area = area
            .on_enter(Message::PeekEnter)
            .on_exit(Message::Island(Event::PointerLeft))
            .on_scroll(|d| {
                // Rueda hacia abajo = página siguiente (lo decide ScrollGate).
                Message::Wheel(match d {
                    ScrollDelta::Lines { y, .. } => Delta::Lines(y),
                    ScrollDelta::Pixels { y, .. } => Delta::Pixels(y),
                })
            });
        // Destino de DnD sobre la caja visible: al entrar con la isla cerrada, abre Shelf.
        let dnd = widget::dnd_destination(area, vec![dnd::URI_LIST_MIME.into()])
            .on_enter(|_, _, _| Message::DndEnter)
            .on_leave(|| Message::DndLeft)
            .on_data_received(|_mime, data| Message::Dropped(data))
            // Los datos posteriores al drop llegan solo por on_finish.
            .on_finish(|_mime, data, _action, _x, _y| Message::Dropped(data));
        // Superficie transparente de todo el ancho; la caja va en su `box_x` y su `box_top`
        // (centrada si no se conoce el ancho del monitor).
        let root = widget::container(dnd)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(match self.config.pill_edge {
                Edge::Top => Alignment::Start,
                Edge::Bottom => Alignment::End,
            });
        match self.placement() {
            Some(box_x) => root
                .align_x(Alignment::Start)
                .align_y(Alignment::Start)
                .padding(
                    cosmic::iced::Padding::new(0.0)
                        .left(box_x as f32)
                        .top(self.box_top(hit_h) as f32),
                )
                .into(),
            None => root.align_x(Alignment::Center).into(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let outputs = cosmic::iced::event::listen_with(|event, _status, id| match event {
            cosmic::iced::Event::PlatformSpecific(
                cosmic::iced::event::PlatformSpecific::Wayland(wl_event::Event::Output(ev, wl)),
            ) => match ev {
                wl_event::OutputEvent::Created(Some(info))
                | wl_event::OutputEvent::InfoUpdate(info) => {
                    let size = info.logical_size;
                    info.name.map(|n| Message::OutputSeen(wl, n, size))
                }
                wl_event::OutputEvent::Removed => Some(Message::OutputGone(wl)),
                wl_event::OutputEvent::Created(None) => None,
            },
            // El primer configure llega como `Opened`; los siguientes, como `Resized`.
            cosmic::iced::Event::Window(
                window::Event::Resized(size) | window::Event::Opened { size, .. },
            ) => Some(Message::SurfaceResized(
                id,
                size.width.round() as u32,
                size.height.round() as u32,
            )),
            _ => None,
        });
        let config = self
            .core()
            .watch_config::<MorphConfig>(APP_ID)
            .map(|u| Message::Config(u.config, u.keys));
        let keys = cosmic::iced::event::listen_with(|event, status, _id| match event {
            // Solo llegan con foco de teclado (la isla expandida usa OnDemand).
            cosmic::iced::Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                Some(Message::CtrlHeld(m.control()))
            }
            cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                // Un Esc que ya consumió un campo de texto no cierra nada más.
                if status == cosmic::iced::event::Status::Captured
                    && key == keyboard::Key::Named(Named::Escape)
                {
                    return None;
                }
                Some(Message::Key(key))
            }
            _ => None,
        });
        // Un solo tick por minuto, alineado al minuto exacto.
        let tick = Subscription::run(|| {
            cosmic::iced::stream::channel(
                1,
                |mut tx: cosmic::iced::futures::channel::mpsc::Sender<Message>| async move {
                    loop {
                        tokio::time::sleep(clock::next_minute_delay(chrono::Local::now())).await;
                        let _ = tx.try_send(Message::Tick);
                    }
                },
            )
        });
        let desk = desktops::subscription().map(Message::Desktops);
        let media = crate::media::mpris::subscription().map(Message::Media);
        let hud_volume = crate::sys::audio::subscription().map(Message::HudRaw);
        let hud_brightness = crate::sys::brightness::subscription().map(Message::HudRaw);
        let sys_battery = crate::sys::power::subscription().map(Message::SysBattery);
        let sys_net = crate::sys::network::subscription().map(Message::SysNet);
        let sys_bt = crate::sys::bluetooth::subscription().map(Message::SysBt);
        let mut subs = vec![
            keys,
            tick,
            desk,
            outputs,
            config,
            media,
            hud_volume,
            hud_brightness,
            sys_battery,
            sys_net,
            sys_bt,
            term_ui::events(),
            term_ui::shortcuts(),
        ];
        if self.term.visible() {
            subs.push(term_ui::blink());
        }
        if self.term.dragging {
            subs.push(term_ui::drag_events());
        }
        // Con la pastilla pulsada, el puntero y el botón se siguen aunque salgan de la caja.
        if self.pill_drag.is_some() {
            subs.push(cosmic::iced::event::listen_with(
                |event, _status, id| match event {
                    cosmic::iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                        Some(Message::PillDragMove(id, position.x, position.y))
                    }
                    cosmic::iced::Event::Touch(cosmic::iced::touch::Event::FingerMoved {
                        position,
                        ..
                    }) => Some(Message::PillDragMove(id, position.x, position.y)),
                    cosmic::iced::Event::Mouse(mouse::Event::ButtonReleased(
                        mouse::Button::Left,
                    ))
                    | cosmic::iced::Event::Touch(
                        cosmic::iced::touch::Event::FingerLifted { .. }
                        | cosmic::iced::touch::Event::FingerLost { .. },
                    )
                    // Sin botón y fuera de la superficie: la suelta se perdió.
                    | cosmic::iced::Event::Mouse(mouse::Event::CursorLeft) => {
                        Some(Message::PillDragEnd)
                    }
                    _ => None,
                },
            ));
        }
        if self.playing() {
            match self.state {
                IslandState::Collapsed => {
                    subs.push(cosmic::iced::time::every(TICK).map(|_| Message::BarsTick))
                }
                IslandState::Expanded { page: Page::Musica } => {
                    subs.push(cosmic::iced::time::every(TICK).map(|_| Message::LyricsTick))
                }
                IslandState::Expanded { .. } => {}
            }
        }
        // Solo mientras la pastilla se muestra por hover y el puntero ya salió: 1 s y vuelve a la línea.
        if peek::leave_timer_needed(
            self.config.hide_line,
            self.line_hover,
            self.pointer_inside,
            self.pill_drag.is_some(),
        ) {
            subs.push(cosmic::iced::time::every(Duration::from_secs(1)).map(|_| Message::LineHide));
        }
        // Solo mientras la pastilla está asomada: en reposo no se lee /proc.
        if self.peek_active() {
            subs.push(crate::sys::stats::subscription().map(Message::PeekSample));
        }
        if self.state == (IslandState::Expanded { page: Page::Musica }) {
            subs.push(
                cosmic::iced::time::every(Duration::from_secs(2)).map(|_| Message::VolumePoll),
            );
        }
        if self.state == (IslandState::Expanded { page: Page::Dev }) {
            subs.push(cosmic::iced::time::every(Duration::from_secs(3)).map(|_| Message::DevPoll));
        }
        if self.dev_enabled()
            && (!self.config.github_repos.is_empty() || !self.config.gitlab_projects.is_empty())
        {
            subs.push(cosmic::iced::time::every(Duration::from_secs(60)).map(|_| Message::GhTick));
        }
        // Solo mientras dura la animación: en reposo no hay temporizador.
        if self.anim.is_some() {
            subs.push(
                cosmic::iced::time::every(std::time::Duration::from_millis(16))
                    .map(|_| Message::AnimFrame),
            );
        }
        Subscription::batch(subs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_anchor_follows_edge() {
        assert_eq!(
            rest_anchor(Edge::Top),
            Anchor::TOP | Anchor::LEFT | Anchor::RIGHT
        );
        assert_eq!(
            rest_anchor(Edge::Bottom),
            Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT
        );
    }

    #[test]
    fn rest_zone_follows_panel_level() {
        assert_eq!(rest_zone(false), 0);
        assert_eq!(rest_zone(true), -1);
    }

    #[test]
    fn only_https_urls_open() {
        assert!(is_https_url("https://github.com/o/r/actions/runs/1"));
        assert!(!is_https_url("http://x"));
        assert!(!is_https_url("file:///etc/passwd"));
        assert!(!is_https_url("--help"));
    }

    #[test]
    fn volume_read_acceptance() {
        assert!(accept_volume_read(3, 3, false, false));
        assert!(!accept_volume_read(2, 3, false, false));
        assert!(!accept_volume_read(3, 3, true, false));
        assert!(!accept_volume_read(3, 3, false, true));
    }
    use crate::lyrics::{Lyrics, LyricsState, TrackKey};

    fn key(t: &str) -> TrackKey {
        TrackKey {
            title: t.into(),
            artist: "A".into(),
            album: String::new(),
            duration_s: None,
        }
    }

    #[test]
    fn accept_art_rejects_previous_source() {
        let a = ArtSource::File("/a.png".into());
        let b = ArtSource::File("/b.png".into());
        assert!(accept_art(Some(&b), &b));
        assert!(!accept_art(Some(&b), &a));
        assert!(!accept_art(None, &a));
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate("corto", 22), "corto");
        let t = truncate("una canción con un título larguísimo", 10);
        assert_eq!(t.chars().count(), 10);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn accept_only_current_loading() {
        let (a, b) = (key("a"), key("b"));
        let loading_b = LyricsState::Loading(b.clone());
        assert!(!accept_lyrics(&loading_b, Some(&b), &a));
        assert!(accept_lyrics(&loading_b, Some(&b), &b));
        assert!(!accept_lyrics(&LyricsState::Idle, Some(&b), &b));
        assert!(!accept_lyrics(&loading_b, None, &b));
    }

    #[test]
    fn lyrics_after_rules() {
        let (a, b) = (key("a"), key("b"));
        let loading_b = LyricsState::Loading(b.clone());
        assert_eq!(
            lyrics_after(&loading_b, Some(&b), (b.clone(), Err("x".into()))),
            LyricsState::Failed(b.clone())
        );
        assert_eq!(
            lyrics_after(&loading_b, Some(&b), (a.clone(), Ok(Lyrics::Instrumental))),
            loading_b
        );
        assert_eq!(
            lyrics_after(&loading_b, Some(&b), (b.clone(), Ok(Lyrics::Instrumental))),
            LyricsState::Ready(b, Lyrics::Instrumental)
        );
    }

    #[test]
    fn recreates_only_for_current_surface() {
        let a = window::Id::unique();
        let b = window::Id::unique();
        assert!(should_recreate(a, a, true));
        assert!(!should_recreate(a, b, true));
        assert!(!should_recreate(a, a, false));
    }
}

#[cfg(test)]
mod shelf_click_tests {
    use super::*;

    #[test]
    fn clic_tras_arrastre_se_ignora() {
        assert!(click_allowed(false, None));
        assert!(!click_allowed(true, None));
        assert!(!click_allowed(false, Some(Duration::from_millis(100))));
        assert!(click_allowed(false, Some(Duration::from_millis(400))));
    }
}
