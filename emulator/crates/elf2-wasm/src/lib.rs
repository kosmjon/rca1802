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
