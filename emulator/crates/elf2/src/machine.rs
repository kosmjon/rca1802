//! The Elf II machine: CPU plus the system it is plugged into.
//!
//! `Machine` owns the CPU and a `System`; `System` implements `cdp1802::Bus` and
//! routes every CPU operation to memory and peripherals (requirement 3.9).
//! Peripherals are added in tasks 6-10; until then ports read the floating-bus
//! value and outputs are ignored.

use crate::config::{Config, RomSlot};
use crate::error::{ConfigError, MachineError};
use crate::event::{Event, RunResult, StopReason};
use crate::keypad::{HexDisplay, HexKeypad};
use crate::memory::{sha256_hex, MemoryMap};
use crate::panel::{FrontPanel, Switch};
use crate::parallel::{Cassette, Cdp1852Ports};
use crate::pixie::{Cdp1861, FRAME_BYTES};
use crate::serial::{BitBangSerial, TxOutput};
use crate::vid1::Vid1Terminal;
use cdp1802::{Bus, Cpu, CpuState, Signals};

/// Bytes fed to the first three reads after reset when the Giant Board start-up
/// jumper is set: `LBR F000`.
const BOOT_F000: [u8; 3] = [0xC0, 0xF0, 0x00];

/// Everything except the CPU.
pub struct System {
    pub(crate) config: Config,
    pub(crate) mem: MemoryMap,
    pub(crate) panel: FrontPanel,
    pub(crate) events: Vec<Event>,
    pub(crate) q: bool,
    pub(crate) q_changed: bool,
    /// Debug overrides for EF1-EF4 (`None` = driven by hardware).
    pub(crate) forced_ef: [Option<bool>; 4],
    /// Reads of 0000-0002 still to be replaced by `BOOT_F000`.
    pub(crate) boot_remaining: u8,
    pub(crate) keypad: HexKeypad,
    pub(crate) display: HexDisplay,
    /// The next write is the memory cycle of a Load-mode DMA-IN.
    pub(crate) dma_in_write: bool,
    /// CDP1861, if fitted.
    pub(crate) pixie: Option<Cdp1861>,
    /// `true`: INP 1 turns the display on, OUT 1 off.
    pub(crate) pixie_on_is_inp: bool,
    /// Giant Board serial port, if fitted.
    pub(crate) serial: Option<BitBangSerial>,
    /// VID-1 terminal on the other end of the serial cable (present with the serial port).
    pub(crate) vid1: Option<Vid1Terminal>,
    /// Giant Board CDP1852 ports, if fitted.
    pub(crate) parallel: Option<Cdp1852Ports>,
    /// Cassette interface slot, if fitted (v1 stub).
    pub(crate) cassette: Option<Cassette>,
    /// Parallel ASCII keyboard (INP 5), if fitted.
    pub(crate) pkeyboard: Option<crate::pkeyboard::ParallelKeyboard>,
    /// Parallel dot-matrix printer (OUT 3), if fitted.
    pub(crate) printer: Option<crate::printer::Printer>,
    /// EF line (1-4) the parallel keyboard strobes, when fitted.
    pub(crate) pkeyboard_ef: u8,
    /// EF line (1-4) the printer "ready" drives, when fitted.
    pub(crate) printer_ef: u8,
    /// Machine cycles elapsed (same count as `Cpu::cycles`, kept here for peripherals).
    pub(crate) cycle: u64,
}

impl System {
    fn show(&mut self, v: u8) {
        if self.display.latch(v) {
            self.events.push(Event::HexDisplay(v));
        }
    }

    fn pixie_switch(&mut self, inp: bool) {
        let on = inp == self.pixie_on_is_inp;
        if let Some(p) = &mut self.pixie {
            p.set_on(on);
        }
    }

    /// /CLEAR: reset peripherals that are wired to it.
    fn reset_peripherals(&mut self) {
        if let Some(p) = &mut self.pixie {
            p.reset();
        }
        if let Some(p) = &mut self.parallel {
            p.reset();
        }
    }
}

impl Bus for System {
    #[inline]
    fn read(&mut self, addr: u16) -> u8 {
        if self.boot_remaining > 0 {
            if addr < 3 {
                self.boot_remaining -= 1;
                return BOOT_F000[addr as usize];
            }
            self.boot_remaining = 0;
        }
        self.mem.read(addr)
    }

    #[inline]
    fn write(&mut self, addr: u16, val: u8) {
        if self.dma_in_write {
            // Load mode: the display shows the byte entered, or with MP on the byte
            // already in memory (the write is blocked). [verify] MP display behaviour.
            self.dma_in_write = false;
            let shown = if self.mem.protect {
                self.mem.read(addr)
            } else {
                val
            };
            self.show(shown);
        }
        self.mem.write(addr, val);
    }

