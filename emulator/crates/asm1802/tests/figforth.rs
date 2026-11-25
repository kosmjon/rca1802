//! Assemble the fig-FORTH 1802 port (RCA/FIG dialect: dotted directives,
//! `H'xxxx` numbers, `.EQU name, value`) and check that it actually boots and
//! runs in the emulator over the bit-banged serial port
//! (`sample-code/include/baud_serial.asm`), which replaces the CDP1854 UART
//! the original source targeted.
//!
//! This source used to be checked byte-for-byte against a reference
//! `FigForth.hex` shipped alongside it. That reference turned out to share
//! two latent bugs with the source as originally transcribed (both in the
//! "operand ends at the first space outside LOW/HIGH/a comma" dialect rule
//! this assembler and, apparently, whatever assembled the reference also
//! follow):
//!   - `BR NEXT - 1` assembled as `BR NEXT`, so the inner interpreter never
//!     reached the `SEP R3` that dispatches into a primitive.
//!   - `.DW * + 2` (the standard primitive code-field initializer, used
//!     ~100 times) assembled as `.DW *`, making every primitive
//!     self-referential; by luck a few "worked" because the resulting
//!     garbage bytes happened to decode as harmless 1-byte opcodes that
//!     landed back on the real code, but most did not.
//!
//! Both are fixed in `sample-code/asm/FigForth.ASM` (all `LABEL - N` /
//! `LABEL + N` spacing compacted throughout), which is why the byte-exact
//! comparison no longer applies: the source is now intentionally corrected.
//! The real test of correctness is that it boots and actually runs FORTH,
//! which is what this file checks instead.

use elf2::{Config, Event, Machine};
use std::path::{Path, PathBuf};

fn source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../sample-code/asm")
}

fn assemble() -> Option<asm1802::Assembly> {
    let dir = source_dir();
    let path = dir.join("FigForth.ASM");
    if !path.exists() {
        eprintln!("sample-code/asm/FigForth.ASM not found; skipping");
        return None;
    }
    Some(asm1802::assemble_file(&path, &[dir], &[]).expect("FigForth.ASM should be readable"))
}

#[test]
fn fig_forth_assembles_with_the_bit_banged_serial_include() {
    let Some(asm) = assemble() else { return };
    assert!(asm.ok(), "errors: {:?}", asm.errors);

    let sym = |name: &str| {
        asm.symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("no symbol {name}"))
            .value
    };
    // The SCRT CALL/RETURN trampoline and the shared serial routines must
    // all have resolved to real addresses.
    for name in ["CALL", "RETURN", "BSINIT", "SEROUT", "SERIN", "SERBRK"] {
        sym(name);
    }
}

/// Boot the assembled image and drive it over the emulator's default
/// bit-banged serial port (`Config::default()`, no overrides): the same
/// port and defaults a real user gets. Characters are paced 100ms apart,
/// which is well past one echoed frame at 300 baud (~33ms) plus the FORTH
/// loop's own overhead between keystrokes -- pasting faster than that can
/// start `SERIN` polling mid-frame on the next character, which is a
/// real-time property of polled bit-banged serial at this baud rate, not a
/// bug in the port.
fn boot_and_type(hex: &str, line: &str, settle_secs: u64) -> (String, u32) {
    let mut m = Machine::new(Config::default()).unwrap();
    m.load_hex(hex).unwrap();
    m.set_switch(elf2::Switch::Run, true);
    let cps = u64::from(m.config().cycles_per_second());

    let mut out = String::new();
    let mut framing_errors = 0u32;
    let mut pump = |m: &mut Machine, cycles: u64| {
        let mut left = cycles;
        while left > 0 {
            let r = m.run(left.min(cps / 100));
            left = left.saturating_sub(r.cycles.max(1));
            for e in m.events() {
                match *e {
                    Event::TermChar(c) => out.push((c & 0x7F) as char),
                    Event::TermFramingError => framing_errors += 1,
                    _ => {}
                }
            }
        }
    };

    pump(&mut m, 5 * cps); // boot banner
    let gap = cps / 10; // 100ms
    for &b in line.as_bytes() {
        m.term_key(b);
        pump(&mut m, gap);
    }
    pump(&mut m, settle_secs * cps);
    (out, framing_errors)
}

#[test]
fn fig_forth_boots_and_prints_its_banner() {
    let Some(asm) = assemble() else { return };
    assert!(asm.ok(), "errors: {:?}", asm.errors);
    let (out, framing_errors) = boot_and_type(&asm.to_hex(), "", 0);
    assert_eq!(framing_errors, 0);
    assert!(
        out.contains("1802 FIG-FORTH"),
        "expected the boot banner, got {out:?}"
    );
}

#[test]
fn fig_forth_computes_and_echoes_over_bit_banged_serial() {
    let Some(asm) = assemble() else { return };
    assert!(asm.ok(), "errors: {:?}", asm.errors);
    let (out, framing_errors) = boot_and_type(&asm.to_hex(), "1 2 + .\r", 1);
    assert_eq!(framing_errors, 0, "output so far: {out:?}");
    assert!(
        out.ends_with("1 2 + . 3   OK\r\n"),
        "echoed input, the computed sum (3) and the OK prompt, got {out:?}"
    );
}
