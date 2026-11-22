use super::*;
use cdp1802::{disasm, Operand};
use opcodes::ALL_INSTRUCTIONS;
use std::collections::BTreeSet;

/// Assemble and require success.
fn asm(src: &str) -> Assembly {
    let a = assemble(src);
    assert!(a.ok(), "errors: {:?}\n{}", a.errors, src);
    a
}

fn bytes_at(a: &Assembly, addr: u16, n: usize) -> Vec<u8> {
    (0..n)
        .map(|k| a.byte(addr + k as u16).expect("byte written"))
        .collect()
}

fn errors(src: &str) -> Vec<String> {
    assemble(src)
        .errors
        .into_iter()
        .map(|d| d.message)
        .collect()
}

fn sym(a: &Assembly, name: &str) -> i64 {
    a.symbols
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no symbol {name}"))
        .value
}

/// Every instruction with every operand variant assembles to bytes that the
/// disassembler decodes back to the same mnemonic and operand.
#[test]
fn every_instruction_round_trips_through_disasm() {
    let mut opcodes = BTreeSet::new();
    for &m in ALL_INSTRUCTIONS {
        let kind = opcodes::lookup(m).unwrap();
        let cases: Vec<(String, Operand)> = match kind {
            Kind::Implied(_) => vec![(String::new(), Operand::None)],
            Kind::Reg(_) => (0..16)
                .map(|r| (format!("R{r:X}"), Operand::Reg(r)))
                .collect(),
            Kind::RegNotZero(_) => (1..16).map(|r| (format!("{r}"), Operand::Reg(r))).collect(),
            Kind::Port(_) => (1..8).map(|p| (format!("{p}"), Operand::Port(p))).collect(),
            Kind::Imm(_) => vec![("0x5A".into(), Operand::Imm(0x5A))],
            Kind::Short(_) => vec![("0x1F0".into(), Operand::Addr(0x1F0))],
            Kind::Long(_) => vec![("0x1234".into(), Operand::Addr(0x1234))],
            other => panic!("{m}: unexpected kind {other:?}"),
        };
        for (operand, expect) in cases {
            let src = format!("    ORG 0x100\n    {m} {operand}\n");
            let a = asm(&src);
            let mut buf = [0u8; 3];
            for (k, b) in buf.iter_mut().enumerate() {
                *b = a.byte(0x100 + k as u16).unwrap_or(0);
            }
            let ins = disasm(&buf, 0x100);
            assert_eq!(ins.mnemonic, m, "{src}");
            assert_eq!(ins.operand, expect, "{src}");
            let len = a.segments()[0].1.len();
            assert_eq!(usize::from(ins.len), len, "{src}");
            opcodes.insert(buf[0]);
        }
    }
    // Every opcode except 68 (unused on the 1802).
    assert_eq!(opcodes.len(), 255);
    assert!(!opcodes.contains(&0x68));
}

#[test]
fn aliases_and_macros() {
    let a = asm("    BPZ $\n    BGE $\n    BM $\n    BL $\n    LBPZ 0\n    LBM 0\n    RSHR\n    RSHL\n    NBR\n    NLBR\n");
    assert_eq!(
        bytes_at(&a, 0, 16),
        [0x33, 0x00, 0x33, 0x02, 0x3B, 0x04, 0x3B, 0x06, 0xC3, 0, 0, 0xCB, 0, 0, 0x76, 0x7E]
    );
    assert_eq!(bytes_at(&a, 16, 2), [0x38, 0xC8]);

    let a = asm("    CALL SUB\n    RETURN   comment words\n    NOP\nSUB SEP R7  0x1A\n    SEP     R7         plain sep, comment\n");
    assert_eq!(
        bytes_at(&a, 0, 8),
        [0xD4, 0x00, 0x05, 0xD5, 0xC4, 0xD7, 0x1A, 0xD7]
    );
    assert_eq!(a.byte(8), None);
}

