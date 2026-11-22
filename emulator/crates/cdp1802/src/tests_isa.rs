//! Instruction-set tests (task 3).

use crate::test_bus::{IoEvent, TestBus};
use crate::{Cpu, CpuState};
use proptest::prelude::*;

const PC: u16 = 0x0100;

/// A running CPU with P = 3 and R3 = 0100, program loaded there.
fn setup(prog: &[u8]) -> (Cpu, TestBus) {
    let mut bus = TestBus::new();
    let mut cpu = Cpu::new();
    cpu.set_control(true, true, &mut bus);
    cpu.step(&mut bus); // initialization cycle
    cpu.p = 3;
    cpu.r[3] = PC;
    bus.load(PC, prog);
    bus.log.clear();
    (cpu, bus)
}

/// Run one instruction and return its cycle count.
fn step(cpu: &mut Cpu, bus: &mut TestBus) -> u32 {
    cpu.step(bus)
}

// ---------------------------------------------------------------- registers / memory

#[test]
fn ldn_lda_str() {
    let (mut cpu, mut bus) = setup(&[0x05, 0x45, 0x56]);
    cpu.r[5] = 0x2000;
    cpu.r[6] = 0x3000;
    bus.mem[0x2000] = 0x11;
    bus.mem[0x2001] = 0x22;
    step(&mut cpu, &mut bus); // LDN R5
    assert_eq!((cpu.d, cpu.r[5]), (0x11, 0x2000));
    step(&mut cpu, &mut bus); // LDA R5
    assert_eq!((cpu.d, cpu.r[5]), (0x11, 0x2001));
    step(&mut cpu, &mut bus); // STR R6
    assert_eq!(bus.mem[0x3000], 0x11);
    assert_eq!(cpu.r[3], PC + 3);
}

#[test]
fn inc_dec_wrap() {
    let (mut cpu, mut bus) = setup(&[0x17, 0x27, 0x27]);
    cpu.r[7] = 0xFFFF;
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.r[7], 0x0000);
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.r[7], 0xFFFF);
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.r[7], 0xFFFE);
}

#[test]
fn glo_ghi_plo_phi() {
    let (mut cpu, mut bus) = setup(&[0x8A, 0x9A, 0xAB, 0xBB]);
    cpu.r[0xA] = 0x1234;
    cpu.r[0xB] = 0xFFFF;
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.d, 0x34);
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.d, 0x12);
    step(&mut cpu, &mut bus); // PLO B with D = 12
    assert_eq!(cpu.r[0xB], 0xFF12);
    step(&mut cpu, &mut bus); // PHI B
    assert_eq!(cpu.r[0xB], 0x1212);
}

#[test]
fn ldx_ldxa_stxd_irx() {
    let (mut cpu, mut bus) = setup(&[0xF0, 0x72, 0x73, 0x60]);
    cpu.x = 4;
    cpu.r[4] = 0x2000;
    bus.mem[0x2000] = 0x77;
    step(&mut cpu, &mut bus); // LDX
    assert_eq!((cpu.d, cpu.r[4]), (0x77, 0x2000));
    step(&mut cpu, &mut bus); // LDXA
    assert_eq!((cpu.d, cpu.r[4]), (0x77, 0x2001));
    cpu.d = 0x99;
    step(&mut cpu, &mut bus); // STXD
    assert_eq!((bus.mem[0x2001], cpu.r[4]), (0x99, 0x2000));
    step(&mut cpu, &mut bus); // IRX
    assert_eq!(cpu.r[4], 0x2001);
}

#[test]
fn ldi_and_immediate_advance() {
    let (mut cpu, mut bus) = setup(&[0xF8, 0x5A, 0xF9, 0x0F]);
    step(&mut cpu, &mut bus);
    assert_eq!((cpu.d, cpu.r[3]), (0x5A, PC + 2));
    step(&mut cpu, &mut bus);
    assert_eq!((cpu.d, cpu.r[3]), (0x5F, PC + 4));
}

