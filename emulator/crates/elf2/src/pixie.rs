//! CDP1861 "Pixie" video display controller (requirement 6, `pixie1861.md`).
//!
//! The chip is a counter of machine cycles: 14 per scanline, 262 scanlines per
//! frame. While the display is on it asserts /INT before the active area, asserts
//! DMA-OUT on each of the 128 active lines until it has received 8 bytes, and drives
//! EF1 (display status). It keeps no video memory: the bytes the CPU sends by DMA
//! are the picture, so the resolution comes from the software (64x128, 64x64, 64x32).
//!
//! Timing constants marked [verify] are to be checked against the CDP1861 datasheet.

/// Machine cycles per scanline.
pub const CYCLES_PER_LINE: u32 = 14;
/// Scanlines per frame (NTSC).
pub const LINES_PER_FRAME: u32 = 262;
/// Machine cycles per frame.
pub const FRAME_CYCLES: u32 = CYCLES_PER_LINE * LINES_PER_FRAME;
/// First active display line.
pub const FIRST_ACTIVE_LINE: u32 = 80;
/// Number of active display lines (one DMA burst each).
pub const ACTIVE_LINES: u32 = 128;
/// Bytes per active line (64 pixels).
pub const BYTES_PER_LINE: usize = 8;
/// Frame bitmap size: 128 lines x 8 bytes.
pub const FRAME_BYTES: usize = ACTIVE_LINES as usize * BYTES_PER_LINE;
/// /INT is held for this many machine cycles (requirement 6.2).
pub const INT_CYCLES: u32 = 28;
/// The first DMA cycle comes this many cycles after the interrupt-acknowledge cycle
/// starts: an interrupt routine of exactly 28 cycles is followed by the first burst.
pub const DMA_AFTER_ACK: u32 = 29;
/// Cycle within the frame at which /INT is asserted (two lines before the display,
/// allowing for interrupt latency). [verify]
pub const INT_START: u32 = FIRST_ACTIVE_LINE * CYCLES_PER_LINE - DMA_AFTER_ACK - 1;
/// EF1 (display status) is asserted during these line ranges: the four lines before
/// the display starts and the last four active lines. [verify] against the datasheet;
/// `pixie1861.md` mentions only the second window.
pub const EF1_LINES: [(u32, u32); 2] = [
    (FIRST_ACTIVE_LINE - 4, FIRST_ACTIVE_LINE),
    (
        FIRST_ACTIVE_LINE + ACTIVE_LINES - 4,
        FIRST_ACTIVE_LINE + ACTIVE_LINES,
    ),
];

const FIRST_ACTIVE_CYCLE: u32 = FIRST_ACTIVE_LINE * CYCLES_PER_LINE;
const END_ACTIVE_CYCLE: u32 = (FIRST_ACTIVE_LINE + ACTIVE_LINES) * CYCLES_PER_LINE;

pub struct Cdp1861 {
    pub(crate) on: bool,
    /// Machine cycle within the frame, 0..FRAME_CYCLES.
    pub(crate) cycle: u32,
    line_buf: [u8; BYTES_PER_LINE],
    line_bytes: u8,
    /// Column (cycle within the line) at which the first DMA byte of the line arrived.
    line_delay: u8,
    pub(crate) frame: [u8; FRAME_BYTES],
    /// Set when a frame has been completed (or the display turned off); cleared by the reader.
    pub frame_ready: bool,
}

impl Default for Cdp1861 {
    fn default() -> Self {
        Self::new()
    }
}

impl Cdp1861 {
    pub fn new() -> Self {
        Cdp1861 {
            on: false,
            cycle: 0,
            line_buf: [0; BYTES_PER_LINE],
            line_bytes: 0,
            line_delay: 0,
            frame: [0; FRAME_BYTES],
            frame_ready: false,
        }
    }

    /// Display on/off (INP 1 / OUT 1). Turning it off publishes a blank frame.
    pub fn set_on(&mut self, on: bool) {
        if self.on && !on {
            self.frame = [0; FRAME_BYTES];
            self.frame_ready = true;
        }
        self.on = on;
        self.line_bytes = 0;
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// /CLEAR resets the chip: display off.
    pub fn reset(&mut self) {
        self.set_on(false);
    }

    #[inline]
    fn line(&self) -> u32 {
        self.cycle / CYCLES_PER_LINE
    }

    #[inline]
    fn in_active_area(&self) -> bool {
        (FIRST_ACTIVE_CYCLE..END_ACTIVE_CYCLE).contains(&self.cycle)
    }

    /// DMA-OUT request: active line and fewer than 8 bytes received on it.
    #[inline]
    pub fn dma_request(&self) -> bool {
        self.on && self.in_active_area() && (self.line_bytes as usize) < BYTES_PER_LINE
    }

    /// /INT request (level, held for `INT_CYCLES`).
    #[inline]
    pub fn interrupt(&self) -> bool {
        self.on && (INT_START..INT_START + INT_CYCLES).contains(&self.cycle)
    }

    /// EF1 display status.
    #[inline]
    pub fn ef1(&self) -> bool {
        let l = self.line();
        self.on && EF1_LINES.iter().any(|&(a, b)| (a..b).contains(&l))
    }

    /// A DMA-OUT cycle delivered `v`.
    pub fn dma_out(&mut self, v: u8) {
        if !self.dma_request() {
            return;
        }
        if self.line_bytes == 0 {
            self.line_delay = (self.cycle % CYCLES_PER_LINE) as u8;
        }
        self.line_buf[self.line_bytes as usize] = v;
        self.line_bytes += 1;
    }

    /// Interrupt acknowledged: align the line timing so the first DMA burst starts
    /// `DMA_AFTER_ACK` cycles after this acknowledge cycle began.
    pub fn interrupt_ack(&mut self) {
        if self.interrupt() {
            self.cycle = FIRST_ACTIVE_CYCLE - DMA_AFTER_ACK;
        }
    }

    /// Advance by `n` machine cycles.
    pub fn tick(&mut self, n: u32) {
        for _ in 0..n {
            let next = self.cycle + 1;
            if self.in_active_area() && next.is_multiple_of(CYCLES_PER_LINE) {
                self.commit_line();
                if next == END_ACTIVE_CYCLE && self.on {
                    self.frame_ready = true;
                }
            }
            self.cycle = if next == FRAME_CYCLES { 0 } else { next };
        }
    }

    /// Store the line just finished. Bytes that arrived late (DMA delayed by an
    /// instruction) are shifted right by the delay, one byte (8 pixels) per cycle.
    fn commit_line(&mut self) {
        let idx = (self.line() - FIRST_ACTIVE_LINE) as usize * BYTES_PER_LINE;
        let row = &mut self.frame[idx..idx + BYTES_PER_LINE];
        row.fill(0);
        if self.on {
            let d = self.line_delay as usize;
            for k in 0..self.line_bytes as usize {
                if d + k < BYTES_PER_LINE {
                    row[d + k] = self.line_buf[k];
                }
            }
        }
        self.line_bytes = 0;
    }

    /// The last completed frame: 128 lines x 8 bytes, MSB = leftmost pixel.
    pub fn frame(&self) -> &[u8; FRAME_BYTES] {
        &self.frame
    }
}
