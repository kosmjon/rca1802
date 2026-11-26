//! Parallel dot-matrix printer (1982-style), driven by `OUT 3`.
//!
//! Model: the Elf II writes a byte to the printer with `OUT 3`. Printable bytes
//! (0x20-0x7E) place a glyph at the print head and advance it; a few control
//! codes move the head and the paper:
//!
//! | Code | Effect |
//! |---|---|
//! | CR (0x0D) | carriage return: head to column 0 (also line-feeds if `cr_feeds`) |
//! | LF (0x0A) | line feed: advance the paper one line |
//! | FF (0x0C) | form feed: advance to the top of the next page |
//! | BS (0x08) | backspace one column (stops at column 0) |
//! | HT (0x09) | tab to the next multiple of 8 columns |
//!
//! The paper is continuous: printed lines accumulate forever (fan-fold), and the
//! front end shows them as white paper. *Tear off* ([`Printer::tear_off`]) throws
//! the current sheet away and starts fresh.
//!
//! **Ready / busy.** A real printer is not instantaneous. After each byte the
//! printer is *busy* for a short time, during which its EF "ready" line reads low;
//! software that polls EF before `OUT 3` therefore sees it toggle. [`is_ready`]
//! takes the current machine-cycle count so the busy window is measured in emulated
//! time.

use crate::config::PrinterConfig;

/// A printed line: the glyphs across the page (trailing blanks trimmed on read).
type Line = Vec<u8>;

pub struct Printer {
    width: usize,
    page_lines: usize,
    cr_feeds: bool,
    /// Emulated machine cycles the printer stays busy after accepting a byte.
    busy_cycles: u64,
    /// The paper: every line printed so far, oldest first.
    lines: Vec<Line>,
    /// Current print-head column on the last line.
    col: usize,
    /// Lines printed on the current page (for form feed).
    line_in_page: usize,
    /// Machine-cycle time at which the printer becomes ready again.
    busy_until: u64,
}

impl Printer {
    pub fn new(cfg: &PrinterConfig, cycles_per_second: u32) -> Self {
        // busy_ms of emulated time, converted to machine cycles.
        let busy_cycles = u64::from(cfg.busy_ms) * u64::from(cycles_per_second) / 1000;
        Printer {
            width: cfg.width.max(1) as usize,
            page_lines: cfg.page_lines.max(1) as usize,
            cr_feeds: cfg.cr_feeds,
            busy_cycles,
            lines: vec![Vec::new()],
            col: 0,
            line_in_page: 0,
            busy_until: 0,
        }
    }

    /// The printer's EF "ready" line as the CPU sees it at machine-cycle `cycle`:
    /// high (ready) once the busy window after the last byte has elapsed.
    pub fn is_ready(&self, cycle: u64) -> bool {
        cycle >= self.busy_until
    }

    /// `OUT 3`: accept one byte and start a busy window.
    pub fn write(&mut self, byte: u8, cycle: u64) {
        self.busy_until = cycle + self.busy_cycles;
        match byte {
            0x0A => self.line_feed(),
            0x0D => {
                self.col = 0;
                if self.cr_feeds {
                    self.line_feed();
                }
            }
            0x0C => self.form_feed(),
            0x08 => self.col = self.col.saturating_sub(1),
            0x09 => {
                let next = (self.col / 8 + 1) * 8;
                self.col = next.min(self.width - 1);
            }
            0x20..=0x7E => self.print(byte),
            // Other control codes are ignored (a real printer does its own thing;
            // we keep the paper readable).
            _ => {}
        }
    }

    fn print(&mut self, c: u8) {
        let line = self.lines.last_mut().expect("at least one line");
        if line.len() <= self.col {
            line.resize(self.col + 1, b' ');
        }
        line[self.col] = c;
        self.col += 1;
        if self.col >= self.width {
            // Auto-wrap at the right margin, like a printer at end of line.
            self.col = 0;
            self.line_feed();
        }
    }

    fn line_feed(&mut self) {
        self.lines.push(Vec::new());
        self.line_in_page += 1;
        if self.line_in_page >= self.page_lines {
            self.line_in_page = 0;
        }
    }

    fn form_feed(&mut self) {
        // Advance to the top of the next page: pad with blank lines.
        self.col = 0;
        let remaining = self.page_lines - self.line_in_page;
        for _ in 0..remaining {
            self.lines.push(Vec::new());
        }
        self.line_in_page = 0;
    }

    /// Tear off the printed sheet: discard all paper and start fresh.
    pub fn tear_off(&mut self) {
        self.lines = vec![Vec::new()];
        self.col = 0;
        self.line_in_page = 0;
    }

    /// The paper so far, one string per line (trailing blanks trimmed).
    pub fn paper(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| {
                let s: String = l.iter().map(|&b| b as char).collect();
                s.trim_end().to_string()
            })
            .collect()
    }

    /// Number of printed lines (including the current, possibly empty, line).
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Print-head position as (line, column), both from 0.
    pub fn head(&self) -> (usize, usize) {
        (self.lines.len() - 1, self.col)
    }
}
