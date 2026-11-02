//! Hilo Wayland (cctk): workspaces ext_workspace_v1 + toplevels cosmic/foreign.
//! Patrón de cosmic-applet-workspaces y cosmic-app-list.

use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use cosmic::cctk::cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::State as ToplevelState;
use cosmic::cctk::cosmic_protocols::toplevel_management::v1::client::zcosmic_toplevel_manager_v1::ZcosmicToplelevelManagementCapabilitiesV1 as Cap;
use cosmic::cctk::{
    self,
    sctk::{
        self,
        output::{OutputHandler, OutputState},
        reexports::{
            calloop::{self, channel::Channel},
            calloop_wayland_source::WaylandSource,
            protocols::ext::{
                foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
                workspace::v1::client::ext_workspace_handle_v1::{
                    ExtWorkspaceHandleV1, State as WsState,
                },
            },
        },
        registry::{ProvidesRegistryState, RegistryState},
        seat::{SeatHandler, SeatState},
    },
    toplevel_info::{ToplevelInfoHandler, ToplevelInfoState},
    toplevel_management::{ToplevelManagerHandler, ToplevelManagerState},
    wayland_client::{
        Connection, QueueHandle, WEnum,
        globals::registry_queue_init,
        protocol::{wl_output::WlOutput, wl_seat::WlSeat},
    },
    workspace::{WorkspaceHandler, WorkspaceState},
};
use cosmic::iced::futures::{SinkExt, StreamExt, channel::mpsc};
use cosmic::iced::{Subscription, stream};

use super::{
    DesktopsCmd, DesktopsEvent, Snapshot, TITLES_WANTED, Window, Workspace, number_by_output,
    significant_change, sort_group,
};

fn debug() -> bool {
    static D: OnceLock<bool> = OnceLock::new();
    *D.get_or_init(|| crate::envcompat::var_os("MORPH_DEBUG").is_some())
}

static CMD_TX: OnceLock<calloop::channel::Sender<DesktopsCmd>> = OnceLock::new();

/// Envía un comando al hilo Wayland (se ignora si aún no arrancó).
pub fn send(cmd: DesktopsCmd) {
    if let Some(tx) = CMD_TX.get() {
        let _ = tx.send(cmd);
    }
}

pub fn subscription() -> Subscription<DesktopsEvent> {
    Subscription::run(|| {
        stream::channel(16, |mut out: mpsc::Sender<DesktopsEvent>| async move {
            let (ev_tx, mut ev_rx) = mpsc::unbounded::<DesktopsEvent>();
            let (cmd_tx, cmd_rx) = calloop::channel::channel::<DesktopsCmd>();
            let _ = CMD_TX.set(cmd_tx);
            std::thread::spawn(move || run(ev_tx, cmd_rx));
            while let Some(ev) = ev_rx.next().await {
                if out.send(ev).await.is_err() {
                    break;
                }
            }
            cosmic::iced::futures::future::pending::<()>().await;
        })
    })
}

struct State {
    exit: bool,
    tx: mpsc::UnboundedSender<DesktopsEvent>,
    output_state: OutputState,
    registry_state: RegistryState,
    seat_state: SeatState,
    workspace_state: WorkspaceState,
    toplevel_info_state: ToplevelInfoState,
    toplevel_manager_state: ToplevelManagerState,
    /// id (índice) -> manejador; nunca se compacta para que los ids sigan estables.
    ws_handles: Vec<ExtWorkspaceHandleV1>,
    win_handles: Vec<ExtForeignToplevelHandleV1>,
    /// Último snapshot publicado, para no repetir.
    last: Option<Snapshot>,
}

fn id_of<T: PartialEq + Clone>(list: &mut Vec<T>, item: &T) -> u32 {
    match list.iter().position(|x| x == item) {
        Some(i) => i as u32,
        None => {
            list.push(item.clone());
            (list.len() - 1) as u32
        }
    }
}

