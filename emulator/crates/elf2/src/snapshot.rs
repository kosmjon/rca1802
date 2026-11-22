//! Program files (`.bin`, Intel HEX) and machine snapshots (requirement 10).
//!
//! Snapshot layout (all integers little-endian):
//!
//! | Offset | Size | Contents |
//! |---|---|---|
//! | 0 | 8 | magic `ELF2SNAP` |
//! | 8 | 4 | format version (`SNAPSHOT_VERSION`) |
//! | 12 | 4 | length N of the JSON state |
//! | 16 | N | JSON: configuration, ROM list, CPU, front panel, peripherals |
//! | 16+N | 65536 | memory image (RAM and ROM contents) |
//!
//! Not saved: breakpoints (a debugger setting), and serial characters in flight or
//! still queued (the terminal is a separate device; anything mid-transmission is
//! lost, as when a real cable is unplugged).

use crate::error::MachineError;
use crate::hexfile;
use crate::machine::Machine;
use crate::memory::sha256_hex;
use crate::panel::FrontPanel;
use crate::Config;
use cdp1802::{CpuInternals, CpuState};
use serde::{Deserialize, Serialize};

pub const SNAPSHOT_MAGIC: &[u8; 8] = b"ELF2SNAP";
pub const SNAPSHOT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct RomRecord {
    name: String,
    base: u16,
    len: usize,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
struct CpuRecord {
    r: [u16; 16],
    d: u8,
    df: bool,
    p: u8,
    x: u8,
    t: u8,
    i: u8,
    n: u8,
    ie: bool,
    q: bool,
    state: u8,
    cycles: u64,
    init_pending: bool,
    paused_from: Option<u8>,
}

#[derive(Serialize, Deserialize)]
struct PanelRecord {
    run: bool,
    load: bool,
    mp: bool,
    dma_in_pending: bool,
}

#[derive(Serialize, Deserialize)]
struct PixieRecord {
    on: bool,
    cycle: u32,
    frame: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct Vid1Record {
    cells: Vec<u8>,
    row: usize,
    col: usize,
}

#[derive(Serialize, Deserialize)]
struct ParallelRecord {
    input: u8,
    input_ready: bool,
    output: u8,
    output_ready: bool,
}

#[derive(Serialize, Deserialize)]
struct State {
    config: Config,
    roms: Vec<RomRecord>,
    cpu: CpuRecord,
    panel: PanelRecord,
    q: bool,
    forced_ef: [Option<bool>; 4],
    boot_remaining: u8,
    keypad: u8,
    display: u8,
    cycle: u64,
    pixie: Option<PixieRecord>,
    vid1: Option<Vid1Record>,
    parallel: Option<ParallelRecord>,
}

fn state_to_u8(s: CpuState) -> u8 {
    match s {
        CpuState::Reset => 0,
        CpuState::Load => 1,
        CpuState::Pause => 2,
        CpuState::Running => 3,
        CpuState::Idle => 4,
    }
}

fn u8_to_state(v: u8) -> Result<CpuState, MachineError> {
    Ok(match v {
        0 => CpuState::Reset,
        1 => CpuState::Load,
        2 => CpuState::Pause,
        3 => CpuState::Running,
        4 => CpuState::Idle,
        _ => return Err(MachineError::Snapshot(format!("unknown CPU state {v}"))),
    })
}

impl Machine {
    // ------------------------------------------------------------- program files

    fn check_range(&self, base: u16, len: usize) -> Result<(), MachineError> {
        if usize::from(base) + len > 0x10000 {
            return Err(MachineError::TooLarge { base, len });
        }
        for k in 0..len {
            let a = base + k as u16;
            if !self.sys.mem.is_mapped(a) {
                return Err(MachineError::OutsideMemory {
                    addr: a,
                    ram_size: self.sys.config.ram_size,
                });
            }
        }
        Ok(())
    }

    /// Load a binary image at `base`. Memory protect does not apply (this is the
    /// equivalent of entering it from the keypad). Nothing is written unless every
    /// byte lands in RAM or ROM.
    pub fn load_bin(&mut self, base: u16, bytes: &[u8]) -> Result<usize, MachineError> {
        self.check_range(base, bytes.len())?;
        Ok(self.sys.mem.poke(base, bytes))
    }

    /// Load an Intel HEX file. Returns (lowest address, byte count). Nothing is written
    /// if any record is invalid or lands outside RAM/ROM.
    pub fn load_hex(&mut self, text: &str) -> Result<(u16, usize), MachineError> {
        let chunks = hexfile::parse(text).map_err(MachineError::Hex)?;
        for c in &chunks {
            self.check_range(c.addr, c.data.len())?;
        }
        let mut low = u16::MAX;
        let mut n = 0;
        for c in &chunks {
            low = low.min(c.addr);
            n += self.sys.mem.poke(c.addr, &c.data);
        }
        Ok((if n == 0 { 0 } else { low }, n))
    }

    /// Bytes from `start` to `end` inclusive.
    pub fn save_bin(&self, start: u16, end: u16) -> Vec<u8> {
        if end < start {
            return Vec::new();
        }
        self.sys.mem.peek(start, usize::from(end - start) + 1)
    }

    /// Intel HEX for `start` to `end` inclusive.
    pub fn save_hex(&self, start: u16, end: u16) -> String {
        hexfile::format(start, &self.save_bin(start, end))
    }

    // ------------------------------------------------------------- snapshots

    /// Save the complete machine state.
    pub fn snapshot(&self) -> Vec<u8> {
        let cpu = &self.cpu;
        let int = cpu.internals();
        let p = &self.sys.panel;
        let state = State {
            config: self.sys.config.clone(),
            roms: self
                .sys
                .mem
                .roms()
                .iter()
                .map(|r| RomRecord {
                    name: r.name.clone(),
                    base: r.base,
                    len: r.len,
                    sha256: r.sha256.clone(),
                })
                .collect(),
            cpu: CpuRecord {
                r: cpu.r,
                d: cpu.d,
                df: cpu.df,
                p: cpu.p,
                x: cpu.x,
                t: cpu.t,
                i: cpu.i,
                n: cpu.n,
                ie: cpu.ie,
                q: cpu.q,
                state: state_to_u8(cpu.state),
                cycles: cpu.cycles,
                init_pending: int.init_pending,
                paused_from: int.paused_from.map(state_to_u8),
            },
            panel: PanelRecord {
                run: p.run,
                load: p.load,
                mp: p.mp,
                dma_in_pending: p.dma_in_pending,
            },
            q: self.sys.q,
            forced_ef: self.sys.forced_ef,
            boot_remaining: self.sys.boot_remaining,
            keypad: self.sys.keypad.latch,
            display: self.sys.display.value,
            cycle: self.sys.cycle,
            pixie: self.sys.pixie.as_ref().map(|x| PixieRecord {
                on: x.on,
                cycle: x.cycle,
                frame: x.frame.to_vec(),
            }),
            vid1: self.sys.vid1.as_ref().map(|v| Vid1Record {
                cells: v.cells.clone(),
                row: v.row,
                col: v.col,
            }),
            parallel: self.sys.parallel.as_ref().map(|x| ParallelRecord {
                input: x.input,
                input_ready: x.input_ready,
                output: x.output,
                output_ready: x.output_ready,
            }),
        };
        let json = serde_json::to_vec(&state).expect("state serialises");
        let mut out = Vec::with_capacity(16 + json.len() + 0x10000);
        out.extend_from_slice(SNAPSHOT_MAGIC);
        out.extend_from_slice(&SNAPSHOT_VERSION.to_le_bytes());
        out.extend_from_slice(&(json.len() as u32).to_le_bytes());
        out.extend_from_slice(&json);
        out.extend_from_slice(self.sys.mem.raw());
        out
    }

    /// Rebuild a machine from a snapshot.
    pub fn from_snapshot(data: &[u8]) -> Result<Machine, MachineError> {
        let bad = |m: &str| MachineError::Snapshot(m.to_string());
        if data.len() < 16 || &data[..8] != SNAPSHOT_MAGIC {
            return Err(bad("not an Elf II snapshot"));
        }
        let version = u32::from_le_bytes(data[8..12].try_into().unwrap());
        if version != SNAPSHOT_VERSION {
            return Err(MachineError::Snapshot(format!(
                "format version {version}; this emulator reads version {SNAPSHOT_VERSION}"
            )));
        }
        let n = u32::from_le_bytes(data[12..16].try_into().unwrap()) as usize;
        if data.len() != 16 + n + 0x10000 {
            return Err(bad("truncated or wrong size"));
        }
        let state: State = serde_json::from_slice(&data[16..16 + n])
            .map_err(|e| MachineError::Snapshot(format!("state: {e}")))?;
        let image = &data[16 + n..];

        // Fresh machine for the configuration, minus its ROM list: ROMs are placed
        // from the image below and re-added with their checksums.
        let mut config = state.config;
        config.roms.clear();
        let mut m = Machine::new(config.clone()).map_err(|e| bad(&e.to_string()))?;
        for r in &state.roms {
            let end = usize::from(r.base) + r.len;
            if end > image.len() {
                return Err(bad("ROM outside the image"));
            }
            let bytes = &image[usize::from(r.base)..end];
            if sha256_hex(bytes) != r.sha256 {
                return Err(MachineError::Snapshot(format!(
                    "ROM '{}' does not match its checksum",
                    r.name
                )));
            }
            m.load_rom(&r.name, r.base, bytes)?;
        }
        // RAM contents, skipping addresses covered by a ROM.
        let ram = config.ram_size as usize;
        let rom_at = |a: usize| {
            state
                .roms
                .iter()
                .any(|r| (usize::from(r.base)..usize::from(r.base) + r.len).contains(&a))
        };
        for (a, &b) in image.iter().enumerate().take(ram) {
            if !rom_at(a) {
                m.sys.mem.poke(a as u16, &[b]);
            }
        }

        let c = &state.cpu;
        let cpu = &mut m.cpu;
        cpu.r = c.r;
        cpu.d = c.d;
        cpu.df = c.df;
        cpu.p = c.p;
        cpu.x = c.x;
        cpu.t = c.t;
        cpu.i = c.i;
        cpu.n = c.n;
        cpu.ie = c.ie;
        cpu.q = c.q;
        cpu.state = u8_to_state(c.state)?;
        cpu.cycles = c.cycles;
        cpu.set_internals(CpuInternals {
            init_pending: c.init_pending,
            paused_from: c.paused_from.map(u8_to_state).transpose()?,
        });

        let s = &mut m.sys;
        s.panel = FrontPanel {
            run: state.panel.run,
            load: state.panel.load,
            mp: state.panel.mp,
            in_held: false,
            dma_in_pending: state.panel.dma_in_pending,
        };
        s.mem.protect = state.panel.mp;
        s.q = state.q;
        s.forced_ef = state.forced_ef;
        s.boot_remaining = state.boot_remaining;
        s.keypad.latch = state.keypad;
        s.display.value = state.display;
        s.cycle = state.cycle;
        if let (Some(p), Some(r)) = (&mut s.pixie, &state.pixie) {
            if r.frame.len() != p.frame.len() {
                return Err(bad("Pixie frame has the wrong size"));
            }
            p.on = r.on;
            p.cycle = r.cycle % crate::pixie::FRAME_CYCLES;
            p.frame.copy_from_slice(&r.frame);
        }
        if let (Some(v), Some(r)) = (&mut s.vid1, &state.vid1) {
            if r.cells.len() != v.cells.len() || r.row >= crate::vid1::ROWS || r.col >= v.columns()
            {
                return Err(bad("VID-1 screen does not match the configuration"));
            }
            v.cells.copy_from_slice(&r.cells);
            v.row = r.row;
            v.col = r.col;
        }
        if let (Some(p), Some(r)) = (&mut s.parallel, &state.parallel) {
            p.input = r.input;
            p.input_ready = r.input_ready;
            p.output = r.output;
            p.output_ready = r.output_ready;
        }
        Ok(m)
    }

    /// Replace this machine's state with a snapshot. On error the machine is unchanged.
    pub fn restore(&mut self, data: &[u8]) -> Result<(), MachineError> {
        *self = Machine::from_snapshot(data)?;
        Ok(())
    }
}
