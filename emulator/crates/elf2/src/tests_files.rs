//! Program files and snapshot tests (task 11).

use crate::hexfile;
use crate::*;
use proptest::prelude::*;

const PIXIE: &[u8] = include_bytes!("../../../tests/programs/pixie64x128.bin");
const PIXIE_HEX: &str = include_str!("../../../tests/programs/pixie64x128.hex");
const ECHO: &[u8] = include_bytes!("../../../tests/programs/serial_echo.bin");

fn machine() -> Machine {
    Machine::new(Config::default()).unwrap()
}

fn full_ram() -> Machine {
    Machine::new(Config {
        ram_size: 65536,
        ..Config::default()
    })
    .unwrap()
}

// ------------------------------------------------------------------ .bin

#[test]
fn bin_load_and_save() {
    let mut m = machine();
    assert_eq!(m.load_bin(0x0200, &[1, 2, 3, 4]).unwrap(), 4);
    assert_eq!(m.save_bin(0x0200, 0x0203), [1, 2, 3, 4]);
    assert_eq!(m.save_bin(0x0203, 0x0203), [4]);
    assert!(m.save_bin(0x0203, 0x0200).is_empty());
}

#[test]
fn bin_load_ignores_memory_protect() {
    let mut m = machine();
    m.set_switch(Switch::Mp, true);
    m.load_bin(0, &[0xAA]).unwrap();
    assert_eq!(m.peek(0, 1), [0xAA]);
}

#[test]
fn bin_load_outside_memory_is_rejected_and_changes_nothing() {
    let mut m = machine();
    // 24K RAM ends at 5FFF; 4 bytes at 5FFE would reach 6001.
    let e = m.load_bin(0x5FFE, &[9, 9, 9, 9]).unwrap_err();
    assert!(
        matches!(e, MachineError::OutsideMemory { addr: 0x6000, .. }),
        "{e}"
    );
    assert_eq!(m.peek(0x5FFE, 2), [0, 0], "memory unchanged");
    assert!(e.to_string().contains("0000-5FFF"));
    assert!(matches!(
        m.load_bin(0xFFFF, &[1, 2]),
        Err(MachineError::TooLarge { .. })
    ));
}

#[test]
fn bin_load_into_rom_is_allowed() {
    let mut m = machine();
    m.load_rom("mon", 0xF000, &[0; 16]).unwrap();
    m.load_bin(0xF000, &[0x7B]).unwrap();
    assert_eq!(m.peek(0xF000, 1), [0x7B]);
}

// ------------------------------------------------------------------ Intel HEX

#[test]
fn hex_files_from_the_test_programs_load() {
    let mut m = machine();
    assert_eq!(m.load_hex(PIXIE_HEX).unwrap(), (0x0000, PIXIE.len()));
    assert_eq!(m.peek(0, PIXIE.len()), PIXIE);
}

#[test]
fn hex_format_matches_the_standard() {
    let text = hexfile::format(0x0100, &[0x02, 0x33, 0x7A]);
    assert_eq!(text, ":0301000002337A4D\n:00000001FF\n");
    // 16-byte records.
    let text = hexfile::format(0, &[0u8; 20]);
    assert_eq!(text.lines().count(), 3);
    assert!(text.starts_with(":10000000"));
    assert!(text.lines().nth(1).unwrap().starts_with(":04001000"));
}

#[test]
fn hex_errors_report_the_line() {
    let check = |text: &str, line: usize, needle: &str| {
        let e = hexfile::parse(text).unwrap_err();
        assert_eq!(e.line, line, "{e}");
        assert!(e.msg.contains(needle), "{e}");
    };
    check(":0301000002337AC6\n", 1, "checksum");
    check("\n\n:0301000002337A\n", 3, "length");
    check(":0301000002337A4D\nhello\n", 2, "':'");
    check(":03010000GG337AC5\n", 1, "hex digits");
    check(":0301000602337A47\n", 1, "record type");
    check(":020000040001F9\n", 1, "64K");
    check(":00000001FF\n:0301000002337A4D\n", 2, "after the end");
    check(
        ":10FFF80000000000000000000000000000000000F9\n",
        1,
        "past address FFFF",
    );
}

#[test]
fn hex_extended_zero_and_start_records_are_accepted() {
    let text =
        ":020000040000FA\n:020000020000FC\n:0400000300000000F9\n:0301000002337A4D\n:00000001FF\n";
    let c = hexfile::parse(text).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].addr, 0x0100);
}

#[test]
fn invalid_hex_leaves_memory_unchanged() {
    let mut m = machine();
    let text = ":0301000002337A4D\n:03020000FFFFFF00\n"; // second record bad checksum
    assert!(matches!(m.load_hex(text), Err(MachineError::Hex(_))));
    assert_eq!(m.peek(0x0100, 3), [0, 0, 0]);
    // Valid records, but one lands outside RAM.
    let text = format!(
        "{}{}",
        hexfile::format(0x0100, &[1]),
        hexfile::format(0x7000, &[1])
    )
    .replace(":00000001FF\n:", ":");
    assert!(matches!(
        m.load_hex(&text),
        Err(MachineError::OutsideMemory { addr: 0x7000, .. })
    ));
    assert_eq!(m.peek(0x0100, 1), [0]);
}

