//! Instruction execution (S1). Semantics follow the RCA CDP1802 user manual
//! (MPM-201). `R(P)` has already been incremented past the opcode by the fetch.

use crate::bus::Bus;
use crate::cpu::{Cpu, CpuState};

impl Cpu {
    #[inline]
    fn rp(&self) -> u16 {
        self.r[self.p as usize]
    }
    #[inline]
    fn rx(&self) -> u16 {
        self.r[self.x as usize]
    }
    #[inline]
    fn inc(&mut self, n: u8) {
        self.r[n as usize] = self.r[n as usize].wrapping_add(1);
    }
    #[inline]
    fn dec(&mut self, n: u8) {
        self.r[n as usize] = self.r[n as usize].wrapping_sub(1);
    }

    /// Read the immediate byte at M(R(P)) and advance R(P).
    #[inline]
    fn imm<B: Bus + ?Sized>(&mut self, bus: &mut B) -> u8 {
        let v = bus.read(self.rp());
        self.inc(self.p);
        v
    }

    /// `a + b + carry` as 9-bit arithmetic: D gets the low 8 bits, DF the carry.
    #[inline]
    fn add_with(&mut self, a: u8, b: u8, carry: bool) {
        let s = u16::from(a) + u16::from(b) + u16::from(carry);
        self.d = s as u8;
        self.df = s > 0xFF;
    }

    /// `a - b - borrow` where `borrow = !df_in`; DF = 1 when no borrow occurred.
    #[inline]
    fn sub_with(&mut self, a: u8, b: u8, df_in: bool) {
        self.add_with(a, !b, df_in);
    }

    /// Short branch: if `cond`, R(P).0 <- M(R(P)); else skip the immediate byte.
    #[inline]
    fn short_branch<B: Bus + ?Sized>(&mut self, cond: bool, bus: &mut B) {
        let pc = self.rp();
        if cond {
            let lo = bus.read(pc);
            self.r[self.p as usize] = (pc & 0xFF00) | u16::from(lo);
        } else {
            self.r[self.p as usize] = pc.wrapping_add(1);
        }
    }

    /// Long branch: if `cond`, R(P) <- M(R(P)), M(R(P)+1); else skip both bytes.
    #[inline]
    fn long_branch<B: Bus + ?Sized>(&mut self, cond: bool, bus: &mut B) {
        let pc = self.rp();
        if cond {
            let hi = bus.read(pc);
            let lo = bus.read(pc.wrapping_add(1));
            self.r[self.p as usize] = u16::from_be_bytes([hi, lo]);
        } else {
            self.r[self.p as usize] = pc.wrapping_add(2);
        }
    }

    /// Long skip: if `cond`, R(P) += 2.
    #[inline]
    fn long_skip(&mut self, cond: bool) {
        if cond {
            self.r[self.p as usize] = self.rp().wrapping_add(2);
        }
    }

    /// RET (`ie` = true) / DIS (`ie` = false): X,P <- M(R(X)); R(X) += 1.
    #[inline]
    fn ret_dis<B: Bus + ?Sized>(&mut self, ie: bool, bus: &mut B) {
        let v = bus.read(self.rx());
        self.inc(self.x);
        self.x = v >> 4;
        self.p = v & 0x0F;
        self.ie = ie;
    }

    #[inline]
    fn set_q_line<B: Bus + ?Sized>(&mut self, q: bool, bus: &mut B) {
        self.q = q;
        bus.set_q(q);
    }