#[test]
fn sep_sex() {
    let (mut cpu, mut bus) = setup(&[0xE7, 0xD5]);
    cpu.r[5] = 0x0200;
    bus.mem[0x0200] = 0xC4; // NOP
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.x, 7);
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.p, 5);
    step(&mut cpu, &mut bus); // executes at R5
    assert_eq!(cpu.r[5], 0x0201);
}

// ---------------------------------------------------------------- logic and arithmetic

/// Run a single F-group or 7-group operation with D, M(R(X)) (or immediate) and DF.
fn alu(op: u8, d: u8, m: u8, df: bool) -> (u8, bool) {
    let immediate = matches!(op, 0x7C | 0x7D | 0x7F) || (op >= 0xF8 && op != 0xFE);
    let prog = if immediate { [op, m] } else { [op, 0x00] };
    let (mut cpu, mut bus) = setup(&prog);
    cpu.x = 4;
    cpu.r[4] = 0x2000;
    bus.mem[0x2000] = m;
    cpu.d = d;
    cpu.df = df;
    step(&mut cpu, &mut bus);
    assert_eq!(
        cpu.r[3],
        PC + if immediate { 2 } else { 1 },
        "R(P) after {op:02X}"
    );
    (cpu.d, cpu.df)
}

#[test]
fn logic_ops() {
    assert_eq!(alu(0xF1, 0xF0, 0x0F, false).0, 0xFF); // OR
    assert_eq!(alu(0xF2, 0xF0, 0x3C, false).0, 0x30); // AND
    assert_eq!(alu(0xF3, 0xFF, 0x0F, false).0, 0xF0); // XOR
    assert_eq!(alu(0xF9, 0x01, 0x80, false).0, 0x81); // ORI
    assert_eq!(alu(0xFA, 0xFF, 0x0F, false).0, 0x0F); // ANI
    assert_eq!(alu(0xFB, 0xAA, 0xFF, false).0, 0x55); // XRI
                                                      // Logic ops leave DF alone.
    assert!(alu(0xF1, 0, 0, true).1);
}

#[test]
fn add_and_subtract_examples() {
    // ADD: 3A + 7F = B9, no carry; 80 + 80 = 00, carry.
    assert_eq!(alu(0xF4, 0x3A, 0x7F, true), (0xB9, false));
    assert_eq!(alu(0xF4, 0x80, 0x80, false), (0x00, true));
    // ADC uses DF.
    assert_eq!(alu(0x74, 0xFF, 0x00, true), (0x00, true));
    // SD: M - D. 40 - 20 = 20, no borrow (DF = 1); 20 - 40 = E0, borrow (DF = 0).
    assert_eq!(alu(0xF5, 0x20, 0x40, false), (0x20, true));
    assert_eq!(alu(0xF5, 0x40, 0x20, true), (0xE0, false));
    // SM: D - M.
    assert_eq!(alu(0xF7, 0x40, 0x20, false), (0x20, true));
    assert_eq!(alu(0xF7, 0x00, 0x01, true), (0xFF, false));
    // SDB / SMB with borrow in (DF = 0) subtract one more.
    assert_eq!(alu(0x75, 0x20, 0x40, false), (0x1F, true));
    assert_eq!(alu(0x77, 0x40, 0x20, false), (0x1F, true));
    // Immediate forms.
    assert_eq!(alu(0xFC, 0x01, 0xFF, false), (0x00, true)); // ADI
    assert_eq!(alu(0xFD, 0x01, 0x00, true), (0xFF, false)); // SDI
    assert_eq!(alu(0xFF, 0x05, 0x03, false), (0x02, true)); // SMI
    assert_eq!(alu(0x7C, 0x01, 0x01, true), (0x03, false)); // ADCI
    assert_eq!(alu(0x7D, 0x01, 0x05, false), (0x03, true)); // SDBI
    assert_eq!(alu(0x7F, 0x05, 0x01, false), (0x03, true)); // SMBI
}

