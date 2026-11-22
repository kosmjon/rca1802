//! CPU state and the step loop.

use crate::bus::Bus;

/// What the CPU is doing, derived from the /CLEAR and /WAIT control lines and IDL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuState {
    /// /CLEAR low, /WAIT high. Registers held in their reset values.
    Reset,
    /// /CLEAR low, /WAIT low. No fetching; DMA-IN requests are serviced (front-panel load).
    Load,
    /// /CLEAR high, /WAIT low. Clock to the CPU stopped; nothing changes.
    Pause,
    /// Executing instructions.
    Running,
    /// Stopped by IDL, waiting for DMA or an interrupt.
    Idle,
}

/// Hidden sequencing state of the CPU (see [`Cpu::internals`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuInternals {
    /// The next step is the initialization cycle after reset.
    pub init_pending: bool,
    /// State to resume when Pause ends.
    pub paused_from: Option<CpuState>,
}

/// RCA CDP1802 register state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    /// Scratchpad registers R0-RF.
    pub r: [u16; 16],
    /// Accumulator.
    pub d: u8,
    /// Data flag (carry / not borrow).
    pub df: bool,
    /// Program counter designator.
    pub p: u8,
    /// Data pointer designator.
    pub x: u8,
    /// Saved X and P (high and low nibble) from interrupt entry or MARK.
    pub t: u8,
    /// High nibble of the current instruction.
    pub i: u8,
    /// Low nibble of the current instruction.
    pub n: u8,
    /// Interrupt enable.
    pub ie: bool,
    /// Q output flip-flop.
    pub q: bool,
    /// Run state.
    pub state: CpuState,
    /// Machine cycles elapsed since power-on (including reset, load and pause time).
    pub cycles: u64,
    /// Set when leaving Reset or Load: the next step is the 1-cycle initialization cycle.
    init_pending: bool,
    /// State to return to when Pause ends.
    paused_from: Option<CpuState>,
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

impl Cpu {
    /// A CPU in the Reset state with all registers cleared.
    pub fn new() -> Self {
        Cpu {
            r: [0; 16],
            d: 0,
            df: false,
            p: 0,
            x: 0,
            t: 0,
            i: 0,
            n: 0,
            ie: true,
            q: false,
            state: CpuState::Reset,
            cycles: 0,
            init_pending: true,
            paused_from: None,
        }
    }