#[test]
fn qelfexe_syntax() {
    let src = "\
R4         EQU     4         REGISTER DEFINITION
FECH       EQU     0D7H      SEP R7 - PAGE 0 MACRO
STACK      DW      0x3FF7     HIGHEST ADD. FOR PROGRAM
           LDI     LOW $+3    CHANGE PC
           DB      FECH,STACK SET STACK
           BNZ     $-6 - ..UNTIL
ATMSG      DB      ' ','A','T' ERROR MESSAGE TEMPLATE
           DB      ' ',0A3H
           LDI     \"!\"       PRINT \"!\" ON NEW LINE
           LDI     LOW ATMSG - THEN TYPE \"AT\"
           LDI     HIGH ATMSG
           SEP     R4        (THIS IS NOT EXECUTED)
           SHL               ELSE MULTIPLY BY 2
           ORG     STACK+#80   #00
           B1      *          SYNCHRONIZE
           BNZ     *-2
";
    let a = asm(src);
    assert_eq!(sym(&a, "STACK"), 0);
    assert_eq!(sym(&a, "ATMSG"), 0x08);
    assert_eq!(
        bytes_at(&a, 0, 0x15),
        [
            0x3F, 0xF7, // DW
            0xF8, 0x05, // LDI LOW $+3
            0xD7, 0x00, // DB FECH,STACK
            0x3A, 0x00, // BNZ $-6
            0x20, 0x41, 0x54, 0x20, 0xA3, // DB ...
            0xF8, 0x21, // LDI "!"
            0xF8, 0x08, // LDI LOW ATMSG
            0xF8, 0x00, // LDI HIGH ATMSG
            0xD4, // SEP R4
            0xFE, // SHL
        ]
    );
    assert_eq!(a.byte(0x15), None);
    assert_eq!(bytes_at(&a, 0x80, 4), [0x34, 0x80, 0x3A, 0x80]);
}