#[test]
fn shifts() {
    assert_eq!(alu(0xF6, 0x81, 0, false), (0x40, true)); // SHR
    assert_eq!(alu(0xFE, 0x81, 0, false), (0x02, true)); // SHL
    assert_eq!(alu(0x76, 0x02, 0, true), (0x81, false)); // SHRC
    assert_eq!(alu(0x7E, 0x40, 0, true), (0x81, false)); // SHLC
    assert_eq!(alu(0xF6, 0x02, 0, true), (0x01, false));
}

/// 9-bit reference definitions from the RCA manual.
fn reference(op: u8, d: u8, m: u8, df: bool) -> (u8, bool) {
    let (d, m, c) = (i32::from(d), i32::from(m), i32::from(df));
    let r = match op {
        0xF4 | 0xFC => d + m,           // ADD / ADI
        0x74 | 0x7C => d + m + c,       // ADC / ADCI
        0xF5 | 0xFD => m - d,           // SD / SDI
        0x75 | 0x7D => m - d - (1 - c), // SDB / SDBI
        0xF7 | 0xFF => d - m,           // SM / SMI
        0x77 | 0x7F => d - m - (1 - c), // SMB / SMBI
        _ => unreachable!(),
    };
    let df_out = match op {
        0xF4 | 0xFC | 0x74 | 0x7C => r > 0xFF,
        _ => r >= 0, // subtraction: DF = 1 means no borrow
    };
    ((r & 0xFF) as u8, df_out)
}

proptest! {
    /// Property 2: arithmetic and DF match the 9-bit reference model.
    #[test]
    fn prop_arithmetic(
        op in prop::sample::select(std::vec![0xF4u8, 0xFC, 0x74, 0x7C, 0xF5, 0xFD, 0x75, 0x7D, 0xF7, 0xFF, 0x77, 0x7F]),
        d in any::<u8>(), m in any::<u8>(), df in any::<bool>())
    {
        prop_assert_eq!(alu(op, d, m, df), reference(op, d, m, df), "{} ({:02X})", crate::disasm(&[op, m], PC), op);
    }
}

// ---------------------------------------------------------------- branches and skips

/// Short branch at `addr` with target low byte `t`; returns the new R(P).
fn short_branch(op: u8, addr: u16, t: u8, f: impl FnOnce(&mut Cpu, &mut TestBus)) -> u16 {
    let (mut cpu, mut bus) = setup(&[]);
    cpu.r[3] = addr;
    bus.load(addr, &[op, t]);
    f(&mut cpu, &mut bus);
    assert_eq!(step(&mut cpu, &mut bus), 2);
    cpu.r[3]
}

#[test]
fn short_branches() {
    let taken = 0x0155;
    let not = PC + 2;
    assert_eq!(short_branch(0x30, PC, 0x55, |_, _| {}), taken); // BR
    assert_eq!(short_branch(0x38, PC, 0x55, |_, _| {}), not); // SKP
    assert_eq!(short_branch(0x31, PC, 0x55, |c, _| c.q = true), taken); // BQ
    assert_eq!(short_branch(0x39, PC, 0x55, |c, _| c.q = true), not); // BNQ
    assert_eq!(short_branch(0x32, PC, 0x55, |c, _| c.d = 0), taken); // BZ
    assert_eq!(short_branch(0x3A, PC, 0x55, |c, _| c.d = 1), taken); // BNZ
    assert_eq!(short_branch(0x33, PC, 0x55, |c, _| c.df = true), taken); // BDF
    assert_eq!(short_branch(0x3B, PC, 0x55, |c, _| c.df = true), not); // BNF
    for k in 0..4u8 {
        let set = move |_: &mut Cpu, b: &mut TestBus| b.ef[k as usize] = true;
        assert_eq!(short_branch(0x34 + k, PC, 0x55, set), taken, "B{}", k + 1);
        assert_eq!(short_branch(0x3C + k, PC, 0x55, set), not, "BN{}", k + 1);
        assert_eq!(
            short_branch(0x3C + k, PC, 0x55, |_, _| {}),
            taken,
            "BN{}",
            k + 1
        );
    }
}

