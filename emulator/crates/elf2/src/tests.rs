//! Machine core tests (task 5).

use crate::*;
use cdp1802::CpuState;
use proptest::prelude::*;

fn machine() -> Machine {
    Machine::new(Config::default()).unwrap()
}

/// Switch from reset to run, then step past the initialization cycle.
fn start(m: &mut Machine) {
    m.set_switch(Switch::Run, true);
    m.step();
}

// ------------------------------------------------------------------ configuration

#[test]
fn default_config_matches_signal_map() {
    let c = Config::default();
    assert_eq!(c.ram_size, 24 * 1024);
    assert_eq!(c.clock_hz, 1_789_773);
    assert_eq!(c.unmapped_read, 0xFF);
    assert_eq!(c.serial.unwrap().rx_ef, 3);
    assert_eq!(c.vid1.columns, 64);
    assert!(c.pixie.is_some());
    assert!(!c.cassette);
    c.validate().unwrap();
}

#[test]
fn config_json_round_trip_and_partial_json() {
    let mut c = Config {
        ram_size: 4096,
        ..Config::default()
    };
    c.serial.as_mut().unwrap().rx_ef = 4;
    c.roms.push(RomSlot {
        name: "giant".into(),
        sha256: "0".repeat(64),
        base: 0xF000,
    });
    assert_eq!(Config::from_json(&c.to_json()).unwrap(), c);

    let p = Config::from_json(r#"{ "ram_size": 1024, "serial": { "baud": 110 } }"#).unwrap();
    assert_eq!(p.ram_size, 1024);
    assert_eq!(p.serial.unwrap().baud, 110);
    assert_eq!(
        p.serial.unwrap().rx_ef,
        3,
        "missing nested field takes its default"
    );
    assert_eq!(p.clock_hz, DEFAULT_CLOCK_HZ);
    assert_eq!(Config::from_json("{}").unwrap(), Config::default());
}

#[test]
fn config_validation() {
    let err = |json: &str| Config::from_json(json).unwrap_err();
    assert!(matches!(
        err(r#"{"ram_size": 100}"#),
        ConfigError::Invalid {
            field: "ram_size",
            ..
        }
    ));
    assert!(matches!(
        err(r#"{"serial": {"rx_ef": 2}}"#),
        ConfigError::Invalid {
            field: "serial.rx_ef",
            ..
        }
    ));
    assert!(matches!(
        err(r#"{"serial": {"data_bits": 9}}"#),
        ConfigError::Invalid { .. }
    ));
    assert!(matches!(
        err(r#"{"vid1": {"columns": 80}}"#),
        ConfigError::Invalid { .. }
    ));
    assert!(matches!(err("{ not json"), ConfigError::Json(_)));
    assert!(Machine::new(Config {
        ram_size: 10,
        ..Config::default()
    })
    .is_err());
}

// ------------------------------------------------------------------ memory map

#[test]
fn ram_size_and_unmapped_reads() {
    let mut m = machine();
    assert_eq!(m.poke(0x5FFF, &[0x12, 0x34]), 1, "6000 is outside 24K RAM");
    assert_eq!(m.peek(0x5FFF, 2), [0x12, 0xFF]);
    let m = Machine::new(Config {
        unmapped_read: 0x00,
        ..Config::default()
    })
    .unwrap();
    assert_eq!(m.peek(0x8000, 1), [0x00]);
}

#[test]
fn rom_is_read_only_and_checksummed() {
    let mut m = machine();
    let rom = [0x7B, 0x30, 0x00];
    m.load_rom("test", 0xF000, &rom).unwrap();
    assert_eq!(m.peek(0xF000, 3), rom);
    assert_eq!(m.config().roms[0].sha256, sha256_hex(&rom));
    assert!(m.missing_roms().is_empty());

    // CPU writes to ROM are ignored: STR R5 with R5 = F000.
    m.poke(0x0000, &[0x55, 0x00]);
    start(&mut m);
    m.cpu_mut().r[5] = 0xF000;
    m.cpu_mut().d = 0xEE;
    m.step();
    assert_eq!(m.peek(0xF000, 1), [0x7B]);

    // ROM overlaying RAM hides the RAM byte; overlap with another ROM is refused.
    m.load_rom("low", 0x0100, &[0xAA]).unwrap();
    assert_eq!(m.peek(0x0100, 1), [0xAA]);
    assert!(matches!(
        m.load_rom("clash", 0xF002, &[1, 2]),
        Err(MachineError::RomOverlap { .. })
    ));
    assert!(matches!(
        m.load_rom("big", 0xFFFF, &[1, 2]),
        Err(MachineError::TooLarge { .. })
    ));
}

#[test]
fn configured_rom_must_match_checksum() {
    let rom = [1u8, 2, 3];
    let mut c = Config::default();
    c.roms.push(RomSlot {
        name: "mon".into(),
        sha256: sha256_hex(&rom).to_uppercase(),
        base: 0xF000,
    });
    let mut m = Machine::new(c).unwrap();
    assert_eq!(m.missing_roms().len(), 1);
    assert!(matches!(
        m.load_rom("mon", 0, &[9, 9]),
        Err(MachineError::RomChecksum { .. })
    ));
    m.load_rom("mon", 0x1234, &rom).unwrap();
    assert_eq!(
        m.peek(0xF000, 3),
        rom,
        "configured base wins over the argument"
    );
    assert!(m.missing_roms().is_empty());
}

// ------------------------------------------------------------------ front panel

#[test]
fn switch_combinations_select_cpu_state() {
    let mut m = machine();
    assert_eq!(m.cpu().state, CpuState::Reset);
    m.set_switch(Switch::Load, true);
    assert_eq!(m.cpu().state, CpuState::Load);
    m.set_switch(Switch::Load, false);
    m.set_switch(Switch::Run, true);
    assert_eq!(m.cpu().state, CpuState::Running);
    m.set_switch(Switch::Load, true);
    assert_eq!(m.cpu().state, CpuState::Pause);
    m.set_switch(Switch::Load, false);
    assert_eq!(m.cpu().state, CpuState::Running);
    m.set_switch(Switch::Run, false);
    assert_eq!(m.cpu().state, CpuState::Reset);
}

#[test]
fn run_starts_at_0000() {
    let mut m = machine();
    m.poke(0x0000, &[0x7B, 0x00]); // SEQ, IDL
    m.set_switch(Switch::Run, true);
    m.run(100);
    assert!(m.q());
    assert_eq!(m.cpu().state, CpuState::Idle);
    assert!(m.events().contains(&Event::Q(true)));
    assert!(m.events().contains(&Event::Halted));
}

#[test]
fn reset_clears_q_and_registers() {
    let mut m = machine();
    m.poke(0x0000, &[0x7B, 0x00]);
    m.set_switch(Switch::Run, true);
    m.run(100);
    m.set_switch(Switch::Run, false);
    assert!(!m.q());
    assert_eq!((m.cpu().p, m.cpu().x, m.cpu().r[0]), (0, 0, 0));
    assert!(m.cpu().ie);
}

#[test]
fn pause_freezes_cpu() {
    let mut m = machine();
    m.poke(0x0000, &[0x30, 0x00]); // BR 0000
    m.set_switch(Switch::Run, true);
    m.run(50);
    m.set_switch(Switch::Load, true);
    let before = m.cpu().r;
    let r = m.run(1000);
    assert!(r.cycles >= 1000, "time keeps passing");
    assert_eq!(m.cpu().r, before);
}

#[test]
fn load_mode_in_press_writes_one_byte_and_mp_blocks_it() {
    let mut m = machine();
    m.set_switch(Switch::Load, true);
    m.hex_key(7);
    m.hex_key(0xB);
    m.press_in(true);
    m.run(10);
    m.press_in(false);
    m.run(10);
    m.hex_key(0);
    m.hex_key(0);
    m.press_in(true);
    m.run(10);
    m.press_in(false);
    assert_eq!(m.peek(0, 3), [0x7B, 0x00, 0x00]);
    assert_eq!(m.cpu().r[0], 2, "one DMA per press");

    // MP on: IN still steps R0 but memory is unchanged.
    m.set_switch(Switch::Mp, true);
    m.hex_key(5);
    m.hex_key(5);
    m.press_in(true);
    m.run(10);
    m.press_in(false);
    assert_eq!(m.peek(2, 1), [0x00]);
    assert_eq!(m.cpu().r[0], 3);
}

#[test]
fn in_button_drives_ef4_in_run_mode_only() {
    let mut m = machine();
    m.press_in(true);
    assert!(!m.ef(4), "not in reset");
    m.press_in(false);
    m.set_switch(Switch::Run, true);
    m.press_in(true);
    assert!(m.ef(4));
    m.press_in(false);
    assert!(!m.ef(4));
}

#[test]
fn giant_board_start_up_jumper_runs_f000() {
    let mut m = Machine::new(Config {
        start_at_f000: true,
        ..Config::default()
    })
    .unwrap();
    m.load_rom("mon", 0xF000, &[0x7B, 0x00]).unwrap(); // SEQ, IDL
    m.poke(0x0000, &[0x00]); // IDL at 0000 would leave Q off
    m.set_switch(Switch::Run, true);
    m.run(100);
    assert!(m.q(), "ran the ROM at F000");
    assert_eq!(m.cpu().r[0], 0xF002);
    // RAM at 0000-0002 is unchanged and readable normally afterwards.
    assert_eq!(m.peek(0, 1), [0x00]);
}

#[test]
fn forced_ef_overrides_hardware() {
    let mut m = machine();
    m.force_ef(2, Some(true));
    assert!(m.ef(2));
    m.force_ef(2, None);
    assert!(!m.ef(2));
}

// ------------------------------------------------------------------ run loop

#[test]
fn breakpoint_stops_before_instruction_and_resumes() {
    let mut m = machine();
    m.poke(0x0000, &[0xC4, 0x7B, 0x00]); // NOP, SEQ, IDL
    m.set_breakpoint(0x0001, true);
    m.set_switch(Switch::Run, true);
    let r = m.run(1000);
    assert_eq!(r.stop, StopReason::Breakpoint(0x0001));
    assert!(!m.q(), "SEQ not executed yet");
    assert_eq!(m.events(), [Event::Breakpoint(0x0001)]);
    let r = m.run(1000);
    assert_eq!(r.stop, StopReason::Budget);
    assert!(m.q());
}

#[test]
fn run_never_splits_an_instruction() {
    let mut m = machine();
    m.poke(0x0000, &[0xC4, 0xC4, 0xC4, 0x30, 0x00]);
    start(&mut m);
    let r = m.run(4);
    assert_eq!(r.cycles, 6, "two 3-cycle NOPs");
}

proptest! {
    /// Property 9: with MP on, no sequence of CPU instructions or DMA-IN cycles changes RAM.
    #[test]
    fn prop_memory_protect(prog in proptest::collection::vec(any::<u8>(), 64),
                           regs in any::<[u16; 16]>(), presses in 0usize..8, latch in any::<u8>()) {
        let mut m = machine();
        let mut image = vec![0u8; 24 * 1024];
        image[..64].copy_from_slice(&prog);
        for (k, b) in image.iter_mut().enumerate().skip(64) {
            *b = (k * 7) as u8;
        }
        m.poke(0, &image);
        m.set_switch(Switch::Mp, true);

        // DMA-IN through the front panel.
        m.set_switch(Switch::Load, true);
        m.hex_key(latch >> 4);
        m.hex_key(latch & 0x0F);
        for _ in 0..presses {
            m.press_in(true);
            m.run(4);
            m.press_in(false);
        }
        m.set_switch(Switch::Load, false);

        // Arbitrary instructions with arbitrary register contents.
        m.set_switch(Switch::Run, true);
        m.step();
        m.cpu_mut().r = regs;
        m.cpu_mut().r[0] = 0;
        m.cpu_mut().p = 0;
        m.run(5_000);
        prop_assert_eq!(m.peek(0, 24 * 1024), image);
    }
}
