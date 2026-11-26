//! Parallel ASCII keyboard, read with `INP 5`.
//!
//! A key press strobes a byte into the port and raises an EF "data ready" line
//! (EF2 by convention). Software polls that EF and reads the byte with `INP 5`,
//! which clears the strobe. This is independent of the VID-1 serial keyboard: a
//! machine may have either or both. The front end feeds the same key presses to
//! whichever input devices are fitted.
//!
//! Several keys pressed before the program reads are queued, so fast typing is
//! not lost; the strobe stays high until the queue drains.

use std::collections::VecDeque;

#[derive(Debug, Default, Clone)]
pub struct ParallelKeyboard {
    queue: VecDeque<u8>,
    /// The last byte returned by `INP 5` (what the port latch still holds).
    last: u8,
}

impl ParallelKeyboard {
    pub fn new() -> Self {
        ParallelKeyboard::default()
    }

    /// A key was pressed: queue its 7-bit ASCII code.
    pub fn press(&mut self, ascii: u8) {
        self.queue.push_back(ascii & 0x7F);
    }

    /// The EF "data ready" strobe: high while a byte is waiting to be read.
    pub fn ready(&self) -> bool {
        !self.queue.is_empty()
    }

    /// `INP 5`: return the next byte and clear its strobe. With nothing waiting,
    /// the port still holds the last byte read (as a real latch would).
    pub fn read(&mut self) -> u8 {
        if let Some(b) = self.queue.pop_front() {
            self.last = b;
        }
        self.last
    }

    /// Number of keys still queued (for tests and diagnostics).
    pub fn pending(&self) -> usize {
        self.queue.len()
    }
}