#[test]
fn short_branch_uses_page_of_immediate_byte() {
    // Opcode at 01FF, immediate byte at 0200: the branch stays in page 02.
    assert_eq!(short_branch(0x30, 0x01FF, 0x10, |_, _| {}), 0x0210);
    // Not taken from 01FF skips to 0201.
    assert_eq!(short_branch(0x38, 0x01FF, 0x10, |_, _| {}), 0x0201);
}

fn long_op(op: u8, f: impl FnOnce(&mut Cpu, &mut TestBus)) -> u16 {
    let (mut cpu, mut bus) = setup(&[op, 0x12, 0x34]);
    f(&mut cpu, &mut bus);
    assert_eq!(step(&mut cpu, &mut bus), 3);
    cpu.r[3]
}

#[test]
fn long_branches_and_skips() {
    let taken = 0x1234;
    let skip = PC + 3;
    let next = PC + 1;
    assert_eq!(long_op(0xC0, |_, _| {}), taken); // LBR
    assert_eq!(long_op(0xC8, |_, _| {}), skip); // LSKP
    assert_eq!(long_op(0xC4, |_, _| {}), next); // NOP
    assert_eq!(long_op(0xC1, |c, _| c.q = true), taken); // LBQ
    assert_eq!(long_op(0xC9, |c, _| c.q = true), skip); // LBNQ not taken
    assert_eq!(long_op(0xC2, |c, _| c.d = 0), taken); // LBZ
    assert_eq!(long_op(0xCA, |c, _| c.d = 0), skip); // LBNZ not taken
    assert_eq!(long_op(0xC3, |c, _| c.df = true), taken); // LBDF
    assert_eq!(long_op(0xCB, |c, _| c.df = true), skip); // LBNF not taken
    assert_eq!(long_op(0xC5, |c, _| c.q = false), skip); // LSNQ
    assert_eq!(long_op(0xC5, |c, _| c.q = true), next);
    assert_eq!(long_op(0xC6, |c, _| c.d = 1), skip); // LSNZ
    assert_eq!(long_op(0xC7, |c, _| c.df = false), skip); // LSNF
    assert_eq!(long_op(0xCC, |c, _| c.ie = true), skip); // LSIE
    assert_eq!(long_op(0xCC, |c, _| c.ie = false), next);
    assert_eq!(long_op(0xCD, |c, _| c.q = true), skip); // LSQ
    assert_eq!(long_op(0xCE, |c, _| c.d = 0), skip); // LSZ
    assert_eq!(long_op(0xCF, |c, _| c.df = true), skip); // LSDF
    assert_eq!(long_op(0xCF, |c, _| c.df = false), next);
}

/// Condition of a short (30-3F) or long (C0-CF) branch/skip, from the manual's tables.
fn condition(op: u8, cpu: &Cpu, ef: [bool; 4]) -> bool {
    let n = op & 0x0F;
    let base = match n & 7 {
        0 => true,
        1 => cpu.q,
        2 => cpu.d == 0,
        3 => cpu.df,
        k => ef[(k - 4) as usize],
    };
    if op >> 4 == 3 {
        return if n & 8 != 0 { !base } else { base };
    }
    match n {
        0x0 => true,
        0x1 => cpu.q,
        0x2 => cpu.d == 0,
        0x3 => cpu.df,
        0x5 => !cpu.q,
        0x6 => cpu.d != 0,
        0x7 => !cpu.df,
        0x8 => true,
        0x9 => !cpu.q,
        0xA => cpu.d != 0,
        0xB => !cpu.df,
        0xC => cpu.ie,
        0xD => cpu.q,
        0xE => cpu.d == 0,
        0xF => cpu.df,
        _ => false, // C4 NOP
    }
}

