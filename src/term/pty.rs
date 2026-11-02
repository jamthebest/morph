//! Sesión de terminal: PTY + emulador (`alacritty_terminal`).
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{ClipboardType, Config, Term, TermMode};
use alacritty_terminal::tty::{self, Options as PtyOptions, Shell};
use alacritty_terminal::vte::ansi::Color;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::grid::{self, Snapshot};
use super::palette::{Palette, resolve};

/// Eventos reducidos que la UI necesita.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TermEvent {
    /// Hay salida nueva.
    Wakeup,
    Title(String),
    Exit,
    Bell,
    ClipboardStore(String),
}

/// Modos del emulador relevantes para la UI.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode {
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub mouse_reporting: bool,
    pub alt_screen: bool,
    pub alternate_scroll: bool,
}

/// Listener de `alacritty_terminal`: reenvía a un canal tokio sin bloquear.
#[derive(Clone)]
struct Proxy {
    tx: UnboundedSender<TermEvent>,
    sender: Arc<OnceLock<EventLoopSender>>,
    /// Ya hay un `Wakeup` pendiente de consumir (coalescencia).
    wake_pending: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    palette: Palette,
    win: Arc<Mutex<WindowSize>>,
}

impl Proxy {
    fn emit(&self, ev: TermEvent) {
        let _ = self.tx.send(ev);
    }

    fn pty_write(&self, text: String) {
        if let Some(s) = self.sender.get() {
            let _ = s.send(Msg::Input(Cow::Owned(text.into_bytes())));
        }
    }
}

impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => {
                if !self.wake_pending.swap(true, Ordering::AcqRel) {
                    self.emit(TermEvent::Wakeup);
                }
            }
            Event::Title(t) => self.emit(TermEvent::Title(t)),
            Event::Bell => self.emit(TermEvent::Bell),
            // Solo el portapapeles (no la selección primaria), como alacritty.
            Event::ClipboardStore(ClipboardType::Clipboard, text) => {
                self.emit(TermEvent::ClipboardStore(text))
            }
            Event::PtyWrite(text) => self.pty_write(text),
            Event::ColorRequest(index, format) => {
                let color = match index {
                    0..=255 => resolve(Color::Indexed(index as u8), &self.palette, true),
                    257 => self.palette.bg,
                    _ => self.palette.fg,
                };
                self.pty_write(format(color));
            }
            Event::TextAreaSizeRequest(format) => {
                let ws = *self.win.lock().unwrap_or_else(|e| e.into_inner());
                self.pty_write(format(ws));
            }
            Event::Exit | Event::ChildExit(_) if !self.exited.swap(true, Ordering::AcqRel) => {
                self.emit(TermEvent::Exit);
            }
            _ => {}
        }
    }
}

struct Size {
    cols: usize,
    rows: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

fn window_size(cols: u16, rows: u16, cell_px: (f32, f32)) -> WindowSize {
    WindowSize {
        num_lines: rows.max(1),
        num_cols: cols.max(1),
        cell_width: cell_px.0.round().max(1.0) as u16,
        cell_height: cell_px.1.round().max(1.0) as u16,
    }
}

pub struct Session {
    term: Arc<FairMutex<Term<Proxy>>>,
    sender: EventLoopSender,
    wake_pending: Arc<AtomicBool>,
    palette: Palette,
    win: Arc<Mutex<WindowSize>>,
    pid: u32,
}

impl Session {
    /// Lanza `shell` (o `$SHELL`/`/bin/bash` como login shell si es `None`) en
    /// `cwd`. Devuelve la sesión y el canal de eventos.
    pub fn spawn(
        shell: Option<&str>,
        args: &[&str],
        cwd: &Path,
        cols: u16,
        rows: u16,
        cell_px: (f32, f32),
    ) -> Result<(Session, UnboundedReceiver<TermEvent>), String> {
        let (program, args): (String, Vec<String>) = match shell {
            Some(p) => (p.to_string(), args.iter().map(|a| a.to_string()).collect()),
            None => {
                let p = std::env::var("SHELL")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "/bin/bash".into());
                let a = if args.is_empty() {
                    vec!["-l".to_string()]
                } else {
                    args.iter().map(|a| a.to_string()).collect()
                };
                (p, a)
            }
        };
        let mut env = HashMap::new();
        env.insert("TERM".to_string(), "xterm-256color".to_string());
        env.insert("COLORTERM".to_string(), "truecolor".to_string());
        let opts = PtyOptions {
            shell: Some(Shell::new(program, args)),
            working_directory: Some(cwd.to_path_buf()),
            drain_on_exit: true,
            env,
        };
        let ws = window_size(cols, rows, cell_px);
        let pty = tty::new(&opts, ws, 0).map_err(|e| format!("no se pudo abrir la PTY: {e}"))?;

