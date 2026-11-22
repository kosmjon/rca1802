//! VID-1 video display terminal: the screen end of the serial link (requirement 8).
//!
//! The VID-1 has its own UART and 1 KB of screen RAM. Characters received on its
//! serial input are drawn at the cursor; a few control codes move the cursor, scroll
//! or clear the screen. It is full duplex with no local echo: a key typed on the
//! keyboard goes out on the serial line and appears here only if the Elf II sends it
//! back.
//!
//! Behaviour that is not documented (marked [verify]): the cursor wraps to the next
//! line as soon as the last column is written; BS and cursor-left stop at the first
//! column; the codes for cursor up/down/right/home and erase-to-end are not bound
//! until the configuration lists them.

use crate::config::{Vid1Action, Vid1Config};

/// Lines on the screen.
pub const ROWS: usize = 16;

pub struct Vid1Terminal {
    cols: usize,
    pub(crate) cells: Vec<u8>,
    pub(crate) row: usize,
    pub(crate) col: usize,
    /// Action for each 7-bit code.
    actions: [Option<Vid1Action>; 128],
}

impl Vid1Terminal {
    pub fn new(cfg: &Vid1Config) -> Self {
        let mut actions = [None; 128];
        for c in &cfg.codes {
            actions[(c.code & 0x7F) as usize] = Some(c.action);
        }
        let cols = cfg.columns as usize;
        Vid1Terminal {
            cols,
            cells: vec![b' '; ROWS * cols],
            row: 0,
            col: 0,
            actions,
        }
    }

    /// Characters per line (32 or 64).
    pub fn columns(&self) -> usize {
        self.cols
    }

    /// Screen contents, `ROWS` lines of `columns()` characters, row by row.
    pub fn screen(&self) -> &[u8] {
        &self.cells
    }

    /// One line of the screen.
    pub fn line(&self, row: usize) -> &[u8] {
        &self.cells[row * self.cols..(row + 1) * self.cols]
    }

    /// Cursor position as (row, column), both from 0.
    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// Process one received character. Bit 7 is ignored (7-bit ASCII).
    pub fn put(&mut self, byte: u8) {
        let c = byte & 0x7F;
        match c {
            0x20..=0x7E => self.print(c),
            _ => {
                if let Some(a) = self.actions[c as usize] {
                    self.act(a);
                }
            }
        }
    }

    fn print(&mut self, c: u8) {
        self.cells[self.row * self.cols + self.col] = c;
        self.col += 1;
        if self.col == self.cols {
            self.col = 0;
            self.line_feed();
        }
    }

    fn line_feed(&mut self) {
        if self.row + 1 == ROWS {
            self.cells.copy_within(self.cols.., 0);
            let last = (ROWS - 1) * self.cols;
            self.cells[last..].fill(b' ');
        } else {
            self.row += 1;
        }
    }

    fn erase(&mut self, from: usize) {
        self.cells[from..].fill(b' ');
    }

    fn act(&mut self, a: Vid1Action) {
        let here = self.row * self.cols + self.col;
        match a {
            Vid1Action::CursorLeft => self.col = self.col.saturating_sub(1),
            Vid1Action::CursorRight => self.col = (self.col + 1).min(self.cols - 1),
            Vid1Action::CursorUp => self.row = self.row.saturating_sub(1),
            Vid1Action::LineFeed => self.line_feed(),
            Vid1Action::CursorDown => self.row = (self.row + 1).min(ROWS - 1),
            Vid1Action::CarriageReturn => self.col = 0,
            Vid1Action::Home => {
                self.row = 0;
                self.col = 0;
            }
            Vid1Action::ClearScreen => {
                self.erase(0);
                self.row = 0;
                self.col = 0;
            }
            Vid1Action::EraseToEol => {
                let end = (self.row + 1) * self.cols;
                self.cells[here..end].fill(b' ');
            }
            Vid1Action::EraseToEos => self.erase(here),
            Vid1Action::Delete => self.cells[here] = b' ',
            Vid1Action::Rubout => {
                self.col = self.col.saturating_sub(1);
                self.cells[self.row * self.cols + self.col] = b' ';
            }
        }
    }

    /// Clear the screen and home the cursor.
    pub fn clear(&mut self) {
        self.act(Vid1Action::ClearScreen);
    }

    /// The screen as text, one string per line with trailing spaces removed.
    pub fn text(&self) -> Vec<String> {
        (0..ROWS)
            .map(|r| String::from_utf8_lossy(self.line(r)).trim_end().to_string())
            .collect()
    }
}
