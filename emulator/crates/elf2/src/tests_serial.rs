//! Giant Board serial port tests (task 8).

use crate::serial::{BitBangSerial, TxOutput};
use crate::*;
use proptest::prelude::*;

const ECHO: &[u8] = include_bytes!("../../../tests/programs/serial_echo.bin");
const CPS: u32 = 223_721; // machine cycles per second at 1.789773 MHz

fn cfg(baud: u32, bits: u8, parity: Parity, stop: u8, inv_tx: bool, inv_rx: bool) -> SerialConfig {
    SerialConfig {
        baud,
        data_bits: bits,
        parity,
        stop_bits: stop,
        tx_inverted: inv_tx,
        rx_inverted: inv_rx,
        ..SerialConfig::default()
    }
}

fn parity_of(data: u16, p: Parity) -> Option<bool> {
    let ones = data.count_ones() % 2 == 1;
    match p {
        Parity::None => None,
        Parity::Even => Some(ones),
        Parity::Odd => Some(!ones),
    }
}

/// Space-level (true) for each bit of a frame, in transmission order.
fn frame_bits(c: &SerialConfig, byte: u8) -> Vec<bool> {
    let data = u16::from(byte) & ((1 << c.data_bits) - 1);
    let mut v = vec![true]; // start
    for k in 0..c.data_bits {
        v.push(data & (1 << k) == 0);
    }
    if let Some(p) = parity_of(data, c.parity) {
        v.push(!p);
    }
    v.extend(std::iter::repeat_n(false, c.stop_bits as usize));
    v
}

/// Drive `q_changed` with a frame whose bit boundaries are at `k * cpb * rate + jitter[k]`.
fn drive_tx(
    s: &mut BitBangSerial,
    c: &SerialConfig,
    bits: &[bool],
    t0: u64,
    rate: f64,
    jitter: &[f64],
) -> u64 {
    let cpb = f64::from(CPS) / f64::from(c.baud);
    let mut level = false; // idle = mark
    s.q_changed(t0 - 1, c.tx_inverted); // Q at its idle (mark) level
    let mut last_t = t0;
    for (k, &space) in bits.iter().enumerate() {
        if space != level {
            let t = (t0 as f64
                + k as f64 * cpb * rate
                + jitter.get(k).copied().unwrap_or(0.0) * cpb) as u64;
            let t = t.max(last_t);
            s.q_changed(t, space != c.tx_inverted);
            level = space;
            last_t = t;
        }
    }
    let end = (t0 as f64 + bits.len() as f64 * cpb * rate) as u64;
    if level {
        s.q_changed(end, c.tx_inverted); // back to mark
    }
    end
}

fn decode(s: &mut BitBangSerial, until: u64) -> Vec<TxOutput> {
    s.tick(until);
    s.take_tx().collect()
}

// ------------------------------------------------------------------ TX decoder

#[test]
fn decodes_a_clean_frame() {
    let c = cfg(300, 8, Parity::None, 1, false, false);
    let mut s = BitBangSerial::new(c, CPS);
    let end = drive_tx(&mut s, &c, &frame_bits(&c, 0x5A), 1000, 1.0, &[]);
    assert_eq!(decode(&mut s, end + 5000), [TxOutput::Char(0x5A)]);
}

#[test]
fn back_to_back_frames_and_idle_gaps() {
    let c = cfg(300, 8, Parity::None, 1, false, false);
    let mut s = BitBangSerial::new(c, CPS);
    let mut t = 500;
    for &b in b"Hi!" {
        t = drive_tx(&mut s, &c, &frame_bits(&c, b), t, 1.0, &[]);
    }
    t += 20_000; // idle gap
    t = drive_tx(&mut s, &c, &frame_bits(&c, 0x0D), t, 1.0, &[]);
    let got = decode(&mut s, t + 5000);
    assert_eq!(
        got,
        [b'H', b'i', b'!', 0x0D].map(TxOutput::Char),
        "got {got:?}"
    );
}

#[test]
fn framing_and_parity_errors_are_reported_and_decoding_resynchronises() {
    let c = cfg(300, 8, Parity::Even, 1, false, false);
    let mut s = BitBangSerial::new(c, CPS);
    // Wrong parity bit.
    let mut bad = frame_bits(&c, 0x41);
    let p = 9;
    bad[p] = !bad[p];
    let mut t = drive_tx(&mut s, &c, &bad, 1000, 1.0, &[]);
    // Stop bit at space (framing error).
    let mut bad = frame_bits(&c, 0x42);
    *bad.last_mut().unwrap() = true;
    t = drive_tx(&mut s, &c, &bad, t + 3000, 1.0, &[]);
    // A good frame afterwards.
    t = drive_tx(&mut s, &c, &frame_bits(&c, 0x43), t + 3000, 1.0, &[]);
    assert_eq!(
        decode(&mut s, t + 5000),
        [
            TxOutput::FramingError,
            TxOutput::FramingError,
            TxOutput::Char(0x43)
        ]
    );
}

