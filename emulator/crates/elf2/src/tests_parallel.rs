//! CDP1852 parallel ports and cassette slot tests (task 10).

use crate::*;
use cdp1802::CpuState;

fn machine(parallel: bool, sr_to_int: bool, cassette: bool) -> Machine {
    Machine::new(Config {
        parallel_1852: parallel,
        parallel_sr_to_int: sr_to_int,
        cassette,
        ..Config::default()
    })
    .unwrap()
}

/// LDI 20, PLO R2, SEX R2, INP 7, OUT 4 (show it; R2 -> 21), STR R2 (D at 0021), IDL
const READ_AND_SHOW: [u8; 9] = [0xF8, 0x20, 0xA2, 0xE2, 0x6F, 0x64, 0x52, 0x00, 0x00];

#[test]
fn default_config_has_ports_unconnected_and_no_cassette() {
    let c = Config::default();
    assert!(!c.parallel_1852 && !c.parallel_sr_to_int && !c.cassette);
    let mut m = Machine::new(c).unwrap();
    assert!(!m.parallel_input(0x12));
    assert_eq!(m.parallel_output(), None);
    assert!(m.cassette().is_none());
    // INP 7 reads the floating bus.
    m.poke(0, &READ_AND_SHOW);
    m.set_switch(Switch::Run, true);
    m.run(100);
    assert_eq!(m.peek(0x20, 1), [0xFF]);
}

#[test]
fn inp7_reads_the_strobed_byte() {
    let mut m = machine(true, false, false);
    assert!(m.parallel_input(0xA5));
    m.poke(0, &READ_AND_SHOW);
    m.set_switch(Switch::Run, true);
    m.run(100);
    assert_eq!(m.peek(0x20, 1), [0xA5], "INP stores the byte at M(R(X))");
    assert_eq!(m.hex_display(), 0xA5);
    assert_eq!(m.peek(0x21, 1), [0xA5], "and in D");
}

#[test]
fn out7_latches_for_the_device() {
    let mut m = machine(true, false, false);
    // LDI 20, PLO R2, SEX R2, LDI 3C, STR R2, OUT 7, IDL
    m.poke(0, &[0xF8, 0x20, 0xA2, 0xE2, 0xF8, 0x3C, 0x52, 0x67, 0x00]);
    m.set_switch(Switch::Run, true);
    m.run(100);
    assert_eq!(m.parallel_output(), Some(0x3C));
    assert_eq!(m.parallel_output(), None, "taken once");
}

/// Main loop at 0000 (P = 0): BR 0000. Interrupt routine on R1 at 0040: SEQ, IDL.
fn interrupt_program(m: &mut Machine) {
    m.poke(0, &[0x30, 0x00]);
    m.poke(0x40, &[0x7B, 0x00]);
    m.set_switch(Switch::Run, true);
    m.step();
    m.cpu_mut().r[1] = 0x0040;
}

#[test]
fn service_request_interrupts_only_when_jumpered() {
    // Jumpered: a strobed byte raises INT; the routine runs (SEQ).
    let mut m = machine(true, true, false);
    interrupt_program(&mut m);
    m.run(100);
    assert!(!m.q(), "no interrupt before a byte arrives");
    m.parallel_input(0x01);
    m.run(100);
    assert!(m.q(), "interrupt routine ran");
    assert_eq!(m.cpu().p, 1);

    // Not jumpered: the byte waits, no interrupt.
    let mut m = machine(true, false, false);
    interrupt_program(&mut m);
    m.parallel_input(0x01);
    m.run(1000);
    assert!(!m.q());
    assert_eq!(m.cpu().p, 0);
}

#[test]
fn reading_the_port_clears_the_service_request() {
    let mut m = machine(true, true, false);
    // Interrupt routine: SEX R2, INP 7, SEQ, then RET with X,P = 0,0 from M(R2).
    // Main program: BR 0000. After one interrupt and return, IE = 1 again and the
    // cleared request must not interrupt a second time.
    m.poke(0, &[0x30, 0x00]);
    m.poke(0x40, &[0xE2, 0x6F, 0x7B, 0x70]);
    m.set_switch(Switch::Run, true);
    m.step();
    m.cpu_mut().r[1] = 0x0040;
    m.cpu_mut().r[2] = 0x0100; // INP 7 stores at 0100; RET reads X,P from 0101 = 00
    m.parallel_input(0x00);
    m.run(200);
    assert!(m.q(), "routine ran");
    assert_eq!(m.cpu().p, 0, "returned to the main program");
    assert!(m.cpu().ie);
    let r1 = m.cpu().r[1];
    m.run(1000);
    assert_eq!(m.cpu().r[1], r1, "no second interrupt");
    assert_eq!(m.cpu().state, CpuState::Running);
}

#[test]
fn reset_clears_service_requests() {
    let mut m = machine(true, true, false);
    m.parallel_input(0x55);
    m.set_switch(Switch::Run, true);
    m.set_switch(Switch::Run, false);
    interrupt_program(&mut m);
    m.run(1000);
    assert!(!m.q(), "the pending request was cleared by reset");
}

#[test]
fn cassette_stub_reads_idle_and_sees_q() {
    let mut m = machine(false, false, true);
    // SEQ, REQ, SEQ, REQ, then loop on B2 (would branch if EF2 were asserted).
    m.poke(0, &[0x7B, 0x7A, 0x7B, 0x7A, 0x35, 0x00, 0x00]);
    m.set_switch(Switch::Run, true);
    m.run(100);
    assert_eq!(m.cassette().unwrap().q_edges(), 4);
    assert!(!m.ef(2));
    assert_eq!(m.cpu().state, CpuState::Idle, "B2 not taken, reached IDL");
}
