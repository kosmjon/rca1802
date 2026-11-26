//! WebAssembly bindings: the `Emulator` class used by the web front end.
//!
//! All emulated time is in machine cycles; the caller decides how many to run per
//! slice. Events from each `run` are returned as packed `u32` values (see
//! [`EventKind`]); bitmaps are read straight from wasm memory through the pointers
//! returned by `pixie_frame_ptr` and `vid1_screen_ptr`, with no copy per frame.

use elf2::cdp1802::{disasm, CpuState};
use elf2::{Config, Event, Machine, StopReason, Switch};
use serde_json::json;
use wasm_bindgen::prelude::*;

/// Emulator version string.
#[wasm_bindgen]
pub fn version() -> String {
    elf2::cdp1802::VERSION.to_string()
}

/// The last path component (file name) of `s`, lower-cased for comparison.
fn basename_lower(s: &str) -> String {
    s.rsplit(['/', '\\'])
        .next()
        .unwrap_or(s)
        .to_ascii_lowercase()
}

/// Normalise a path for suffix comparison: lower-case, backslashes to
/// slashes, and any leading `./` or `../` segments dropped.
fn norm_path(s: &str) -> String {
    let s = s.replace('\\', "/").to_ascii_lowercase();
    let kept: Vec<&str> = s
        .split('/')
        .skip_while(|seg| seg.is_empty() || *seg == "." || *seg == "..")
        .collect();
    kept.join("/")
}

/// Finds the uploaded file an `INCLUDE "req"` names, among `files`. Prefers a
/// path match (the uploaded path ends with the requested path, e.g.
/// `proj/asm/include/baud_serial.asm` matches `include/baud_serial.asm`), so
/// that two files with the same base name in different folders stay distinct.
/// Falls back to a unique base-name match, which covers `INCLUDE "x"` when the
/// uploaded file was added from a plain (path-less) file pick. Returns `None`
/// if nothing matches, or if only an ambiguous base-name match exists.
fn find_include<'a>(files: &'a [SourceFile], req: &str) -> Option<&'a SourceFile> {
    let want = norm_path(req);
    // 1. Exact normalised path, or uploaded path ending in `/want`.
    let path_hits: Vec<&SourceFile> = files
        .iter()
        .filter(|f| {
            let have = norm_path(&f.name);
            have == want || have.ends_with(&format!("/{want}"))
        })
        .collect();
    if let [one] = path_hits.as_slice() {
        return Some(one);
    }
    if path_hits.len() > 1 {
        return None; // ambiguous; caller reports "not found" for a clear message
    }
    // 2. Unique base-name match.
    let base = basename_lower(req);
    let name_hits: Vec<&SourceFile> = files
        .iter()
        .filter(|f| basename_lower(&f.name) == base)
        .collect();
    match name_hits.as_slice() {
        [one] => Some(one),
        _ => None,
    }
}

/// True if `name` ends in `.m18` (case-insensitive), i.e. is MOL1802 source.
fn is_m18(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 4 && b[b.len() - 4..].eq_ignore_ascii_case(b".m18")
}

/// Replaces a `.m18` extension with `.asm` (case-insensitive match).
fn as_asm_name(name: &str) -> String {
    format!("{}.asm", &name[..name.len() - 4])
}

/// Transpiles every `.m18` file in `files` to `asm1802` source in place:
/// the file is renamed to `.asm`, its text becomes the transpiled output,
/// and its `INCLUDE "x.m18"` lines are rewritten to name `x.asm` (so they
/// resolve against the other transpiled files). This mirrors the mol1802
/// CLI's transpile-then-assemble pipeline. Returns the transpile diagnostics
/// (file name attached), leaving `files` partly transformed if any failed.
fn transpile_m18(files: &mut [SourceFile]) -> Vec<Diag> {
    let mut errors = Vec::new();
    for f in files.iter_mut() {
        if !is_m18(&f.name) {
            continue;
        }
        match mol1802::transpile(&f.text) {
            Ok(asm) => {
                let (asm, _targets) = mol1802::rewrite_m18_includes(&asm);
                f.text = asm;
                f.name = as_asm_name(&f.name);
            }
            Err(diags) => {
                for d in diags {
                    errors.push(Diag {
                        // transpile leaves file empty; name it here.
                        file: f.name.clone(),
                        line: d.line,
                        message: d.message,
                    });
                }
            }
        }
    }
    errors
}