    fn out(&mut self, port: u8, val: u8) {
        match port {
            1 => self.pixie_switch(false),
            3 => {
                if let Some(p) = &mut self.printer {
                    // EF is sampled during execute; the printer accepts on this cycle.
                    p.write(val, self.cycle + 1);
                    self.events.push(Event::PrinterChar(val));
                }
            }
            4 => self.show(val),
            7 => {
                if let Some(p) = &mut self.parallel {
                    p.cpu_write(val);
                }
            }
            _ => {}
        }
    }

    fn inp(&mut self, port: u8) -> u8 {
        match port {
            1 => {
                // The 1861 does not drive the data bus.
                self.pixie_switch(true);
                self.config.unmapped_read
            }
            4 => self.keypad.byte(),
            5 => match &mut self.pkeyboard {
                Some(k) => k.read(),
                None => self.config.unmapped_read,
            },
            7 => match &mut self.parallel {
                Some(p) => p.cpu_read(),
                None => self.config.unmapped_read,
            },
            _ => self.config.unmapped_read,
        }
    }

    fn ef(&self, line: u8) -> bool {
        if let Some(v) = self.forced_ef[(line - 1) as usize] {
            return v;
        }
        let mut level = match line {
            1 => self.pixie.as_ref().is_some_and(|p| p.ef1()),
            2 => self
                .cassette
                .as_ref()
                .is_some_and(|c| c.ef2(self.cycle + 1)),
            // IN button, only in Run mode.
            4 => self.panel.in_held && self.panel.run && !self.panel.load,
            _ => false,
        };
        // Serial RX shares EF4 with the IN button when the jumper selects it: the
        // line is the OR of the two (requirement 5.2).
        if let Some(s) = &self.serial {
            if s.config().rx_ef == line {
                // EF is sampled during the instruction's execute cycle.
                level |= s.rx_ef(self.cycle + 1);
            }
        }
        // Parallel keyboard strobe: high while a key byte is waiting to be read.
        if let Some(k) = &self.pkeyboard {
            if self.pkeyboard_ef == line {
                level |= k.ready();
            }
        }
        // Printer "ready": high when the printer can accept the next byte.
        if let Some(p) = &self.printer {
            if self.printer_ef == line {
                level |= p.is_ready(self.cycle + 1);
            }
        }
        level
    }

    fn set_q(&mut self, q: bool) {
        if q != self.q {
            self.q = q;
            self.q_changed = true;
            if let Some(s) = &mut self.serial {
                // Q changes in the instruction's second cycle.
                s.q_changed(self.cycle + 2, q);
            }
            if let Some(c) = &mut self.cassette {
                c.q_changed(self.cycle + 2, q);
            }
        }
    }

    fn dma_in(&mut self) -> u8 {
        self.panel.dma_in_pending = false;
        // Only the front panel raises DMA-IN in this machine.
        self.dma_in_write = true;
        self.keypad.byte()
    }

    fn dma_out(&mut self, val: u8) {
        if let Some(p) = &mut self.pixie {
            p.dma_out(val);
        }
    }

    fn signals(&self) -> Signals {
        let (dma_out, mut interrupt) = match &self.pixie {
            Some(p) => (p.dma_request(), p.interrupt()),
            None => (false, false),
        };
        if self.config.parallel_sr_to_int {
            interrupt |= self.parallel.as_ref().is_some_and(|p| p.input_ready());
        }
        Signals {
            dma_in: self.panel.dma_in_pending,
            dma_out,
            interrupt,
        }
    }

    fn tick(&mut self, cycles: u32) {
        self.cycle += u64::from(cycles);
        if let Some(p) = &mut self.pixie {
            p.tick(cycles);
            if p.frame_ready {
                p.frame_ready = false;
                self.events.push(Event::PixieFrame);
            }
        }
        if let Some(s) = &mut self.serial {
            s.tick(self.cycle);
            for out in s.take_tx() {
                self.events.push(match out {
                    TxOutput::Char(c) => {
                        if let Some(v) = &mut self.vid1 {
                            v.put(c);
                        }
                        Event::TermChar(c)
                    }
                    TxOutput::FramingError => Event::TermFramingError,
                });
            }
        }
    }

    fn interrupt_ack(&mut self) {
        if let Some(p) = &mut self.pixie {
            p.interrupt_ack();
        }
    }
}

