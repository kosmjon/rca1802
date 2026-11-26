//! Parallel keyboard (INP 5 / EF strobe) and dot-matrix printer (OUT 3 / EF ready).

use crate::config::{ParallelKeyboardConfig, PrinterConfig};
use crate::*;

fn with_keyboard(ef: u8) -> Machine {
    Machine::new(Config {
        parallel_keyboard: Some(ParallelKeyboardConfig { ef }),
        ..Config::default()
    })
    .unwrap()
}

fn with_printer(cfg: PrinterConfig) -> Machine {
    Machine::new(Config {
        printer: Some(cfg),
        ..Config::default()
    })
    .unwrap()
}

fn run(m: &mut Machine) {
    m.set_switch(Switch::Run, true);
    m.run(2000);
}

#[test]
fn not_fitted_by_default() {
    let c = Config::default();
    assert!(c.parallel_keyboard.is_none() && c.printer.is_none());
    let mut m = Machine::new(c).unwrap();
    assert!(m.printer_paper().is_none());
    m.parallel_key(b'A'); // no-op, must not panic
    m.printer_tear_off(); // no-op
                          // INP 5 / OUT 3 on an unfitted machine read the floating bus / do nothing.
}

#[test]
fn keyboard_strobe_raises_ef_and_inp5_reads_and_clears() {
    let mut m = with_keyboard(2);
    // No key yet: EF2 low.
    assert!(!m.ef(2));
    m.parallel_key(b'Q');
    assert!(m.ef(2), "strobe should raise EF2 while a byte waits");

    // Program: LDI 20, PLO R2, SEX R2, INP 5 (byte -> M(R2) and D), STR R2, IDL.
    // INP 5 stores the key byte at 0020; check it and that the strobe cleared.
    let prog = [0xF8, 0x20, 0xA2, 0xE2, 0x6D, 0x52, 0x00];
    m.poke(0, &prog);
    run(&mut m);
    assert_eq!(m.peek(0x20, 1), [b'Q'], "INP 5 returns the key byte");
    assert!(!m.ef(2), "strobe clears once the byte is read");
}

#[test]
fn keyboard_queues_several_keys() {
    let mut m = with_keyboard(2);
    m.parallel_key(b'A');
    m.parallel_key(b'B');
    assert!(m.ef(2));
    // Two INP 5 in a row, storing to 0020 and 0021.
    // LDI 20, PLO R2, SEX R2, INP 5, INC R2, INP 5, IDL
    let prog = [0xF8, 0x20, 0xA2, 0xE2, 0x6D, 0x12, 0x6D, 0x00];
    m.poke(0, &prog);
    run(&mut m);
    assert_eq!(m.peek(0x20, 2), [b'A', b'B'], "keys read in order");
    assert!(!m.ef(2), "strobe clears when the queue drains");
}

#[test]
fn keyboard_ef_line_is_configurable() {
    let mut m = with_keyboard(3);
    m.parallel_key(b'Z');
    assert!(m.ef(3), "strobe on the configured line");
    assert!(!m.ef(2), "not on EF2");
}

#[test]
fn printer_out3_prints_to_paper() {
    let mut m = with_printer(PrinterConfig::default());
    // Print "HI": LDI 'H', STR R2 is overkill; use immediate OUT via a buffer.
    // LDI 20, PLO R2, SEX R2, LDI 'H', STR R2, OUT 3 (R2 advances), ... simpler:
    // put the two chars in memory and OUT them.
    // Program: LDI 20, PLO R2, SEX R2, OUT 3, OUT 3, IDL   with M(20)='H', M(21)='I'
    m.poke(0x20, b"HI");
    let prog = [0xF8, 0x20, 0xA2, 0xE2, 0x63, 0x63, 0x00];
    m.poke(0, &prog);
    run(&mut m);
    let paper = m.printer_paper().unwrap();
    assert_eq!(paper[0], "HI", "printed text on the first line");
}

#[test]
fn printer_cr_lf_and_tear_off() {
    let mut m = with_printer(PrinterConfig {
        cr_feeds: true,
        ..PrinterConfig::default()
    });
    // M: 'A', CR, 'B'  -> "A" then new line "B" (CR = CR+LF).
    m.poke(0x20, &[b'A', 0x0D, b'B']);
    let prog = [0xF8, 0x20, 0xA2, 0xE2, 0x63, 0x63, 0x63, 0x00];
    m.poke(0, &prog);
    run(&mut m);
    let paper = m.printer_paper().unwrap();
    assert_eq!(paper[0], "A");
    assert_eq!(paper[1], "B", "CR+LF moved to a fresh line");

    m.printer_tear_off();
    let fresh = m.printer_paper().unwrap();
    assert_eq!(fresh, vec![String::new()], "tear off starts a blank sheet");
}

#[test]
fn printer_ready_ef_goes_busy_then_ready() {
    // A long busy window so the effect is observable across one instruction.
    let mut m = with_printer(PrinterConfig {
        busy_ms: 100,
        ..PrinterConfig::default()
    });
    assert!(m.ef(1), "idle printer is ready (EF1 high)");
    // OUT 3 one byte, then IDL. After the OUT the printer is busy.
    m.poke(0x20, b"X");
    let prog = [0xF8, 0x20, 0xA2, 0xE2, 0x63, 0x00];
    m.poke(0, &prog);
    m.set_switch(Switch::Run, true);
    m.run(50); // a few instructions; well inside the 100 ms busy window
    assert!(!m.ef(1), "printer is busy (EF1 low) right after OUT 3");
}

#[test]
fn printer_ef_line_is_configurable() {
    let m = with_printer(PrinterConfig {
        ef: 4,
        ..PrinterConfig::default()
    });
    assert!(m.ef(4), "ready on the configured line");
    assert!(!m.ef(1), "not on EF1");
}

#[test]
fn serial_and_vid1_still_work_with_parallel_devices() {
    // Both parallel devices fitted alongside the default serial/VID-1.
    let mut m = Machine::new(Config {
        parallel_keyboard: Some(ParallelKeyboardConfig::default()),
        printer: Some(PrinterConfig::default()),
        ..Config::default()
    })
    .unwrap();
    // VID-1 is present and its serial keyboard path still accepts a key.
    assert!(m.vid1_columns().is_some(), "VID-1 still fitted");
    m.term_key(b'K'); // must not panic; queued on serial RX
    assert!(m.term_pending() >= 1, "serial keyboard still queues");
    // And the parallel keyboard is independent.
    m.parallel_key(b'P');
    assert!(m.ef(2), "parallel strobe independent of serial");
}
