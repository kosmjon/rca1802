//! CLI tests for `INCLUDE "x.m18"`: each included `.m18` is transpiled to a
//! sibling `.asm`, and the include line in the output is rewritten to name it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mol1802-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&Path]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mol1802"))
        .args(args)
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn m18_include_is_transpiled_and_assembled() {
    let dir = scratch_dir("one");
    std::fs::write(
        dir.join("top.m18"),
        "    ORG 0\n    INCLUDE \"lib.m18\"\n    INCLUDE \"plain.asm\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("lib.m18"), "LIB: R3.0 : D\n").unwrap();
    std::fs::write(dir.join("plain.asm"), "    NOP\n").unwrap();

    let o = run(&[&dir.join("top.m18"), Path::new("--assemble")]);
    assert!(o.status.success(), "{}", stderr(&o));

    let top = std::fs::read_to_string(dir.join("top.asm")).unwrap();
    assert!(top.contains("INCLUDE \"lib.asm\""), "{top}");
    assert!(top.contains("INCLUDE \"plain.asm\""), "{top}");
    assert_eq!(
        std::fs::read_to_string(dir.join("lib.asm")).unwrap(),
        "LIB: GLO R3\n"
    );
    // GLO R3 (0x83), NOP (0xC4)
    let hex = std::fs::read_to_string(dir.join("top.hex")).unwrap();
    assert!(hex.contains("83C4"), "{hex}");
}

#[test]
fn nested_m18_includes_resolve_relative_to_the_including_file() {
    let dir = scratch_dir("nested");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("top.m18"), "    INCLUDE \"sub/a.m18\"\n").unwrap();
    std::fs::write(dir.join("sub/a.m18"), "    INCLUDE \"b.m18\"\n").unwrap();
    std::fs::write(dir.join("sub/b.m18"), "    R4.0 : D\n").unwrap();

    let o = run(&[&dir.join("top.m18")]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(std::fs::read_to_string(dir.join("top.asm"))
        .unwrap()
        .contains("INCLUDE \"sub/a.asm\""));
    assert!(std::fs::read_to_string(dir.join("sub/a.asm"))
        .unwrap()
        .contains("INCLUDE \"b.asm\""));
    assert!(std::fs::read_to_string(dir.join("sub/b.asm"))
        .unwrap()
        .contains("GLO R4"));
}

#[test]
fn missing_m18_include_fails_and_writes_no_parent_output() {
    let dir = scratch_dir("missing");
    std::fs::write(dir.join("top.m18"), "    INCLUDE \"nope.m18\"\n").unwrap();
    let o = run(&[&dir.join("top.m18")]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("nope.m18"), "{}", stderr(&o));
    assert!(!dir.join("top.asm").exists());
}

#[test]
fn include_cycle_is_reported() {
    let dir = scratch_dir("cycle");
    std::fs::write(dir.join("a.m18"), "    INCLUDE \"b.m18\"\n").unwrap();
    std::fs::write(dir.join("b.m18"), "    INCLUDE \"a.m18\"\n").unwrap();
    let o = run(&[&dir.join("a.m18")]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("cycle"), "{}", stderr(&o));
    assert!(!dir.join("a.asm").exists());
}

#[test]
fn error_in_included_file_names_that_file_and_blocks_parent() {
    let dir = scratch_dir("childerr");
    std::fs::write(dir.join("top.m18"), "    INCLUDE \"bad.m18\"\n").unwrap();
    std::fs::write(dir.join("bad.m18"), "    NOT A STATEMENT : : :\n").unwrap();
    let o = run(&[&dir.join("top.m18")]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("bad.m18:1"), "{}", stderr(&o));
    assert!(!dir.join("top.asm").exists());
}