/// Assemble uploaded source. `files_json` is a JSON array of
/// `{ "name": string, "text": string }`. Files named `*.m18` are MOL1802
/// source and are transpiled to `asm1802` first; `*.asm` (and `*.inc`) are
/// assembled directly. `INCLUDE` is resolved against the uploaded set,
/// matched on the base file name (case-insensitively), with a `.m18` include
/// resolving to its transpiled `.asm`.
///
/// The entry file is the one no other uploaded file includes. If that is
/// ambiguous (none or several), assembly is not attempted and the result's
/// `main` is null with a `message` explaining why, so the caller can ask the
/// user to choose; pass `main` to [`assemble_files_from`] to pick explicitly.
///
/// Returns JSON:
/// `{ ok, main, hex, listing, errors: [{file,line,message}], lo, hi, message }`.
#[wasm_bindgen]
pub fn assemble_files(files_json: &str) -> String {
    assemble_files_from(files_json, "")
}

/// Like [`assemble_files`], but assemble `main` (matched by base name) as the
/// entry file. An empty `main` auto-detects it. A `.m18` main name also
/// matches its transpiled `.asm`.
#[wasm_bindgen]
pub fn assemble_files_from(files_json: &str, main: &str) -> String {
    let mut files: Vec<SourceFile> = match serde_json::from_str(files_json) {
        Ok(f) => f,
        Err(e) => {
            return json!({ "ok": false, "message": format!("bad files JSON: {e}") }).to_string()
        }
    };
    if files.is_empty() {
        return json!({ "ok": false, "message": "no files were given" }).to_string();
    }

    // MOL1802 (.m18) source must be transpiled to asm1802 before assembly.
    // A .m18 main name refers to its transpiled .asm afterwards.
    let main = if is_m18(main) {
        as_asm_name(main)
    } else {
        main.to_string()
    };
    let main = main.as_str();
    let transpile_errors = transpile_m18(&mut files);
    if !transpile_errors.is_empty() {
        let errors: Vec<_> = transpile_errors
            .iter()
            .map(|d| json!({ "file": d.file, "line": d.line, "message": d.message }))
            .collect();
        return json!({
            "ok": false,
            "main": first_root_name(&files, main),
            "hex": "",
            "listing": "",
            "errors": errors,
            "lo": -1,
            "hi": -1,
            "message": "MOL1802 transpile failed",
        })
        .to_string();
    }

    // Which uploaded files are named by some file's INCLUDE line.
    let mut included = std::collections::HashSet::new();
    for f in &files {
        for line in f.text.lines() {
            if let Some(req) = asm1802::include_name(line) {
                if let Some(g) = find_include(&files, &req) {
                    included.insert(g.name.clone());
                }
            }
        }
    }

    let entry = if main.is_empty() {
        let roots: Vec<usize> = (0..files.len())
            .filter(|&i| !included.contains(&files[i].name))
            .collect();
        match roots.as_slice() {
            [one] => &files[*one],
            [] => {
                return json!({ "ok": false, "main": serde_json::Value::Null,
                    "message": "every file is included by another, so the main file is ambiguous; choose one" }).to_string();
            }
            many => {
                let names: Vec<&str> = many.iter().map(|&i| files[i].name.as_str()).collect();
                return json!({ "ok": false, "main": serde_json::Value::Null,
                    "message": format!("several files are not included by any other, so the main file is ambiguous ({}); choose one", names.join(", ")) }).to_string();
            }
        }
    } else {
        match find_include(&files, main) {
            Some(f) => f,
            None => return json!({ "ok": false, "message": format!("main file `{main}` was not uploaded") }).to_string(),
        }
    };

    let mut load = |req: &str, _from: &str| -> Result<(String, String), String> {
        match find_include(&files, req) {
            Some(f) => Ok((f.name.clone(), f.text.clone())),
            None => Err(format!("`{req}` was not among the uploaded files")),
        }
    };
    let asm = asm1802::assemble_with(&entry.name, &entry.text, &[], &mut load);

    let errors: Vec<_> = asm
        .errors
        .iter()
        .map(|d| json!({ "file": d.file, "line": d.line, "message": d.message }))
        .collect();
    let (lo, hi) = asm
        .range()
        .map_or((-1, -1), |(l, h)| (i32::from(l), i32::from(h)));
    json!({
        "ok": asm.ok(),
        "main": entry.name,
        "hex": asm.to_hex(),
        "listing": asm.listing_text(),
        "errors": errors,
        "lo": lo,
        "hi": hi,
    })
    .to_string()
}

