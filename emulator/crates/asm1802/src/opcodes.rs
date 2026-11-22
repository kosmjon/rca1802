//! Mnemonic table.

/// How a mnemonic encodes its operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// One byte, no operand.
    Implied(u8),
    /// Opcode | register (0-15).
    Reg(u8),
    /// `LDN`: opcode | register (1-15; `LDN R0` would be `IDL`).
    RegNotZero(u8),
    /// `OUT` / `INP`: opcode + port (1-7).
    Port(u8),
    /// Opcode, immediate byte.
    Imm(u8),
    /// Opcode, low byte of a target in the same page.
    Short(u8),
    /// Opcode, 16-bit target (high byte first).
    Long(u8),
    /// `CALL addr` macro: `SEP R4`, `DW addr`.
    Call,
    /// `SEP R7 byte` macro: `SEP R7`, `DB byte`.
    Fetch,
    Db,
    Dw,
    Ds,
    /// `ALIGN n`: reserve bytes (like `DS`) up to the next multiple of `n`.
    Align,
    Equ,
    Org,
    End,
    /// `INCLUDE file`: expanded before assembly; a no-op here.
    Include,
    /// `PAGE`: listing page break (RCA/FIG style); a no-op, no operand.
    Page,
    // Tiny BASIC IL (Pittman's interpretive language)
    /// `SX n`: 00 + n (0-7).
    IlSx,
    /// `LB n`: 09, byte.
    IlLb,
    /// `LN n`: 0A, word.
    IlLn,
    /// `PC "text"`: 24, string with the last byte's bit 7 set.
    IlPc,
    /// `JS a` (30) / `J a` (38): 11-bit address relative to the IL start.
    IlJump(u8),
    /// `BR a`: 60 + signed offset (-31..31) from the next byte; `BR *` is 60.
    IlBr,
    /// `BV` / `BN` / `BE a`: base + forward offset (1-31); `*` is the base.
    IlBranch(u8),
    /// `BC a "text"`: like [`Kind::IlBranch`] with base 80, then a string.
    IlBc,
}

impl Kind {
    /// Whether the mnemonic takes an operand field. When it doesn't, the
    /// rest of the line is a comment.
    pub(crate) fn takes_operand(self) -> bool {
        !matches!(
            self,
            Kind::Implied(_) | Kind::End | Kind::Include | Kind::Page
        )
    }

    /// Directives valid in both 1802 and IL code; they don't switch modes.
    pub(crate) fn is_neutral(self) -> bool {
        matches!(
            self,
            Kind::Db
                | Kind::Dw
                | Kind::Ds
                | Kind::Align
                | Kind::Equ
                | Kind::Org
                | Kind::End
                | Kind::Include
                | Kind::Page
        )
    }
}

/// Look up an upper-case Tiny BASIC IL mnemonic.
pub(crate) fn il_lookup(m: &str) -> Option<Kind> {
    use Kind::*;
    Some(match m {
        "SX" => IlSx,
        "NO" => Implied(0x08),
        "LB" => IlLb,
        "LN" => IlLn,
        "DS" => Implied(0x0B),
        "SP" => Implied(0x0C),
        "SB" => Implied(0x10),
        "RB" => Implied(0x11),
        "FV" => Implied(0x12),
        "SV" => Implied(0x13),
        "GS" => Implied(0x14),
        "RS" => Implied(0x15),
        "GO" => Implied(0x16),
        "NE" => Implied(0x17),
        "AD" => Implied(0x18),
        "SU" => Implied(0x19),
        "MP" => Implied(0x1A),
        "DV" => Implied(0x1B),
        "CP" => Implied(0x1C),
        "NX" => Implied(0x1D),
        "LS" => Implied(0x1F),
        "PN" => Implied(0x20),
        "PQ" => Implied(0x21),
        "PT" => Implied(0x22),
        "NL" => Implied(0x23),
        "PC" => IlPc,
        "GL" => Implied(0x27),
        "IL" => Implied(0x2A),
        "MT" => Implied(0x2B),
        "XQ" => Implied(0x2C),
        "WS" => Implied(0x2D),
        "US" => Implied(0x2E),
        "RT" => Implied(0x2F),
        "JS" => IlJump(0x30),
        "J" => IlJump(0x38),
        "BR" => IlBr,
        "BC" => IlBc,
        "BV" => IlBranch(0xA0),
        "BN" => IlBranch(0xC0),
        "BE" => IlBranch(0xE0),
        _ => return None,
    })
}

