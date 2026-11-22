//! Smoke test in a real WebAssembly runtime (Node.js): `wasm-pack test --node crates/elf2-wasm`.
#![cfg(target_arch = "wasm32")]

use elf2_wasm::{Emulator, EventKind};
use wasm_bindgen_test::*;

const ECHO: &[u8] = include_bytes!("../../../tests/programs/serial_echo.bin");
const PIXIE: &[u8] = include_bytes!("../../../tests/programs/pixie64x128.bin");

fn chars(events: &[u32]) -> Vec<u8> {
    events
        .iter()
        .filter(|&&e| e >> 24 == EventKind::TermChar as u32)
        .map(|&e| e as u8)
        .collect()
}

#[wasm_bindgen_test]
fn echo_program_runs_in_wasm() {
    let mut e = Emulator::new("{}").unwrap();
    e.load_bin(0, ECHO).unwrap();
    e.set_switch(0, true);
    let mut got = Vec::new();
    for &k in b"wasm" {
        e.term_key(k);
        for _ in 0..10 {
            e.run(5_000);
            got.extend(chars(&e.events()));
        }
    }
    assert_eq!(got, b"wasm");
}

#[wasm_bindgen_test]
fn pixie_frame_is_readable_from_wasm_memory() {
    let mut e = Emulator::new("{}").unwrap();
    e.load_bin(0, PIXIE).unwrap();
    let pic: Vec<u8> = (0..1024u32).map(|i| (i * 13) as u8).collect();
    e.load_bin(0x0100, &pic).unwrap();
    e.set_switch(0, true);
    let mut frames = 0;
    for _ in 0..100 {
        e.run(2_000);
        frames += e
            .events()
            .iter()
            .filter(|&&x| x >> 24 == EventKind::PixieFrame as u32)
            .count();
        if frames >= 2 {
            break;
        }
    }
    assert!(frames >= 2);
    let ptr = e.pixie_frame_ptr() as usize;
    // Inside wasm, the pointer is a plain address in linear memory.
    let frame = unsafe { std::slice::from_raw_parts(ptr as *const u8, 1024) };
    assert_eq!(frame, &pic[..]);
}