        let pid = pty.child().id();
        let (tx, rx) = unbounded_channel();
        let wake_pending = Arc::new(AtomicBool::new(false));
        let proxy = Proxy {
            tx,
            sender: Arc::new(OnceLock::new()),
            wake_pending: wake_pending.clone(),
            exited: Arc::new(AtomicBool::new(false)),
            palette: Palette::default(),
            win: Arc::new(Mutex::new(ws)),
        };
        let win = proxy.win.clone();
        let size = Size {
            cols: usize::from(ws.num_cols),
            rows: usize::from(ws.num_lines),
        };
        let term = Arc::new(FairMutex::new(Term::new(
            Config::default(),
            &size,
            proxy.clone(),
        )));
        let event_loop = EventLoop::new(term.clone(), proxy.clone(), pty, true, false)
            .map_err(|e| format!("no se pudo iniciar el bucle de la PTY: {e}"))?;
        let sender = event_loop.channel();
        let _ = proxy.sender.set(sender.clone());
        // El hilo del bucle se detiene con `Msg::Shutdown` (ver `Drop`); no se
        // guarda su `JoinHandle` (soltarlo lo desacopla).
        drop(event_loop.spawn());
        Ok((
            Session {
                term,
                sender,
                wake_pending,
                palette: Palette::default(),
                win,
                pid,
            },
            rx,
        ))
    }

    pub fn write(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let _ = self.sender.send(Msg::Input(Cow::Owned(bytes.to_vec())));
    }