proptest! {
    /// Property 3: branches and skips go to the documented address for all flag states.
    #[test]
    fn prop_branches(
        op in prop_oneof![0x30u8..=0x3F, 0xC0u8..=0xCF],
        addr in 0u16..0xFFF0, q in any::<bool>(), d in prop_oneof![Just(0u8), any::<u8>()],
        df in any::<bool>(), ie in any::<bool>(), ef in any::<[bool; 4]>(),
        b1 in any::<u8>(), b2 in any::<u8>())
    {
        let (mut cpu, mut bus) = setup(&[]);
        cpu.r[3] = addr;
        bus.load(addr, &[op, b1, b2]);
        cpu.q = q; cpu.d = d; cpu.df = df; cpu.ie = ie; bus.ef = ef;
        let c = condition(op, &cpu, ef);
        let cyc = step(&mut cpu, &mut bus);
        let after = addr.wrapping_add(1);
        let expect = if op >> 4 == 3 {
            prop_assert_eq!(cyc, 2);
            if c { (after & 0xFF00) | u16::from(b1) } else { after.wrapping_add(1) }
        } else {
            prop_assert_eq!(cyc, 3);
            let is_skip = matches!(op & 0x0F, 0x5..=0x8 | 0xC..=0xF);
            match (op == 0xC4, is_skip, c) {
                (true, _, _) => after,
                (false, true, true) => after.wrapping_add(2),
                (false, true, false) => after,
                (false, false, true) => u16::from_be_bytes([b1, b2]),
                (false, false, false) => after.wrapping_add(2),
            }
        };
        prop_assert_eq!(cpu.r[3], expect, "{} ({:02X})", crate::disasm(&[op, b1, b2], addr), op);
    }
}

// ---------------------------------------------------------------- control and I/O

#[test]
fn req_seq_drive_q() {
    let (mut cpu, mut bus) = setup(&[0x7B, 0x7A]);
    step(&mut cpu, &mut bus);
    assert!(cpu.q);
    step(&mut cpu, &mut bus);
    assert!(!cpu.q);
    assert_eq!(bus.log, [IoEvent::Q(true), IoEvent::Q(false)]);
}

#[test]
fn out_and_inp() {
    let (mut cpu, mut bus) = setup(&[0x64, 0x6C, 0x68]);
    cpu.x = 5;
    cpu.r[5] = 0x2000;
    bus.mem[0x2000] = 0xAB;
    bus.inp_value[4] = 0xCD;
    step(&mut cpu, &mut bus); // OUT 4
    assert_eq!(cpu.r[5], 0x2001);
    step(&mut cpu, &mut bus); // INP 4
    assert_eq!((cpu.d, bus.mem[0x2001], cpu.r[5]), (0xCD, 0xCD, 0x2001));
    let d = cpu.d;
    step(&mut cpu, &mut bus); // 68: no-op on the 1802
    assert_eq!((cpu.d, cpu.r[5]), (d, 0x2001));
    assert_eq!(bus.log, [IoEvent::Out(4, 0xAB), IoEvent::Inp(4)]);
}

#[test]
fn all_out_inp_ports() {
    for port in 1..=7u8 {
        let (mut cpu, mut bus) = setup(&[0x60 + port, 0x68 + port]);
        cpu.x = 2;
        cpu.r[2] = 0x2000;
        bus.inp_value[port as usize] = port;
        step(&mut cpu, &mut bus);
        step(&mut cpu, &mut bus);
        assert_eq!(bus.log, [IoEvent::Out(port, 0), IoEvent::Inp(port)]);
        assert_eq!(cpu.d, port);
    }
}

#[test]
fn ret_dis_mark_sav() {
    // MARK then RET restores X and P and sets IE.
    let (mut cpu, mut bus) = setup(&[0x79, 0x70]);
    cpu.x = 6;
    cpu.r[2] = 0x2005;
    cpu.ie = false;
    step(&mut cpu, &mut bus); // MARK: T = 63, M(2005) = 63, X = 3, R2 = 2004
    assert_eq!(
        (cpu.t, bus.mem[0x2005], cpu.x, cpu.r[2]),
        (0x63, 0x63, 3, 0x2004)
    );
    // RET reads M(R(X)) = M(R3) which now points at the next program byte; set up R(X) instead.
    cpu.x = 2;
    cpu.r[2] = 0x2005;
    step(&mut cpu, &mut bus); // RET
    assert_eq!((cpu.x, cpu.p, cpu.ie, cpu.r[2]), (6, 3, true, 0x2006));

    let (mut cpu, mut bus) = setup(&[0x71]);
    cpu.x = 2;
    cpu.r[2] = 0x2000;
    bus.mem[0x2000] = 0x45;
    step(&mut cpu, &mut bus); // DIS
    assert_eq!((cpu.x, cpu.p, cpu.ie, cpu.r[2]), (4, 5, false, 0x2001));

    let (mut cpu, mut bus) = setup(&[0x78]);
    cpu.x = 2;
    cpu.r[2] = 0x2000;
    cpu.t = 0x5A;
    step(&mut cpu, &mut bus); // SAV
    assert_eq!(bus.mem[0x2000], 0x5A);
}

