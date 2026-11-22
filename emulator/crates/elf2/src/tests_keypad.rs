//! Hex keypad, display and front-panel load/verify tests (task 6).

use crate::*;
use proptest::prelude::*;

fn machine() -> Machine {
    Machine::new(Config::default()).unwrap()
}

fn key_byte(m: &mut Machine, b: u8) {
    m.hex_key(b >> 4);
    m.hex_key(b & 0x0F);
}

/// Load-mode entry: keys then IN, one byte per press.
fn enter(m: &mut Machine, bytes: &[u8]) {
    for &b in bytes {
        key_byte(m, b);
        m.press_in(true);
        m.run(4);
        m.press_in(false);
        m.run(4);
    }
}

/// Keypad echo program: INP 4 -> M(R1), D; OUT 4 shows it; loop.
///
/// ```text
/// 0000 F8 20   LDI 20
/// 0002 A1      PLO R1      R1 = 0020
/// 0003 E1      SEX R1
/// 0004 6C      INP 4       M(R1), D <- keypad
/// 0005 64      OUT 4       display <- M(R1); R1 += 1
/// 0006 21      DEC R1
/// 0007 30 04   BR 0004
/// ```
const ECHO: [u8; 9] = [0xF8, 0x20, 0xA1, 0xE1, 0x6C, 0x64, 0x21, 0x30, 0x04];

#[test]
fn keypad_latches_last_two_keys() {
    let mut m = machine();
    m.hex_key(1);
    assert_eq!(m.keypad_byte(), 0x01);
    m.hex_key(2);
    assert_eq!(m.keypad_byte(), 0x12);
    m.hex_key(0xF);
    assert_eq!(m.keypad_byte(), 0x2F);
}

#[test]
fn out4_drives_display_and_reports_changes() {
    let mut m = machine();
    // LDI 20, PLO R1, SEX R1, LDI 5A, STR R1, OUT 4, DEC R1, OUT 4, IDL
    m.poke(
        0,
        &[
            0xF8, 0x20, 0xA1, 0xE1, 0xF8, 0x5A, 0x51, 0x64, 0x21, 0x64, 0x00,
        ],
    );
    m.set_switch(Switch::Run, true);
    m.run(200);
    assert_eq!(m.hex_display(), 0x5A);
    let shown: Vec<_> = m
        .events()
        .iter()
        .filter(|e| matches!(e, Event::HexDisplay(_)))
        .collect();
    assert_eq!(
        shown,
        [&Event::HexDisplay(0x5A)],
        "second OUT 4 of the same value is not an event"
    );
}

#[test]
fn load_verify_run_with_front_panel() {
    let mut m = machine();

    // 1. LOAD on, enter the program with the keypad and IN.
    m.set_switch(Switch::Load, true);
    enter(&mut m, &ECHO);
    assert_eq!(m.peek(0, ECHO.len()), ECHO);
    assert_eq!(m.hex_display(), 0x04, "display shows the last byte entered");

    // 2. Verify: reset (LOAD off, RUN off), MP on, LOAD on; each IN steps through
    //    memory and shows the stored byte, while the keypad byte is not written.
    m.set_switch(Switch::Load, false);
    m.set_switch(Switch::Mp, true);
    m.set_switch(Switch::Load, true);
    key_byte(&mut m, 0xEE);
    for &expected in &ECHO {
        m.press_in(true);
        m.run(4);
        m.press_in(false);
        m.run(4);
        assert_eq!(m.hex_display(), expected);
    }
    assert_eq!(m.peek(0, ECHO.len()), ECHO, "MP kept memory unchanged");

    // 3. Run: MP off, LOAD off, RUN on. Keys show on the display.
    m.set_switch(Switch::Load, false);
    m.set_switch(Switch::Mp, false);
    m.set_switch(Switch::Run, true);
    key_byte(&mut m, 0x42);
    m.run(1000);
    assert_eq!(m.hex_display(), 0x42);
    key_byte(&mut m, 0xA7);
    m.run(1000);
    assert_eq!(m.hex_display(), 0xA7);
}

#[test]
fn ef4_program_waits_for_in() {
    // 0000 3F 00  BN4 0000   wait while IN is not pressed
    // 0002 7B     SEQ
    // 0003 00     IDL
    let mut m = machine();
    m.poke(0, &[0x3F, 0x00, 0x7B, 0x00]);
    m.set_switch(Switch::Run, true);
    m.run(1000);
    assert!(!m.q());
    m.press_in(true);
    m.run(1000);
    assert!(m.q());
}

proptest! {
    /// Property 8: any key sequence leaves the last two nibbles in the latch,
    /// the most recent in the low nibble.
    #[test]
    fn prop_keypad_latch(keys in proptest::collection::vec(0u8..16, 1..40)) {
        let mut m = machine();
        for &k in &keys {
            m.hex_key(k);
        }
        let n = keys.len();
        let hi = if n >= 2 { keys[n - 2] } else { 0 };
        prop_assert_eq!(m.keypad_byte(), (hi << 4) | keys[n - 1]);
    }
}
