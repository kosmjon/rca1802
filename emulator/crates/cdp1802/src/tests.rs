//! Tests for CPU state, reset, control lines and the step loop (task 2).

use crate::test_bus::{IoEvent, TestBus};
use crate::{Cpu, CpuState, Signals};
use proptest::prelude::*;

/// A CPU that has left reset and finished its initialization cycle.
fn running() -> (Cpu, TestBus) {
    let mut bus = TestBus::new();
    let mut cpu = Cpu::new();
    cpu.set_control(true, true, &mut bus);
    assert_eq!(cpu.step(&mut bus), 1, "initialization cycle");
    bus.log.clear();
    (cpu, bus)
}

#[test]
fn version_is_set() {
    assert!(!crate::VERSION.is_empty());
}

#[test]
fn reset_values() {
    let mut bus = TestBus::new();
    let mut cpu = Cpu::new();
    cpu.r = [0x1234; 16];
    cpu.d = 0x55;
    cpu.df = true;
    cpu.t = 0x9A;
    cpu.x = 5;
    cpu.p = 7;
    cpu.i = 3;
    cpu.n = 4;
    cpu.q = true;
    cpu.ie = false;
    cpu.reset(&mut bus);
    assert_eq!((cpu.i, cpu.n, cpu.x, cpu.p), (0, 0, 0, 0));
    assert_eq!(cpu.r[0], 0);
    assert!(cpu.ie);
    assert!(!cpu.q);
    // Not affected by reset.
    assert_eq!(cpu.r[1], 0x1234);
    assert_eq!(cpu.d, 0x55);
    assert!(cpu.df);
    assert_eq!(cpu.t, 0x9A);
    assert_eq!(bus.log, [IoEvent::Q(false)]);
}

#[test]
fn control_lines_select_state() {
    let mut bus = TestBus::new();
    let mut cpu = Cpu::new();
    cpu.set_control(false, true, &mut bus);
    assert_eq!(cpu.state, CpuState::Reset);
    cpu.set_control(false, false, &mut bus);
    assert_eq!(cpu.state, CpuState::Load);
    cpu.set_control(true, true, &mut bus);
    assert_eq!(cpu.state, CpuState::Running);
    cpu.set_control(true, false, &mut bus);
    assert_eq!(cpu.state, CpuState::Pause);
    cpu.set_control(true, true, &mut bus);
    assert_eq!(cpu.state, CpuState::Running);
}

#[test]
fn leaving_reset_takes_one_init_cycle_then_fetches_from_0000() {
    let mut bus = TestBus::new();
    bus.load(0x0000, &[0x7B]); // SEQ (semantics come in task 3; only the fetch matters here)
    let mut cpu = Cpu::new();
    cpu.set_control(true, true, &mut bus);
    assert_eq!(cpu.step(&mut bus), 1);
    assert_eq!(cpu.r[0], 0, "no fetch during the initialization cycle");
    assert_eq!(cpu.step(&mut bus), 2);
    assert_eq!(cpu.r[0], 1, "fetched from M(0000) via R0");
    assert_eq!((cpu.i, cpu.n), (7, 0xB));
}

#[test]
fn reset_and_pause_do_nothing_but_time_passes() {
    let mut bus = TestBus::new();
    let mut cpu = Cpu::new();
    bus.signals = Signals {
        dma_in: true,
        dma_out: true,
        interrupt: true,
    };
    let before = cpu.clone();
    assert_eq!(cpu.step(&mut bus), 1);
    assert_eq!(cpu.r, before.r);
    assert_eq!(bus.ticked, 1);

    let (mut cpu, mut bus) = running();
    cpu.set_control(true, false, &mut bus);
    bus.signals = Signals {
        dma_in: true,
        dma_out: true,
        interrupt: true,
    };
    let before = cpu.clone();
    for _ in 0..5 {
        assert_eq!(cpu.step(&mut bus), 1);
    }
    assert_eq!(cpu.r, before.r);
    assert_eq!(cpu.ie, before.ie);
    assert!(bus.log.is_empty());
}

#[test]
fn load_mode_services_dma_in_only() {
    let mut bus = TestBus::new();
    let mut cpu = Cpu::new();
    cpu.set_control(false, false, &mut bus);
    bus.load(0x0000, &[0x00; 4]);
    bus.dma_in_value = 0xA5;
    bus.signals.dma_in = true;
    cpu.step(&mut bus);
    bus.dma_in_value = 0x5A;
    cpu.step(&mut bus);
    assert_eq!(&bus.mem[0..3], &[0xA5, 0x5A, 0x00]);
    assert_eq!(cpu.r[0], 2);
    // Repeating the same control lines must not reset R0.
    cpu.set_control(false, false, &mut bus);
    assert_eq!(cpu.r[0], 2);
    // No fetch, no interrupt in Load mode.
    bus.signals = Signals {
        dma_in: false,
        dma_out: false,
        interrupt: true,
    };
    cpu.step(&mut bus);
    assert_eq!(cpu.r[0], 2);
    assert!(cpu.ie);
}

#[test]
fn request_priority() {
    // DMA-IN beats DMA-OUT and INT.
    let (mut cpu, mut bus) = running();
    bus.signals = Signals {
        dma_in: true,
        dma_out: true,
        interrupt: true,
    };
    bus.dma_in_value = 0x11;
    assert_eq!(cpu.step(&mut bus), 1);
    assert_eq!(bus.log, [IoEvent::DmaIn(0x11)]);
    assert!(cpu.ie);

    // DMA-OUT beats INT.
    let (mut cpu, mut bus) = running();
    bus.signals = Signals {
        dma_in: false,
        dma_out: true,
        interrupt: true,
    };
    assert_eq!(cpu.step(&mut bus), 1);
    assert_eq!(bus.log, [IoEvent::DmaOut(0x00)]);
    assert!(cpu.ie);

    // INT ignored when IE = 0: an instruction executes instead.
    let (mut cpu, mut bus) = running();
    cpu.ie = false;
    bus.signals.interrupt = true;
    assert_eq!(cpu.step(&mut bus), 2);
    assert_eq!(cpu.r[0], 1, "instruction fetched");
}