#[test]
fn break_condition_is_not_a_stream_of_characters() {
    let c = cfg(300, 8, Parity::None, 1, false, false);
    let mut s = BitBangSerial::new(c, CPS);
    s.q_changed(1000, true); // Q = 1 is space; held for 20 bit times
    let got = decode(&mut s, 1000 + 20 * 746);
    assert_eq!(got, [TxOutput::FramingError]);
}

#[test]
fn glitch_shorter_than_half_a_bit_is_ignored() {
    let c = cfg(300, 8, Parity::None, 1, false, false);
    let mut s = BitBangSerial::new(c, CPS);
    s.q_changed(1000, true);
    s.q_changed(1100, false);
    assert!(decode(&mut s, 20_000).is_empty());
}

proptest! {
    /// Property 6 (TX): any byte at any supported setting decodes back to itself with
    /// each bit boundary displaced by up to +-10% of a bit, and with a constant rate
    /// error of up to +-3% (at 11 bits per frame the two together stay under the
    /// half-bit limit any centre-sampling receiver has).
    #[test]
    fn prop_tx_decode_with_jitter(
        byte in any::<u8>(),
        baud in prop::sample::select(vec![110u32, 300, 600, 1200, 2400]),
        bits in 7u8..=8, par in 0u8..3, stop in 1u8..=2, inv in any::<bool>(),
        rate in 0.97f64..1.03, jitter in proptest::collection::vec(-0.1f64..0.1, 12))
    {
        let parity = [Parity::None, Parity::Even, Parity::Odd][par as usize];
        let c = cfg(baud, bits, parity, stop, inv, false);
        let mut s = BitBangSerial::new(c, CPS);
        // The line is idle (mark) before the frame: Q = 1 when inverted.
        let bits_v = frame_bits(&c, byte);
        let end = drive_tx(&mut s, &c, &bits_v, 5000, rate, &jitter);
        let want = byte & ((1u16 << bits) - 1) as u8;
        prop_assert_eq!(decode(&mut s, end + 20_000), vec![TxOutput::Char(want)]);
    }
}

// ------------------------------------------------------------------ RX scheduler

/// A software-style receiver: wait for the EF line to assert (start bit), then sample
/// each bit at its centre, relative to the start edge. Returns the byte.
fn software_receive(s: &mut BitBangSerial, c: &SerialConfig, from: u64) -> Option<(u8, u64)> {
    let cpb = f64::from(CPS) / f64::from(c.baud);
    let mut t = from;
    let limit = from + (30.0 * cpb) as u64 + 2_000_000;
    while t < limit {
        s.tick(t);
        let space = s.rx_ef(t) != c.rx_inverted; // undo the line inversion to get "space"
        if space {
            let mut data = 0u16;
            for k in 0..c.data_bits {
                let ts = t + ((f64::from(k) + 1.5) * cpb) as u64;
                s.tick(ts);
                let sp = s.rx_ef(ts) != c.rx_inverted;
                if !sp {
                    data |= 1 << k;
                }
            }
            let mut next = c.data_bits as f64 + 1.5;
            if let Some(p) = parity_of(data, c.parity) {
                let ts = t + (next * cpb) as u64;
                s.tick(ts);
                let sp = s.rx_ef(ts) != c.rx_inverted;
                assert_eq!(!sp, p, "parity bit");
                next += 1.0;
            }
            for _ in 0..c.stop_bits {
                let ts = t + (next * cpb) as u64;
                s.tick(ts);
                assert!(s.rx_ef(ts) == c.rx_inverted, "stop bit must be mark");
                next += 1.0;
            }
            return Some((data as u8, t + (next * cpb) as u64));
        }
        t += 20;
    }
    None
}

proptest! {
    /// Property 6 (RX): the scheduler followed by a software-style receive loop
    /// recovers every byte, at any supported setting and polarity.
    #[test]
    fn prop_rx_round_trip(
        bytes in proptest::collection::vec(any::<u8>(), 1..4),
        baud in prop::sample::select(vec![110u32, 300, 1200, 2400]),
        bits in 7u8..=8, par in 0u8..3, stop in 1u8..=2, inv in any::<bool>())
    {
        let parity = [Parity::None, Parity::Even, Parity::Odd][par as usize];
        let c = cfg(baud, bits, parity, stop, false, inv);
        let mut s = BitBangSerial::new(c, CPS);
        for &b in &bytes { s.send(b, 0); }
        let mut t = 0;
        for &b in &bytes {
            let (got, end) = software_receive(&mut s, &c, t).expect("a character arrives");
            prop_assert_eq!(got, b & ((1u16 << bits) - 1) as u8);
            t = end;
        }
    }
}