    /// Execute one fetched opcode and return its machine cycles.
    pub(crate) fn execute<B: Bus + ?Sized>(&mut self, op: u8, bus: &mut B) -> u32 {
        let n = op & 0x0F;
        match op >> 4 {
            0x0 => {
                if n == 0 {
                    // IDL
                    self.state = CpuState::Idle;
                } else {
                    // LDN: D <- M(R(N))
                    self.d = bus.read(self.r[n as usize]);
                }
            }
            0x1 => self.inc(n), // INC
            0x2 => self.dec(n), // DEC
            0x3 => {
                let cond = match n & 7 {
                    0 => true,          // BR / SKP
                    1 => self.q,        // BQ / BNQ
                    2 => self.d == 0,   // BZ / BNZ
                    3 => self.df,       // BDF / BNF
                    k => bus.ef(k - 3), // B1-B4 / BN1-BN4
                };
                // 38 (SKP) is the negated BR: never branch, skip one byte.
                let cond = if n & 8 != 0 { !cond } else { cond };
                self.short_branch(cond, bus);
            }
            0x4 => {
                // LDA: D <- M(R(N)); R(N) += 1
                self.d = bus.read(self.r[n as usize]);
                self.inc(n);
            }
            0x5 => bus.write(self.r[n as usize], self.d), // STR
            0x6 => match n {
                0 => self.inc(self.x), // IRX
                1..=7 => {
                    // OUT N: bus <- M(R(X)); R(X) += 1
                    let v = bus.read(self.rx());
                    bus.out(n, v);
                    self.inc(self.x);
                }
                8 => {
                    // 68: no I/O device selected (N lines = 0) on the 1802. The data
                    // bus is not driven; treated as a 2-cycle no-operation.
                }
                _ => {
                    // INP N-8: M(R(X)) <- bus; D <- bus
                    let v = bus.inp(n - 8);
                    bus.write(self.rx(), v);
                    self.d = v;
                }
            },
            0x7 => match n {
                0x0 => self.ret_dis(true, bus),  // RET
                0x1 => self.ret_dis(false, bus), // DIS
                0x2 => {
                    // LDXA
                    self.d = bus.read(self.rx());
                    self.inc(self.x);
                }
                0x3 => {
                    // STXD
                    bus.write(self.rx(), self.d);
                    self.dec(self.x);
                }
                0x4 => {
                    // ADC: D,DF <- M(R(X)) + D + DF
                    let m = bus.read(self.rx());
                    self.add_with(m, self.d, self.df);
                }
                0x5 => {
                    // SDB: D,DF <- M(R(X)) - D - (NOT DF)
                    let m = bus.read(self.rx());
                    self.sub_with(m, self.d, self.df);
                }
                0x6 => {
                    // SHRC / RSHR: ring shift right through DF
                    let out = self.d & 1 != 0;
                    self.d = (self.d >> 1) | (u8::from(self.df) << 7);
                    self.df = out;
                }
                0x7 => {
                    // SMB: D,DF <- D - M(R(X)) - (NOT DF)
                    let m = bus.read(self.rx());
                    self.sub_with(self.d, m, self.df);
                }
                0x8 => bus.write(self.rx(), self.t), // SAV
                0x9 => {
                    // MARK: T <- X,P; M(R(2)) <- T; X <- P; R(2) -= 1
                    self.t = (self.x << 4) | self.p;
                    bus.write(self.r[2], self.t);
                    self.x = self.p;
                    self.dec(2);
                }
                0xA => self.set_q_line(false, bus), // REQ
                0xB => self.set_q_line(true, bus),  // SEQ
                0xC => {
                    // ADCI
                    let m = self.imm(bus);
                    self.add_with(m, self.d, self.df);
                }
                0xD => {
                    // SDBI
                    let m = self.imm(bus);
                    self.sub_with(m, self.d, self.df);
                }
                0xE => {
                    // SHLC / RSHL: ring shift left through DF
                    let out = self.d & 0x80 != 0;
                    self.d = (self.d << 1) | u8::from(self.df);
                    self.df = out;
                }
                _ => {
                    // SMBI
                    let m = self.imm(bus);
                    self.sub_with(self.d, m, self.df);
                }
            },
            0x8 => self.d = self.r[n as usize] as u8, // GLO
            0x9 => self.d = (self.r[n as usize] >> 8) as u8, // GHI
            0xA => self.r[n as usize] = (self.r[n as usize] & 0xFF00) | u16::from(self.d), // PLO
            0xB => self.r[n as usize] = (self.r[n as usize] & 0x00FF) | (u16::from(self.d) << 8), // PHI
            0xC => match n {
                0x0 => self.long_branch(true, bus),        // LBR
                0x1 => self.long_branch(self.q, bus),      // LBQ
                0x2 => self.long_branch(self.d == 0, bus), // LBZ
                0x3 => self.long_branch(self.df, bus),     // LBDF
                0x4 => {}                                  // NOP
                0x5 => self.long_skip(!self.q),            // LSNQ
                0x6 => self.long_skip(self.d != 0),        // LSNZ
                0x7 => self.long_skip(!self.df),           // LSNF
                0x8 => self.long_skip(true),               // LSKP / NLBR
                0x9 => self.long_branch(!self.q, bus),     // LBNQ
                0xA => self.long_branch(self.d != 0, bus), // LBNZ
                0xB => self.long_branch(!self.df, bus),    // LBNF
                0xC => self.long_skip(self.ie),            // LSIE
                0xD => self.long_skip(self.q),             // LSQ
                0xE => self.long_skip(self.d == 0),        // LSZ
                _ => self.long_skip(self.df),              // LSDF
            },
            0xD => self.p = n, // SEP
            0xE => self.x = n, // SEX
            _ => {
                // F0-F7 operate on M(R(X)); F8-FF on the immediate byte M(R(P)).
                let shift = n & 7 == 6;
                let m = if shift {
                    0
                } else if n < 8 {
                    bus.read(self.rx())
                } else {
                    self.imm(bus)
                };
                match n & 7 {
                    0 => self.d = m,                      // LDX / LDI
                    1 => self.d |= m,                     // OR / ORI
                    2 => self.d &= m,                     // AND / ANI
                    3 => self.d ^= m,                     // XOR / XRI
                    4 => self.add_with(m, self.d, false), // ADD / ADI
                    5 => self.sub_with(m, self.d, true),  // SD / SDI
                    6 => {
                        if n == 6 {
                            // SHR
                            self.df = self.d & 1 != 0;
                            self.d >>= 1;
                        } else {
                            // SHL
                            self.df = self.d & 0x80 != 0;
                            self.d <<= 1;
                        }
                    }
                    _ => self.sub_with(self.d, m, true), // SM / SMI
                }
            }
        }
        Self::instruction_cycles(op)
    }
}
