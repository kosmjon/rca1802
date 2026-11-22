//! Giant Board bit-banged serial port (requirement 7).
//!
//! The Elf II side has no UART: software toggles Q to transmit and polls an EF line
//! to receive. This module is the other end of the cable, i.e. the UART in the VID-1:
//!
//! * **TX decoder**: watches Q changes (timestamped in machine cycles) and decodes
//!   asynchronous frames, sampling each bit at its centre and re-synchronising on
//!   every start bit, so the jitter of software delay loops is tolerated.
//! * **RX scheduler**: turns queued characters into line levels at machine-cycle
//!   times, which the CPU reads through EF3 or EF4.
//!
//! Line levels: *mark* (idle, stop bits, data 1) and *space* (start bit, data 0).
//! Default polarity ([verify] against the Giant Board schematic):
//! Q = 0 is mark, Q = 1 is space; space on RX makes the EF line asserted (`B3` taken).
//! `tx_inverted` / `rx_inverted` swap these.

use crate::config::{Parity, SerialConfig};
use std::collections::VecDeque;

/// Result of decoding one frame from the TX line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxOutput {
    Char(u8),
    /// Stop bit was space, or parity was wrong.
    FramingError,
}

#[derive(Debug, Clone, Copy)]
enum TxState {
    /// Waiting for a start bit. `armed` is false until the line has been mark, so a
    /// line stuck at space (break) is not read as a stream of characters.
    Idle { armed: bool },
    /// Receiving a frame that started (mark-to-space edge) at `start`.
    Frame { start: u64, sample: u8, value: u16 },
}

#[derive(Debug, Clone, Copy)]
struct RxFrame {
    start: u64,
    /// Line level for each bit, `true` = space.
    space: [bool; 12],
    nbits: u8,
}

pub struct BitBangSerial {
    cfg: SerialConfig,
    /// Machine cycles per bit.
    cpb: f64,
    cycles_per_ms: f64,
    // --- TX (Q -> VID-1)
    tx_space: bool,
    tx_state: TxState,
    tx_out: Vec<TxOutput>,
    // --- RX (VID-1 -> EF)
    rx_queue: VecDeque<(u8, u64)>,
    rx_frame: Option<RxFrame>,
    rx_idle_since: u64,
}

impl BitBangSerial {
    pub fn new(cfg: SerialConfig, cycles_per_second: u32) -> Self {
        let cps = f64::from(cycles_per_second);
        BitBangSerial {
            cfg,
            cpb: cps / f64::from(cfg.baud),
            cycles_per_ms: cps / 1000.0,
            // Q = 0 at power-on: mark with normal polarity, space (break) if inverted.
            tx_space: cfg.tx_inverted,
            tx_state: TxState::Idle {
                armed: !cfg.tx_inverted,
            },
            tx_out: Vec::new(),
            rx_queue: VecDeque::new(),
            rx_frame: None,
            rx_idle_since: 0,
        }
    }

    pub fn config(&self) -> &SerialConfig {
        &self.cfg
    }

    /// Bits in a frame after the start bit: data, parity, first stop bit.
    fn frame_samples(&self) -> u8 {
        self.cfg.data_bits + u8::from(self.cfg.parity != Parity::None) + 1
    }

    fn parity_bit(&self, data: u16) -> bool {
        let ones = data.count_ones() % 2 == 1;
        match self.cfg.parity {
            Parity::Even => ones,
            Parity::Odd => !ones,
            Parity::None => false,
        }
    }

    // ------------------------------------------------------------------ TX decoder

    /// Q changed at machine-cycle time `now`.
    pub fn q_changed(&mut self, now: u64, q: bool) {
        self.advance_tx(now);
        let space = q != self.cfg.tx_inverted;
        match self.tx_state {
            TxState::Idle { armed } => {
                if !space {
                    self.tx_state = TxState::Idle { armed: true };
                } else if armed && !self.tx_space {
                    self.tx_state = TxState::Frame {
                        start: now,
                        sample: 0,
                        value: 0,
                    };
                }
            }
            TxState::Frame { .. } => {}
        }
        self.tx_space = space;
    }

