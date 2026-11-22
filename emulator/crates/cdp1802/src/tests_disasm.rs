//! Disassembler tests (task 4).

use crate::test_bus::TestBus;
use crate::{disasm, Cpu, Operand};
use proptest::prelude::*;
use std::format;
use std::string::String;
use std::vec::Vec;

/// Disassemble a whole buffer into "AAAA: text" lines.
fn listing(bytes: &[u8], base: u16) -> Vec<String> {
    let mut out = Vec::new();
    let mut k = 0usize;
    while k < bytes.len() {
        let ins = disasm(&bytes[k..], base.wrapping_add(k as u16));
        out.push(format!("{:04X}: {}", ins.addr, ins));
        k += ins.len as usize;
    }
    out
}

#[test]
fn hand_checked_listing() {
    // A small program covering each operand format, checked by hand against the
    // RCA CDP1802 instruction summary.
    let prog = [
        0xF8, 0x03, // 0000 LDI 03
        0xB3, // 0002 PHI R3
        0xF8, 0x10, // 0003 LDI 10
        0xA3, // 0005 PLO R3
        0xD3, // 0006 SEP R3
        0xE2, // 0007 SEX R2
        0x64, // 0008 OUT 4
        0x6C, // 0009 INP 4
        0x3F, 0x09, // 000A BN4 0009
        0x7B, // 000C SEQ
        0xC0, 0x12, 0x34, // 000D LBR 1234
        0xC4, // 0010 NOP
        0x38, // 0011 SKP
        0xC8, // 0012 LSKP
        0x70, // 0013 RET
        0x68, // 0014 DB 68
        0x00, // 0015 IDL
        0x05, // 0016 LDN R5
        0x7C, 0x01, // 0017 ADCI 01
        0xFE, // 0019 SHL
        0xF6, // 001A SHR
        0x60, // 001B IRX
    ];
    let expect = [
        "0000: LDI 03",
        "0002: PHI R3",
        "0003: LDI 10",
        "0005: PLO R3",
        "0006: SEP R3",
        "0007: SEX R2",
        "0008: OUT 4",
        "0009: INP 4",
        "000A: BN4 0009",
        "000C: SEQ",
        "000D: LBR 1234",
        "0010: NOP",
        "0011: SKP",
        "0012: LSKP",
        "0013: RET",
        "0014: DB 68",
        "0015: IDL",
        "0016: LDN R5",
        "0017: ADCI 01",
        "0019: SHL",
        "001A: SHR",
        "001B: IRX",
    ];
    assert_eq!(listing(&prog, 0), expect);
}

#[test]
fn short_branch_target_page_follows_immediate_byte() {
    assert_eq!(format!("{}", disasm(&[0x30, 0x10], 0x01FF)), "BR 0210");
    assert_eq!(format!("{}", disasm(&[0x30, 0x10], 0x01FE)), "BR 0110");
}

#[test]
fn truncated_operands() {
    assert_eq!(disasm(&[0xF8], 0).operand, Operand::Truncated);
    assert_eq!(format!("{}", disasm(&[0xC0, 0x12], 0)), "LBR ??");
    assert_eq!(disasm(&[0xC0, 0x12], 0).len, 3);
}

#[test]
fn every_opcode_has_a_mnemonic() {
    for op in 0..=255u8 {
        let ins = disasm(&[op, 0, 0], 0);
        assert!(!ins.mnemonic.is_empty(), "opcode {op:02X}");
        assert!((1..=3).contains(&ins.len));
    }
}

#[test]
fn length_matches_execution_for_non_control_opcodes() {
    // For instructions that do not change the flow of control, R(P) must advance by
    // exactly the disassembled length.
    for op in 0..=255u8 {
        let control = matches!(op, 0x30..=0x3F | 0xC0..=0xCF | 0xD0..=0xDF | 0x70 | 0x71);
        // INC/DEC/LDA/PLO/PHI on R3 modify R(P) itself.
        let touches_pc = matches!(op, 0x13 | 0x23 | 0x43 | 0xA3 | 0xB3);
        if control || touches_pc {
            continue;
        }
        let mut bus = TestBus::new();
        let mut cpu = Cpu::new();
        cpu.set_control(true, true, &mut bus);
        cpu.step(&mut bus);
        cpu.p = 3;
        cpu.r[3] = 0x0100;
        cpu.x = 2;
        cpu.r[2] = 0x2000;
        // Point every other register away from the program.
        for (k, r) in cpu.r.iter_mut().enumerate() {
            if k != 2 && k != 3 {
                *r = 0x3000;
            }
        }
        bus.load(0x0100, &[op, 0x00, 0x00]);
        cpu.step(&mut bus);
        let ins = disasm(&[op, 0, 0], 0x0100);
        assert_eq!(
            cpu.r[3].wrapping_sub(0x0100),
            u16::from(ins.len),
            "{ins} ({op:02X})"
        );
    }
}

proptest! {
    /// Disassembly never panics and always reports a length within the instruction set's range.
    #[test]
    fn prop_disasm_total(bytes in proptest::collection::vec(any::<u8>(), 1..4), addr in any::<u16>()) {
        let ins = disasm(&bytes, addr);
        prop_assert!((1..=3).contains(&ins.len));
        prop_assert_eq!(ins.addr, addr);
        let _ = format!("{ins}");
    }
}