pub struct Machine {
    pub(crate) cpu: Cpu,
    pub(crate) sys: System,
    pub(crate) breakpoints: Vec<bool>,
    pub(crate) breakpoint_count: usize,
    /// Set after stopping at a breakpoint so the next run executes that instruction.
    pub(crate) resume_past_breakpoint: bool,
}

impl Machine {
    /// A powered-on machine with all switches off (CPU held in reset).
    pub fn new(config: Config) -> Result<Machine, ConfigError> {
        config.validate()?;
        let mem = MemoryMap::new(config.ram_size, config.unmapped_read);
        let config_pixie = config.pixie;
        let config_serial = config.serial;
        let config_vid1 = config.vid1.clone();
        let has_parallel = config.parallel_1852;
        let has_cassette = config.cassette;
        let config_pkeyboard = config.parallel_keyboard;
        let config_printer = config.printer;
        let cycles_per_second = config.cycles_per_second();
        let mut sys = System {
            config,
            mem,
            panel: FrontPanel::default(),
            events: Vec::with_capacity(64),
            q: false,
            q_changed: false,
            forced_ef: [None; 4],
            boot_remaining: 0,
            keypad: HexKeypad::default(),
            display: HexDisplay::default(),
            dma_in_write: false,
            pixie: config_pixie.map(|_| Cdp1861::new()),
            pixie_on_is_inp: config_pixie.is_none_or(|p| p.on_is_inp),
            serial: config_serial.map(|c| BitBangSerial::new(c, cycles_per_second)),
            vid1: config_serial.map(|_| Vid1Terminal::new(&config_vid1)),
            parallel: has_parallel.then(Cdp1852Ports::default),
            cassette: has_cassette.then(Cassette::default),
            pkeyboard: config_pkeyboard.map(|_| crate::pkeyboard::ParallelKeyboard::new()),
            printer: config_printer.map(|c| crate::printer::Printer::new(&c, cycles_per_second)),
            pkeyboard_ef: config_pkeyboard.map_or(0, |c| c.ef),
            printer_ef: config_printer.map_or(0, |c| c.ef),
            cycle: 0,
        };
        let mut cpu = Cpu::new();
        cpu.reset(&mut sys);
        sys.q_changed = false;
        Ok(Machine {
            cpu,
            sys,
            breakpoints: vec![false; 0x10000],
            breakpoint_count: 0,
            resume_past_breakpoint: false,
        })
    }

    pub fn config(&self) -> &Config {
        &self.sys.config
    }

    pub fn cpu(&self) -> &Cpu {
        &self.cpu
    }

    /// Mutable CPU access for the debugger (register edits).
    pub fn cpu_mut(&mut self) -> &mut Cpu {
        &mut self.cpu
    }

    pub fn panel(&self) -> &FrontPanel {
        &self.sys.panel
    }

    pub fn q(&self) -> bool {
        self.sys.q
    }

    /// Current level of EF `line` (1-4) as the CPU sees it.
    pub fn ef(&self, line: u8) -> bool {
        self.sys.ef(line)
    }

    // ------------------------------------------------------------- ROMs and memory

    /// Load a ROM image. If the configuration lists a ROM with this name, the image
    /// must match its SHA-256 and is placed at its base address; otherwise it is
    /// placed at `base` and added to the configuration.
    pub fn load_rom(&mut self, name: &str, base: u16, bytes: &[u8]) -> Result<(), MachineError> {
        let actual = sha256_hex(bytes);
        let slot = self
            .sys
            .config
            .roms
            .iter()
            .find(|r| r.name == name)
            .cloned();
        let base = match &slot {
            Some(s) if !s.sha256.eq_ignore_ascii_case(&actual) => {
                return Err(MachineError::RomChecksum {
                    name: name.to_string(),
                    expected: s.sha256.to_ascii_lowercase(),
                    actual,
                })
            }
            Some(s) => s.base,
            None => base,
        };
        self.sys.mem.load_rom(name, base, bytes)?;
        if slot.is_none() {
            self.sys.config.roms.push(RomSlot {
                name: name.to_string(),
                sha256: actual,
                base,
            });
        }
        Ok(())
    }

    /// ROMs currently loaded.
    pub fn roms(&self) -> &[crate::memory::LoadedRom] {
        self.sys.mem.roms()
    }

    /// ROMs named in the configuration that have not been loaded.
    pub fn missing_roms(&self) -> Vec<&RomSlot> {
        let loaded = self.sys.mem.roms();
        self.sys
            .config
            .roms
            .iter()
            .filter(|s| !loaded.iter().any(|r| r.name == s.name))
            .collect()
    }

