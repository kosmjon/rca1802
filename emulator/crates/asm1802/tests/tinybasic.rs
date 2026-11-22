//! Assemble the Netronics Tiny BASIC recreation source (QELFEXE dialect,
//! 1802 code plus Tom Pittman's IL).

use std::path::Path;

fn source() -> Option<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../sample-code/asm/Tinybasi.asm");
    let bytes = std::fs::read(path).ok()?;
    Some(bytes.iter().map(|&b| b as char).collect())
}

#[test]
fn tiny_basic_assembles() {
    let Some(src) = source() else {
        eprintln!("sample-code/asm/Tinybasi.asm not found; skipping");
        return;
    };
    let asm = asm1802::assemble(&src);
    assert!(asm.ok(), "errors: {:?}", asm.errors);

    let sym = |name: &str| {
        asm.symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no symbol {name}"))
            .value
    };
    let word = |a: u16| u16::from_be_bytes([asm.byte(a).unwrap(), asm.byte(a + 1).unwrap()]);

    // LBR COLDV; LBR MONITOR; SEP R0; IDL
    let start: Vec<u8> = (0..8).map(|a| asm.byte(a).unwrap()).collect();
    assert_eq!(start, [0xC0, 0x01, 0x00, 0xC0, 0xF0, 0x00, 0xD0, 0x00]);
    assert_eq!(sym("COLDV"), 0x0100);
    assert_eq!(sym("AESTK"), 0x0080);

    // The source's data area holds values captured from a running copy:
    // AIL (IL start) = 0766 and NXA (statement executor) = 0773.
    assert_eq!(sym(":STRT"), 0x0766);
    assert_eq!(i64::from(word(sym("AIL") as u16)), sym(":STRT"));
    assert_eq!(i64::from(word(sym("NXA") as u16)), sym(":STMT"));

    // The program ends right where the display buffer starts.
    assert_eq!(asm.range(), Some((0x0000, 0x0DAF)));
    assert_eq!(sym("BUFF"), 0x0DB0);
}

/// `BV * !18` style remarks give the error number Tiny BASIC prints for that
/// error stop, as an IL-relative address. Every one must match the addresses
/// the assembler produced.
#[test]
fn il_error_numbers_match() {
    let Some(src) = source() else { return };
    let asm = asm1802::assemble(&src);
    let base = asm
        .symbols
        .iter()
        .find(|s| s.name == ":STRT")
        .unwrap()
        .value;
    let mut checked = 0;
    for l in &asm.listing {
        let Some(pos) = l.source.find("* !") else {
            continue;
        };
        let digits: String = l.source[pos + 3..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let want: i64 = digits.parse().unwrap();
        // For BC the remark points past the first string byte (where a
        // mismatch on the first character stops), not past the whole string.
        let len = if l.bytes[0] & 0xE0 == 0x80 {
            2
        } else {
            l.bytes.len() as i64
        };
        let after = i64::from(l.addr.unwrap()) + len;
        assert_eq!(after - base, want, "line {}: {}", l.line, l.source);
        // The error-stop branch has a zero offset.
        assert_eq!(l.bytes[0] & 0x1F, 0, "line {}: {}", l.line, l.source);
        checked += 1;
    }
    assert_eq!(checked, 26);
}

#[test]
fn hex_output_parses_back() {
    let src = "    ORG 0x10\n    DB 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'\n    ORG 0x300\n    LBR 0x1234\n";
    let asm = asm1802::assemble(src);
    assert!(asm.ok(), "{:?}", asm.errors);
    let chunks = elf2::hexfile::parse(&asm.to_hex()).unwrap();
    let mut mem = vec![None; 0x10000];
    for c in chunks {
        for (k, b) in c.data.iter().enumerate() {
            mem[usize::from(c.addr) + k] = Some(*b);
        }
    }
    for a in 0..=0xFFFFu16 {
        assert_eq!(mem[usize::from(a)], asm.byte(a), "address {a:04X}");
    }
}