    /// Apply the reset values: I, N, Q, X, P and R0 cleared, IE set.
    /// D, DF, T and R1-RF keep their contents, as on the chip.
    pub fn reset<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        self.i = 0;
        self.n = 0;
        self.x = 0;
        self.p = 0;
        self.r[0] = 0;
        self.ie = true;
        self.q = false;
        bus.set_q(false);
        self.init_pending = true;
    }

    /// Set the /CLEAR and /WAIT control inputs (`true` = high = inactive).
    ///
    /// | /CLEAR | /WAIT | state |
    /// |---|---|---|
    /// | low | high | Reset |
    /// | low | low | Load |
    /// | high | high | Running (after one initialization cycle) |
    /// | high | low | Pause |
    pub fn set_control<B: Bus + ?Sized>(&mut self, clear_n: bool, wait_n: bool, bus: &mut B) {
        let new = match (clear_n, wait_n) {
            (false, true) => CpuState::Reset,
            (false, false) => CpuState::Load,
            (true, false) => CpuState::Pause,
            (true, true) => match self.state {
                // Leaving Pause resumes where it stopped (Running or Idle).
                CpuState::Pause => self.resume_state(),
                CpuState::Reset | CpuState::Load => CpuState::Running,
                s => s,
            },
        };
        // Reset values are applied on entering Reset or Load. Load from Reset keeps
        // R0 at 0; calling again with the same lines changes nothing (so DMA-IN
        // progress in Load is kept).
        if (new == CpuState::Reset || new == CpuState::Load) && new != self.state {
            self.reset(bus);
        }
        if new == CpuState::Pause && self.state != CpuState::Pause {
            self.paused_from = Some(self.state);
        }
        self.state = new;
    }

    fn resume_state(&mut self) -> CpuState {
        match self.paused_from.take() {
            Some(CpuState::Idle) => CpuState::Idle,
            Some(CpuState::Reset) | Some(CpuState::Load) | None => {
                self.init_pending = true;
                CpuState::Running
            }
            Some(_) => CpuState::Running,
        }
    }

    /// Run one unit of work and return the machine cycles it took:
    /// a DMA cycle, an interrupt cycle, an idle cycle, the initialization cycle,
    /// or one complete instruction (2 cycles, 3 for C0-CF).
    ///
    /// Requests are sampled only here, i.e. between instructions, as on the chip.
    /// `bus.tick` is called with the same cycle count before returning.
    pub fn step<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        let cycles = self.step_inner(bus);
        self.cycles += u64::from(cycles);
        bus.tick(cycles);
        cycles
    }

    fn step_inner<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u32 {
        match self.state {
            CpuState::Reset | CpuState::Pause => return 1,
            CpuState::Load => {
                if bus.signals().dma_in {
                    self.dma_in_cycle(bus);
                }
                return 1;
            }
            CpuState::Running | CpuState::Idle => {}
        }

        let sig = bus.signals();
        if sig.dma_in {
            self.dma_in_cycle(bus);
            self.state = CpuState::Running;
            return 1;
        }
        if sig.dma_out {
            self.dma_out_cycle(bus);
            self.state = CpuState::Running;
            return 1;
        }
        if sig.interrupt && self.ie {
            self.interrupt_cycle();
            bus.interrupt_ack();
            self.state = CpuState::Running;
            return 1;
        }
        if self.state == CpuState::Idle {
            return 1;
        }
        if self.init_pending {
            // Initialization cycle after reset: no fetch.
            self.init_pending = false;
            return 1;
        }

        let opcode = self.fetch(bus);
        self.execute(opcode, bus)
    }

    /// S2 DMA-IN: M(R0) <- peripheral byte, R0 += 1.
    fn dma_in_cycle<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        let v = bus.dma_in();
        bus.write(self.r[0], v);
        self.r[0] = self.r[0].wrapping_add(1);
    }

    /// S2 DMA-OUT: peripheral <- M(R0), R0 += 1.
    fn dma_out_cycle<B: Bus + ?Sized>(&mut self, bus: &mut B) {
        let v = bus.read(self.r[0]);
        bus.dma_out(v);
        self.r[0] = self.r[0].wrapping_add(1);
    }

    /// S3 interrupt: T <- (X,P), X <- 2, P <- 1, IE <- 0.
    fn interrupt_cycle(&mut self) {
        self.t = (self.x << 4) | self.p;
        self.x = 2;
        self.p = 1;
        self.ie = false;
    }

    /// S0 fetch: opcode <- M(R(P)), R(P) += 1.
    fn fetch<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 {
        let pc = self.r[self.p as usize];
        let op = bus.read(pc);
        self.r[self.p as usize] = pc.wrapping_add(1);
        self.i = op >> 4;
        self.n = op & 0x0F;
        op
    }

    /// Sequencing state not visible as registers, for saving and restoring snapshots.
    pub fn internals(&self) -> CpuInternals {
        CpuInternals {
            init_pending: self.init_pending,
            paused_from: self.paused_from,
        }
    }

    /// Restore state saved with [`Cpu::internals`].
    pub fn set_internals(&mut self, i: CpuInternals) {
        self.init_pending = i.init_pending;
        self.paused_from = i.paused_from;
    }

    /// Machine cycles for an opcode: 3 for C0-CF (long branch, long skip, NOP), else 2.
    pub const fn instruction_cycles(opcode: u8) -> u32 {
        if opcode & 0xF0 == 0xC0 {
            3
        } else {
            2
        }
    }
}
