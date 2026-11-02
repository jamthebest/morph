//! Widget de terminal: dibuja una `Snapshot` y traduce eventos a mensajes.
//!
//! No es dueño de la `Session`: informa por callbacks (`on_input`, `on_copy`,
//! `on_scroll`, `on_resize`, `on_select`…) y la app los aplica. Limitación: las
//! operaciones de texto (URL, palabra, copiar) asumen un carácter por columna,
//! así que con caracteres anchos en la misma línea pueden desplazarse; el dibujo
//! sí usa `Span::cells`.
use cosmic::iced::Font;
use cosmic::iced::core::mouse::click::{Click, Kind};
use cosmic::iced::core::text::{self, LineHeight, Shaping, Wrapping};
use cosmic::iced::core::widget::{Tree, tree};
use cosmic::iced::core::{
    Border, Clipboard, Color, Element, Event, Layout, Length, Pixels, Point, Rectangle, Shell,
    Size, Widget, keyboard, layout, mouse, renderer,
};

use super::font::mono_font;
use super::grid::{CursorShape, Snapshot};
use super::keys;
use super::palette::Palette;
use super::pty::Mode;

pub type Cell = (u16, u16);

/// Columnas y filas que caben en `w`×`h` px (mínimo 1×1).
pub fn grid_size(w: f32, h: f32, cell: (f32, f32)) -> (u16, u16) {
    if cell.0 <= 0.0 || cell.1 <= 0.0 {
        return (1, 1);
    }
    let n = |len: f32, c: f32| ((len / c).floor().max(1.0)).min(f32::from(u16::MAX)) as u16;
    (n(w, cell.0), n(h, cell.1))
}

/// Celda bajo un punto relativo al widget (ajustada a la rejilla).
pub fn point_to_cell(x: f32, y: f32, cell: (f32, f32), cols: u16, rows: u16) -> Cell {
    let idx = |v: f32, c: f32, n: u16| {
        if c <= 0.0 || v <= 0.0 {
            0
        } else {
            ((v / c) as u32).min(u32::from(n.max(1)) - 1) as u16
        }
    };
    (idx(x, cell.0, cols), idx(y, cell.1, rows))
}

/// URL (`http(s)://…`) que cubre la columna `col` de una línea.
pub fn url_at(line: &str, col: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let stop = |c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '<' | '>' | ')');
    let mut i = 0;
    while i < chars.len() {
        let rest: String = chars[i..chars.len().min(i + 8)].iter().collect();
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let mut end = i;
            while end < chars.len() && !stop(chars[end]) {
                end += 1;
            }
            while end > i && matches!(chars[end - 1], '.' | ',' | ';' | ':' | '!' | '?') {
                end -= 1;
            }
            if (i..end).contains(&col) {
                return Some(chars[i..end].iter().collect());
            }
            i = end.max(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

/// Rango de columnas `[ini, fin]` de la palabra bajo `col`.
pub fn word_range(line: &str, col: usize) -> (usize, usize) {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return (col, col);
    }
    let col = col.min(chars.len() - 1);
    let class = |c: char| {
        if c.is_whitespace() {
            0
        } else if matches!(
            c,
            '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '|' | ',' | ';'
        ) {
            1
        } else {
            2
        }
    };
    let k = class(chars[col]);
    let mut a = col;
    while a > 0 && class(chars[a - 1]) == k {
        a -= 1;
    }
    let mut b = col;
    while b + 1 < chars.len() && class(chars[b + 1]) == k {
        b += 1;
    }
    (a, b)
}

/// Texto seleccionado (líneas unidas con `\n`, sin espacios finales).
pub fn selected_text(snap: &Snapshot, a: Cell, b: Cell) -> String {
    let (start, end) = if (a.1, a.0) <= (b.1, b.0) {
        (a, b)
    } else {
        (b, a)
    };
    let mut out: Vec<String> = Vec::new();
    for row in start.1..=end.1 {
        let text = snap.line_text(usize::from(row));
        let from = if row == start.1 {
            usize::from(start.0)
        } else {
            0
        };
        let take = if row == end.1 {
            (usize::from(end.0) + 1).saturating_sub(from)
        } else {
            usize::MAX
        };
        let piece: String = text.chars().skip(from).take(take).collect();
        out.push(piece.trim_end().to_string());
    }
    out.join("\n")
}

/// Líneas por «muesca» de la rueda.
const WHEEL_LINES: i32 = 3;

type Cb<'a, A, M> = Option<Box<dyn Fn(A) -> M + 'a>>;

pub struct TermView<'a, Message> {
    snapshot: &'a Snapshot,
    cell: (f32, f32),
    font_size: f32,
    mode: Mode,
    focused: bool,
    cursor_on: bool,
    selection: Option<(Cell, Cell)>,
    fg: Color,
    bg: Color,
    on_input: Cb<'a, Vec<u8>, Message>,
    on_paste_request: Option<Box<dyn Fn() -> Message + 'a>>,
    on_copy: Cb<'a, String, Message>,
    on_scroll: Cb<'a, i32, Message>,
    on_resize: Option<Box<dyn Fn(u16, u16) -> Message + 'a>>,
    on_select: Option<Box<dyn Fn(Cell, Cell) -> Message + 'a>>,
    on_select_clear: Option<Box<dyn Fn() -> Message + 'a>>,
    on_open_url: Cb<'a, String, Message>,
}