#[test]
fn labels_may_be_mnemonic_names() {
    // (A `\` line continuation would strip the first line's indent.)
    let a = asm("           SEP     R3
CALL       PHI     RF         CALL ROUTINE
           BR      CALL-1
           LDI     CALL
RETURN:    RETURN
           LDI     RETURN
");
    assert_eq!(sym(&a, "CALL"), 1);
    assert_eq!(sym(&a, "RETURN"), 6);
    assert_eq!(
        bytes_at(&a, 0, 9),
        [0xD3, 0xBF, 0x30, 0x00, 0xF8, 0x01, 0xD5, 0xF8, 0x06]
    );
}

#[test]
fn forward_references_settle() {
    let a = asm("           ORG     START
           LBR     LATER
LATER      EQU     B+1
B          EQU     START+0x10
START      EQU     0x200
");
    assert_eq!(sym(&a, "LATER"), 0x211);
    assert_eq!(bytes_at(&a, 0x200, 3), [0xC0, 0x02, 0x11]);
}

#[test]
fn ds_reserves_and_end_stops() {
    let a = asm("    DB 1\nBUF DS 3\n    DB 2\n    END\n    DB 3\n");
    assert_eq!(sym(&a, "BUF"), 1);
    assert_eq!(a.byte(4), Some(2));
    assert_eq!(a.byte(1), None);
    assert_eq!(a.byte(5), None);
    assert_eq!(a.to_bin().unwrap(), (0, vec![1, 0, 0, 0, 2]));
}

#[test]
fn strings_and_items() {
    let a = asm("    DB 'HELLO', 0x0D, 'IT''S'\n    DW 1, 0x1234,'A'\n");
    assert_eq!(
        &a.to_bin().unwrap().1,
        b"HELLO\x0DIT'S\x00\x01\x12\x34\x00\x41"
    );
}

#[test]
fn predefined_registers_and_overrides() {
    let a = asm("    SEX R2\n    GLO RA\n    PHI R15\n");
    assert_eq!(bytes_at(&a, 0, 3), [0xE2, 0x8A, 0xBF]);
    let a = asm("R2 EQU 5\n    SEX R2\n");
    assert_eq!(a.byte(0), Some(0xE5));
}

#[test]
fn errors_are_reported() {
    let cases = [
        ("    ORG 0xF0\n    BR 0x200\n", "not in page"),
        ("    LBR NOWHERE\n", "undefined symbol `NOWHERE`"),
        ("    FOO 1\n", "unknown mnemonic `FOO`"),
        ("A   NOP\nA   NOP\n", "already defined at line 1"),
        ("    SEX 16\n", "register 16 out of range"),
        ("    OUT 0\n", "port 0 out of range"),
        ("    INP 8\n", "port 8 out of range"),
        ("    LDN R0\n", "LDN R0"),
        ("    LDI\n", "needs an operand"),
        ("    EQU 3\n", "EQU needs a label"),
        ("    DB 1\n    ORG 0\n    DB 2\n", "overwrites address 0000"),
        ("    ORG 0xFFFF\n    LBR 0\n", "past address FFFF"),
        ("    LDI 'AB'\n", "string"),
        ("    DW 0x10000\n", "16 bits"),
        ("1BAD NOP\n", "bad label"),
    ];
    for (src, want) in cases {
        let errs = errors(src);
        assert!(
            errs.iter().any(|e| e.contains(want)),
            "{src:?}: expected {want:?}, got {errs:?}"
        );
    }
    let a = assemble("    NOP\n    LBR NOWHERE\n");
    assert_eq!(a.errors[0].line, 2);
}

#[test]
fn short_branch_page_is_that_of_the_operand_byte() {
    // Opcode at 01FF, operand byte at 0200: the target must be in page 02.
    asm("    ORG 0x1FF\n    BR 0x210\n");
    let errs = errors("    ORG 0x1FF\n    BR 0x1F0\n");
    assert!(errs[0].contains("not in page 02"), "{errs:?}");
}

#[test]
fn hex_output() {
    let a = asm("    ORG 0x10\n    DB 1,2,3\n    ORG 0x100\n    LBR 0x1234\n");
    assert_eq!(
        a.to_hex(),
        ":03001000010203E7\n:03010000C01234F6\n:00000001FF\n"
    );
    assert_eq!(a.segments().len(), 2);
    assert_eq!(a.range(), Some((0x10, 0x102)));
}

#[test]
fn listing_has_addresses_bytes_and_symbols() {
    let a = assemble("; hi\nX   EQU 0x12\nST  LDI X\n    DB 1,2,3,4,5,6\n    FOO\n");
    let text = a.listing_text();
    assert!(text.contains(&format!("    2  {:4}  = 0012", "")), "{text}");
    assert!(text.contains("    3  0000  F8 12"), "{text}");
    assert!(text.contains("    4  0002  01 02 03 04"), "{text}");
    assert!(text.contains("       0006  05 06"), "{text}");
    assert!(text.contains("error: unknown mnemonic `FOO`"), "{text}");
    assert!(text.contains(&format!("{:<16} 0000", "ST")), "{text}");
}

#[test]
fn tiny_basic_il() {
    let a = asm("           ORG     0x100
           LDI     1
:S         PC      'A^B'   comment
           SX      3
           LB      0x1234
           LN      0x1234
           DS                IL DS, not define-storage
:L         BR      :S
           BR      :F
           BC      :F !5 'GO'
           BV      * !17
           BE      :F
           BN      :F
           JS      :L
           J       :S
:F         RT
           BR      :F        still IL
           LDI     2         back to 1802
           BR      $
           DS      2
           DB      9
");
    let want = [
        0xF8, 0x01, // LDI 1
        0x24, 0x01, 0xC2, // PC 'A^B'
        0x03, // SX 3
        0x09, 0x34, // LB
        0x0A, 0x12, 0x34, // LN
        0x0B, // DS
        0x55, // BR :S (-11)
        0x6A, // BR :F (+10)
        0x89, 0x47, 0xCF, // BC :F 'GO'
        0xA0, // BV *
        0xE5, // BE :F
        0xC4, // BN :F
        0x30, 0x0A, // JS :L (IL-relative 00A)
        0x38, 0x00, // J :S
        0x2F, // RT
        0x5E, // BR :F (-2)
        0xF8, 0x02, // LDI 2
        0x30, 0x1C, // 1802 BR $
    ];
    assert_eq!(bytes_at(&a, 0x100, want.len()), want);
    assert_eq!(a.byte(0x11E), None);
    assert_eq!(a.byte(0x120), Some(9));
}

#[test]
fn il_errors() {
    let il = "    ORG 0x100\n:S  RT\n";
    let cases = [
        ("    BE :S\n", "out of range"),
        ("    BC :S 'X'\n", "out of range"),
        ("    BR $+1\n", "out of range"),
        ("    BR $+40\n", "out of range"),
        ("    BV $+33\n", "out of range"),
        ("    JS 0x0FF\n", "not within 7FF"),
        ("    J 0x900\n", "not within 7FF"),
        ("    SX 8\n", "SX depth 8 out of range"),
        ("    PC\n", "missing string"),
        ("    PC ''\n", "empty string"),
        ("    PC 'ABC\n", "unterminated"),
        ("    BC $+5\n", "missing string"),
    ];
    for (line, want) in cases {
        let src = format!("{il}{line}");
        let errs = errors(&src);
        assert!(
            errs.iter().any(|e| e.contains(want)),
            "{src:?}: expected {want:?}, got {errs:?}"
        );
    }
    // Other delimiters and control characters.
    let a = asm(&format!("{il}    PC /J^@^/\n    BC * !12 \"'\"\n"));
    assert_eq!(bytes_at(&a, 0x101, 5), [0x24, 0x0A, 0x80, 0x80, 0xA7]);
}

/// Assemble `main` with an in-memory set of include files.
fn with_files(main: &str, files: &[(&str, &str)]) -> Assembly {
    let map: std::collections::HashMap<String, String> = files
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    assemble_with("main.asm", main, &[], &mut |req, _from| {
        map.get(req)
            .map(|t| (req.to_string(), t.clone()))
            .ok_or_else(|| format!("can't find `{req}`"))
    })
}

#[test]
fn include_inserts_lines_in_place() {
    let a = with_files(
        "    LDI 1\n    INCLUDE \"defs.asm\"  constants\n    LDI K\n",
        &[
            ("defs.asm", "K   EQU 0x42\n    INCLUDE more.asm\n"),
            ("more.asm", "    SEQ\n"),
        ],
    );
    assert!(a.ok(), "{:?}", a.errors);
    assert_eq!(a.to_bin().unwrap().1, [0xF8, 0x01, 0x7B, 0xF8, 0x42]);
    let k = a.symbols.iter().find(|s| s.name == "K").unwrap();
    assert_eq!((k.file.as_str(), k.line), ("defs.asm", 1));

    let text = a.listing_text();
    assert!(text.contains("---- defs.asm ----"), "{text}");
    assert!(text.contains("---- more.asm ----"), "{text}");
    assert!(text.contains("---- main.asm ----"), "{text}");
}

#[test]
fn include_errors_name_the_file() {
    let a = with_files(
        "    INCLUDE 'sub.asm'\n",
        &[("sub.asm", "    NOP\n    FOO\n")],
    );
    assert_eq!(a.errors.len(), 1);
    let d = &a.errors[0];
    assert_eq!((d.file.as_str(), d.line), ("sub.asm", 2));
    assert_eq!(d.to_string(), "sub.asm:2: unknown mnemonic `FOO`");

    let a = with_files("    NOP\n    INCLUDE nothere.asm\n", &[]);
    assert_eq!(a.errors[0].line, 2);
    assert!(a.errors[0].message.contains("can't find `nothere.asm`"));

    let a = with_files("    INCLUDE a.asm\n", &[("a.asm", "    INCLUDE a.asm\n")]);
    assert!(
        a.errors[0].message.contains("includes itself"),
        "{:?}",
        a.errors
    );

    for bad in ["    INCLUDE\n", "    INCLUDE \"x.asm\n"] {
        let a = with_files(bad, &[]);
        assert!(!a.ok(), "{bad:?}");
    }

    // Duplicate labels across files say where the first one is.
    let a = with_files("X   NOP\n    INCLUDE s.asm\n", &[("s.asm", "X   NOP\n")]);
    assert!(
        a.errors[0]
            .message
            .contains("already defined at main.asm:1"),
        "{:?}",
        a.errors
    );
}

#[test]
fn plain_assemble_rejects_include() {
    let errs = errors("    INCLUDE x.asm\n");
    assert!(errs[0].contains("INCLUDE needs a file"), "{errs:?}");
}

#[test]
fn assemble_file_resolves_relative_and_include_dirs() {
    let dir = std::env::temp_dir().join(format!("asm1802-inc-{}", std::process::id()));
    let sub = dir.join("sub");
    let lib = dir.join("lib");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(dir.join("main.asm"), "    INCLUDE sub/a.asm\n").unwrap();
    // Relative to sub/, then the -I folder.
    std::fs::write(sub.join("a.asm"), "    INCLUDE b.asm\n    INCLUDE c.asm\n").unwrap();
    std::fs::write(sub.join("b.asm"), "    DB 1\n").unwrap();
    std::fs::write(lib.join("c.asm"), "    DB 2\n").unwrap();

    let a = assemble_file(&dir.join("main.asm"), std::slice::from_ref(&lib), &[]).unwrap();
    assert!(a.ok(), "{:?}", a.errors);
    assert_eq!(a.to_bin().unwrap().1, [1, 2]);

    let a = assemble_file(&dir.join("main.asm"), &[], &[]).unwrap();
    assert!(a.errors[0].message.contains("can't find `c.asm`"));

    assert!(assemble_file(&dir.join("missing.asm"), &[], &[]).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rca_dc_and_equals() {
    let a = asm("    DC #01020300, #80, 'AB', #ABC\nSTACK   =*-1\nX = STACK+2\n    DC X\n");
    assert_eq!(
        a.to_bin().unwrap().1,
        [0x01, 0x02, 0x03, 0x00, 0x80, 0x41, 0x42, 0x0A, 0xBC, 0x0A]
    );
    assert_eq!(sym(&a, "STACK"), 8);
    assert_eq!(sym(&a, "X"), 10);
    assert!(errors("    = 5\n")[0].contains("EQU needs a label"));
}

#[test]
fn align_pads_up_to_the_next_multiple() {
    let a = asm("    ORG 0x10\n    DB 1,2,3\n    ALIGN 8\nALIGNED EQU $\n    DB 0xAA\n");
    assert_eq!(sym(&a, "ALIGNED"), 0x18);
    assert_eq!(a.byte(0x18), Some(0xAA));
    assert_eq!(a.byte(0x13), None); // the padding is reserved, not zero-filled

    // Already aligned: a no-op, zero bytes reserved.
    let a = asm("    ORG 0x20\n    ALIGN 8\nALIGNED EQU $\n    DB 1\n");
    assert_eq!(sym(&a, "ALIGNED"), 0x20);

    // ALIGN 1 is always a no-op.
    let a = asm("    ORG 0x11\n    ALIGN 1\nALIGNED EQU $\n");
    assert_eq!(sym(&a, "ALIGNED"), 0x11);

    // Non-power-of-2 alignment works too (it's just "next multiple of n").
    let a = asm("    ORG 0x11\n    ALIGN 10\nALIGNED EQU $\n");
    assert_eq!(sym(&a, "ALIGNED"), 20);
}

#[test]
fn align_errors() {
    for bad in ["    ALIGN 0\n", "    ALIGN -1\n", "    ALIGN\n"] {
        let errs = errors(bad);
        assert!(!errs.is_empty(), "{bad:?}");
    }
}

#[test]
fn rca_fig_forth_syntax() {
    // Dotted directives (RCA/FIG style): .ORG, .DB, .DW, .END, .EQU, .PAGE.
    let a = asm("    .ORG 0x10\n    .DB 1,2,3\n    .PAGE\n    .DW 0x1234\n    .END\n    .DB 9\n");
    assert_eq!(a.to_bin().unwrap().1, [1, 2, 3, 0x12, 0x34]);
    assert_eq!(a.byte(0x15), None); // .END stopped assembly before the DB 9

    // `.EQU name, value` (no label column) alongside the usual `name EQU value`.
    let a = asm("    .EQU FIRSTB, H'4000\nSTACK   EQU 9\n    .DW FIRSTB\n    .DW STACK\n");
    assert_eq!(sym(&a, "FIRSTB"), 0x4000);
    assert_eq!(sym(&a, "STACK"), 9);
    assert_eq!(a.to_bin().unwrap().1, [0x40, 0x00, 0x00, 0x09]);

    // `H'xxxx` numbers (no closing quote), including inside DB/DW item lists
    // and right next to a comma or another H' value with no space.
    let a = asm("    .DB H'83,\"LI\",H'D4\n    .DW H'070A\n    LDI (H'10+2)>>8\n");
    assert_eq!(
        a.to_bin().unwrap().1,
        [0x83, b'L', b'I', 0xD4, 0x07, 0x0A, 0xF8, 0x00]
    );

    let errs = errors("    .EQU 5\n"); // no comma, no label: still an error
    assert!(
        errs.iter().any(|e| e.contains("EQU needs a label")),
        "{errs:?}"
    );
}

#[test]
fn conditional_assembly() {
    // IFDEF true branch, IFNDEF false branch skipped.
    let a = asm("    DEFINE FOO\n    IFDEF FOO\n    DB 1\n    ELSE\n    DB 2\n    ENDIF\n    IFNDEF FOO\n    DB 3\n    ELSE\n    DB 4\n    ENDIF\n");
    assert_eq!(a.to_bin().unwrap().1, [1, 4]);

    // Not defined: IFDEF false branch (via ELSE), IFNDEF true branch.
    let a = asm("    IFDEF FOO\n    DB 1\n    ELSE\n    DB 2\n    ENDIF\n    IFNDEF FOO\n    DB 3\n    ENDIF\n");
    assert_eq!(a.to_bin().unwrap().1, [2, 3]);

    // No ELSE: IFDEF false with nothing after it is simply empty.
    let a = asm("    IFDEF NOPE\n    DB 1\n    ENDIF\n    DB 2\n");
    assert_eq!(a.to_bin().unwrap().1, [2]);

    // Nesting, including a nested IFDEF entirely inside a false branch (its
    // own condition must not matter, and DEFINE inside a false branch must
    // not take effect).
    let a = asm("    IFDEF OUTER\n\
         \x20   DEFINE INNER\n\
         \x20   IFDEF MISSING\n\
         \x20       DB 9\n\
         \x20   ENDIF\n\
         \x20   DB 1\n\
         \x20ENDIF\n\
         \x20IFDEF INNER\n\
         \x20   DB 2\n\
         \x20ELSE\n\
         \x20   DB 3\n\
         \x20ENDIF\n");
    assert_eq!(a.to_bin().unwrap().1, [3]); // OUTER undefined: all of it skipped,
                                            // so INNER was never DEFINEd either.

    // Skipped branches don't follow INCLUDE.
    let a = with_files(
        "    IFDEF NOPE\n    INCLUDE missing.asm\n    ENDIF\n    DB 5\n",
        &[],
    );
    assert!(a.ok(), "{:?}", a.errors);
    assert_eq!(a.to_bin().unwrap().1, [5]);
}

#[test]
fn conditional_assembly_errors() {
    for (src, want) in [
        ("    IFDEF X\n    DB 1\n", "unterminated"),
        ("    ENDIF\n", "ENDIF without"),
        ("    ELSE\n", "ELSE without"),
        (
            "    IFDEF X\n    ELSE\n    ELSE\n    ENDIF\n",
            "second ELSE",
        ),
    ] {
        let errs = errors(src);
        assert!(
            errs.iter().any(|e| e.contains(want)),
            "{src:?}: expected {want:?}, got {errs:?}"
        );
    }
}

#[test]
fn predefined_names_select_a_branch_from_the_start() {
    // The use case this is for: a shared, included file picks behaviour
    // based on a name the including program (or a `-D`) defined, without
    // editing the shared file.
    let a = with_files(
        "    DEFINE CALLER_B\n    INCLUDE shared.asm\n",
        &[(
            "shared.asm",
            "    IFDEF CALLER_A\n    DB 0xA\n    ENDIF\n    IFDEF CALLER_B\n    DB 0xB\n    ENDIF\n",
        )],
    );
    assert_eq!(a.to_bin().unwrap().1, [0xB]);

    // Same, via a name predefined like a command-line `-D` instead of a
    // `DEFINE` line.
    let a = assemble_with(
        "main.asm",
        "    IFDEF FROM_CLI\n    DB 7\n    ENDIF\n",
        &["FROM_CLI"],
        &mut |_, _| Err("no includes needed".into()),
    );
    assert!(a.ok(), "{:?}", a.errors);
    assert_eq!(a.to_bin().unwrap().1, [7]);
}
