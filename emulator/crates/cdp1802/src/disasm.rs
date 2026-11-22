//! Disassembler with RCA mnemonics.
//!
//! Output format (all numbers hex, upper case, no prefix):
//!
//! | Kind | Example |
//! |---|---|
//! | Register operand | `LDN R5`, `SEP R3` |
//! | I/O port | `OUT 4`, `INP 1` |
//! | Immediate byte | `LDI 5A` |
//! | Short branch (target resolved to a full address) | `BR 0155` |
//! | Long branch | `LBR 1234` |
//! | No operand | `NOP`, `RET`, `LSKP` |
//! | Opcode 68 (no 1802 instruction) | `DB 68` |
//!
//! Skips (`SKP`, `LSKP`, `LSNQ` ...) are listed as one-byte instructions; the
//! bytes they skip are disassembled as whatever follows.

use core::fmt;

/// Operand of a decoded instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    None,
    /// Register number 0-15.
    Reg(u8),
    /// I/O port 1-7.
    Port(u8),
    /// Immediate byte.
    Imm(u8),
    /// Branch target address.
    Addr(u16),
    /// Raw data byte (opcode with no instruction).
    Byte(u8),
    /// Operand bytes missing from the input slice.
    Truncated,
}

/// One decoded instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    /// Address of the opcode.
    pub addr: u16,
    pub opcode: u8,
    pub mnemonic: &'static str,
    pub operand: Operand,
    /// Length in bytes (1, 2 or 3).
    pub len: u8,
}

impl fmt::Display for Instruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.mnemonic)?;
        match self.operand {
            Operand::None => Ok(()),
            Operand::Reg(r) => write!(f, " R{r:X}"),
            Operand::Port(p) => write!(f, " {p}"),
            Operand::Imm(v) | Operand::Byte(v) => write!(f, " {v:02X}"),
            Operand::Addr(a) => write!(f, " {a:04X}"),
            Operand::Truncated => f.write_str(" ??"),
        }
    }
}

const SHORT_BRANCH: [&str; 16] = [
    "BR", "BQ", "BZ", "BDF", "B1", "B2", "B3", "B4", "SKP", "BNQ", "BNZ", "BNF", "BN1", "BN2",
    "BN3", "BN4",
];
const LONG: [&str; 16] = [
    "LBR", "LBQ", "LBZ", "LBDF", "NOP", "LSNQ", "LSNZ", "LSNF", "LSKP", "LBNQ", "LBNZ", "LBNF",
    "LSIE", "LSQ", "LSZ", "LSDF",
];
const GROUP7: [&str; 16] = [
    "RET", "DIS", "LDXA", "STXD", "ADC", "SDB", "SHRC", "SMB", "SAV", "MARK", "REQ", "SEQ", "ADCI",
    "SDBI", "SHLC", "SMBI",
];
const GROUPF: [&str; 16] = [
    "LDX", "OR", "AND", "XOR", "ADD", "SD", "SHR", "SM", "LDI", "ORI", "ANI", "XRI", "ADI", "SDI",
    "SHL", "SMI",
];
const REG_OPS: [&str; 16] = [
    "LDN", "INC", "DEC", "", "LDA", "STR", "", "", "GLO", "GHI", "PLO", "PHI", "", "SEP", "SEX", "",
];

/// Decode the instruction at the start of `bytes`, located at address `addr`.
///
/// # Panics
/// Panics if `bytes` is empty.
pub fn disasm(bytes: &[u8], addr: u16) -> Instruction {
    let op = bytes[0];
    let hi = op >> 4;
    let n = op & 0x0F;
    let byte1 = bytes.get(1).copied();
    let word = match (bytes.get(1), bytes.get(2)) {
        (Some(&h), Some(&l)) => Some(u16::from_be_bytes([h, l])),
        _ => None,
    };
    let imm = |b: Option<u8>| b.map_or(Operand::Truncated, Operand::Imm);

    let (mnemonic, operand, len) = match hi {
        0x0 if n == 0 => ("IDL", Operand::None, 1),
        0x3 if n == 8 => ("SKP", Operand::None, 1),
        0x3 => {
            // The target is in the page of the immediate byte (addr + 1).
            let page = addr.wrapping_add(1) & 0xFF00;
            let target = byte1.map_or(Operand::Truncated, |b| Operand::Addr(page | u16::from(b)));
            (SHORT_BRANCH[n as usize], target, 2)
        }
        0x6 => match n {
            0 => ("IRX", Operand::None, 1),
            1..=7 => ("OUT", Operand::Port(n), 1),
            8 => ("DB", Operand::Byte(op), 1),
            _ => ("INP", Operand::Port(n - 8), 1),
        },
        0x7 => {
            let m = GROUP7[n as usize];
            if matches!(n, 0xC | 0xD | 0xF) {
                (m, imm(byte1), 2)
            } else {
                (m, Operand::None, 1)
            }
        }
        0xC => {
            let m = LONG[n as usize];
            if matches!(n, 0x0..=0x3 | 0x9..=0xB) {
                (m, word.map_or(Operand::Truncated, Operand::Addr), 3)
            } else {
                (m, Operand::None, 1)
            }
        }
        0xF => {
            let m = GROUPF[n as usize];
            if n >= 8 && n != 0xE {
                (m, imm(byte1), 2)
            } else {
                (m, Operand::None, 1)
            }
        }
        _ => (REG_OPS[hi as usize], Operand::Reg(n), 1),
    };
    Instruction {
        addr,
        opcode: op,
        mnemonic,
        operand,
        len,
    }
}
