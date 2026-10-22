//! A VT screen as a grid of styled cells (§3.6.3): the byte stream of an
//! SSH session becomes the same line-oriented frames the iTerm2 source
//! produces, so one renderer draws both.

use std::collections::VecDeque;

use muster_proto::v1::{TerminalFrame, TerminalLine};
use vte::{Params, Perform};

/// Colour: 0 = default, 0x01RRGGBB = rgb, 0x02000000 | index = palette.
pub type Color = u32;

pub const FLAG_BOLD: u8 = 1;
pub const FLAG_ITALIC: u8 = 2;
pub const FLAG_UNDERLINE: u8 = 4;
pub const FLAG_INVERSE: u8 = 8;
pub const FLAG_DIM: u8 = 16;
pub const FLAG_STRIKE: u8 = 32;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub flags: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}

/// One run of equally styled cells, the wire form of `TerminalLine.styles`
/// (11 bytes per run: len u16, fg u32, bg u32, flags u8; big-endian).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StyleRun {
    pub len: u16,
    pub style: Style,
}

pub fn encode_styles(cells: &[Cell]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut runs: Vec<StyleRun> = Vec::new();
    for cell in cells {
        match runs.last_mut() {
            Some(run) if run.style == cell.style && run.len < u16::MAX => run.len += 1,
            _ => runs.push(StyleRun {
                len: 1,
                style: cell.style,
            }),
        }
    }
    // A line with only default style carries no runs at all.
    if runs.len() == 1 && runs[0].style == Style::default() {
        return out;
    }
    for run in runs {
        out.extend_from_slice(&run.len.to_be_bytes());
        out.extend_from_slice(&run.style.fg.to_be_bytes());
        out.extend_from_slice(&run.style.bg.to_be_bytes());
        out.push(run.style.flags);
    }
    out
}

/// The inverse of the 11-byte run encoding; shells decode it themselves.
#[cfg(test)]
fn decode_styles(bytes: &[u8]) -> Vec<StyleRun> {
    bytes
        .chunks_exact(11)
        .map(|c| StyleRun {
            len: u16::from_be_bytes([c[0], c[1]]),
            style: Style {
                fg: u32::from_be_bytes([c[2], c[3], c[4], c[5]]),
                bg: u32::from_be_bytes([c[6], c[7], c[8], c[9]]),
                flags: c[10],
            },
        })
        .collect()
}

#[derive(Clone)]
struct Screen {
    rows: Vec<Vec<Cell>>,
    cursor: (u16, u16),
    saved_cursor: (u16, u16),
    scroll_top: u16,
    scroll_bottom: u16,
}

impl Screen {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            rows: (0..rows)
                .map(|_| vec![Cell::default(); cols as usize])
                .collect(),
            cursor: (0, 0),
            saved_cursor: (0, 0),
            scroll_top: 0,
            scroll_bottom: rows.saturating_sub(1),
        }
    }
}

/// The emulator. Feed bytes, take frames.
pub struct Grid {
    cols: u16,
    rows: u16,
    main: Screen,
    alt: Option<Screen>,
    pen: Style,
    dirty: Vec<bool>,
    all_dirty: bool,
    rev: u64,
    scrollback: VecDeque<Vec<Cell>>,
    scrollback_limit: usize,
    autowrap: bool,
    origin_mode: bool,
    cursor_visible: bool,
    pending_wrap: bool,
    pub bracketed_paste: bool,
    pub app_cursor_keys: bool,
    pub title: String,
    parser: vte::Parser,
    /// Replies the host expects (DSR, DA): the session sends them back.
    replies: Vec<u8>,
}