    /// Pega texto; con bracketed paste activo lo envuelve en `ESC[200~ … ESC[201~`.
    pub fn paste(&self, text: &str) {
        if self.mode().bracketed_paste {
            // Sin ESC dentro: evita cerrar el pegado antes de tiempo.
            let clean = text.replace('\x1b', "");
            let mut out = b"\x1b[200~".to_vec();
            out.extend_from_slice(clean.as_bytes());
            out.extend_from_slice(b"\x1b[201~");
            self.write(&out);
        } else {
            self.write(text.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
        }
    }

    pub fn resize(&self, cols: u16, rows: u16, cell_px: (f32, f32)) {
        let ws = window_size(cols, rows, cell_px);
        *self.win.lock().unwrap_or_else(|e| e.into_inner()) = ws;
        self.term.lock().resize(Size {
            cols: usize::from(ws.num_cols),
            rows: usize::from(ws.num_lines),
        });
        let _ = self.sender.send(Msg::Resize(ws));
    }

    /// Desplaza el historial: positivo hacia atrás (arriba), negativo hacia abajo.
    /// No emite `Wakeup`: quien llama debe redibujar.
    pub fn scroll(&self, lines: i32) {
        self.term.lock().scroll_display(Scroll::Delta(lines));
    }

    /// Vuelve al final del historial (al teclear o pegar con el historial desplazado).
    pub fn scroll_to_bottom(&self) {
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// Directorio actual del shell (`/proc/<pid>/cwd`), si se puede leer.
    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        std::fs::read_link(format!("/proc/{}/cwd", self.pid)).ok()
    }

    pub fn snapshot(&self) -> Snapshot {
        // Se re-arma el aviso: el próximo cambio volverá a emitir `Wakeup`.
        self.wake_pending.store(false, Ordering::Release);
        let term = self.term.lock();
        grid::build(&term, &self.palette)
    }

    pub fn mode(&self) -> Mode {
        let m = *self.term.lock().mode();
        Mode {
            app_cursor: m.contains(TermMode::APP_CURSOR),
            bracketed_paste: m.contains(TermMode::BRACKETED_PASTE),
            mouse_reporting: m.intersects(TermMode::MOUSE_MODE),
            alt_screen: m.contains(TermMode::ALT_SCREEN),
            alternate_scroll: m.contains(TermMode::ALTERNATE_SCROLL),
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.sender.send(Msg::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn spawn_sh(script: &str) -> (Session, UnboundedReceiver<TermEvent>) {
        Session::spawn(
            Some("/bin/sh"),
            &["-c", script],
            Path::new("/"),
            40,
            10,
            (8.0, 16.0),
        )
        .expect("spawn")
    }

    /// Consume eventos hasta que `done` sea cierto o venza el plazo.
    fn pump(
        rx: &mut UnboundedReceiver<TermEvent>,
        mut done: impl FnMut(&TermEvent) -> bool,
    ) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            match rx.try_recv() {
                Ok(ev) => {
                    if done(&ev) {
                        return true;
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        false
    }

    fn wait_for(s: &Session, pred: impl Fn(&Snapshot) -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if pred(&s.snapshot()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn salida_con_color_y_exit() {
        let (s, mut rx) = spawn_sh(r#"printf '\033[31mhola\033[0m\n'"#);
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit), "sin Exit");
        let snap = s.snapshot();
        let span = snap
            .lines
            .iter()
            .flatten()
            .find(|sp| sp.text.contains("hola"))
            .expect("hola en la instantánea");
        assert_eq!(span.fg, super::super::palette::Palette::default().ansi[1]);
    }

    #[test]
    fn paste_con_bracketed_paste() {
        let (s, mut rx) = spawn_sh(r#"printf '\033[?2004h'; cat -v"#);
        assert!(wait_for(&s, |_| s.mode().bracketed_paste), "modo no activo");
        s.paste("abc");
        assert!(
            wait_for(&s, |sn| sn.line_text(0).contains("^[[200~abc^[[201~")),
            "{:?}",
            s.snapshot().line_text(0)
        );
        s.write(&[0x04, 0x04]); // EOF (dos: la entrada no terminó en salto de línea)
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit));
    }

    #[test]
    fn paste_sin_bracketed_es_texto_plano() {
        let (s, mut rx) = spawn_sh("cat -v");
        s.paste("xyz");
        assert!(wait_for(&s, |sn| sn.line_text(0).contains("xyz")));
        assert!(!s.snapshot().line_text(0).contains("200~"));
        s.write(&[0x04, 0x04]);
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit));
    }

    #[test]
    fn resize_cambia_la_instantanea() {
        let (s, mut rx) = spawn_sh("cat");
        assert_eq!((s.snapshot().cols, s.snapshot().rows), (40, 10));
        s.resize(60, 20, (8.0, 16.0));
        let snap = s.snapshot();
        assert_eq!((snap.cols, snap.rows), (60, 20));
        assert_eq!(snap.lines.len(), 20);
        s.write(&[0x04]);
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit));
    }

    #[test]
    fn titulo_y_campana() {
        let (_s, mut rx) = spawn_sh(r#"printf '\033]0;mi titulo\007\007'"#);
        let mut title = false;
        let mut bell = false;
        pump(&mut rx, |e| {
            match e {
                TermEvent::Title(t) if t == "mi titulo" => title = true,
                TermEvent::Bell => bell = true,
                _ => {}
            }
            title && bell
        });
        assert!(title && bell, "title={title} bell={bell}");
    }

    #[test]
    fn historial_y_scroll() {
        let (s, mut rx) = spawn_sh("i=1; while [ $i -le 40 ]; do echo linea$i; i=$((i+1)); done");
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit));
        assert_eq!(s.snapshot().display_offset, 0);
        s.scroll(5);
        assert_eq!(s.snapshot().display_offset, 5);
        s.scroll(-100);
        assert_eq!(s.snapshot().display_offset, 0);
        s.scroll(7);
        s.scroll_to_bottom();
        assert_eq!(s.snapshot().display_offset, 0);
    }

    #[test]
    fn cwd_del_shell() {
        let (s, _rx) = spawn_sh("sleep 2");
        assert_eq!(s.cwd().as_deref(), Some(Path::new("/")));
    }

    #[test]
    fn span_nuevo_tras_caracter_ancho() {
        let (s, mut rx) = spawn_sh("printf 'a日b'");
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit));
        let snap = s.snapshot();
        let texts: Vec<_> = snap.lines[0].iter().map(|sp| sp.text.as_str()).collect();
        assert_eq!(texts[0], "a日", "{texts:?}");
        assert!(texts[1].starts_with('b'), "{texts:?}");
        assert_eq!(snap.lines[0][0].cells, 3);
    }

    #[test]
    fn responde_consulta_de_color() {
        let (s, mut rx) = spawn_sh(r#"printf '\033]11;?\007'; cat -v"#);
        assert!(
            wait_for(&s, |sn| sn.line_text(0).contains("rgb:0000/0000/0000")),
            "{:?}",
            s.snapshot().line_text(0)
        );
        s.write(&[0x04, 0x04]);
        assert!(pump(&mut rx, |e| *e == TermEvent::Exit));
    }

    #[test]
    fn invalido_devuelve_error() {
        let r = Session::spawn(
            Some("/no/existe/shell"),
            &[],
            Path::new("/"),
            10,
            5,
            (8.0, 16.0),
        );
        assert!(r.is_err());
    }
}
