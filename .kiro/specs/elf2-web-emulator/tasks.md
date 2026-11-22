# Implementation plan: Netronics Elf II browser emulator

Each task builds on the previous ones and ends with passing tests. Requirement
numbers refer to `requirements.md`; property numbers refer to "Correctness
properties" in `design.md`.

- [x] 1. Set up the workspace and CI skeleton
  - Create `emulator/Cargo.toml` (workspace) and empty crates `cdp1802`, `elf2`, `elf2-wasm`, `elf2-cli` as laid out in the design.
  - Add `proptest` as a dev-dependency of `cdp1802` and `elf2`; `serde`/`serde_json` to `elf2`; `wasm-bindgen` to `elf2-wasm`.
  - Add `.github/workflows/ci.yml` running `cargo fmt --check`, `cargo clippy`, `cargo test --workspace` on push and pull request.
  - _Requirements: 1.4, 2.6_

- [x] 2. CPU core state, Bus trait and execution loop
  - [x] 2.1 Define `Cpu` state, `CpuState`, `Signals` and the `Bus` trait exactly as in the design.
    - Implement reset (R0 = 0, P = 0, X = 0, Q = 0, IE = 1, I/N cleared).
    - Write a `TestBus` (64K RAM, settable EF lines and signals, records OUT/INP/Q calls) for all core tests.
    - _Requirements: 2.2, 2.5, 3.5_
  - [x] 2.2 Implement `step()` priority order: DMA-IN, DMA-OUT, INT, idle, fetch/execute; return machine cycles and call `bus.tick`.
    - _Requirements: 2.3, 2.4_
  - [x]* 2.3 Property tests for DMA (property 4) and interrupt entry (property 5, entry part).
    - _Requirements: 2.4_

- [x] 3. Instruction set
  - [x] 3.1 Register and memory instructions: IDL, LDN, INC, DEC, GLO, GHI, PLO, PHI, LDA, STR, LDX, LDXA, STXD, LDI, SEP, SEX, IRX, SAV, MARK, RET, DIS, REQ, SEQ.
    - Table-driven unit tests with before/after states for each.
    - _Requirements: 2.1, 2.7_
  - [x] 3.2 Logic and arithmetic: OR, AND, XOR, ADD, ADC, SD, SDB, SM, SMB and immediate forms, SHR, SHRC, SHL, SHLC.
    - _Requirements: 2.1, 2.7_
  - [x]* 3.3 Property test: arithmetic and DF against a 9-bit reference model (property 2).
    - _Requirements: 2.7_
  - [x] 3.4 Short branches (30-3F), long branches and skips (C0-CF), NOP (C4), OUT 1-7 (61-67), INP 1-7 (69-6F), and opcode 68 as defined for the 1802.
    - _Requirements: 2.1, 2.3_
  - [x]* 3.5 Property tests: branch semantics (property 3) and cycle accounting for all 256 opcodes (property 1); RET/DIS part of property 5.
    - _Requirements: 2.3, 2.7_

- [x] 4. Disassembler
  - Implement `disasm(bytes, addr) -> (text, len)` with RCA mnemonics for all opcodes.
  - Unit test against a hand-checked listing; use it in core test failure messages.
  - _Requirements: 10.2_

- [x] 5. Machine core: configuration, memory map, front panel
  - [x] 5.1 `Config` and `SerialConfig` with serde JSON, defaults per the signal map (24K RAM, 1.789773 MHz, unmapped = FF, RX on EF3).
    - _Requirements: 3.1, 3.2, 3.3, 3.9_
  - [x] 5.2 `MemoryMap`: RAM, ROM slots with SHA-256, unmapped reads, MP write gate for CPU and DMA.
    - _Requirements: 3.2, 3.4, 3.7, 13.3_
  - [x]* 5.3 Property test: memory protect (property 9).
    - _Requirements: 3.7_
  - [x] 5.4 `Machine` implementing `Bus`, routing ports and EF lines to peripherals; `run(cycles)` with the event buffer and breakpoints.
    - _Requirements: 3.9, 5.2, 11.1_
  - [x] 5.5 `FrontPanel` state machine for RUN/LOAD (Reset, Load, Run, Pause) and the `start_at_f000` option.
    - _Requirements: 3.5, 3.8_

