//! End-to-end tests of the elf2-cli binary with the project test programs.

use std::io::Write;
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_elf2-cli");

fn programs() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/programs")
}

#[test]
fn help_lists_options() {
    let out = Command::new(BIN).arg("--help").output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("--boot-f000") && text.contains("--rom"));
}

#[test]
fn echo_program_over_stdin_and_stdout() {
    let hex = programs().join("serial_echo.hex");
    let mut child = Command::new(BIN)
        .args([
            "--load",
            hex.to_str().unwrap(),
            "--fast",
            "--exit-after-input",
            "200",
            "--screen",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"Hello\nElf\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "Hello\rElf\r");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("|Elf"), "screen dump:\n{err}");
}

#[test]
fn boot_f000_runs_a_rom_at_f000() {
    // ROM at F000: SEQ, IDL. With --boot-f000 the CPU runs 0000: LBR F000.
    let dir = std::env::temp_dir().join(format!("elf2-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let rom = dir.join("tiny.bin");
    std::fs::write(&rom, [0x7B, 0x00]).unwrap();
    let out = Command::new(BIN)
        .args([
            "--rom",
            &format!("{}@F000", rom.display()),
            "--boot-f000",
            "--no-stdin",
            "--fast",
            "--cycles",
            "100",
            "--dump",
            "0000-0002",
        ])
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(err.contains("0000: C0 F0 00"), "{err}");
    assert!(err.contains("IDL at F002"), "{err}");
    assert!(err.contains("Q=1"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn errors_are_reported() {
    let out = Command::new(BIN)
        .args(["--load", "nope.bin"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("@ADDR"));
}