impl State {
    fn snapshot(&mut self) -> Snapshot {
        let mut workspaces = Vec::new();
        let mut infos = Vec::new();
        for g in self.workspace_state.workspace_groups() {
            let output = g
                .outputs
                .first()
                .and_then(|o| self.output_state.info(o))
                .and_then(|i| i.name);
            let mut group: Vec<_> = g
                .workspaces
                .iter()
                .filter_map(|h| self.workspace_state.workspace_info(h))
                .map(|w| {
                    (
                        w.coordinates.clone(),
                        w.handle.clone(),
                        w.name.clone(),
                        w.state.contains(WsState::Active),
                        output.clone(),
                    )
                })
                .collect();
            sort_group(&mut group, |t| (t.0.as_slice(), t.2.as_str()));
            infos.extend(group.into_iter().map(|(_, h, n, a, o)| (h, n, a, o)));
        }
        for (handle, name, active, output) in infos {
            let id = id_of(&mut self.ws_handles, &handle);
            workspaces.push(Workspace {
                id,
                name,
                active,
                output,
                index: 0,
            });
        }
        number_by_output(&mut workspaces);
        let tops: Vec<_> = self
            .toplevel_info_state
            .toplevels()
            .map(|t| {
                (
                    t.foreign_toplevel.clone(),
                    t.app_id.clone(),
                    t.title.clone(),
                    t.workspace.iter().next().cloned(),
                    t.state.contains(&ToplevelState::Activated),
                )
            })
            .collect();
        let mut windows = Vec::new();
        for (handle, app_id, title, ws, active) in tops {
            let id = id_of(&mut self.win_handles, &handle);
            let workspace = ws.map(|w| id_of(&mut self.ws_handles, &w));
            windows.push(Window {
                id,
                app_id,
                title,
                workspace,
                active,
            });
        }
        Snapshot {
            workspaces,
            windows,
        }
    }

    fn publish(&mut self) {
        let snap = self.snapshot();
        let changed = match &self.last {
            None => true,
            Some(last) if *last == snap => false,
            Some(last) => significant_change(last, &snap) || TITLES_WANTED.load(Ordering::Relaxed),
        };
        if !changed {
            return;
        }
        if debug() {
            eprintln!(
                "[morph::desktops] snapshot: escritorios={:?} ventanas={:?}",
                snap.workspaces
                    .iter()
                    .map(|w| format!(
                        "{}:{}@{}#{}{}",
                        w.id,
                        w.name,
                        w.output.as_deref().unwrap_or("?"),
                        w.index,
                        if w.active { "*" } else { "" }
                    ))
                    .collect::<Vec<_>>(),
                snap.windows
                    .iter()
                    .map(|w| format!(
                        "{}:{}@{:?}{}",
                        w.id,
                        w.app_id,
                        w.workspace,
                        if w.active { "*" } else { "" }
                    ))
                    .collect::<Vec<_>>()
            );
        }
        self.last = Some(snap.clone());
        let _ = self.tx.unbounded_send(DesktopsEvent::Snapshot(snap));
    }

    fn handle_cmd(&mut self, cmd: DesktopsCmd) {
        match cmd {
            DesktopsCmd::ActivateWorkspace(id) => {
                if let Some(h) = self.ws_handles.get(id as usize) {
                    h.activate();
                    if let Ok(m) = self.workspace_state.workspace_manager().get() {
                        m.commit();
                    }
                }
            }
            DesktopsCmd::ActivateWindow(id) => {
                let Some(cosmic_top) = self.cosmic_toplevel(id) else {
                    return;
                };
                if let Some(seat) = self.seat_state.seats().next() {
                    self.toplevel_manager_state
                        .manager
                        .activate(&cosmic_top, &seat);
                }
            }
            DesktopsCmd::Refresh => {
                // Publica aunque solo cambien títulos (TITLES_WANTED acaba de activarse).
                self.publish();
            }
        }
    }