#[derive(serde::Deserialize)]
struct SourceFile {
    name: String,
    text: String,
}

/// A diagnostic with its source file name, used while collecting transpile
/// errors (which `mol1802` reports without a file name).
struct Diag {
    file: String,
    line: usize,
    message: String,
}

/// Best guess at the entry file name for a result returned before assembly
/// (e.g. a transpile failure): the chosen `main` if given, else the sole
/// file no other includes, else null. Only for display in the error report.
fn first_root_name(files: &[SourceFile], main: &str) -> serde_json::Value {
    if !main.is_empty() {
        if let Some(f) = find_include(files, main) {
            return json!(f.name);
        }
    }
    let mut included = std::collections::HashSet::new();
    for f in files {
        for line in f.text.lines() {
            if let Some(req) = asm1802::include_name(line) {
                if let Some(g) = find_include(files, &req) {
                    included.insert(g.name.clone());
                }
            }
        }
    }
    let roots: Vec<&SourceFile> = files
        .iter()
        .filter(|f| !included.contains(&f.name))
        .collect();
    match roots.as_slice() {
        [one] => json!(one.name),
        _ => serde_json::Value::Null,
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(msg: &str);
}

/// Runs when the module is instantiated: route Rust panics to the browser console
/// so a crash in the worker is reported instead of failing silently.
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(target_arch = "wasm32")]
    std::panic::set_hook(Box::new(|info| {
        console_error(&format!("elf2 panic: {info}"))
    }));
}

/// Event codes in the top 8 bits of a packed event; the low 24 bits carry the value.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    PixieFrame = 1,
    TermChar = 2,
    TermFramingError = 3,
    HexDisplay = 4,
    Q = 5,
    Breakpoint = 6,
    Halted = 7,
    PrinterChar = 8,
}

/// Pack an event as `(kind << 24) | value`.
pub fn pack(e: &Event) -> u32 {
    let (k, v) = match *e {
        Event::PixieFrame => (EventKind::PixieFrame, 0),
        Event::TermChar(c) => (EventKind::TermChar, u32::from(c)),
        Event::TermFramingError => (EventKind::TermFramingError, 0),
        Event::HexDisplay(b) => (EventKind::HexDisplay, u32::from(b)),
        Event::Q(q) => (EventKind::Q, u32::from(q)),
        Event::Breakpoint(a) => (EventKind::Breakpoint, u32::from(a)),
        Event::Halted => (EventKind::Halted, 0),
        Event::PrinterChar(c) => (EventKind::PrinterChar, u32::from(c)),
    };
    ((k as u32) << 24) | v
}

fn state_name(s: CpuState) -> &'static str {
    match s {
        CpuState::Reset => "reset",
        CpuState::Load => "load",
        CpuState::Pause => "pause",
        CpuState::Running => "running",
        CpuState::Idle => "idle",
    }
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
pub struct Emulator {
    m: Machine,
    events: Vec<u32>,
    last_stop_breakpoint: Option<u16>,
}