fn color(c: alacritty_terminal::vte::ansi::Rgb) -> Color {
    Color::from_rgb8(c.r, c.g, c.b)
}

impl<'a, Message> TermView<'a, Message> {
    pub fn new(snapshot: &'a Snapshot, cell: (f32, f32), font_size: f32, mode: Mode) -> Self {
        let palette = Palette::default();
        Self {
            snapshot,
            cell,
            font_size,
            mode,
            focused: true,
            cursor_on: true,
            selection: snapshot.selection,
            fg: color(palette.fg),
            bg: color(palette.bg),
            on_input: None,
            on_paste_request: None,
            on_copy: None,
            on_scroll: None,
            on_resize: None,
            on_select: None,
            on_select_clear: None,
            on_open_url: None,
        }
    }

    /// Terminal con el foco: recibe teclado y dibuja el cursor relleno.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Fase de parpadeo del cursor (`false` = oculto).
    pub fn cursor_visible(mut self, on: bool) -> Self {
        self.cursor_on = on;
        self
    }

    /// Selección a pintar (por defecto la de la instantánea).
    pub fn selection(mut self, sel: Option<(Cell, Cell)>) -> Self {
        self.selection = sel;
        self
    }

    pub fn on_input(mut self, f: impl Fn(Vec<u8>) -> Message + 'a) -> Self {
        self.on_input = Some(Box::new(f));
        self
    }

    pub fn on_paste_request(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_paste_request = Some(Box::new(f));
        self
    }

    pub fn on_copy(mut self, f: impl Fn(String) -> Message + 'a) -> Self {
        self.on_copy = Some(Box::new(f));
        self
    }

    /// Positivo = hacia atrás en el historial.
    pub fn on_scroll(mut self, f: impl Fn(i32) -> Message + 'a) -> Self {
        self.on_scroll = Some(Box::new(f));
        self
    }

    pub fn on_resize(mut self, f: impl Fn(u16, u16) -> Message + 'a) -> Self {
        self.on_resize = Some(Box::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(Cell, Cell) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(f));
        self
    }

    pub fn on_select_clear(mut self, f: impl Fn() -> Message + 'a) -> Self {
        self.on_select_clear = Some(Box::new(f));
        self
    }

    pub fn on_open_url(mut self, f: impl Fn(String) -> Message + 'a) -> Self {
        self.on_open_url = Some(Box::new(f));
        self
    }

    fn input(&self, shell: &mut Shell<'_, Message>, bytes: Vec<u8>) {
        if let Some(f) = &self.on_input {
            shell.publish(f(bytes));
        }
    }

    fn cell_at(&self, bounds: Rectangle, p: Point) -> Cell {
        point_to_cell(
            p.x - bounds.x,
            p.y - bounds.y,
            self.cell,
            self.snapshot.cols,
            self.snapshot.rows,
        )
    }
}

