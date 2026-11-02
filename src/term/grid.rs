//! Instantánea inmutable de la rejilla para dibujar.
use alacritty_terminal::event::EventListener;
use alacritty_terminal::index::Point;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{CursorShape as AnsiCursor, Rgb};

use super::palette::{Palette, dim, resolve};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Underline,
    Beam,
    HollowBlock,
}

/// Tramo de texto contiguo con el mismo estilo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    /// Celdas de la rejilla que ocupa el tramo (un carácter ancho ocupa dos).
    pub cells: u16,
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Se intercambiaron fg/bg por el atributo INVERSE.
    pub inverse_applied: bool,
}

/// Copia de lo visible. Posiciones como `(columna, fila)` de la ventana
/// visible (fila 0 = primera visible).
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub cols: u16,
    pub rows: u16,
    pub lines: Vec<Vec<Span>>,
    pub cursor: Option<(u16, u16)>,
    pub cursor_shape: CursorShape,
    pub display_offset: usize,
    pub selection: Option<((u16, u16), (u16, u16))>,
}

fn view_pos(p: Point, offset: usize, cols: u16, rows: u16) -> (u16, u16) {
    let row = (p.line.0 + offset as i32).clamp(0, i32::from(rows) - 1) as u16;
    let last = usize::from(cols).saturating_sub(1);
    let abs_row = p.line.0 + offset as i32;
    // Fuera del viewport: la columna se ajusta al borde correspondiente.
    let col = if abs_row < 0 {
        0
    } else if abs_row >= i32::from(rows) {
        last
    } else {
        p.column.0.min(last)
    } as u16;
    (col, row)
}

pub fn build<T: EventListener>(term: &Term<T>, palette: &Palette) -> Snapshot {
    use alacritty_terminal::grid::Dimensions;
    let cols = term.columns() as u16;
    let rows = term.screen_lines() as u16;
    let content = term.renderable_content();
    let offset = content.display_offset;
    let mut lines: Vec<Vec<Span>> = vec![Vec::new(); usize::from(rows)];
    // La fila acaba en un carácter ancho: el siguiente abre un tramo nuevo
    // (evita que el dibujo se desplace por el ancho real del glifo).
    let mut wide_end = vec![false; usize::from(rows)];

    for indexed in content.display_iter {
        let cell = &indexed.cell;
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let row = indexed.point.line.0 + offset as i32;
        if row < 0 || row >= i32::from(rows) {
            continue;
        }
        let mut fg = resolve(cell.fg, palette, true);
        let mut bg = resolve(cell.bg, palette, false);
        let inverse = cell.flags.contains(Flags::INVERSE);
        if inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell.flags.contains(Flags::DIM) {
            fg = dim(fg);
        }
        let hidden = cell.flags.contains(Flags::HIDDEN);
        if hidden {
            fg = bg;
        }
        let bold = cell.flags.contains(Flags::BOLD);
        let italic = cell.flags.contains(Flags::ITALIC);
        let underline = cell.flags.intersects(Flags::ALL_UNDERLINES);
        let is_wide = cell.flags.contains(Flags::WIDE_CHAR);
        let n_cells = if is_wide { 2 } else { 1 };
        let prev_wide = std::mem::replace(&mut wide_end[row as usize], is_wide);
        let line = &mut lines[row as usize];
        match line.last_mut() {
            Some(last)
                if !prev_wide
                    && last.fg == fg
                    && last.bg == bg
                    && last.bold == bold
                    && last.italic == italic
                    && last.underline == underline
                    && last.inverse_applied == inverse =>
            {
                push_cell(&mut last.text, cell.c, cell.zerowidth());
                last.cells += n_cells;
            }
            _ => {
                let mut text = String::new();
                push_cell(&mut text, cell.c, cell.zerowidth());
                line.push(Span {
                    text,
                    cells: n_cells,
                    fg,
                    bg,
                    bold,
                    italic,
                    underline,
                    inverse_applied: inverse,
                });
            }
        }
    }

    let cursor_visible =
        content.mode.contains(TermMode::SHOW_CURSOR) && content.cursor.shape != AnsiCursor::Hidden;
    let cursor = if cursor_visible {
        let row = content.cursor.point.line.0 + offset as i32;
        (0..i32::from(rows)).contains(&row).then(|| {
            (
                content
                    .cursor
                    .point
                    .column
                    .0
                    .min(usize::from(cols).saturating_sub(1)) as u16,
                row as u16,
            )
        })
    } else {
        None
    };
    let cursor_shape = match content.cursor.shape {
        AnsiCursor::Underline => CursorShape::Underline,
        AnsiCursor::Beam => CursorShape::Beam,
        AnsiCursor::HollowBlock => CursorShape::HollowBlock,
        _ => CursorShape::Block,
    };
    let selection = content.selection.map(|r| {
        (
            view_pos(r.start, offset, cols, rows),
            view_pos(r.end, offset, cols, rows),
        )
    });

    Snapshot {
        cols,
        rows,
        lines,
        cursor,
        cursor_shape,
        display_offset: offset,
        selection,
    }
}

fn push_cell(text: &mut String, c: char, zero: Option<&[char]>) {
    text.push(c);
    if let Some(z) = zero {
        text.extend(z.iter());
    }
}

impl Snapshot {
    /// Texto plano de una fila (para pruebas y copiar).
    pub fn line_text(&self, row: usize) -> String {
        self.lines
            .get(row)
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line};

    #[test]
    fn view_pos_ajusta_columna_fuera_de_vista() {
        let p = |l: i32, c: usize| Point::new(Line(l), Column(c));
        assert_eq!(view_pos(p(-5, 7), 0, 20, 10), (0, 0));
        assert_eq!(view_pos(p(15, 7), 0, 20, 10), (19, 9));
        assert_eq!(view_pos(p(3, 7), 0, 20, 10), (7, 3));
        assert_eq!(view_pos(p(3, 70), 0, 20, 10), (19, 3));
    }
}