#[wasm_bindgen]
impl Emulator {
    /// Create a machine from a JSON configuration (`"{}"` for the default Elf II).
    #[wasm_bindgen(constructor)]
    pub fn new(config_json: &str) -> Result<Emulator, JsError> {
        let cfg = Config::from_json(config_json).map_err(err)?;
        let m = Machine::new(cfg).map_err(err)?;
        Ok(Emulator {
            m,
            events: Vec::with_capacity(256),
            last_stop_breakpoint: None,
        })
    }

    /// The configuration actually in use, as JSON (includes ROM checksums).
    pub fn config_json(&self) -> String {
        self.m.config().to_json()
    }

    /// Machine cycles per second for the configured clock.
    pub fn cycles_per_second(&self) -> u32 {
        self.m.config().cycles_per_second()
    }

    // ---------------------------------------------------------------- ROMs, files

    pub fn load_rom(&mut self, name: &str, base: u16, bytes: &[u8]) -> Result<(), JsError> {
        self.m.load_rom(name, base, bytes).map_err(err)
    }

    /// ROMs listed in the configuration but not loaded: JSON `[{name, sha256, base}]`.
    pub fn missing_roms_json(&self) -> String {
        let list: Vec<_> = self
            .m
            .missing_roms()
            .iter()
            .map(|r| json!({ "name": r.name, "sha256": r.sha256, "base": r.base }))
            .collect();
        serde_json::Value::Array(list).to_string()
    }

    /// Load a binary at `base`; returns the byte count.
    pub fn load_bin(&mut self, base: u16, bytes: &[u8]) -> Result<u32, JsError> {
        self.m.load_bin(base, bytes).map(|n| n as u32).map_err(err)
    }

    /// Load Intel HEX; returns the lowest address loaded.
    pub fn load_hex(&mut self, text: &str) -> Result<u16, JsError> {
        self.m.load_hex(text).map(|(lo, _)| lo).map_err(err)
    }

    pub fn save_bin(&self, start: u16, end: u16) -> Vec<u8> {
        self.m.save_bin(start, end)
    }

    pub fn save_hex(&self, start: u16, end: u16) -> String {
        self.m.save_hex(start, end)
    }

    /// Key in `C0 F0 00` (LBR F000) at 0000 to start the Giant Board monitor.
    pub fn boot_f000(&mut self) -> Result<(), JsError> {
        self.m
            .load_bin(0, &[0xC0, 0xF0, 0x00])
            .map(|_| ())
            .map_err(err)
    }

    // ---------------------------------------------------------------- running

    /// Run about `cycles` machine cycles (or until a breakpoint). Returns the number
    /// of events produced; fetch them with `events()`.
    pub fn run(&mut self, cycles: u32) -> u32 {
        let r = self.m.run(u64::from(cycles));
        self.collect(r.stop);
        self.events.len() as u32
    }

    /// One CPU unit (instruction, DMA, interrupt or idle cycle). Returns its cycles.
    pub fn step(&mut self) -> u32 {
        let c = self.m.step();
        self.collect(StopReason::Budget);
        c
    }

    fn collect(&mut self, stop: StopReason) {
        self.events.clear();
        self.events.extend(self.m.events().iter().map(pack));
        self.last_stop_breakpoint = match stop {
            StopReason::Breakpoint(a) => Some(a),
            StopReason::Budget => None,
        };
    }

    /// Packed events from the last `run` or `step`.
    pub fn events(&self) -> Vec<u32> {
        self.events.clone()
    }

    /// Address of the breakpoint that stopped the last `run`, or -1.
    pub fn stopped_at(&self) -> i32 {
        self.last_stop_breakpoint.map_or(-1, i32::from)
    }

    pub fn set_breakpoint(&mut self, addr: u16, on: bool) {
        self.m.set_breakpoint(addr, on);
    }

    // ---------------------------------------------------------------- front panel

