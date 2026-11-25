use super::*;

/// Assemble `src` with asm1802 and return the flat image (start, bytes).
fn assemble(src: &str) -> (u16, Vec<u8>) {
    let asm = asm1802::assemble(src);
    assert!(asm.ok(), "assembly failed:\n{src}\nerrors: {:?}", asm.errors);
    asm.to_bin().expect("some bytes")
}

/// The round-trip gate: disassembling `bytes` loaded at `org` and reassembling
/// the listing must reproduce the same bytes.
fn assert_round_trips(org: u16, bytes: &[u8], opts: &Options) {
    let prog = Program::new(org, bytes.to_vec());
    let listing = disassemble(&prog, opts);
    let text = listing.to_text();
    let (start, out) = assemble(&text);
    assert_eq!(start, org, "org changed\n--- listing ---\n{text}");
    assert_eq!(
        out, bytes,
        "bytes changed on reassembly\n--- listing ---\n{text}"
    );
}

#[test]
fn labels_a_branch_target() {
    // 0000 BR 0004 ; skip two data bytes, then NOP at 0004.
    //   30 04   (BR to 0004, short branch in page 0)
    //   00 00   (data skipped)
    //   C4      (NOP)
    let bytes = [0x30, 0x04, 0xFF, 0xFF, 0xC4];
    let prog = Program::new(0x0000, bytes.to_vec());
    let listing = disassemble(&prog, &Options::default());
    let text = listing.to_text();
    assert!(text.contains("BR L0004"), "no label on branch:\n{text}");
    assert!(
        text.lines().any(|l| l.starts_with("L0004:")),
        "no label defined at target:\n{text}"
    );
    // The two skipped bytes between the branch and the target are data.
    assert!(text.contains("DB"), "skipped bytes not data:\n{text}");
}

#[test]
fn unreached_bytes_after_unconditional_branch_are_data() {
    // LBR to 0005 over two bytes that are not reachable as code.
    //   C0 00 05   LBR 0005
    //   68 68      unreachable (68 has no 1802 instruction)
    //   C4         NOP at 0005
    let bytes = [0xC0, 0x00, 0x05, 0x68, 0x68, 0xC4];
    let prog = Program::new(0x0000, bytes.to_vec());
    let text = disassemble(&prog, &Options::default()).to_text();
    assert!(text.contains("LBR L0005"), "{text}");
    // The two 68 bytes are data, not decoded as instructions.
    assert!(text.contains("DB 0x68"), "68 not data:\n{text}");
}

#[test]
fn scrt_call_collapses_to_a_call_directive() {
    // SEP R4 then inline DW 0006 (call); the three bytes collapse to CALL.
    //   D4 00 06   call 0006
    //   C4         NOP (continues here after the call)
    //   00         pad to 0006
    //   D5         SEP R5 (return) at 0006
    let bytes = [0xD4, 0x00, 0x06, 0xC4, 0x00, 0x00, 0xD5];
    let prog = Program::new(0x0000, bytes.to_vec());
    let text = disassemble(&prog, &Options::default()).to_text();
    assert!(text.contains("CALL L0006"), "call not collapsed:\n{text}");
    // The three call bytes are one line now, so no bare SEP R4 or DW remains.
    assert!(!text.contains("SEP R4"), "call left as SEP R4:\n{text}");
    assert!(!text.contains("DW "), "call left a DW word:\n{text}");
    assert!(
        text.lines().any(|l| l.starts_with("L0006:")),
        "no label at call target:\n{text}"
    );
}

#[test]
fn no_scrt_disassembles_the_word_as_code() {
    let bytes = [0xD4, 0x00, 0x06, 0xC4, 0x00, 0x00, 0xD5];
    let opts = Options {
        scrt: false,
        ..Options::default()
    };
    let text = disassemble(&Program::new(0, bytes.to_vec()), &opts).to_text();
    // Without SCRT, the byte after SEP R4 is decoded as an instruction, so
    // there is no DW address word.
    assert!(!text.contains("DW "), "should not treat word as data:\n{text}");
}

#[test]
fn round_trip_small_program() {
    // A mix of ALU, immediate, short and long branches, I/O and data.
    let src = concat!(
        "        ORG 0x0100\n",
        "START:  LDI 0x2A\n",
        "        PLO R7\n",
        "        SEX R2\n",
        "LOOP:   OUT 4\n",
        "        BNZ LOOP\n",
        "        LBR DONE\n",
        "TABLE:  DB 0x01,0x02,0x03\n",
        "DONE:   SEQ\n",
        "        BR START\n",
    );
    let (org, bytes) = assemble(src);
    assert_round_trips(org, &bytes, &Options::default());
}

