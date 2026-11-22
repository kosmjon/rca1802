//! Tests with the owner's Giant Board monitor ROM, if it is present in the workspace
//! (`cassette-loader/reference/giantboard.hex`). Skipped otherwise; the ROM is not
//! part of the emulator.

use crate::*;

const ROM_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../cassette-loader/reference/giantboard.hex"
);

fn monitor() -> Option<Machine> {
    let text = std::fs::read_to_string(ROM_PATH).ok()?;
    let chunks = hexfile::parse(&text).unwrap();
    let mut rom = Vec::new();
    for c in &chunks {
        rom.extend_from_slice(&c.data);
    }
    let mut m = Machine::new(Config::default()).unwrap();
    m.load_rom("giantboard", chunks[0].addr, &rom).unwrap();
    m.load_bin(0, &[0xC0, 0xF0, 0x00]).unwrap();
    m.set_switch(Switch::Run, true);
    m.run(50_000);
    Some(m)
}

/// Key two hex digits and press/release IN, with time to run in between, like a user.
fn enter(m: &mut Machine, byte: u8) {
    m.hex_key(byte >> 4);
    m.hex_key(byte & 0x0F);
    m.run(20_000);
    m.press_in(true);
    m.run(20_000);
    m.press_in(false);
    m.run(20_000);
}

#[test]
fn monitor_waits_for_in_after_sizing_ram() {
    let Some(m) = monitor() else { return };
    let c = m.cpu();
    assert_eq!(c.p, 4, "in the key routine");
    assert!((0xF029..=0xF02A).contains(&c.r[4]), "R4 = {:04X}", c.r[4]);
    assert_eq!(c.r[2], 0x5FFF, "stack at the top of 24K RAM");
}

#[test]
fn monitor_echoes_a_keyed_byte_on_the_display() {
    let Some(mut m) = monitor() else { return };
    enter(&mut m, 0x04);
    assert_eq!(m.hex_display(), 0x04);
}