    /// Switch 0 = RUN, 1 = LOAD, 2 = MP.
    pub fn set_switch(&mut self, sw: u8, on: bool) {
        let s = match sw {
            0 => Switch::Run,
            1 => Switch::Load,
            2 => Switch::Mp,
            _ => return,
        };
        self.m.set_switch(s, on);
    }

    pub fn press_in(&mut self, down: bool) {
        self.m.press_in(down);
    }

    pub fn hex_key(&mut self, nibble: u8) {
        self.m.hex_key(nibble);
    }

    pub fn hex_display(&self) -> u8 {
        self.m.hex_display()
    }

    pub fn q(&self) -> bool {
        self.m.q()
    }

    /// EF lines 1-4 as bits 0-3.
    pub fn ef_bits(&self) -> u8 {
        (1..=4).fold(0, |acc, l| acc | (u8::from(self.m.ef(l)) << (l - 1)))
    }

    /// Force EF `line` (1-4): `state` 0 or 1 forces the level, -1 releases it.
    pub fn force_ef(&mut self, line: u8, state: i8) {
        let level = match state {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        };
        self.m.force_ef(line, level);
    }

    // ---------------------------------------------------------------- terminal

    pub fn term_key(&mut self, code: u8) {
        self.m.term_key(code);
    }

    pub fn term_paste(&mut self, text: &str) {
        self.m.term_paste(text);
    }

    pub fn term_pending(&self) -> u32 {
        self.m.term_pending() as u32
    }

    /// Clear the VID-1 screen (terminal-side; nothing is sent to the Elf II).
    pub fn vid1_clear(&mut self) {
        self.m.vid1_clear();
    }

    /// VID-1 screen as text lines joined with `\n`, trailing spaces removed.
    pub fn vid1_text(&self) -> String {
        self.m.vid1_text().join("\n")
    }

    // ---------------------------------------------------------------- parallel I/O

    /// A key pressed on the parallel keyboard (INP 5 / EF strobe). No-op if the
    /// parallel keyboard is not fitted. Independent of `term_key` (VID-1 serial).
    pub fn parallel_key(&mut self, code: u8) {
        self.m.parallel_key(code);
    }

    /// The printer paper as text lines joined with `\n`, or empty if no printer
    /// is fitted (trailing blanks on each line removed).
    pub fn printer_text(&self) -> String {
        self.m.printer_paper().unwrap_or_default().join("\n")
    }

    /// Print-head position as `line * 65536 + column`, or -1 if no printer.
    pub fn printer_head(&self) -> i64 {
        self.m
            .printer_head()
            .map_or(-1, |(l, c)| (l as i64) * 65536 + c as i64)
    }

    /// Tear off the printed sheet and start fresh. No-op if no printer is fitted.
    pub fn printer_tear_off(&mut self) {
        self.m.printer_tear_off();
    }

    // ---------------------------------------------------------------- video memory

    /// Pointer (in wasm memory) to the 1024-byte Pixie frame, or 0 if not fitted.
    pub fn pixie_frame_ptr(&self) -> u32 {
        self.m.pixie_frame().map_or(0, |f| f.as_ptr() as u32)
    }

    pub fn pixie_on(&self) -> bool {
        self.m.pixie_on()
    }

    /// Pointer to the VID-1 screen (16 x columns bytes), or 0 if not fitted.
    pub fn vid1_screen_ptr(&self) -> u32 {
        self.m.vid1_screen().map_or(0, |s| s.as_ptr() as u32)
    }

    /// VID-1 characters per line (0 if not fitted).
    pub fn vid1_columns(&self) -> u32 {
        self.m.vid1_columns().unwrap_or(0) as u32
    }

    /// VID-1 cursor as `row * 256 + column`, or -1 if not fitted.
    pub fn vid1_cursor(&self) -> i32 {
        self.m
            .vid1_cursor()
            .map_or(-1, |(r, c)| (r * 256 + c) as i32)
    }

    // ---------------------------------------------------------------- debugger

