//! CDP1861 tests (task 7) using the project test programs in `tests/programs/`.

use crate::pixie::*;
use crate::*;
use cdp1802::{Bus, Signals};

const PROG_64X128: &[u8] = include_bytes!("../../../tests/programs/pixie64x128.bin");
const PROG_64X32: &[u8] = include_bytes!("../../../tests/programs/pixie64x32.bin");

/// A deterministic picture: no two rows alike, every bit position used.
fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| ((i * 37) as u8) ^ ((i >> 3) as u8).rotate_left(3))
        .collect()
}

fn machine_with(prog: &[u8], picture: &[u8]) -> Machine {
    let mut m = Machine::new(Config::default()).unwrap();
    m.poke(0x0000, prog);
    m.poke(0x0100, picture);
    m.set_switch(Switch::Run, true);
    m
}

/// Run until `n` PixieFrame events have been seen.
fn run_frames(m: &mut Machine, n: usize) {
    let mut seen = 0;
    for _ in 0..(n + 2) * 10 {
        m.run(u64::from(FRAME_CYCLES) / 4);
        seen += m
            .events()
            .iter()
            .filter(|e| **e == Event::PixieFrame)
            .count();
        if seen >= n {
            return;
        }
    }
    panic!("only {seen} of {n} frames");
}

#[test]
fn frame_rate_is_about_60_hz() {
    let per_second = Config::default().cycles_per_second() as f64;
    let hz = per_second / f64::from(FRAME_CYCLES);
    assert!((60.0..62.0).contains(&hz), "{hz:.2} Hz");
}

#[test]
fn display_64x128_shows_the_buffer() {
    let pic = pattern(1024);
    let mut m = machine_with(PROG_64X128, &pic);
    run_frames(&mut m, 3);
    assert!(m.pixie_on());
    assert_eq!(&m.pixie_frame().unwrap()[..], &pic[..]);
}

#[test]
fn display_64x32_repeats_each_row_four_times() {
    let pic = pattern(256);
    let mut m = machine_with(PROG_64X32, &pic);
    run_frames(&mut m, 3);
    let frame = m.pixie_frame().unwrap();
    for line in 0..128 {
        let row = line / 4;
        assert_eq!(
            &frame[line * 8..line * 8 + 8],
            &pic[row * 8..row * 8 + 8],
            "scanline {line}"
        );
    }
}

/// A bus around a bare 1861 that acts like a CPU servicing requests at once.
struct Probe {
    p: Cdp1861,
}
impl Bus for Probe {
    fn read(&mut self, _: u16) -> u8 {
        0
    }
    fn write(&mut self, _: u16, _: u8) {}
    fn out(&mut self, _: u8, _: u8) {}
    fn inp(&mut self, _: u8) -> u8 {
        0
    }
    fn ef(&self, _: u8) -> bool {
        false
    }
    fn set_q(&mut self, _: bool) {}
    fn dma_in(&mut self) -> u8 {
        0
    }
    fn dma_out(&mut self, v: u8) {
        self.p.dma_out(v)
    }
    fn signals(&self) -> Signals {
        Signals {
            dma_in: false,
            dma_out: self.p.dma_request(),
            interrupt: self.p.interrupt(),
        }
    }
    fn tick(&mut self, n: u32) {
        self.p.tick(n)
    }
}

#[test]
fn chip_timing_per_frame() {
    // Count, cycle by cycle over two frames, how long INT is held, how many DMA-OUT
    // cycles occur per line, and where EF1 is asserted.
    let mut p = Cdp1861::new();
    p.set_on(true);
    let mut int_run = 0u32;
    let mut int_runs = Vec::new();
    let mut dma_per_line = std::collections::BTreeMap::<u32, u32>::new();
    let mut ef1_lines = std::collections::BTreeSet::new();
    for c in 0..2 * FRAME_CYCLES {
        let cyc = c % FRAME_CYCLES;
        if p.interrupt() {
            int_run += 1;
        } else if int_run > 0 {
            int_runs.push(int_run);
            int_run = 0;
        }
        if p.ef1() {
            ef1_lines.insert(cyc / CYCLES_PER_LINE);
        }
        if p.dma_request() {
            *dma_per_line.entry(cyc / CYCLES_PER_LINE).or_default() += 1;
            p.dma_out(0xAA);
        }
        p.tick(1);
    }
    assert_eq!(
        int_runs,
        [INT_CYCLES, INT_CYCLES],
        "INT held 28 cycles once per frame"
    );
    assert_eq!(dma_per_line.len(), ACTIVE_LINES as usize);
    assert!(dma_per_line
        .values()
        .all(|&n| n == 2 * BYTES_PER_LINE as u32));
    assert_eq!(*dma_per_line.keys().next().unwrap(), FIRST_ACTIVE_LINE);
    let expect: std::collections::BTreeSet<u32> = (76..80).chain(204..208).collect();
    assert_eq!(ef1_lines, expect);
    assert_eq!(p.frame()[0], 0xAA);
}