    /// Debugger/file write: ignores memory protect; unmapped addresses are skipped.
    pub fn poke(&mut self, addr: u16, bytes: &[u8]) -> usize {
        self.sys.mem.poke(addr, bytes)
    }

    pub fn peek(&self, addr: u16, len: usize) -> Vec<u8> {
        self.sys.mem.peek(addr, len)
    }

    /// Address has RAM or a ROM (otherwise it reads the floating-bus value).
    pub fn is_mapped(&self, addr: u16) -> bool {
        self.sys.mem.is_mapped(addr)
    }

    // ------------------------------------------------------------- front panel

    pub fn set_switch(&mut self, sw: Switch, on: bool) {
        let p = &mut self.sys.panel;
        match sw {
            Switch::Run => p.run = on,
            Switch::Load => p.load = on,
            Switch::Mp => p.mp = on,
        }
        self.sys.mem.protect = self.sys.panel.mp;
        self.apply_control_lines();
    }

    fn apply_control_lines(&mut self) {
        let before = self.cpu.state;
        let (clear_n, wait_n) = self.sys.panel.control_lines();
        self.cpu.set_control(clear_n, wait_n, &mut self.sys);
        let after = self.cpu.state;
        if matches!(after, CpuState::Reset | CpuState::Load) && before != after {
            self.sys.reset_peripherals();
        }
        if after == CpuState::Load && before != CpuState::Load {
            self.sys.panel.dma_in_pending = false;
        }
        let left_reset = matches!(before, CpuState::Reset | CpuState::Load)
            && matches!(after, CpuState::Running | CpuState::Pause);
        if left_reset && self.sys.config.start_at_f000 {
            self.sys.boot_remaining = BOOT_F000.len() as u8;
        }
    }

    /// IN button pressed (`down`) or released. In Load mode a press requests one
    /// DMA-IN cycle; in Run mode the button drives EF4 while held.
    pub fn press_in(&mut self, down: bool) {
        let was = self.sys.panel.in_held;
        self.sys.panel.in_held = down;
        if down && !was && self.cpu.state == CpuState::Load {
            self.sys.panel.dma_in_pending = true;
        }
    }

    /// Last CDP1861 frame (128 lines x 8 bytes, MSB = leftmost pixel), if fitted.
    pub fn pixie_frame(&self) -> Option<&[u8; FRAME_BYTES]> {
        self.sys.pixie.as_ref().map(|p| p.frame())
    }

    /// Whether the CDP1861 display is switched on.
    pub fn pixie_on(&self) -> bool {
        self.sys.pixie.as_ref().is_some_and(|p| p.is_on())
    }

    /// A key typed at the VID-1 keyboard: queued for sending on the serial RX line.
    pub fn term_key(&mut self, ascii: u8) {
        if let Some(s) = &mut self.sys.serial {
            s.send(ascii & 0x7F, 0);
        }
    }

    /// Text pasted into the terminal, sent with the configured pacing.
    pub fn term_paste(&mut self, text: &str) {
        if let Some(s) = &mut self.sys.serial {
            s.paste(text);
        }
    }

    /// The VID-1 screen: 16 lines of `vid1_columns()` characters, row by row.
    pub fn vid1_screen(&self) -> Option<&[u8]> {
        self.sys.vid1.as_ref().map(|v| v.screen())
    }

    /// VID-1 cursor as (row, column).
    pub fn vid1_cursor(&self) -> Option<(usize, usize)> {
        self.sys.vid1.as_ref().map(|v| v.cursor())
    }

    /// VID-1 characters per line.
    pub fn vid1_columns(&self) -> Option<usize> {
        self.sys.vid1.as_ref().map(|v| v.columns())
    }

    /// The VID-1 screen as text lines with trailing spaces removed.
    pub fn vid1_text(&self) -> Vec<String> {
        self.sys.vid1.as_ref().map_or_else(Vec::new, |v| v.text())
    }

    /// Giant Board parallel input: a device strobes in a byte. Returns false if the
    /// ports are not fitted.
    pub fn parallel_input(&mut self, byte: u8) -> bool {
        match &mut self.sys.parallel {
            Some(p) => {
                p.strobe_input(byte);
                true
            }
            None => false,
        }
    }

    /// Giant Board parallel output: the device takes the byte written by OUT 7, if any.
    pub fn parallel_output(&mut self) -> Option<u8> {
        self.sys.parallel.as_mut().and_then(|p| p.take_output())
    }