    /// Registers, flags and the instruction at the program counter, as JSON.
    pub fn state_json(&self) -> String {
        let c = self.m.cpu();
        let pc = c.r[c.p as usize];
        let ins = disasm(&self.m.peek(pc, 3), pc);
        let p = self.m.panel();
        json!({
            "r": c.r, "d": c.d, "df": c.df, "p": c.p, "x": c.x, "t": c.t,
            "i": c.i, "n": c.n, "ie": c.ie, "q": c.q,
            "state": state_name(c.state), "cycles": c.cycles,
            "pc": pc, "instruction": ins.to_string(),
            "ef": [self.m.ef(1), self.m.ef(2), self.m.ef(3), self.m.ef(4)],
            "panel": { "run": p.run, "load": p.load, "mp": p.mp, "in": p.in_held },
            "display": self.m.hex_display(),
            "keypad": self.m.keypad_byte(),
            "pixie_on": self.m.pixie_on(),
        })
        .to_string()
    }

    /// Disassemble `count` instructions from `addr`: JSON `[{addr, bytes, text}]`.
    pub fn disasm_json(&self, addr: u16, count: u32) -> String {
        let mut out = Vec::new();
        let mut a = addr;
        for _ in 0..count {
            let bytes = self.m.peek(a, 3);
            let ins = disasm(&bytes, a);
            out.push(json!({
                "addr": a,
                "bytes": &bytes[..ins.len as usize],
                "text": ins.to_string(),
            }));
            a = a.wrapping_add(u16::from(ins.len));
        }
        serde_json::Value::Array(out).to_string()
    }

    pub fn peek(&self, addr: u16, len: u32) -> Vec<u8> {
        self.m.peek(addr, len as usize)
    }

    /// Address has RAM or a ROM.
    pub fn is_mapped(&self, addr: u16) -> bool {
        self.m.is_mapped(addr)
    }

    /// Loaded ROMs as JSON `[{name, base, len}]`.
    pub fn roms_json(&self) -> String {
        let list: Vec<_> = self
            .m
            .roms()
            .iter()
            .map(|r| json!({ "name": r.name, "base": r.base, "len": r.len }))
            .collect();
        serde_json::Value::Array(list).to_string()
    }

    /// Debugger write (ignores memory protect). Returns bytes stored.
    pub fn poke(&mut self, addr: u16, bytes: &[u8]) -> u32 {
        self.m.poke(addr, bytes) as u32
    }

    /// Set register `reg` (0-15) to `value`.
    pub fn set_register(&mut self, reg: u8, value: u16) {
        if reg < 16 {
            self.m.cpu_mut().r[reg as usize] = value;
        }
    }

    /// Set a CPU field by name: "d", "df", "p", "x", "t", "ie" (the debugger's editor).
    /// Q is left out: it drives hardware and changes only through SEQ/REQ.
    pub fn set_cpu_field(&mut self, name: &str, value: u16) -> Result<(), JsError> {
        let c = self.m.cpu_mut();
        match name {
            "d" => c.d = value as u8,
            "df" => c.df = value != 0,
            "p" => c.p = (value & 0x0F) as u8,
            "x" => c.x = (value & 0x0F) as u8,
            "t" => c.t = value as u8,
            "ie" => c.ie = value != 0,
            _ => return Err(JsError::new(&format!("unknown CPU field '{name}'"))),
        }
        Ok(())
    }

    // ---------------------------------------------------------------- snapshots

    pub fn snapshot(&self) -> Vec<u8> {
        self.m.snapshot()
    }