impl Grid {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        Self {
            cols,
            rows,
            main: Screen::new(cols, rows),
            alt: None,
            pen: Style::default(),
            dirty: vec![true; rows as usize],
            all_dirty: true,
            rev: 0,
            scrollback: VecDeque::new(),
            scrollback_limit: 2000,
            autowrap: true,
            origin_mode: false,
            cursor_visible: true,
            pending_wrap: false,
            bracketed_paste: false,
            app_cursor_keys: false,
            title: String::new(),
            parser: vte::Parser::new(),
            replies: Vec::new(),
        }
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }
    pub fn rows(&self) -> u16 {
        self.rows
    }
    pub fn cursor(&self) -> (u16, u16) {
        self.screen().cursor
    }
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    fn screen(&self) -> &Screen {
        self.alt.as_ref().unwrap_or(&self.main)
    }
    fn screen_mut(&mut self) -> &mut Screen {
        self.alt.as_mut().unwrap_or(&mut self.main)
    }

    /// Feeds host output; returns bytes the host expects back, if any.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut parser = std::mem::take(&mut self.parser);
        parser.advance(self, bytes);
        self.parser = parser;
        std::mem::take(&mut self.replies)
    }

    /// The text of one row, trailing blanks trimmed.
    pub fn row_text(&self, row: u16) -> String {
        let cells = &self.screen().rows[row as usize];
        let mut text: String = cells.iter().map(|c| c.ch).collect();
        while text.ends_with(' ') {
            text.pop();
        }
        text
    }

    /// The rows changed since the last frame (or every row), as a frame.
    pub fn frame(&mut self, session: &str) -> Option<TerminalFrame> {
        let full = self.all_dirty;
        let changed: Vec<u16> = (0..self.rows)
            .filter(|r| full || self.dirty[*r as usize])
            .collect();
        if changed.is_empty() {
            return None;
        }
        self.rev += 1;
        let lines = changed
            .iter()
            .map(|&r| TerminalLine {
                index: r as u32,
                text: self.row_text(r),
                styles: encode_styles(&self.screen().rows[r as usize]),
            })
            .collect();
        self.dirty.fill(false);
        self.all_dirty = false;
        let (cx, cy) = self.screen().cursor;
        Some(TerminalFrame {
            session: session.to_string(),
            rev: self.rev,
            full,
            cols: self.cols as u32,
            rows: self.rows as u32,
            lines,
            cursor_x: cx as u32,
            cursor_y: cy as u32,
            ended: false,
        })
    }

    /// Everything, regardless of what changed.
    pub fn full_frame(&mut self, session: &str) -> TerminalFrame {
        self.all_dirty = true;
        self.frame(session).expect("full frame")
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols && rows == self.rows {
            return;
        }
        for screen in std::iter::once(&mut self.main).chain(self.alt.iter_mut()) {
            for row in &mut screen.rows {
                row.resize(cols as usize, Cell::default());
            }
            while screen.rows.len() < rows as usize {
                screen.rows.push(vec![Cell::default(); cols as usize]);
            }
            while screen.rows.len() > rows as usize {
                // Keep the bottom (the cursor's neighbourhood).
                if screen.cursor.1 == 0 {
                    screen.rows.pop();
                } else {
                    let top = screen.rows.remove(0);
                    self.scrollback.push_back(top);
                    screen.cursor.1 -= 1;
                }
            }
            screen.cursor.0 = screen.cursor.0.min(cols - 1);
            screen.cursor.1 = screen.cursor.1.min(rows - 1);
            screen.scroll_top = 0;
            screen.scroll_bottom = rows - 1;
        }
        while self.scrollback.len() > self.scrollback_limit {
            self.scrollback.pop_front();
        }
        self.cols = cols;
        self.rows = rows;
        self.dirty = vec![true; rows as usize];
        self.all_dirty = true;
        self.pending_wrap = false;
    }

    /// Lines scrolled off the top, oldest first (bounded).
    pub fn scrollback(&self, last: usize) -> Vec<String> {
        self.scrollback
            .iter()
            .rev()
            .take(last)
            .map(|cells| {
                let mut t: String = cells.iter().map(|c| c.ch).collect();
                while t.ends_with(' ') {
                    t.pop();
                }
                t
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    // ------------------------------------------------------------ editing

    fn mark(&mut self, row: u16) {
        if let Some(d) = self.dirty.get_mut(row as usize) {
            *d = true;
        }
    }

    fn mark_all(&mut self) {
        self.all_dirty = true;
    }

    fn scroll_up(&mut self, n: u16) {
        let alt = self.alt.is_some();
        let width = self.cols as usize;
        let mut scrolled_off = Vec::new();
        {
            let screen = self.screen_mut();
            let (top, bottom) = (screen.scroll_top as usize, screen.scroll_bottom as usize);
            for _ in 0..n {
                if top >= screen.rows.len() || bottom >= screen.rows.len() || top > bottom {
                    break;
                }
                let row = screen.rows.remove(top);
                if top == 0 && !alt {
                    scrolled_off.push(row);
                }
                screen.rows.insert(bottom, vec![Cell::default(); width]);
            }
        }
        self.scrollback.extend(scrolled_off);
        while self.scrollback.len() > self.scrollback_limit {
            self.scrollback.pop_front();
        }
        self.mark_all();
    }

    fn scroll_down(&mut self, n: u16) {
        let width = self.cols as usize;
        let screen = self.screen_mut();
        let (top, bottom) = (screen.scroll_top as usize, screen.scroll_bottom as usize);
        for _ in 0..n {
            if top >= screen.rows.len() || bottom >= screen.rows.len() || top > bottom {
                break;
            }
            screen.rows.remove(bottom);
            screen.rows.insert(top, vec![Cell::default(); width]);
        }
        self.mark_all();
    }

    fn linefeed(&mut self) {
        let bottom = self.screen().scroll_bottom;
        let y = self.screen().cursor.1;
        if y == bottom {
            self.scroll_up(1);
        } else if y + 1 < self.rows {
            self.screen_mut().cursor.1 = y + 1;
        }
        self.pending_wrap = false;
    }

    fn put(&mut self, ch: char) {
        if self.pending_wrap && self.autowrap {
            self.screen_mut().cursor.0 = 0;
            self.linefeed();
        }
        let pen = self.pen;
        let cols = self.cols;
        let (x, y) = self.screen().cursor;
        let screen = self.screen_mut();
        if let Some(cell) = screen
            .rows
            .get_mut(y as usize)
            .and_then(|r| r.get_mut(x as usize))
        {
            *cell = Cell { ch, style: pen };
        }
        if x + 1 < cols {
            screen.cursor.0 = x + 1;
            self.pending_wrap = false;
        } else {
            self.pending_wrap = true;
        }
        self.mark(y);
    }

    fn move_to(&mut self, x: i32, y: i32) {
        let (min_y, max_y) = if self.origin_mode {
            (
                self.screen().scroll_top as i32,
                self.screen().scroll_bottom as i32,
            )
        } else {
            (0, self.rows as i32 - 1)
        };
        let x = x.clamp(0, self.cols as i32 - 1) as u16;
        let y = y.clamp(min_y, max_y) as u16;
        self.screen_mut().cursor = (x, y);
        self.pending_wrap = false;
    }

    fn erase_in_line(&mut self, mode: u16) {
        let (x, y) = self.screen().cursor;
        let cols = self.cols as usize;
        let range = match mode {
            0 => x as usize..cols,
            1 => 0..(x as usize + 1).min(cols),
            _ => 0..cols,
        };
        let blank = Cell {
            ch: ' ',
            style: Style {
                bg: self.pen.bg,
                ..Style::default()
            },
        };
        if let Some(row) = self.screen_mut().rows.get_mut(y as usize) {
            for cell in &mut row[range] {
                *cell = blank;
            }
        }
        self.mark(y);
    }

    fn erase_in_display(&mut self, mode: u16) {
        let (_, y) = self.screen().cursor;
        let rows = self.rows;
        match mode {
            0 => {
                self.erase_in_line(0);
                for r in y + 1..rows {
                    self.clear_row(r);
                }
            }
            1 => {
                for r in 0..y {
                    self.clear_row(r);
                }
                self.erase_in_line(1);
            }
            _ => {
                for r in 0..rows {
                    self.clear_row(r);
                }
                if mode == 3 {
                    self.scrollback.clear();
                }
            }
        }
    }

    fn clear_row(&mut self, r: u16) {
        let bg = self.pen.bg;
        if let Some(row) = self.screen_mut().rows.get_mut(r as usize) {
            for cell in row.iter_mut() {
                *cell = Cell {
                    ch: ' ',
                    style: Style {
                        bg,
                        ..Style::default()
                    },
                };
            }
        }
        self.mark(r);
    }

    fn sgr(&mut self, params: &Params) {
        if params.is_empty() {
            self.pen = Style::default();
            return;
        }
        let flat: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        let mut i = 0;
        while i < flat.len() {
            let p = flat[i];
            match p {
                0 => self.pen = Style::default(),
                1 => self.pen.flags |= FLAG_BOLD,
                2 => self.pen.flags |= FLAG_DIM,
                3 => self.pen.flags |= FLAG_ITALIC,
                4 => self.pen.flags |= FLAG_UNDERLINE,
                7 => self.pen.flags |= FLAG_INVERSE,
                9 => self.pen.flags |= FLAG_STRIKE,
                22 => self.pen.flags &= !(FLAG_BOLD | FLAG_DIM),
                23 => self.pen.flags &= !FLAG_ITALIC,
                24 => self.pen.flags &= !FLAG_UNDERLINE,
                27 => self.pen.flags &= !FLAG_INVERSE,
                29 => self.pen.flags &= !FLAG_STRIKE,
                30..=37 => self.pen.fg = 0x0200_0000 | (p - 30) as u32,
                38 | 48 => {
                    let (color, used) = extended_color(&flat[i + 1..]);
                    if p == 38 {
                        self.pen.fg = color;
                    } else {
                        self.pen.bg = color;
                    }
                    i += used;
                }
                39 => self.pen.fg = 0,
                40..=47 => self.pen.bg = 0x0200_0000 | (p - 40) as u32,
                49 => self.pen.bg = 0,
                90..=97 => self.pen.fg = 0x0200_0000 | (p - 90 + 8) as u32,
                100..=107 => self.pen.bg = 0x0200_0000 | (p - 100 + 8) as u32,
                _ => {}
            }
            i += 1;
        }
    }

    fn set_mode(&mut self, private: bool, mode: u16, on: bool) {
        if !private {
            return;
        }
        match mode {
            1 => self.app_cursor_keys = on,
            6 => {
                self.origin_mode = on;
                self.move_to(0, 0);
            }
            7 => self.autowrap = on,
            25 => self.cursor_visible = on,
            2004 => self.bracketed_paste = on,
            47 | 1047 | 1049 => {
                if on && self.alt.is_none() {
                    let mut alt = Screen::new(self.cols, self.rows);
                    if mode == 1049 {
                        self.main.saved_cursor = self.main.cursor;
                    }
                    alt.cursor = self.main.cursor;
                    self.alt = Some(alt);
                    self.mark_all();
                } else if !on && self.alt.is_some() {
                    self.alt = None;
                    if mode == 1049 {
                        self.main.cursor = self.main.saved_cursor;
                    }
                    self.mark_all();
                }
            }
            _ => {}
        }
    }
}

fn extended_color(rest: &[u16]) -> (Color, usize) {
    match rest.first() {
        Some(5) => (
            0x0200_0000 | rest.get(1).copied().unwrap_or(0) as u32 & 0xff,
            2,
        ),
        Some(2) => {
            let r = rest.get(1).copied().unwrap_or(0) as u32 & 0xff;
            let g = rest.get(2).copied().unwrap_or(0) as u32 & 0xff;
            let b = rest.get(3).copied().unwrap_or(0) as u32 & 0xff;
            (0x0100_0000 | (r << 16) | (g << 8) | b, 4)
        }
        _ => (0, 0),
    }
}

fn param(params: &Params, index: usize, default: u16) -> u16 {
    params
        .iter()
        .nth(index)
        .and_then(|p| p.first().copied())
        .filter(|v| *v != 0)
        .unwrap_or(default)
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        self.put(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => self.linefeed(),
            b'\r' => {
                self.screen_mut().cursor.0 = 0;
                self.pending_wrap = false;
            }
            0x08 => {
                let x = self.screen().cursor.0;
                self.screen_mut().cursor.0 = x.saturating_sub(1);
                self.pending_wrap = false;
            }
            b'\t' => {
                let x = self.screen().cursor.0;
                let next = ((x / 8) + 1) * 8;
                self.screen_mut().cursor.0 = next.min(self.cols - 1);
                self.pending_wrap = false;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        let private = intermediates.first() == Some(&b'?');
        let (x, y) = self.screen().cursor;
        match action {
            'A' => self.move_to(x as i32, y as i32 - param(params, 0, 1) as i32),
            'B' | 'e' => self.move_to(x as i32, y as i32 + param(params, 0, 1) as i32),
            'C' | 'a' => self.move_to(x as i32 + param(params, 0, 1) as i32, y as i32),
            'D' => self.move_to(x as i32 - param(params, 0, 1) as i32, y as i32),
            'E' => self.move_to(0, y as i32 + param(params, 0, 1) as i32),
            'F' => self.move_to(0, y as i32 - param(params, 0, 1) as i32),
            'G' | '`' => self.move_to(param(params, 0, 1) as i32 - 1, y as i32),
            'd' => self.move_to(x as i32, param(params, 0, 1) as i32 - 1),
            'H' | 'f' => {
                let row = param(params, 0, 1) as i32 - 1;
                let col = param(params, 1, 1) as i32 - 1;
                let base = if self.origin_mode {
                    self.screen().scroll_top as i32
                } else {
                    0
                };
                self.move_to(col, base + row);
            }
            'J' => self.erase_in_display(param(params, 0, 0)),
            'K' => self.erase_in_line(param(params, 0, 0)),
            'L' => {
                let n = param(params, 0, 1);
                let bottom = self.screen().scroll_bottom;
                let saved_top = self.screen().scroll_top;
                if y <= bottom {
                    self.screen_mut().scroll_top = y;
                    self.scroll_down(n);
                    self.screen_mut().scroll_top = saved_top;
                }
            }
            'M' => {
                let n = param(params, 0, 1);
                let bottom = self.screen().scroll_bottom;
                let saved_top = self.screen().scroll_top;
                if y <= bottom {
                    self.screen_mut().scroll_top = y;
                    self.scroll_up(n);
                    self.screen_mut().scroll_top = saved_top;
                }
            }
            'P' => {
                let n = param(params, 0, 1) as usize;
                let cols = self.cols as usize;
                if let Some(row) = self.screen_mut().rows.get_mut(y as usize) {
                    for _ in 0..n.min(cols - x as usize) {
                        row.remove(x as usize);
                        row.push(Cell::default());
                    }
                }
                self.mark(y);
            }
            '@' => {
                let n = param(params, 0, 1) as usize;
                let cols = self.cols as usize;
                if let Some(row) = self.screen_mut().rows.get_mut(y as usize) {
                    for _ in 0..n.min(cols - x as usize) {
                        row.insert(x as usize, Cell::default());
                        row.pop();
                    }
                }
                self.mark(y);
            }
            'X' => {
                let n = param(params, 0, 1) as usize;
                let cols = self.cols as usize;
                if let Some(row) = self.screen_mut().rows.get_mut(y as usize) {
                    for cell in &mut row[x as usize..(x as usize + n).min(cols)] {
                        *cell = Cell::default();
                    }
                }
                self.mark(y);
            }
            'S' => self.scroll_up(param(params, 0, 1)),
            'T' => self.scroll_down(param(params, 0, 1)),
            'm' => self.sgr(params),
            'r' => {
                let top = param(params, 0, 1).saturating_sub(1);
                let bottom = param(params, 1, self.rows)
                    .saturating_sub(1)
                    .min(self.rows - 1);
                if top < bottom {
                    let screen = self.screen_mut();
                    screen.scroll_top = top;
                    screen.scroll_bottom = bottom;
                }
                self.move_to(0, 0);
            }
            'h' | 'l' => {
                let on = action == 'h';
                for p in params.iter() {
                    if let Some(mode) = p.first() {
                        self.set_mode(private, *mode, on);
                    }
                }
            }
            's' => self.screen_mut().saved_cursor = (x, y),
            'u' => {
                let saved = self.screen().saved_cursor;
                self.move_to(saved.0 as i32, saved.1 as i32);
            }
            'n' => {
                if param(params, 0, 0) == 6 {
                    self.replies
                        .extend_from_slice(format!("\x1b[{};{}R", y + 1, x + 1).as_bytes());
                }
            }
            // Only queries with a single zero/default parameter request a reply.
            // Answering DA2/DA3 with DA1 leaks replies into tmux's shell input.
            'c' if !ignore
                && params.iter().count() == 1
                && params.iter().next() == Some(&[0][..]) =>
            {
                match intermediates {
                    [] => self.replies.extend_from_slice(b"\x1b[?62;22c"),
                    [b'>'] => self.replies.extend_from_slice(b"\x1b[>1;0;0c"),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.screen_mut().saved_cursor = self.screen().cursor,
            b'8' => {
                let saved = self.screen().saved_cursor;
                self.move_to(saved.0 as i32, saved.1 as i32);
            }
            b'D' => self.linefeed(),
            b'E' => {
                self.screen_mut().cursor.0 = 0;
                self.linefeed();
            }
            b'M' => {
                let top = self.screen().scroll_top;
                let y = self.screen().cursor.1;
                if y == top {
                    self.scroll_down(1);
                } else {
                    self.screen_mut().cursor.1 = y - 1;
                }
            }
            b'c' => {
                let (cols, rows) = (self.cols, self.rows);
                *self = Grid::new(cols, rows);
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if let (Some(kind), Some(text)) = (params.first(), params.get(1))
            && matches!(*kind, b"0" | b"2")
        {
            self.title = String::from_utf8_lossy(text).into_owned();
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}
    fn put(&mut self, _byte: u8) {}
    fn unhook(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(grid: &Grid) -> Vec<String> {
        (0..grid.rows()).map(|r| grid.row_text(r)).collect()
    }

    #[test]
    fn prints_wraps_and_scrolls() {
        let mut g = Grid::new(5, 2);
        g.feed(b"hello worl");
        assert_eq!(text(&g), vec!["hello", " worl"]);
        assert_eq!(g.cursor(), (4, 1));
        g.feed(b"d\r\nnext");
        assert_eq!(text(&g), vec!["d", "next"]);
        assert_eq!(g.scrollback(10), vec!["hello", " worl"]);
    }

    #[test]
    fn frames_report_changed_rows_only() {
        let mut g = Grid::new(10, 3);
        let first = g.frame("s").unwrap();
        assert!(first.full);
        assert_eq!(first.lines.len(), 3);
        assert!(g.frame("s").is_none());
        g.feed(b"\x1b[2;1Hrow two");
        let delta = g.frame("s").unwrap();
        assert!(!delta.full);
        assert_eq!(delta.lines.len(), 1);
        assert_eq!(delta.lines[0].index, 1);
        assert_eq!(delta.lines[0].text, "row two");
        assert_eq!((delta.cursor_x, delta.cursor_y), (7, 1));
    }

    #[test]
    fn styles_round_trip_and_erase() {
        let mut g = Grid::new(8, 1);
        g.feed(b"\x1b[1;31mab\x1b[0mcd");
        let frame = g.frame("s").unwrap();
        let runs = decode_styles(&frame.lines[0].styles);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].len, 2);
        assert_eq!(runs[0].style.fg, 0x0200_0001);
        assert_eq!(runs[0].style.flags & FLAG_BOLD, FLAG_BOLD);
        assert_eq!(runs[1].len, 6);
        assert_eq!(runs[1].style, Style::default());
        g.feed(b"\x1b[38;2;10;20;30mZ");
        assert_eq!(g.screen().rows[0][4].style.fg, 0x010a141e);
        g.feed(b"\x1b[2K");
        assert_eq!(g.row_text(0), "");
        assert!(encode_styles(&g.screen().rows[0]).is_empty());
    }

    #[test]
    fn alt_screen_and_resize() {
        let mut g = Grid::new(6, 2);
        g.feed(b"main");
        g.feed(b"\x1b[?1049h\x1b[Halt");
        assert_eq!(text(&g), vec!["alt", ""]);
        g.feed(b"\x1b[?1049l");
        assert_eq!(text(&g), vec!["main", ""]);
        assert_eq!(g.cursor(), (4, 0));
        g.resize(3, 3);
        assert_eq!(g.cols(), 3);
        assert_eq!(text(&g), vec!["mai", "", ""]);
        assert!(g.frame("s").unwrap().full);
        g.feed(b"\x1b[6n");
        assert_eq!(g.feed(b""), b"");
    }

    #[test]
    fn device_attributes_reply_only_to_supported_queries() {
        let mut g = Grid::new(80, 24);
        for query in [b"\x1b[c".as_slice(), b"\x1b[0c"] {
            assert_eq!(g.feed(query), b"\x1b[?62;22c");
        }
        for query in [b"\x1b[>c".as_slice(), b"\x1b[>0c"] {
            assert_eq!(g.feed(query), b"\x1b[>1;0;0c");
        }
        for sequence in [
            b"\x1b[?62;22c".as_slice(),
            b"\x1b[>1;0;0c",
            b"\x1b[=c",
            b"\x1b[1c",
            b"\x1b[0;0c",
            b"\x1b[0:0c",
            b"\x1b[?0c",
        ] {
            assert!(g.feed(sequence).is_empty());
        }
    }

    #[test]
    fn cursor_report_is_a_reply() {
        let mut g = Grid::new(10, 2);
        g.feed(b"\x1b[2;3H");
        let reply = g.feed(b"\x1b[6n");
        assert_eq!(reply, b"\x1b[2;3R");
        assert!(!g.bracketed_paste);
        g.feed(b"\x1b[?2004h");
        assert!(g.bracketed_paste);
    }
}