    /// Take any samples due up to (not including) time `now`.
    fn advance_tx(&mut self, now: u64) {
        while let TxState::Frame {
            start,
            sample,
            value,
        } = self.tx_state
        {
            // Sample 0 is the centre of the start bit, sample k the centre of bit k.
            let t = start as f64 + (f64::from(sample) + 0.5) * self.cpb;
            if t >= now as f64 {
                return;
            }
            let mark = !self.tx_space;
            let data_bits = self.cfg.data_bits;
            let last = self.frame_samples();
            if sample == 0 {
                if mark {
                    // Glitch, not a start bit.
                    self.tx_state = TxState::Idle { armed: true };
                    continue;
                }
            } else if sample <= data_bits {
                let v = if mark {
                    value | (1 << (sample - 1))
                } else {
                    value
                };
                self.tx_state = TxState::Frame {
                    start,
                    sample: sample + 1,
                    value: v,
                };
                continue;
            } else if sample < last {
                // Parity bit.
                if mark != self.parity_bit(value) {
                    self.finish_tx(TxOutput::FramingError);
                    continue;
                }
            } else {
                // First stop bit.
                let out = if mark {
                    TxOutput::Char(value as u8)
                } else {
                    TxOutput::FramingError
                };
                self.finish_tx(out);
                continue;
            }
            self.tx_state = TxState::Frame {
                start,
                sample: sample + 1,
                value,
            };
        }
    }

    fn finish_tx(&mut self, out: TxOutput) {
        self.tx_out.push(out);
        // Re-arm only once the line is mark again (resynchronise on the next start).
        self.tx_state = TxState::Idle {
            armed: !self.tx_space,
        };
    }

    /// Decoded characters and errors since the last call.
    pub fn take_tx(&mut self) -> std::vec::Drain<'_, TxOutput> {
        self.tx_out.drain(..)
    }

    // ------------------------------------------------------------------ RX scheduler

    /// Queue a character to send, after at least `gap_ms` of idle line.
    pub fn send(&mut self, byte: u8, gap_ms: u32) {
        let gap = (f64::from(gap_ms) * self.cycles_per_ms) as u64;
        self.rx_queue.push_back((byte, gap));
    }

    /// Queue typed text with the configured paste pacing. `\n` and `\r\n` become CR.
    pub fn paste(&mut self, text: &str) {
        let mut prev_cr = false;
        for ch in text.chars() {
            let b = match ch {
                '\n' if prev_cr => {
                    prev_cr = false;
                    continue;
                }
                '\n' | '\r' => 0x0D,
                c if c.is_ascii() => c as u8,
                _ => continue,
            };
            prev_cr = ch == '\r';
            let gap = if self.rx_queue.back().is_some_and(|&(b, _)| b == 0x0D) {
                self.cfg.paste_cr_gap_ms
            } else {
                self.cfg.paste_gap_ms
            };
            self.send(b, gap);
        }
    }

    /// Characters waiting to be sent (not counting the one in progress).
    pub fn rx_pending(&self) -> usize {
        self.rx_queue.len()
    }

    fn build_rx_frame(&self, byte: u8, start: u64) -> RxFrame {
        let mask = (1u16 << self.cfg.data_bits) - 1;
        let data = u16::from(byte) & mask;
        let mut space = [false; 12];
        let mut n = 0;
        space[n] = true; // start bit
        n += 1;
        for k in 0..self.cfg.data_bits {
            space[n] = data & (1 << k) == 0;
            n += 1;
        }
        if self.cfg.parity != Parity::None {
            space[n] = !self.parity_bit(data);
            n += 1;
        }
        n += self.cfg.stop_bits as usize; // stop bits are mark (false)
        RxFrame {
            start,
            space,
            nbits: n as u8,
        }
    }

    fn advance_rx(&mut self, now: u64) {
        if let Some(f) = self.rx_frame {
            let end = f.start + (f64::from(f.nbits) * self.cpb).ceil() as u64;
            if now < end {
                return;
            }
            self.rx_frame = None;
            self.rx_idle_since = end;
        }
        if let Some(&(byte, gap)) = self.rx_queue.front() {
            if now >= self.rx_idle_since + gap {
                self.rx_queue.pop_front();
                self.rx_frame = Some(self.build_rx_frame(byte, now));
            }
        }
    }

    /// RX line is at space at time `now`.
    fn rx_space(&self, now: u64) -> bool {
        match self.rx_frame {
            Some(f) if now >= f.start => {
                let idx = ((now - f.start) as f64 / self.cpb) as usize;
                idx < f.nbits as usize && f.space[idx]
            }
            _ => false,
        }
    }

    /// Level of the RX EF line as the CPU sees it (`true` = asserted).
    pub fn rx_ef(&self, now: u64) -> bool {
        self.rx_space(now) != self.cfg.rx_inverted
    }

    /// Advance both directions to time `now`.
    pub fn tick(&mut self, now: u64) {
        self.advance_tx(now);
        self.advance_rx(now);
    }
}