    /// Restore a snapshot; on error the machine is unchanged.
    pub fn restore(&mut self, data: &[u8]) -> Result<(), JsError> {
        self.m.restore(data).map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_core() {
        assert_eq!(version(), elf2::cdp1802::VERSION);
    }

    #[test]
    fn assemble_auto_detects_main_and_resolves_include() {
        let files = r#"[
            {"name":"main.asm","text":"        INCLUDE \"bits.inc\"\n        ORG 0\n        LDI THREE\n"},
            {"name":"bits.inc","text":"THREE   EQU 3\n"}
        ]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], true, "errors: {}", r["errors"]);
        assert_eq!(r["main"], "main.asm");
        assert!(
            r["hex"].as_str().unwrap().contains("F803"),
            "hex: {}",
            r["hex"]
        ); // LDI 3
    }

    #[test]
    fn assemble_reports_missing_include() {
        let files = r#"[{"name":"a.asm","text":"        INCLUDE \"gone.inc\"\n"}]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], false);
        assert!(r["errors"].as_array().unwrap().iter().any(|e| e["message"]
            .as_str()
            .unwrap()
            .contains("not among the uploaded")));
    }

    #[test]
    fn include_resolves_by_subfolder_path_from_a_folder_pick() {
        // Files added from a folder keep their relative paths. An
        // `INCLUDE "include/bits.inc"` must find proj/asm/include/bits.inc.
        let files = r#"[
            {"name":"proj/asm/main.asm","text":"        ORG 0\n        INCLUDE \"include/bits.inc\"\n        LDI THREE\n"},
            {"name":"proj/asm/include/bits.inc","text":"THREE EQU 3\n"}
        ]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], true, "errors: {}", r["errors"]);
        assert_eq!(r["main"], "proj/asm/main.asm");
        assert!(r["hex"].as_str().unwrap().contains("F803"));
    }

    #[test]
    fn include_path_disambiguates_same_base_name_in_two_folders() {
        // Two scrt.asm in different folders. The subfolder path in each
        // INCLUDE selects the intended one even though the base names clash.
        // `main` is chosen explicitly so neither scrt is treated as a root.
        let files = r#"[
            {"name":"a/main.asm","text":"        ORG 0\n        INCLUDE \"lib/scrt.asm\"\n        INCLUDE \"../other/scrt.asm\"\n        LDI K\n        LDI J\n"},
            {"name":"a/lib/scrt.asm","text":"K EQU 7\n"},
            {"name":"other/scrt.asm","text":"J EQU 9\n"}
        ]"#;
        let r: serde_json::Value =
            serde_json::from_str(&assemble_files_from(files, "a/main.asm")).unwrap();
        assert_eq!(r["ok"], true, "errors: {}", r["errors"]);
        // K=7 from a/lib/scrt.asm (F8 07) and J=9 from other/scrt.asm (F8 09).
        let hex = r["hex"].as_str().unwrap();
        assert!(hex.contains("F807"), "hex: {hex}");
        assert!(hex.contains("F809"), "hex: {hex}");
    }

    #[test]
    fn include_falls_back_to_unique_base_name() {
        // Plain (path-less) picks: INCLUDE "bits.inc" finds bits.inc anywhere.
        let files = r#"[
            {"name":"main.asm","text":"        ORG 0\n        INCLUDE \"bits.inc\"\n        LDI THREE\n"},
            {"name":"bits.inc","text":"THREE EQU 3\n"}
        ]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], true, "errors: {}", r["errors"]);
    }

    #[test]
    fn assemble_transpiles_m18_before_assembling() {
        // A .m18 main (MOL1802 `source:dest` statements) that includes another
        // .m18. The pipeline must transpile both before assembling, and the
        // `INCLUDE "lib.m18"` must resolve to the transpiled lib.asm.
        let files = r#"[
            {"name":"main.m18","text":"        ORG 0\n        INCLUDE \"lib.m18\"\n        0x1234:R3\n        SEP R3\n"},
            {"name":"lib.m18","text":"HELPER: R3.0:D\n        SEP R5\n"}
        ]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], true, "errors: {}", r["errors"]);
        assert_eq!(r["main"], "main.asm");
        assert!(!r["hex"].as_str().unwrap().is_empty());
    }

    #[test]
    fn assemble_reports_m18_transpile_errors_with_file_name() {
        // `#FF:R3` (an 8-bit immediate into a full register) is a documented
        // MOL1802 error; it must be reported against the .m18 file.
        let files = r#"[{"name":"bad.m18","text":"        #FF:R3\n"}]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], false, "unexpectedly ok: {r}");
        assert_eq!(r["message"], "MOL1802 transpile failed");
        assert!(r["errors"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["file"] == "bad.m18"));
    }

    #[test]
    fn assemble_ambiguous_main_asks_to_choose() {
        let files = r#"[
            {"name":"one.asm","text":"        ORG 0\n        NOP\n"},
            {"name":"two.asm","text":"        ORG 0\n        NOP\n"}
        ]"#;
        let r: serde_json::Value = serde_json::from_str(&assemble_files(files)).unwrap();
        assert_eq!(r["ok"], false);
        assert!(r["main"].is_null());
        assert!(r["message"].as_str().unwrap().contains("ambiguous"));
        // Choosing one resolves it.
        let r: serde_json::Value =
            serde_json::from_str(&assemble_files_from(files, "two.asm")).unwrap();
        assert_eq!(r["ok"], true);
        assert_eq!(r["main"], "two.asm");
    }

    #[test]
    fn events_are_packed() {
        assert_eq!(pack(&Event::TermChar(b'A')), 0x0200_0041);
        assert_eq!(pack(&Event::Breakpoint(0xF029)), 0x0600_F029);
        assert_eq!(pack(&Event::Q(true)), 0x0500_0001);
        assert_eq!(pack(&Event::PixieFrame) >> 24, EventKind::PixieFrame as u32);
    }

    #[test]
    fn front_panel_and_state_json() {
        let mut e = Emulator::new("{}").unwrap();
        e.load_bin(0, &[0x7B, 0x00]).unwrap(); // SEQ, IDL
        e.set_switch(0, true);
        let n = e.run(100);
        assert!(n >= 2);
        let ev = e.events();
        assert!(ev.contains(&pack(&Event::Q(true))));
        assert!(ev.contains(&pack(&Event::Halted)));
        assert!(e.q());
        let s: serde_json::Value = serde_json::from_str(&e.state_json()).unwrap();
        assert_eq!(s["state"], "idle");
        assert_eq!(s["pc"], 2);
        assert_eq!(s["panel"]["run"], true);
    }

    #[test]
    fn breakpoints_and_disassembly() {
        let mut e = Emulator::new("{}").unwrap();
        e.load_bin(0, &[0xC4, 0x7B, 0x00]).unwrap();
        e.set_breakpoint(1, true);
        e.set_switch(0, true);
        e.run(1000);
        assert_eq!(e.stopped_at(), 1);
        let d: serde_json::Value = serde_json::from_str(&e.disasm_json(0, 3)).unwrap();
        assert_eq!(d[0]["text"], "NOP");
        assert_eq!(d[1]["text"], "SEQ");
        assert_eq!(d[2]["addr"], 2);
    }

    #[test]
    fn boot_f000_and_snapshot_round_trip() {
        let mut e = Emulator::new("{}").unwrap();
        e.load_rom("mon", 0xF000, &[0x7B, 0x00]).unwrap();
        e.boot_f000().unwrap();
        e.set_switch(0, true);
        e.run(100);
        assert!(e.q());
        let snap = e.snapshot();
        let mut f = Emulator::new("{}").unwrap();
        f.restore(&snap).unwrap();
        assert_eq!(f.state_json(), e.state_json());
        assert_eq!(f.missing_roms_json(), "[]");
    }

    #[test]
    fn video_pointers() {
        let e = Emulator::new("{}").unwrap();
        assert_ne!(e.pixie_frame_ptr(), 0);
        assert_ne!(e.vid1_screen_ptr(), 0);
        assert_eq!(e.vid1_columns(), 64);
        assert_eq!(e.vid1_cursor(), 0);
        let e = Emulator::new(r#"{"pixie": null, "serial": null}"#).unwrap();
        assert_eq!(e.pixie_frame_ptr(), 0);
        assert_eq!(e.vid1_cursor(), -1);
    }
}