#[test]
fn interrupt_ack_aligns_first_dma_29_cycles_later() {
    let mut bus = Probe { p: Cdp1861::new() };
    bus.p.set_on(true);
    while !bus.p.interrupt() {
        bus.p.tick(1);
    }
    bus.p.tick(2); // some interrupt latency
    bus.p.interrupt_ack();
    bus.p.tick(1); // the acknowledge cycle
    let mut n = 1;
    while !bus.p.dma_request() {
        bus.p.tick(1);
        n += 1;
    }
    assert_eq!(n, DMA_AFTER_ACK);
    let _ = bus.signals();
}

#[test]
fn no_interrupts_or_dma_when_off() {
    let mut p = Cdp1861::new();
    for _ in 0..FRAME_CYCLES * 2 {
        assert!(!p.interrupt() && !p.dma_request() && !p.ef1());
        p.tick(1);
    }
}

#[test]
fn reset_and_out1_turn_display_off() {
    let pic = pattern(1024);
    let mut m = machine_with(PROG_64X128, &pic);
    run_frames(&mut m, 2);
    assert!(m.pixie_on());
    m.set_switch(Switch::Run, false);
    assert!(!m.pixie_on());
    assert!(
        m.pixie_frame().unwrap().iter().all(|&b| b == 0),
        "blank frame"
    );

    // From software: LDI 20, PLO R2, SEX R2, INP 1 (on), OUT 1 (off), IDL.
    let mut m = Machine::new(Config::default()).unwrap();
    m.poke(0, &[0xF8, 0x20, 0xA2, 0xE2, 0x69, 0x61, 0x00]);
    m.set_switch(Switch::Run, true);
    for _ in 0..5 {
        m.step(); // initialization cycle, LDI, PLO, SEX, INP 1
    }
    assert!(m.pixie_on(), "INP 1 turned it on");
    m.step(); // OUT 1
    assert!(!m.pixie_on(), "OUT 1 turned it off");
}

#[test]
fn on_off_direction_is_configurable() {
    let mut m = Machine::new(Config {
        pixie: Some(PixieConfig { on_is_inp: false }),
        ..Config::default()
    })
    .unwrap();
    m.poke(0, &[0x61, 0x00]); // OUT 1 turns it on in this configuration
    m.set_switch(Switch::Run, true);
    m.run(10);
    assert!(m.pixie_on());
}

#[test]
fn no_pixie_when_not_fitted() {
    let mut m = Machine::new(Config {
        pixie: None,
        ..Config::default()
    })
    .unwrap();
    m.poke(0, &[0x69, 0x30, 0x01]); // INP 1, BR 0001
    m.set_switch(Switch::Run, true);
    m.run(u64::from(FRAME_CYCLES) * 2);
    assert!(m.pixie_frame().is_none());
    assert!(!m.events().contains(&Event::PixieFrame));
}

#[test]
fn three_cycle_instruction_in_main_loop_shifts_lines() {
    // Same 64x128 program, but the main loop is NOP (3) + BR (2): 5 cycles, so an
    // instruction is often in progress when a DMA burst should start. The burst
    // starts late and the line is drawn shifted right by the delay.
    let pic = pattern(1024);
    let mut prog = PROG_64X128.to_vec();
    prog[0x12..0x14].copy_from_slice(&[0x30, 0x40]); // WAIT: BR 0040
    prog.resize(0x43, 0);
    prog[0x40..0x43].copy_from_slice(&[0xC4, 0x30, 0x40]); // 0040: NOP; BR 0040
    let mut m = machine_with(&prog, &pic);
    run_frames(&mut m, 3);
    let frame = m.pixie_frame().unwrap();
    let mut shifted = 0;
    for line in 0..128 {
        let got = &frame[line * 8..line * 8 + 8];
        let want = &pic[line * 8..line * 8 + 8];
        let d = (0..3)
            .find(|&d| got[..d].iter().all(|&b| b == 0) && got[d..] == want[..8 - d])
            .unwrap_or_else(|| panic!("line {line}: {got:02X?} is not a shift of {want:02X?}"));
        if d > 0 {
            shifted += 1;
        }
    }
    assert!(shifted > 0, "some lines must be shifted");
    assert!(shifted < 128, "not every line");
}