/// Look up an upper-case mnemonic.
pub(crate) fn lookup(m: &str) -> Option<Kind> {
    use Kind::*;
    Some(match m {
        // Implied
        "IDL" => Implied(0x00),
        "IRX" => Implied(0x60),
        "RET" => Implied(0x70),
        "DIS" => Implied(0x71),
        "LDXA" => Implied(0x72),
        "STXD" => Implied(0x73),
        "ADC" => Implied(0x74),
        "SDB" => Implied(0x75),
        "SHRC" | "RSHR" => Implied(0x76),
        "SMB" => Implied(0x77),
        "SAV" => Implied(0x78),
        "MARK" => Implied(0x79),
        "REQ" => Implied(0x7A),
        "SEQ" => Implied(0x7B),
        "SHLC" | "RSHL" => Implied(0x7E),
        "SKP" | "NBR" => Implied(0x38),
        "NOP" => Implied(0xC4),
        "LSNQ" => Implied(0xC5),
        "LSNZ" => Implied(0xC6),
        "LSNF" => Implied(0xC7),
        "LSKP" | "NLBR" => Implied(0xC8),
        "LSIE" => Implied(0xCC),
        "LSQ" => Implied(0xCD),
        "LSZ" => Implied(0xCE),
        "LSDF" => Implied(0xCF),
        "LDX" => Implied(0xF0),
        "OR" => Implied(0xF1),
        "AND" => Implied(0xF2),
        "XOR" => Implied(0xF3),
        "ADD" => Implied(0xF4),
        "SD" => Implied(0xF5),
        "SHR" => Implied(0xF6),
        "SM" => Implied(0xF7),
        "SHL" => Implied(0xFE),
        "RETURN" | "RETN" => Implied(0xD5),
        // Register
        "LDN" => RegNotZero(0x00),
        "INC" => Reg(0x10),
        "DEC" => Reg(0x20),
        "LDA" => Reg(0x40),
        "STR" => Reg(0x50),
        "GLO" => Reg(0x80),
        "GHI" => Reg(0x90),
        "PLO" => Reg(0xA0),
        "PHI" => Reg(0xB0),
        "SEP" => Reg(0xD0),
        "SEX" => Reg(0xE0),
        // I/O
        "OUT" => Port(0x60),
        "INP" => Port(0x68),
        // Immediate
        "ADCI" => Imm(0x7C),
        "SDBI" => Imm(0x7D),
        "SMBI" => Imm(0x7F),
        "LDI" => Imm(0xF8),
        "ORI" => Imm(0xF9),
        "ANI" => Imm(0xFA),
        "XRI" => Imm(0xFB),
        "ADI" => Imm(0xFC),
        "SDI" => Imm(0xFD),
        "SMI" => Imm(0xFF),
        // Short branch
        "BR" => Short(0x30),
        "BQ" => Short(0x31),
        "BZ" => Short(0x32),
        "BDF" | "BPZ" | "BGE" => Short(0x33),
        "B1" => Short(0x34),
        "B2" => Short(0x35),
        "B3" => Short(0x36),
        "B4" => Short(0x37),
        "BNQ" => Short(0x39),
        "BNZ" => Short(0x3A),
        "BNF" | "BM" | "BL" => Short(0x3B),
        "BN1" => Short(0x3C),
        "BN2" => Short(0x3D),
        "BN3" => Short(0x3E),
        "BN4" => Short(0x3F),
        // Long branch
        "LBR" => Long(0xC0),
        "LBQ" => Long(0xC1),
        "LBZ" => Long(0xC2),
        "LBDF" | "LBPZ" | "LBGE" => Long(0xC3),
        "LBNQ" => Long(0xC9),
        "LBNZ" => Long(0xCA),
        "LBNF" | "LBM" | "LBL" => Long(0xCB),
        // Macros and directives
        "CALL" => Call,
        "DB" | "DC" => Db,
        "DW" => Dw,
        "DS" => Ds,
        "EQU" => Equ,
        "ORG" => Org,
        "END" => End,
        "ALIGN" => Align,
        "INCLUDE" => Include,
        "PAGE" => Page,
        _ => return None,
    })
}

/// Every 1802 instruction mnemonic (not aliases, macros or directives), for tests.
#[cfg(test)]
pub(crate) const ALL_INSTRUCTIONS: &[&str] = &[
    "IDL", "IRX", "RET", "DIS", "LDXA", "STXD", "ADC", "SDB", "SHRC", "SMB", "SAV", "MARK", "REQ",
    "SEQ", "SHLC", "SKP", "NOP", "LSNQ", "LSNZ", "LSNF", "LSKP", "LSIE", "LSQ", "LSZ", "LSDF",
    "LDX", "OR", "AND", "XOR", "ADD", "SD", "SHR", "SM", "SHL", "LDN", "INC", "DEC", "LDA", "STR",
    "GLO", "GHI", "PLO", "PHI", "SEP", "SEX", "OUT", "INP", "ADCI", "SDBI", "SMBI", "LDI", "ORI",
    "ANI", "XRI", "ADI", "SDI", "SMI", "BR", "BQ", "BZ", "BDF", "B1", "B2", "B3", "B4", "BNQ",
    "BNZ", "BNF", "BN1", "BN2", "BN3", "BN4", "LBR", "LBQ", "LBZ", "LBDF", "LBNQ", "LBNZ", "LBNF",
];