- [x] 6. Hex keypad and display
  - Implement the two-nibble keypad latch, INP 4, IN on EF4 in Run mode, OUT 4 display latch.
  - In Load mode, IN raises one DMA-IN; the byte goes through the MP gate and to the display.
  - Integration test: enter bytes with LOAD + IN, verify with MP on, run, check the display.
  - [x]* 6.1 Property test: keypad latch (property 8).
  - _Requirements: 3.6, 4.2, 4.3, 4.4, 4.5_

- [x] 7. CDP1861 Pixie
  - [x] 7.1 Implement the frame/line/cycle counter, INT held for 28 cycles, 8-cycle DMA-OUT bursts on 128 active lines, EF1 status, frame bitmap, on/off via INP/OUT 1 with `on_is_inp`.
    - Keep the EF1 line numbers and the on/off direction as constants in one module.
    - _Requirements: 6.1, 6.2, 6.3, 6.5, 6.6, 6.9_
  - [x] 7.2 Write a project test program that displays a 64x32 pattern (ISR rewinds R0) and one that uses 64x128; store source and `.hex` in `tests/programs/`.
    - _Requirements: 6.4, 13.4_
  - [x] 7.3 Integration tests: INT duration, exactly 128 bursts of 8 bytes per frame, EF1 timing, rendered bitmap matches the pattern, and a 3-cycle instruction in main code shifts the line.
    - _Requirements: 6.2, 6.3, 6.4, 6.7_

- [x] 8. Giant Board bit-banged serial
  - [x] 8.1 TX decoder: timestamp Q changes, UART receive at bit centres, framing-error detection and resync.
    - _Requirements: 7.1, 7.4, 7.6_
  - [x] 8.2 RX scheduler: character queue to timed EF levels, polarity options, EF3/EF4 selection, EF4 OR with the IN button, paste pacing.
    - _Requirements: 5.2, 7.1, 7.2, 7.3, 8.10_
  - [x]* 8.3 Property test: serial round trip with ±10% jitter (property 6).
    - _Requirements: 7.4, 7.6_
  - [x] 8.4 Project echo program (bit-banged, Q / EF3, 300 baud) and integration test through the full machine.
    - _Requirements: 7.1, 7.4, 13.4_

- [x] 9. VID-1 terminal model
  - Implement the 16 x 64 / 32 screen buffer, cursor, scrolling, the control-code table (BS, LF, FF, CR, DEL defaults; editable entries for the [verify] codes), no local echo.
  - Unit tests for each control code and scrolling at line 16; integration test with the echo program from 8.4.
  - _Requirements: 8.1, 8.2, 8.3, 8.5, 8.6_

- [x] 10. CDP1852 parallel ports and cassette slot
  - Implement INP 7 / OUT 7 latches and service-request routing to INT (off by default).
  - Add the `Cassette` peripheral slot (Q edge input, EF2 source) as a stub that reads idle.
  - _Requirements: 9.1, 9.2, 10.3_

- [x] 11. Program files and snapshots
  - [x] 11.1 `.bin` load/save at an address and Intel HEX load/save with checksum and line-numbered errors; reject oversize loads.
    - _Requirements: 10.1, 10.2_
  - [x]* 11.2 Property test: Intel HEX round trip (property 7).
    - _Requirements: 10.2_
  - [x] 11.3 Snapshot and restore (magic, version, JSON state, 64K memory image) of RAM, CPU and peripheral state, with a format version check.
    - _Requirements: 10.4_

