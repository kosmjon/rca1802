use super::*;

#[test]
fn diagnostic_is_reexported_from_asm1802() {
    let d = Diagnostic {
        file: "test.m18".to_string(),
        line: 1,
        message: "example".to_string(),
    };
    assert_eq!(d.to_string(), "test.m18:1: example");
}

/// A short multi-statement source exercising a chain, a label, a trailing
/// comment, a multi-instruction expansion (the Requirement 10.2 leading
/// comment), a bare branch, and a `D:STK`/`STK:D` pair -- checked line by
/// line against a hand-written expected `asm1802` listing (design.md
/// section 7 / task 9), and then actually assembled. `STK` never inserts
/// its own `SEX R2`, so the source writes one itself.
#[test]
fn multi_statement_source_transpiles_line_by_line() {
    let source = "\
BUFF EQU 0x1234
START: R3.0 : D ; grab the low byte
    BUFF : R4
    /END
    SEX R2
    D:STK
    STK:D
END: RETN
";
    let output = transpile(source).expect("should transpile without errors");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines,
        vec![
            "BUFF EQU 0x1234",
            "START: GLO R3 ; grab the low byte",
            "    LDI HIGH(BUFF) ; BUFF : R4",
            "    PHI R4",
            "    LDI LOW(BUFF)",
            "    PLO R4",
            "    BR END",
            "    SEX R2",
            "    STXD",
            "    IRX ; STK:D",
            "    LDX",
            "END: RETN",
        ]
    );
    // No register-to-register arithmetic appears anywhere in this source,
    // so no init block or scratch byte should be emitted (Requirement 9.2
    // is conditional, not unconditional -- task 8.2's deferred check).
    assert!(!output.contains("MOL1802_SCRATCH"));

    let asm = asm1802::assemble(&output);
    assert!(asm.ok(), "errors: {:?}\n{output}", asm.errors);
}

/// The comma-chained `STK` push/pop of four registers, matching the
/// worked example this syntax was designed against exactly (including
/// the push list and pop list being in reverse order of each other).
/// `STK` never inserts a `SEX`, so none appears in the output even
/// though none was written in the source either.
#[test]
fn comma_chained_stack_transpiles_to_the_worked_example() {
    let source = "    R10,R9,R8,R7:STK\n    STK:R7,R8,R9,R10\n";
    let output = transpile(source).expect("should transpile without errors");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines,
        vec![
            "    GHI R10 ; R10,R9,R8,R7:STK",
            "    STXD",
            "    GLO R10",
            "    STXD",
            "    GHI R9",
            "    STXD",
            "    GLO R9",
            "    STXD",
            "    GHI R8",
            "    STXD",
            "    GLO R8",
            "    STXD",
            "    GHI R7",
            "    STXD",
            "    GLO R7",
            "    STXD",
            "    IRX ; STK:R7,R8,R9,R10",
            "    LDXA",
            "    PLO R7",
            "    LDXA",
            "    PHI R7",
            "    LDXA",
            "    PLO R8",
            "    LDXA",
            "    PHI R8",
            "    LDXA",
            "    PLO R9",
            "    LDXA",
            "    PHI R9",
            "    LDXA",
            "    PLO R10",
            "    LDX",
            "    PHI R10",
        ]
    );
    assert!(!output.contains("SEX"));

    let asm = asm1802::assemble(&output);
    assert!(asm.ok(), "errors: {:?}\n{output}", asm.errors);
}

/// Generated mnemonics line up with a column-9 file (labels padded with
/// spaces, indented lines using 8 spaces) instead of always using a
/// fixed 4-space indent regardless of the file's own convention.
#[test]
fn generated_lines_match_a_column_9_file_convention() {
    // Built from explicit `\n` escapes in one line, not `"\` followed by
    // a real newline: that continuation form strips leading whitespace
    // from the line after it, which would silently un-indent `ORG`.
    let source = "        ORG 0x0000\nPAGE    EQU 0\nSCRT:   LDI PAGE\n        PHI R2\n        SEP R3\nQON:    1:Q\n        RETN\n";
    let output = transpile(source).expect("should transpile without errors");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines,
        vec![
            "        ORG 0x0000",
            "PAGE    EQU 0",
            "SCRT:   LDI PAGE",
            "        PHI R2",
            "        SEP R3",
            "QON:    SEQ",
            "        RETN",
        ]
    );
}

/// A file with no established indentation convention at all (every
/// MOL1802 statement starts at column 1 behind a label) falls back to a
/// plain 4-space indent for generated lines.
#[test]
fn generated_lines_fall_back_to_a_plain_indent_with_no_convention_established() {
    // The label needs a space before the statement, same as everywhere
    // else in this language -- `LOOP:1:Q` with no space is itself a
    // pre-existing, unrelated constraint (asm1802's own column-1-label
    // rule can't otherwise tell where the label ends).
    let output = transpile("LOOP: 1:Q\n").expect("should transpile without errors");
    assert_eq!(output.lines().collect::<Vec<_>>(), vec!["LOOP: SEQ"]);
}

/// A label longer than the detected mnemonic column still gets at least
/// one separating space, rather than running into the mnemonic. Two
/// plain 4-space-indented lines establish an unambiguous column-5
/// convention so the long label (which would reach column 20 on its
/// own) doesn't just win the vote instead.
#[test]
fn a_long_label_still_gets_at_least_one_separating_space() {
    let source = "    D : R3.0\n    D : R4.0\nSOMEVERYLONGLABEL: D : R3.0\n";
    let output = transpile(source).expect("should transpile without errors");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "    PLO R3");
    assert_eq!(lines[1], "    PLO R4");
    assert_eq!(lines[2], "SOMEVERYLONGLABEL: PLO R3");
}