#[test]
fn idle_rx_line_is_mark() {
    for inv in [false, true] {
        let c = cfg(300, 8, Parity::None, 1, false, inv);
        let mut s = BitBangSerial::new(c, CPS);
        s.tick(1000);
        // Normal polarity: mark = EF not asserted. Inverted: asserted.
        assert_eq!(s.rx_ef(1000), inv);
    }
}

#[test]
fn paste_pacing_and_line_endings() {
    let c = SerialConfig {
        paste_gap_ms: 10,
        paste_cr_gap_ms: 100,
        ..SerialConfig::default()
    };
    let mut s = BitBangSerial::new(c, CPS);
    s.paste("a\r\nb\nc");
    assert_eq!(s.rx_pending(), 5, "CRLF and LF each become one CR");
    // Receive them and check the gaps: 'a' immediately, CR after 10 ms, 'b' after the
    // 100 ms CR gap.
    let ms = f64::from(CPS) / 1000.0;
    let mut t = 0u64;
    let mut starts = Vec::new();
    let mut got = Vec::new();
    for _ in 0..5 {
        let (b, end) = software_receive(&mut s, &c, t).unwrap();
        let cpb = f64::from(CPS) / 300.0;
        starts.push(end as f64 - 10.0 * cpb);
        got.push(b);
        t = end;
    }
    assert_eq!(got, [b'a', 0x0D, b'b', 0x0D, b'c']);
    let gap = |i: usize| (starts[i + 1] - starts[i]) / ms - 10.0 * 1000.0 / 300.0;
    assert!((gap(0) - 10.0).abs() < 2.0, "gap after a: {:.1} ms", gap(0));
    assert!(
        (gap(1) - 100.0).abs() < 2.0,
        "gap after CR: {:.1} ms",
        gap(1)
    );
}

// ------------------------------------------------------------------ whole machine

fn echo_machine(cfg: Config) -> Machine {
    let mut m = Machine::new(cfg).unwrap();
    m.poke(0, ECHO);
    m.set_switch(Switch::Run, true);
    m
}

/// Run `cycles` and collect the characters the Elf II sent.
fn run_collect(m: &mut Machine, cycles: u64) -> Vec<Event> {
    let mut out = Vec::new();
    let mut done = 0;
    while done < cycles {
        done += m.run(5_000).cycles;
        out.extend(
            m.events()
                .iter()
                .filter(|e| matches!(e, Event::TermChar(_) | Event::TermFramingError)),
        );
    }
    out
}

#[test]
fn echo_program_echoes_typed_characters() {
    let mut m = echo_machine(Config::default());
    let mut seen = Vec::new();
    for &b in b"Hello, Elf!" {
        m.term_key(b);
        seen.extend(run_collect(&mut m, 25_000)); // ~2 frame times
    }
    seen.extend(run_collect(&mut m, 30_000));
    let text: Vec<u8> = seen
        .iter()
        .map(|e| match e {
            Event::TermChar(c) => *c,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(String::from_utf8_lossy(&text), "Hello, Elf!");
}

#[test]
fn echo_program_handles_pasted_text_all_bytes() {
    let mut m = echo_machine(Config::default());
    let text: String = (0u8..=127)
        .filter(|b| !matches!(b, 10 | 13))
        .map(|b| b as char)
        .collect();
    m.term_paste(&text);
    let mut seen = Vec::new();
    let mut guard = 0;
    while seen.len() < text.len() && guard < 3000 {
        seen.extend(run_collect(&mut m, 25_000));
        guard += 1;
    }
    let got: Vec<u8> = seen
        .iter()
        .map(|e| match e {
            Event::TermChar(c) => *c,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(got, text.as_bytes());
}

#[test]
fn echo_works_through_ef4_and_in_button_still_works() {
    // The same program reads EF3, so move the receiver to EF4 by patching B3 -> B4 and BN3 -> BN4.
    let mut prog = ECHO.to_vec();
    prog[0x05] = 0x3F; // BN4 WAIT
    prog[0x16] = 0x37; // B4 SPACE
    let mut m = Machine::new(Config {
        serial: Some(SerialConfig {
            rx_ef: 4,
            ..SerialConfig::default()
        }),
        ..Config::default()
    })
    .unwrap();
    m.poke(0, &prog);
    m.set_switch(Switch::Run, true);
    // IN held in run mode asserts EF4: the line is the OR of IN and serial RX.
    assert!(!m.ef(4));
    m.press_in(true);
    assert!(m.ef(4));
    m.press_in(false);
    m.term_key(b'Q');
    let seen = run_collect(&mut m, 40_000);
    assert_eq!(seen, [Event::TermChar(b'Q')]);
}

#[test]
fn no_serial_when_not_fitted() {
    let mut m = Machine::new(Config {
        serial: None,
        ..Config::default()
    })
    .unwrap();
    m.term_key(b'x');
    assert_eq!(m.term_pending(), 0);
    m.poke(0, &[0x7B, 0x7A, 0x30, 0x00]);
    m.set_switch(Switch::Run, true);
    assert!(run_collect(&mut m, 1000).is_empty());
}