    /// Cassette slot (v1 stub), if fitted.
    pub fn cassette(&self) -> Option<&Cassette> {
        self.sys.cassette.as_ref()
    }

    /// A key pressed on the parallel keyboard (INP 5): strobes the byte in and
    /// raises its EF line. No-op if the parallel keyboard is not fitted. This is
    /// separate from `term_key` (the VID-1 serial keyboard); the front end may
    /// feed the same key to both.
    pub fn parallel_key(&mut self, ascii: u8) {
        if let Some(k) = &mut self.sys.pkeyboard {
            k.press(ascii);
        }
    }

    /// The printer paper so far: one string per line, trailing blanks trimmed.
    /// `None` if no printer is fitted.
    pub fn printer_paper(&self) -> Option<Vec<String>> {
        self.sys.printer.as_ref().map(|p| p.paper())
    }

    /// Print-head position as (line, column); `None` if no printer is fitted.
    pub fn printer_head(&self) -> Option<(usize, usize)> {
        self.sys.printer.as_ref().map(|p| p.head())
    }

    /// Tear off the printed sheet and start fresh. No-op if no printer is fitted.
    pub fn printer_tear_off(&mut self) {
        if let Some(p) = &mut self.sys.printer {
            p.tear_off();
        }
    }

    /// Clear the VID-1 screen and home its cursor (the terminal's own clear, as if FF
    /// had been received; nothing is sent to the Elf II).
    pub fn vid1_clear(&mut self) {
        if let Some(v) = &mut self.sys.vid1 {
            v.clear();
        }
    }

    /// Characters still waiting to be sent to the Elf II.
    pub fn term_pending(&self) -> usize {
        self.sys.serial.as_ref().map_or(0, |s| s.rx_pending())
    }

    /// Hex key 0-F pressed.
    pub fn hex_key(&mut self, nibble: u8) {
        self.sys.keypad.press(nibble);
    }

    /// Byte currently latched by the hex keypad.
    pub fn keypad_byte(&self) -> u8 {
        self.sys.keypad.byte()
    }

    /// Value shown on the two-digit hex display.
    pub fn hex_display(&self) -> u8 {
        self.sys.display.value()
    }

    /// Force EF `line` (1-4) to a level, or release it with `None`.
    pub fn force_ef(&mut self, line: u8, level: Option<bool>) {
        if (1..=4).contains(&line) {
            self.sys.forced_ef[(line - 1) as usize] = level;
        }
    }

    // ------------------------------------------------------------- execution

    pub fn set_breakpoint(&mut self, addr: u16, on: bool) {
        let b = &mut self.breakpoints[addr as usize];
        if *b != on {
            *b = on;
            if on {
                self.breakpoint_count += 1;
            } else {
                self.breakpoint_count -= 1;
            }
        }
    }

    /// Events produced by the last `run` or `step`.
    pub fn events(&self) -> &[Event] {
        &self.sys.events
    }

    /// Run about `budget` machine cycles (an instruction is never split), or until a
    /// breakpoint. Events from this run are available from `events()`.
    pub fn run(&mut self, budget: u64) -> RunResult {
        self.sys.events.clear();
        let start = self.cpu.cycles;
        let mut stop = StopReason::Budget;
        while self.cpu.cycles - start < budget {
            if self.breakpoint_count > 0 && self.cpu.state == CpuState::Running {
                let pc = self.cpu.r[self.cpu.p as usize];
                if self.breakpoints[pc as usize] && !self.resume_past_breakpoint {
                    self.resume_past_breakpoint = true;
                    self.sys.events.push(Event::Breakpoint(pc));
                    stop = StopReason::Breakpoint(pc);
                    break;
                }
            }
            self.step_unit();
        }
        self.flush_q();
        RunResult {
            cycles: self.cpu.cycles - start,
            stop,
        }
    }

    /// One CPU unit (instruction, DMA, interrupt or idle cycle), ignoring breakpoints.
    pub fn step(&mut self) -> u32 {
        self.sys.events.clear();
        let c = self.step_unit();
        self.flush_q();
        c
    }

    fn step_unit(&mut self) -> u32 {
        let was_idle = self.cpu.state == CpuState::Idle;
        let c = self.cpu.step(&mut self.sys);
        self.resume_past_breakpoint = false;
        if !was_idle && self.cpu.state == CpuState::Idle {
            self.sys.events.push(Event::Halted);
        }
        c
    }

    fn flush_q(&mut self) {
        if self.sys.q_changed {
            self.sys.q_changed = false;
            self.sys.events.push(Event::Q(self.sys.q));
        }
    }
}