- [x] 12. Native runner `elf2-cli`
  - Options: `--config`, `--rom file@addr`, `--load file@addr`, `--run`, `--cycles N`, `--baud`, `--rx-ef`.
  - Connect the VID-1 model to stdin/stdout (raw mode); print the hex display and Q changes to stderr on request.
  - Manual check (owner, not CI): run the Giant Board monitor ROM and record the working serial polarity, baud rate and F000 start setting in `requirements.md`. Done for the monitor: started with `--boot-f000`, it sizes RAM and waits for IN (BN4 at F029); it has no serial code, so serial settings will be checked with Tiny BASIC.
  - _Requirements: 3.8, 7.2, 7.4_

- [x] 13. Wasm API
  - Implement `Emulator` in `elf2-wasm` with the methods in the design; packed event list; zero-copy views for the Pixie frame and VID-1 screen; panic hook.
  - `wasm-pack build --target web` builds without warnings; a `wasm-bindgen-test` smoke test runs the echo program.
  - _Requirements: 1.1, 1.5, 11.1_

- [x] 14. Web front end: shell and worker loop
  - [x] 14.1 `index.html`, CSS layout, `main.js` loading the worker; `worker.js` owning the `Emulator`, time-slice loop with the 50 ms cap and max-speed mode, message protocol.
    - _Requirements: 1.1, 1.2, 1.3, 3.3, 12.1, 12.2, 12.3_
  - [x] 14.2 Front panel: RUN, LOAD, MP switches, IN button, 16 hex keys, two seven-segment digits, Q LED, EF indicators; mouse, touch and keyboard shortcuts.
    - _Requirements: 4.1, 4.5, 5.1, 5.3_
  - [x] 14.3 Pixie canvas: 64x128 bitmap, integer scaling, optional scanline style.
    - _Requirements: 6.8_
  - [x] 14.4 VID-1 canvas: character cells, bitmap font (substitute until the ROM font is available), cursor, focus-based key capture, Ctrl keys, upper-case option, copy, paste, clear.
    - _Requirements: 8.3, 8.4, 8.8, 8.9, 8.10, 8.11_
  - [ ] 14.5 Optional Q-to-speaker audio with Web Audio. (Not done yet: needs Q edge timestamps from the worker, since Q events are coalesced per run.)
    - _Requirements: 5.4_

- [x] 15. Debugger panel
  - Registers with P and X marked, D, DF, Q, IE, EF lines; disassembly around PC; memory viewer/editor; breakpoints; run, stop, step, reset; serial line monitor with framing errors.
  - _Requirements: 11.1, 11.2, 11.3, 11.4_

- [x] 16. Files, ROM manager and persistence
  - File picker and drag-and-drop for `.bin` / `.hex` with a target address; save a range.
  - ROM manager: add ROM files to IndexedDB (keyed by SHA-256), assign base addresses, missing-ROM banner.
  - Config in `localStorage`; snapshot auto-save on page hide and restore on load; `?load=file@addr` for project programs in `web/programs/`.
  - _Requirements: 10.1, 10.2, 10.4, 10.5, 13.1, 13.2, 13.3_

- [ ] 17. GitHub Pages deployment
  - `.github/workflows/pages.yml`: native tests, `wasm-pack build --release`, upload `web/`, deploy.
  - Check the deployed site loads in Chrome, Edge, Firefox and Safari, the initial download is under 2 MB, and the echo and Pixie test programs run at real time.
  - _Requirements: 1.1, 1.2, 1.4, 1.5, 12.1_

- [ ] 18. Resolve [verify] items
  - Using the owner's manuals and ROMs: CDP1861 on/off direction and EF1 lines, Giant Board serial polarity, baud rates and F000 start, VID-1 control codes and font, ASCII keyboard special keys, display content in Load mode with MP on.
  - Update the constants, default config and control-code table; record the sources in `requirements.md`.
  - _Requirements: 3.6, 3.8, 6.5, 6.6, 7.2, 7.4, 8.4, 8.6, 8.9_