    fn cosmic_toplevel(
        &self,
        id: u32,
    ) -> Option<cctk::cosmic_protocols::toplevel_info::v1::client::zcosmic_toplevel_handle_v1::ZcosmicToplevelHandleV1>
    {
        let h = self.win_handles.get(id as usize)?;
        self.toplevel_info_state.info(h)?.cosmic_toplevel.clone()
    }
}

fn run(tx: mpsc::UnboundedSender<DesktopsEvent>, rx: Channel<DesktopsCmd>) {
    // No se usa X_PRIVILEGED_WAYLAND_SOCKET: solo lo define el compositor para sus
    // hijos privilegiados y en otros entornos apunta a un fd inválido.
    let conn = Connection::connect_to_env();
    let Ok(conn) = conn else {
        eprintln!("[morph::desktops] sin conexión Wayland");
        return;
    };
    let (globals, event_queue) = match registry_queue_init::<State>(&conn) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("[morph::desktops] registry_queue_init falló: {e}");
            return;
        }
    };
    let qh = event_queue.handle();
    let mut event_loop = calloop::EventLoop::<State>::try_new().unwrap();
    let handle = event_loop.handle();
    WaylandSource::new(conn, event_queue)
        .insert(handle.clone())
        .unwrap();
    handle
        .insert_source(rx, |e, (), state| match e {
            calloop::channel::Event::Msg(cmd) => state.handle_cmd(cmd),
            calloop::channel::Event::Closed => state.exit = true,
        })
        .unwrap();

    let registry_state = RegistryState::new(&globals);
    let (Some(info), Some(manager)) = (
        ToplevelInfoState::try_new(&registry_state, &qh),
        ToplevelManagerState::try_new(&registry_state, &qh),
    ) else {
        eprintln!("[morph::desktops] el compositor no ofrece los protocolos de toplevel de COSMIC");
        return;
    };
    let mut state = State {
        exit: false,
        tx,
        // Debe ir antes que WorkspaceState: los outputs deben estar enlazados.
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        workspace_state: WorkspaceState::new(&registry_state, &qh),
        toplevel_info_state: info,
        toplevel_manager_state: manager,
        registry_state,
        ws_handles: Vec::new(),
        win_handles: Vec::new(),
        last: None,
    };
    while !state.exit {
        if let Err(e) = event_loop.dispatch(None, &mut state) {
            eprintln!("[morph::desktops] error de dispatch, hilo termina: {e}");
            break;
        }
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    sctk::registry_handlers![OutputState,];
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlOutput) {}
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: sctk::seat::Capability,
    ) {
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: WlSeat,
        _: sctk::seat::Capability,
    ) {
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: WlSeat) {}
}

impl WorkspaceHandler for State {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_state
    }
    fn done(&mut self) {
        self.publish();
    }
}

impl ToplevelInfoHandler for State {
    fn toplevel_info_state(&mut self) -> &mut ToplevelInfoState {
        &mut self.toplevel_info_state
    }
    fn new_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ExtForeignToplevelHandleV1,
    ) {
    }
    fn update_toplevel(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ExtForeignToplevelHandleV1,
    ) {
    }
    fn toplevel_closed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &ExtForeignToplevelHandleV1,
    ) {
    }
    fn info_done(&mut self, _: &Connection, _: &QueueHandle<Self>) {
        self.publish();
    }
}

impl ToplevelManagerHandler for State {
    fn toplevel_manager_state(&mut self) -> &mut ToplevelManagerState {
        &mut self.toplevel_manager_state
    }
    fn capabilities(&mut self, _: &Connection, _: &QueueHandle<Self>, _: Vec<WEnum<Cap>>) {}
}

sctk::delegate_output!(State);
sctk::delegate_registry!(State);
sctk::delegate_seat!(State);
cctk::delegate_workspace!(State);
cctk::delegate_toplevel_info!(State);
cctk::delegate_toplevel_manager!(State);