#[test]
fn idle_waits_for_dma_or_enabled_interrupt() {
    let (mut cpu, mut bus) = running();
    // M(0000) = 00 = IDL
    assert_eq!(cpu.step(&mut bus), 2);
    assert_eq!(cpu.state, CpuState::Idle);
    for _ in 0..10 {
        assert_eq!(cpu.step(&mut bus), 1);
    }
    assert_eq!(cpu.r[0], 1, "no fetch while idle");

    // Interrupt with IE = 0 does not wake it.
    cpu.ie = false;
    bus.signals.interrupt = true;
    cpu.step(&mut bus);
    assert_eq!(cpu.state, CpuState::Idle);

    // DMA-OUT wakes it.
    bus.signals = Signals {
        dma_in: false,
        dma_out: true,
        interrupt: false,
    };
    cpu.step(&mut bus);
    assert_eq!(cpu.state, CpuState::Running);
}

#[test]
fn instruction_cycles() {
    for op in 0..=255u8 {
        let expect = if (0xC0..=0xCF).contains(&op) { 3 } else { 2 };
        assert_eq!(Cpu::instruction_cycles(op), expect, "opcode {op:02X}");
    }
}

#[test]
fn cycle_counter_matches_ticks() {
    let (mut cpu, mut bus) = running();
    bus.load(0x0000, &[0xC4, 0xC4, 0x7B, 0x00]);
    let start = cpu.cycles;
    let t0 = bus.ticked;
    let mut sum = 0u64;
    for _ in 0..8 {
        sum += u64::from(cpu.step(&mut bus));
    }
    assert_eq!(cpu.cycles - start, sum);
    assert_eq!(bus.ticked - t0, sum);
    assert_eq!(sum, 3 + 3 + 2 + 2 + 4); // NOP, NOP, SEQ, IDL, then 4 idle cycles
}

fn arb_cpu() -> impl Strategy<Value = Cpu> {
    (
        any::<[u16; 16]>(),
        any::<u8>(),
        any::<bool>(),
        0u8..16,
        0u8..16,
        any::<u8>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(r, d, df, p, x, t, ie, q)| {
            let mut bus = TestBus::new();
            let mut cpu = Cpu::new();
            cpu.set_control(true, true, &mut bus);
            cpu.step(&mut bus); // initialization cycle
            cpu.r = r;
            cpu.d = d;
            cpu.df = df;
            cpu.p = p;
            cpu.x = x;
            cpu.t = t;
            cpu.ie = ie;
            cpu.q = q;
            cpu
        })
}

proptest! {
    /// Property 4: a DMA-IN cycle stores one byte at M(R0) and increments R0; nothing else changes.
    #[test]
    fn prop_dma_in(cpu0 in arb_cpu(), val in any::<u8>(), fill in any::<u8>()) {
        let mut bus = TestBus::new();
        bus.mem.iter_mut().for_each(|m| *m = fill);
        bus.signals.dma_in = true;
        bus.dma_in_value = val;
        let mut cpu = cpu0.clone();
        prop_assert_eq!(cpu.step(&mut bus), 1);
        let a = cpu0.r[0];
        prop_assert_eq!(bus.mem[a as usize], val);
        let changed = bus.mem.iter().enumerate().filter(|(k, m)| *k != a as usize && **m != fill).count();
        prop_assert_eq!(changed, 0);
        let mut expect = cpu0.clone();
        expect.r[0] = a.wrapping_add(1);
        expect.cycles += 1;
        prop_assert_eq!(&cpu, &expect);
    }

    /// Property 4: a DMA-OUT cycle sends M(R0) and increments R0; nothing else changes.
    #[test]
    fn prop_dma_out(cpu0 in arb_cpu(), val in any::<u8>()) {
        let mut bus = TestBus::new();
        bus.mem[cpu0.r[0] as usize] = val;
        bus.signals.dma_out = true;
        let mut cpu = cpu0.clone();
        prop_assert_eq!(cpu.step(&mut bus), 1);
        prop_assert_eq!(&bus.log, &[IoEvent::DmaOut(val)]);
        let mut expect = cpu0.clone();
        expect.r[0] = cpu0.r[0].wrapping_add(1);
        expect.cycles += 1;
        prop_assert_eq!(&cpu, &expect);
    }

    /// Property 5 (entry): INT with IE = 1 saves X,P in T, sets X = 2, P = 1, IE = 0, in one cycle.
    #[test]
    fn prop_interrupt_entry(cpu0 in arb_cpu()) {
        let mut cpu0 = cpu0;
        cpu0.ie = true;
        let mut bus = TestBus::new();
        bus.signals.interrupt = true;
        let mut cpu = cpu0.clone();
        prop_assert_eq!(cpu.step(&mut bus), 1);
        let mut expect = cpu0.clone();
        expect.t = (cpu0.x << 4) | cpu0.p;
        expect.x = 2;
        expect.p = 1;
        expect.ie = false;
        expect.cycles += 1;
        prop_assert_eq!(&cpu, &expect);
        prop_assert!(bus.log.is_empty());
        // A held interrupt line is not taken again while IE = 0.
        let r1 = cpu.r[1];
        prop_assert_eq!(cpu.step(&mut bus), Cpu::instruction_cycles(bus.mem[r1 as usize]));
    }
}