#[derive(Default)]
struct State {
    anchor: Option<Cell>,
    last_click: Option<Click>,
    last_size: Option<(u16, u16)>,
    mods: keyboard::Modifiers,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for TermView<'_, Message>
where
    Renderer: text::Renderer<Font = Font>,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn layout(&mut self, _tree: &mut Tree, _r: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        let size = grid_size(bounds.width, bounds.height, self.cell);
        if state.last_size != Some(size) {
            state.last_size = Some(size);
            if let Some(f) = &self.on_resize {
                shell.publish(f(size.0, size.1));
            }
        }

        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => state.mods = *m,
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if self.focused => {
                if modifiers.control()
                    && modifiers.shift()
                    && let keyboard::Key::Character(c) = key
                {
                    match c.to_ascii_lowercase().as_str() {
                        "c" => {
                            if let (Some(f), Some((a, b))) = (&self.on_copy, self.selection) {
                                let t = selected_text(self.snapshot, a, b);
                                if !t.is_empty() {
                                    shell.publish(f(t));
                                }
                            }
                            shell.capture_event();
                            return;
                        }
                        "v" => {
                            if let Some(f) = &self.on_paste_request {
                                shell.publish(f());
                            }
                            shell.capture_event();
                            return;
                        }
                        _ => {}
                    }
                }
                if let Some(bytes) =
                    keys::encode(key, *modifiers, text.as_deref(), self.mode.app_cursor)
                {
                    self.input(shell, bytes);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let notches = match *delta {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / self.cell.1.max(1.0),
                };
                let n = if notches > 0.0 {
                    (notches.round() as i32).clamp(1, 50)
                } else if notches < 0.0 {
                    (notches.round() as i32).clamp(-50, -1)
                } else {
                    0
                };
                if n == 0 {
                    return;
                }
                let lines = n * WHEEL_LINES;
                if self.mode.alt_screen && (self.mode.mouse_reporting || self.mode.alternate_scroll)
                {
                    let key = keyboard::Key::Named(if lines > 0 {
                        keyboard::key::Named::ArrowUp
                    } else {
                        keyboard::key::Named::ArrowDown
                    });
                    if let Some(one) = keys::encode(
                        &key,
                        keyboard::Modifiers::empty(),
                        None,
                        self.mode.app_cursor,
                    ) {
                        self.input(shell, one.repeat(lines.unsigned_abs() as usize));
                    }
                } else if let Some(f) = &self.on_scroll {
                    shell.publish(f(lines));
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_over(bounds) else {
                    return;
                };
                let (col, row) = self.cell_at(bounds, p);
                let click = Click::new(p, mouse::Button::Left, state.last_click);
                state.last_click = Some(click);
                let line = self.snapshot.line_text(usize::from(row));
                if state.mods.control()
                    && let (Some(f), Some(url)) =
                        (&self.on_open_url, url_at(&line, usize::from(col)))
                {
                    shell.publish(f(url));
                    shell.capture_event();
                    return;
                }
                match click.kind() {
                    Kind::Single => {
                        state.anchor = Some((col, row));
                        if let Some(f) = &self.on_select_clear {
                            shell.publish(f());
                        }
                    }
                    Kind::Double => {
                        state.anchor = None;
                        let (a, b) = word_range(&line, usize::from(col));
                        if let Some(f) = &self.on_select {
                            shell.publish(f((a as u16, row), (b as u16, row)));
                        }
                    }
                    Kind::Triple => {
                        state.anchor = None;
                        if let Some(f) = &self.on_select {
                            shell.publish(f((0, row), (self.snapshot.cols.saturating_sub(1), row)));
                        }
                    }
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(anchor) = state.anchor {
                    let cell = self.cell_at(bounds, *position);
                    if cell != anchor
                        && let Some(f) = &self.on_select
                    {
                        shell.publish(f(anchor, cell));
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.anchor = None;
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let (cw, ch) = self.cell;
        let rect = |col: f32, row: f32, w: f32, h: f32| Rectangle {
            x: bounds.x + col * cw,
            y: bounds.y + row * ch,
            width: w,
            height: h,
        };
        let quad = |renderer: &mut Renderer, r: Rectangle, c: Color| {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: r,
                    ..Default::default()
                },
                c,
            );
        };
        let draw_text = |renderer: &mut Renderer,
                         content: &str,
                         r: Rectangle,
                         c: Color,
                         bold: bool,
                         italic: bool| {
            let mut font = mono_font();
            if bold {
                font.weight = cosmic::iced::font::Weight::Bold;
            }
            if italic {
                font.style = cosmic::iced::font::Style::Italic;
            }
            renderer.fill_text(
                text::Text {
                    content: content.to_string(),
                    bounds: Size::new(r.width.max(cw), ch),
                    size: Pixels(self.font_size),
                    line_height: LineHeight::Absolute(Pixels(ch)),
                    font,
                    align_x: text::Alignment::Left,
                    align_y: cosmic::iced::alignment::Vertical::Top,
                    shaping: if content.is_ascii() {
                        Shaping::Basic
                    } else {
                        Shaping::Advanced
                    },
                    wrapping: Wrapping::None,
                    ellipsize: text::Ellipsize::None,
                },
                Point::new(r.x, r.y),
                c,
                bounds,
            );
        };

        renderer.with_layer(bounds, |renderer| {
            quad(renderer, bounds, self.bg);

            // Fondos por tramos (se omite el fondo por defecto).
            for (row, line) in self.snapshot.lines.iter().enumerate() {
                let mut col = 0u16;
                for span in line {
                    let bg = color(span.bg);
                    if bg != self.bg {
                        quad(
                            renderer,
                            rect(f32::from(col), row as f32, f32::from(span.cells) * cw, ch),
                            bg,
                        );
                    }
                    col += span.cells;
                }
            }

            // Selección (superposición translúcida).
            if let Some((a, b)) = self.selection {
                let (s, e) = if (a.1, a.0) <= (b.1, b.0) {
                    (a, b)
                } else {
                    (b, a)
                };
                let last = self.snapshot.cols.saturating_sub(1);
                for row in s.1..=e.1.min(self.snapshot.rows.saturating_sub(1)) {
                    let c0 = if row == s.1 { s.0 } else { 0 };
                    let c1 = if row == e.1 { e.0 } else { last };
                    quad(
                        renderer,
                        rect(
                            f32::from(c0),
                            f32::from(row),
                            f32::from(c1 - c0 + 1) * cw,
                            ch,
                        ),
                        Color::from_rgba(0.3, 0.55, 1.0, 0.35),
                    );
                }
            }

            // Texto por tramos.
            for (row, line) in self.snapshot.lines.iter().enumerate() {
                let mut col = 0u16;
                for span in line {
                    let w = f32::from(span.cells) * cw;
                    let r = rect(f32::from(col), row as f32, w, ch);
                    if !span.text.trim().is_empty() {
                        draw_text(
                            renderer,
                            &span.text,
                            r,
                            color(span.fg),
                            span.bold,
                            span.italic,
                        );
                    }
                    if span.underline {
                        quad(
                            renderer,
                            Rectangle {
                                y: r.y + ch - 1.0,
                                height: 1.0,
                                ..r
                            },
                            color(span.fg),
                        );
                    }
                    col += span.cells;
                }
            }

            // Cursor.
            if let Some((c, row)) = self.snapshot.cursor
                && self.cursor_on
            {
                let r = rect(f32::from(c), f32::from(row), cw, ch);
                if !self.focused || self.snapshot.cursor_shape == CursorShape::HollowBlock {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: r,
                            border: Border {
                                color: self.fg,
                                width: 1.0,
                                radius: 0.0.into(),
                            },
                            ..Default::default()
                        },
                        Color::TRANSPARENT,
                    );
                } else {
                    match self.snapshot.cursor_shape {
                        CursorShape::Beam => quad(renderer, Rectangle { width: 2.0, ..r }, self.fg),
                        CursorShape::Underline => quad(
                            renderer,
                            Rectangle {
                                y: r.y + ch - 2.0,
                                height: 2.0,
                                ..r
                            },
                            self.fg,
                        ),
                        _ => {
                            quad(renderer, r, self.fg);
                            // Celda invertida: el carácter con el color de fondo.
                            let line = self.snapshot.line_text(usize::from(row));
                            if let Some(ch_under) = line.chars().nth(usize::from(c))
                                && !ch_under.is_whitespace()
                            {
                                draw_text(
                                    renderer,
                                    &ch_under.to_string(),
                                    r,
                                    self.bg,
                                    false,
                                    false,
                                );
                            }
                        }
                    }
                }
            }
        });
    }
}

impl<'a, Message, Theme, Renderer> From<TermView<'a, Message>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: text::Renderer<Font = Font> + 'a,
{
    fn from(v: TermView<'a, Message>) -> Self {
        Element::new(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::grid::{CursorShape, Span};
    use alacritty_terminal::vte::ansi::Rgb;

    const C: (f32, f32) = (8.0, 16.0);

    fn snap(rows: &[&str]) -> Snapshot {
        let c = Rgb { r: 0, g: 0, b: 0 };
        let width = rows.iter().map(|r| r.chars().count()).max().unwrap_or(0);
        Snapshot {
            cols: width as u16,
            rows: rows.len() as u16,
            lines: rows
                .iter()
                .map(|r| {
                    vec![Span {
                        text: format!("{r:<width$}"),
                        cells: width as u16,
                        fg: c,
                        bg: c,
                        bold: false,
                        italic: false,
                        underline: false,
                        inverse_applied: false,
                    }]
                })
                .collect(),
            cursor: None,
            cursor_shape: CursorShape::Block,
            display_offset: 0,
            selection: None,
        }
    }

    #[test]
    fn tamano_de_rejilla() {
        assert_eq!(grid_size(800.0, 480.0, C), (100, 30));
        assert_eq!(grid_size(807.9, 495.0, C), (100, 30));
        assert_eq!(grid_size(3.0, 3.0, C), (1, 1));
        assert_eq!(grid_size(100.0, 100.0, (0.0, 0.0)), (1, 1));
    }

    #[test]
    fn punto_a_celda() {
        assert_eq!(point_to_cell(0.0, 0.0, C, 100, 30), (0, 0));
        assert_eq!(point_to_cell(17.0, 33.0, C, 100, 30), (2, 2));
        assert_eq!(point_to_cell(-5.0, -5.0, C, 100, 30), (0, 0));
        assert_eq!(point_to_cell(9999.0, 9999.0, C, 100, 30), (99, 29));
    }

    #[test]
    fn detecta_url() {
        let l = "ver https://example.com/a?b=1, y mas";
        assert_eq!(url_at(l, 4).as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(url_at(l, 20).as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(url_at(l, 0), None);
        assert_eq!(url_at(l, 33), None);
        assert_eq!(
            url_at("\"http://x.org/p\"", 3).as_deref(),
            Some("http://x.org/p")
        );
        assert_eq!(
            url_at("(https://x.org/p)", 3).as_deref(),
            Some("https://x.org/p")
        );
        assert_eq!(url_at("httpx://no", 2), None);
    }

    #[test]
    fn palabra() {
        let l = "ls -la /tmp/foo.txt  x";
        assert_eq!(word_range(l, 1), (0, 1));
        assert_eq!(word_range(l, 8), (7, 18));
        assert_eq!(word_range(l, 19), (19, 20));
    }

    #[test]
    fn texto_seleccionado() {
        let s = snap(&["hola mundo", "segunda", "tercera!"]);
        assert_eq!(selected_text(&s, (3, 0), (0, 0)), "hola");
        assert_eq!(selected_text(&s, (5, 0), (6, 1)), "mundo\nsegunda");
        // orden invertido
        assert_eq!(selected_text(&s, (2, 2), (5, 0)), "mundo\nsegunda\nter");
        // sin espacios finales
        assert_eq!(selected_text(&s, (0, 0), (9, 0)), "hola mundo");
        assert_eq!(selected_text(&s, (7, 1), (9, 1)), "");
    }
}