#[test]
fn round_trip_with_scrt_calls() {
    let src = concat!(
        "        ORG 0x0000\n",
        "MAIN:   SEP R4\n",
        "        DW SUB\n",
        "        BR MAIN\n",
        "SUB:    LDI 0x01\n",
        "        SEP R5\n",
    );
    let (org, bytes) = assemble(src);
    assert_round_trips(org, &bytes, &Options::default());
}

#[test]
fn scrt_setup_labels_and_traces_the_handler() {
    // The SCRT setup builds R4 = 0008 (the call handler), then an unconditional
    // LBR ends fall-through. The byte at 0008 is reached only because the
    // register tracker resolves R4 and seeds it as a code entry.
    //   0000 90       GHI R0        (high byte 0)
    //   0001 B4       PHI R4
    //   0002 F8 08    LDI 08
    //   0004 A4       PLO R4        ; R4 = 0008 handler
    //   0005 C0 00 00 LBR 0000      (unconditional end)
    //   0008 7B       SEQ           (the handler code)
    //   0009 30 08    BR 0008
    let bytes = [
        0x90, 0xB4, 0xF8, 0x08, 0xA4, 0xC0, 0x00, 0x00, 0x7B, 0x30, 0x08,
    ];
    let text = disassemble(&Program::new(0, bytes.to_vec()), &Options::default()).to_text();
    assert!(
        text.contains("set up R4 -> SCRT call handler"),
        "no handler role comment:\n{text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("SCRTCALL:")),
        "handler not named SCRTCALL:\n{text}"
    );
    // The low-byte load that built R4 is rewritten to reference the handler.
    assert!(
        text.contains("LDI LOW SCRTCALL"),
        "setup not rewritten to use the symbol:\n{text}"
    );
    // The handler byte 7B must be decoded as code (SEQ), not left as data.
    assert!(text.contains("SEQ"), "handler not traced as code:\n{text}");
}

#[test]
fn full_word_stack_load_is_labelled() {
    // LDI 7F; PHI R2; LDI FF; PLO R2  -> R2 = 7FFF (stack pointer).
    //   F8 7F B2 F8 FF A2   then IDL to end.
    let bytes = [0xF8, 0x7F, 0xB2, 0xF8, 0xFF, 0xA2, 0x00];
    let text = disassemble(&Program::new(0, bytes.to_vec()), &Options::default()).to_text();
    assert!(
        text.contains("= stack pointer"),
        "no stack-pointer comment:\n{text}"
    );
    // 7FFF is outside this tiny image, so it is an EQU named STACK, and the
    // setup loads are rewritten to HIGH/LOW STACK.
    assert!(
        text.lines().any(|l| l.starts_with("STACK") && l.contains("EQU 0x7FFF")),
        "no stack EQU:\n{text}"
    );
    assert!(text.contains("LDI HIGH STACK"), "high load not rewritten:\n{text}");
    assert!(text.contains("LDI LOW STACK"), "low load not rewritten:\n{text}");
}

#[test]
fn illegal_opcode_in_code_is_flagged() {
    // Reached-as-code 0x68 should be flagged as a possible scan error.
    //   7B 68 00   SEQ, then 68 (illegal), then IDL
    let bytes = [0x7B, 0x68, 0x00];
    let text = disassemble(&Program::new(0, bytes.to_vec()), &Options::default()).to_text();
    assert!(
        text.contains("illegal opcode 68"),
        "68 not flagged:\n{text}"
    );
}

#[test]
fn round_trip_sample_pixieanimation() {
    // The committed sample program, if present.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../sample-code/asm/pixieanimation.hex"
    );
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("pixieanimation.hex not found; skipping");
        return;
    };
    let chunks = elf2::hexfile::parse(&text).expect("valid hex");
    let lo = chunks.iter().map(|c| c.addr).min().unwrap();
    let hi = chunks
        .iter()
        .map(|c| u32::from(c.addr) + c.data.len() as u32)
        .max()
        .unwrap();
    let mut bytes = vec![0u8; (hi - u32::from(lo)) as usize];
    for c in &chunks {
        let off = usize::from(c.addr) - usize::from(lo);
        bytes[off..off + c.data.len()].copy_from_slice(&c.data);
    }
    assert_round_trips(lo, &bytes, &Options::default());
}