#[test]
fn idl_stops_until_request() {
    let (mut cpu, mut bus) = setup(&[0x00]);
    step(&mut cpu, &mut bus);
    assert_eq!(cpu.state, CpuState::Idle);
}

#[test]
fn interrupt_round_trip() {
    // Main program at 0100 on P = 3, X = 2. ISR at 0300 on R1: SAV / RET style return.
    let (mut cpu, mut bus) = setup(&[0xC4, 0xC4]);
    cpu.x = 5;
    cpu.r[1] = 0x0300;
    cpu.r[2] = 0x2000;
    // ISR: DEC R2, SAV, RET.  (SAV stores T at M(R2); RET then loads X,P from it.)
    bus.load(0x0300, &[0x22, 0x78, 0x70]);
    bus.signals.interrupt = true;
    step(&mut cpu, &mut bus); // interrupt cycle
    bus.signals.interrupt = false;
    assert_eq!((cpu.p, cpu.x, cpu.t, cpu.ie), (1, 2, 0x53, false));
    for _ in 0..3 {
        step(&mut cpu, &mut bus);
    }
    assert_eq!((cpu.p, cpu.x, cpu.ie), (3, 5, true));
    assert_eq!(
        cpu.r[3], PC,
        "main program resumes where it was interrupted"
    );
}

proptest! {
    /// Property 1: every opcode returns 2 cycles except C0-CF which return 3,
    /// from any register state, and cpu.cycles accumulates the returned value.
    #[test]
    fn prop_cycle_accounting(op in any::<u8>(), r in any::<[u16; 16]>(), d in any::<u8>(),
                             df in any::<bool>(), x in 0u8..16, q in any::<bool>(), ie in any::<bool>())
    {
        let (mut cpu, mut bus) = setup(&[]);
        cpu.r = r; cpu.r[3] = PC; cpu.d = d; cpu.df = df; cpu.x = x; cpu.q = q; cpu.ie = ie;
        bus.load(PC, &[op, 0x00, 0x00]);
        let c0 = cpu.cycles;
        let cyc = step(&mut cpu, &mut bus);
        let expect = if (0xC0..=0xCF).contains(&op) { 3 } else { 2 };
        prop_assert_eq!(cyc, expect, "{}", crate::disasm(&[op, 0, 0], PC));
        prop_assert_eq!(cpu.cycles - c0, u64::from(cyc));
    }

    /// Property 5 (return): RET / DIS load X,P from M(R(X)), increment R(X), set IE to 1 / 0.
    #[test]
    fn prop_ret_dis(dis in any::<bool>(), x in 0u8..16, rx in 0x1000u16..0xF000, v in any::<u8>(), ie in any::<bool>()) {
        prop_assume!(x != 3);
        let (mut cpu, mut bus) = setup(&[if dis { 0x71 } else { 0x70 }]);
        cpu.x = x;
        cpu.r[x as usize] = rx;
        cpu.ie = ie;
        bus.mem[rx as usize] = v;
        step(&mut cpu, &mut bus);
        prop_assert_eq!((cpu.x, cpu.p), (v >> 4, v & 0x0F));
        prop_assert_eq!(cpu.r[x as usize], rx + 1);
        prop_assert_eq!(cpu.ie, !dis);
    }
}