proptest! {
    /// Property 7: load_hex(save_hex(a, b)) restores the same bytes for any range.
    #[test]
    fn prop_hex_round_trip(start in any::<u16>(), len in 1usize..600, seed in any::<u64>()) {
        let end = (usize::from(start) + len - 1).min(0xFFFF) as u16;
        let n = usize::from(end - start) + 1;
        let data: Vec<u8> = (0..n).map(|k| (seed.wrapping_mul(k as u64 + 1) >> 13) as u8).collect();
        let mut a = full_ram();
        a.load_bin(start, &data).unwrap();
        let text = a.save_hex(start, end);
        let mut b = full_ram();
        prop_assert_eq!(b.load_hex(&text).unwrap(), (start, n));
        prop_assert_eq!(b.save_bin(start, end), data);
        // Nothing outside the range was written.
        if start > 0 { prop_assert_eq!(b.peek(start - 1, 1), vec![0]); }
        if end < 0xFFFF { prop_assert_eq!(b.peek(end + 1, 1), vec![0]); }
    }
}

// ------------------------------------------------------------------ snapshots

/// Run until a Pixie frame has been produced.
fn run_frame(m: &mut Machine) {
    for _ in 0..100 {
        m.run(1000);
        if m.events().contains(&Event::PixieFrame) {
            return;
        }
    }
    panic!("no frame");
}

#[test]
fn snapshot_restores_a_running_pixie_machine_exactly() {
    let mut a = machine();
    a.load_rom("mon", 0xF000, &[0x7B, 0x00]).unwrap();
    a.load_bin(0, PIXIE).unwrap();
    let pic: Vec<u8> = (0..1024u32).map(|i| (i * 7 + 3) as u8).collect();
    a.load_bin(0x0100, &pic).unwrap();
    a.hex_key(0x4);
    a.hex_key(0x2);
    a.set_switch(Switch::Run, true);
    run_frame(&mut a);
    a.run(1234); // somewhere mid-frame

    let snap = a.snapshot();
    let mut b = Machine::from_snapshot(&snap).unwrap();
    assert_eq!(b.cpu(), a.cpu());
    assert_eq!(b.peek(0, 0x10000), a.peek(0, 0x10000));
    assert_eq!(b.pixie_frame(), a.pixie_frame());
    assert_eq!(b.keypad_byte(), 0x42);
    assert_eq!(b.config(), a.config());
    assert_eq!(b.roms(), a.roms());
    assert!(b.panel().run);

    // Both continue identically.
    for _ in 0..5 {
        a.run(3000);
        b.run(3000);
        assert_eq!(a.cpu(), b.cpu());
        assert_eq!(a.events(), b.events());
    }
    assert_eq!(a.pixie_frame(), b.pixie_frame());
    assert_eq!(b.snapshot(), a.snapshot());
}

#[test]
fn snapshot_keeps_vid1_screen_and_front_panel() {
    let mut a = machine();
    a.load_bin(0, ECHO).unwrap();
    a.set_switch(Switch::Run, true);
    for &k in b"OK" {
        a.term_key(k);
        a.run(25_000);
    }
    a.run(20_000);
    assert_eq!(a.vid1_text()[0], "OK");
    a.set_switch(Switch::Mp, true);
    a.force_ef(1, Some(true));

    let b = Machine::from_snapshot(&a.snapshot()).unwrap();
    assert_eq!(b.vid1_text()[0], "OK");
    assert_eq!(b.vid1_cursor(), a.vid1_cursor());
    assert!(b.panel().mp);
    assert!(b.ef(1), "forced EF kept");
    // MP still protects memory after restore: a CPU write (STR R5) is blocked.
    let mut b = b;
    let pc = b.cpu().r[b.cpu().p as usize];
    b.poke(pc, &[0x55]);
    b.cpu_mut().r[5] = 0x0200;
    b.cpu_mut().d = 0x99;
    b.step();
    assert_eq!(b.peek(0x0200, 1), [0x00]);
}

#[test]
fn snapshot_errors() {
    let m = machine();
    let good = m.snapshot();
    assert!(matches!(
        Machine::from_snapshot(b"nope"),
        Err(MachineError::Snapshot(_))
    ));
    let mut v = good.clone();
    v[8] = 99; // version
    let e = Machine::from_snapshot(&v).err().unwrap();
    assert!(e.to_string().contains("version 99"), "{e}");
    let mut v = good.clone();
    v.pop();
    assert!(Machine::from_snapshot(&v).is_err(), "truncated");

    // A restore that fails leaves the machine as it was.
    let mut m = machine();
    m.load_bin(0, &[1, 2, 3]).unwrap();
    assert!(m.restore(b"garbage").is_err());
    assert_eq!(m.peek(0, 3), [1, 2, 3]);
}

#[test]
fn snapshot_detects_a_corrupted_rom() {
    let mut m = machine();
    m.load_rom("mon", 0xF000, &[1, 2, 3, 4]).unwrap();
    let mut snap = m.snapshot();
    let n = snap.len();
    snap[n - 0x10000 + 0xF000] ^= 0xFF;
    let e = Machine::from_snapshot(&snap).err().unwrap();
    assert!(e.to_string().contains("'mon'"), "{e}");
}