#[test]
fn source_with_no_register_arithmetic_gets_no_init_block_or_scratch_byte() {
    let output = transpile("    D : R3.0\n").unwrap();
    assert!(!output.contains("MOL1802_SCRATCH"));
    assert!(!output.contains("SEX R6"));
}

#[test]
fn source_with_register_arithmetic_gets_the_init_block_first_and_scratch_byte_last() {
    // Not `"\` + newline here: Rust's string-continuation escape strips
    // leading whitespace from the *next* line too, which would silently
    // un-indent this first instruction into column 1 (where `asm1802`
    // would misread it as a label).
    let source = "\n    LDI #00\n    PLO R3\n    R3 + R4 : R4\n";
    let output = transpile(source).unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.first(), Some(&"    LDI HIGH(MOL1802_SCRATCH)"));
    assert_eq!(lines.last(), Some(&"MOL1802_SCRATCH: DB 0"));
    let scratch_decl_count = lines
        .iter()
        .filter(|l| l.contains("MOL1802_SCRATCH: DB 0"))
        .count();
    assert_eq!(scratch_decl_count, 1, "scratch byte declared exactly once");

    let asm = asm1802::assemble(&output);
    assert!(asm.ok(), "errors: {:?}\n{output}", asm.errors);
}

/// A minimal flat-memory `Bus` for running a transpiled-and-assembled
/// program on the real `cdp1802` CPU core, the same verify-by-running
/// discipline used throughout this project (e.g. the `fig_forth_*` tests
/// in `asm1802`).
struct FlatBus {
    mem: [u8; 0x10000],
}

impl FlatBus {
    fn new() -> Self {
        FlatBus { mem: [0; 0x10000] }
    }
}

impl cdp1802::Bus for FlatBus {
    fn read(&mut self, addr: u16) -> u8 {
        self.mem[usize::from(addr)]
    }
    fn write(&mut self, addr: u16, val: u8) {
        self.mem[usize::from(addr)] = val;
    }
    fn out(&mut self, _port: u8, _val: u8) {}
    fn inp(&mut self, _port: u8) -> u8 {
        0
    }
    fn ef(&self, _line: u8) -> bool {
        false
    }
    fn set_q(&mut self, _q: bool) {}
    fn dma_in(&mut self) -> u8 {
        0
    }
    fn dma_out(&mut self, _val: u8) {}
    fn signals(&self) -> cdp1802::Signals {
        cdp1802::Signals::default()
    }
    fn tick(&mut self, _cycles: u32) {}
}

/// Assembles `source` (already-valid `asm1802` text) and runs it to
/// completion on a real `cdp1802::Cpu` (starting at the lowest assembled
/// address, stepping until the program counter runs past the highest
/// one -- there are no branches in the programs this helper is used for),
/// returning the final CPU state.
fn run_to_completion(source: &str) -> cdp1802::Cpu {
    let asm = asm1802::assemble(source);
    assert!(asm.ok(), "errors: {:?}\n{source}", asm.errors);
    let (lo, hi) = asm.range().expect("program should contain bytes");
    let (start, data) = asm.to_bin().expect("range is non-empty");
    assert_eq!(start, lo);

    let mut bus = FlatBus::new();
    for (offset, byte) in data.iter().enumerate() {
        bus.mem[usize::from(lo) + offset] = *byte;
    }

    let mut cpu = cdp1802::Cpu::new();
    cpu.r[0] = lo; // P defaults to R0 on reset; start execution at `lo`.
    cpu.set_control(true, true, &mut bus); // /CLEAR, /WAIT both high -> Running.

    for _ in 0..10_000 {
        cpu.step(&mut bus);
        if cpu.r[usize::from(cpu.p)] > hi {
            break;
        }
    }
    cpu
}

/// Requirement 9.4: the full 16-bit register-to-register add propagates
/// `DF` from the low-byte `ADD` into the high-byte `ADC`, exactly like
/// hand-written 1802 code, for a case that actually carries.
#[test]
fn full_16bit_register_add_carries_through_the_real_cpu() {
    let source = "\n    LDI #00\n    PHI R3\n    LDI #FF\n    PLO R3\n    LDI #00\n    PHI R4\n    LDI #01\n    PLO R4\n    R3 + R4 : R4\n";
    let output = transpile(source).expect("should transpile without errors");
    let cpu = run_to_completion(&output);
    assert_eq!(cpu.r[3], 0x00FF, "R3 should be left unchanged");
    assert_eq!(
        cpu.r[4], 0x0100,
        "0x00FF + 0x0001 should carry into the high byte"
    );
}

/// The non-carrying case, as a control: no spurious carry should appear
/// when the low-byte add doesn't actually overflow.
#[test]
fn full_16bit_register_add_without_a_carry_through_the_real_cpu() {
    let source = "\n    LDI #00\n    PHI R3\n    LDI #01\n    PLO R3\n    LDI #00\n    PHI R4\n    LDI #01\n    PLO R4\n    R3 + R4 : R4\n";
    let output = transpile(source).expect("should transpile without errors");
    let cpu = run_to_completion(&output);
    assert_eq!(cpu.r[4], 0x0002);
}
