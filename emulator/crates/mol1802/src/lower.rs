//! Lowering: `Statement` -> real `asm1802` instruction text
//! (`design.md`, section "4. Lowering").
//!
//! This module currently covers only the Requirement 3.1 register-transfer
//! table (task 5). The `PORT(n)` special case (Requirement 4, task 6) is
//! checked *before* this table in the real pipeline, since it's the only
//! chain shape that needs the `xtrack` lookback; none of the rows here
//! touch `X` at all.

use crate::ast::{
    Adjust, BranchLength, Condition, Half, Operand, RegRef, StackDirection, StackElement,
};
use crate::xtrack::{preceding_sex, x_still_at, EmittedLine};

/// The reserved register-to-register arithmetic scratch pointer
/// (Requirement 9, design.md section 6): the first register not already
/// claimed by a fixed role (`R1`-`R5`) or DMA convention (`R0`).
const SCRATCH_REG: RegRef = RegRef(6);

/// The compiler-reserved scratch byte's label (Requirement 9.2/9.3).
pub(crate) const SCRATCH_LABEL: &str = "MOL1802_SCRATCH";

/// The threshold between "a few `INC`/`DEC`" and "the general 16-bit
/// add/subtract through `D`" for Requirement 8's constant-arithmetic
/// sugar (named per Requirement 8.3; used starting task 8).
pub(crate) const INC_DEC_THRESHOLD: u32 = 4;

/// Lowers one chain statement `a : b`, routing to the `PORT(n)` special
/// case (Requirement 4) first and the Requirement 3 table otherwise. This
/// is the entry point the eventual pipeline (tasks 7-9) calls for every
/// `Statement::Chain`.
pub(crate) fn lower_chain_statement(
    a: &Operand,
    b: &Operand,
    output_so_far: &[EmittedLine],
) -> Result<Vec<EmittedLine>, String> {
    if matches!(a, Operand::Port(_)) || matches!(b, Operand::Port(_)) {
        return lower_port_chain(a, b, output_so_far);
    }
    if let Some((left, is_add)) = register_arithmetic_chain(a, b) {
        return Ok(lower_register_arithmetic(&left, is_add, b, output_so_far));
    }
    lower_chain(a, b).map(|lines| lines.into_iter().map(EmittedLine::new).collect())
}

/// True if `a : b` is a register-to-register arithmetic chain
/// (Requirement 9) -- exposed so the re-merge step (task 9) can decide,
/// across the whole file, whether the `R6` init block and scratch byte
/// are needed at all.
pub(crate) fn is_register_arithmetic_chain(a: &Operand, b: &Operand) -> bool {
    register_arithmetic_chain(a, b).is_some()
}

/// Recognizes `Rm[.h] + Rn[.h] : Rn[.h]` / `Rm[.h] - Rn[.h] : Rn[.h]`
/// (Requirement 9.1/9.4) in an `Imm`'s raw text, the same way
/// `parse_reg_plus_const` recognizes the constant-arithmetic sugar. Both
/// sides must be plain register/half operands of the *same* shape (both
/// full `Reg`, or both `RegHalf` with the same half), and the right side
/// must be exactly the chain's destination `b`.
fn register_arithmetic_chain(a: &Operand, b: &Operand) -> Option<(Operand, bool)> {
    let Operand::Imm(text) = a else { return None };
    let text = text.trim();
    let idx = text.find(['+', '-'])?;
    let left = crate::parser::parse_operand(text[..idx].trim()).ok()?;
    let is_add = text.as_bytes()[idx] == b'+';
    let right = crate::parser::parse_operand(text[idx + 1..].trim()).ok()?;
    let same_shape = matches!((&left, &right), (Operand::Reg(_), Operand::Reg(_)))
        || matches!(
            (&left, &right),
            (Operand::RegHalf(_, lh), Operand::RegHalf(_, rh)) if lh == rh
        );
    if same_shape && &right == b {
        Some((left, is_add))
    } else {
        None
    }
}

/// `PORT(n) : D` -> `INP n`, `D : PORT(n)` -> `OUT n` (Requirement 4.1,
/// 4.2), guarded by the `xtrack` lookback in *error* mode (Requirement
/// 4.4): the statement immediately before must be a pass-through `SEX
/// Rn` line, or this is the one place MOL1802 refuses to guess. Anything
/// paired with `PORT(n)` other than exactly `D` is the Requirement 4.5
/// error.
fn lower_port_chain(
    a: &Operand,
    b: &Operand,
    output_so_far: &[EmittedLine],
) -> Result<Vec<EmittedLine>, String> {
    let (is_inp, port, other) = match (a, b) {
        (Operand::Port(n), other) => (true, n, other),
        (other, Operand::Port(n)) => (false, n, other),
        _ => unreachable!("lower_port_chain requires a PORT(n) operand on one side"),
    };
    if !matches!(other, Operand::D) {
        return Err(format!(
            "`PORT({port})` only pairs with `D`; move the value into `D` first \
             (e.g. `{other:?} : D` then `D : PORT({port})`)"
        ));
    }
    if preceding_sex(output_so_far).is_none() {
        return Err(format!(
            "`PORT({port})` needs an explicit `SEX Rn` on the line immediately \
             before it (INP/OUT always address memory through whatever X \
             currently designates)"
        ));
    }
    let text = if is_inp {
        format!("INP {port}")
    } else {
        format!("OUT {port}")
    };
    Ok(vec![EmittedLine::new(text)])
}

/// Lowers one register-transfer chain `a : b` to the `asm1802` instruction
/// text that realizes it (Requirement 3.1), or an error naming the
/// unsupported shape (Requirement 1.4). Callers handle the `PORT(n)`
/// special case (Requirement 4) before reaching this function.
pub(crate) fn lower_chain(a: &Operand, b: &Operand) -> Result<Vec<String>, String> {
    // Requirement 3.3: any no-op assignment (not just `D : D`) is accepted
    // and emits nothing.
    if a == b {
        return Ok(Vec::new());
    }

    // Requirement 8: `Rn + k : Rn` / `Rn - k : Rn` for a compile-time
    // constant `k`. Checked before the general immediate-to-full-register
    // row below, since `Rn + k` parses as an opaque `Imm` (MOL1802 has no
    // separate AST node for it) and would otherwise be misread as a
    // 16-bit literal load of the literal text `Rn + k`.
    if let (Operand::Imm(text), Operand::Reg(dest)) = (a, b) {
        if let Some((reg, is_add, k)) = parse_reg_plus_const(text) {
            if reg == *dest {
                return Ok(lower_constant_arithmetic(*dest, is_add, k));
            }
        }
    }

    use Operand::*;
    match (a, b) {
        // D <-> register half.
        (D, RegHalf(r, Half::Low)) => Ok(vec![format!("PLO {}", r.asm_name())]),
        (D, RegHalf(r, Half::High)) => Ok(vec![format!("PHI {}", r.asm_name())]),
        (RegHalf(r, Half::Low), D) => Ok(vec![format!("GLO {}", r.asm_name())]),
        (RegHalf(r, Half::High), D) => Ok(vec![format!("GHI {}", r.asm_name())]),

        // Immediate -> D / Rn.0 / Rn.1: always a single `LDI`, truncated
        // to 8 bits by asm1802 itself.
        (Imm(imm), D) => Ok(vec![format!("LDI {imm}")]),
        (Imm(imm), RegHalf(r, Half::Low)) => {
            Ok(vec![format!("LDI {imm}"), format!("PLO {}", r.asm_name())])
        }
        (Imm(imm), RegHalf(r, Half::High)) => {
            Ok(vec![format!("LDI {imm}"), format!("PHI {}", r.asm_name())])
        }

        // Immediate -> full Rn: the HIGH/LOW split (Requirement 3.1),
        // unless the literal is unambiguously byte-sized as written
        // (Requirement 3.2) -- that's very likely a `.0`/`.1` typo rather
        // than an intentionally-zero high byte.
        (Imm(imm), Reg(r)) => {
            if looks_like_an_explicit_byte(imm) {
                return Err(format!(
                    "`{imm}` is an 8-bit literal; assign it to `{r}.0` or `{r}.1`, \
                     or use a 16-bit literal for the full register `{r}`",
                    r = r.asm_name()
                ));
            }
            Ok(vec![
                format!("LDI HIGH({imm})"),
                format!("PHI {}", r.asm_name()),
                format!("LDI LOW({imm})"),
                format!("PLO {}", r.asm_name()),
            ])
        }

        // Full register -> full register.
        (Reg(m), Reg(n)) => Ok(vec![
            format!("GHI {}", m.asm_name()),
            format!("PHI {}", n.asm_name()),
            format!("GLO {}", m.asm_name()),
            format!("PLO {}", n.asm_name()),
        ]),

        // D <-> (Rn), all four adjust forms, both directions.
        (D, Mem(r, adjust)) => Ok(d_to_mem(*r, *adjust)),
        (Mem(r, adjust), D) => Ok(mem_to_d(*r, *adjust)),

        // Rm.0/Rm.1 <-> (Rn), all four adjust forms, both directions. No
        // 1802 instruction moves memory directly into or out of a
        // register half -- `D` is the unavoidable intermediate, exactly
        // as it is for the `(Rn) <-> D` rows above, so this generalizes
        // those rows rather than adding a new mechanism: read/write `D`
        // exactly as above, then move it into/out of the half with
        // `PLO`/`PHI`/`GLO`/`GHI`.
        (Mem(r, adjust), RegHalf(m, half)) => {
            let mut lines = mem_to_d(*r, *adjust);
            lines.push(format!("{} {}", half_mnemonics(*half).1, m.asm_name()));
            Ok(lines)
        }
        (RegHalf(m, half), Mem(r, adjust)) => {
            let mut lines = vec![format!("{} {}", half_mnemonics(*half).0, m.asm_name())];
            lines.extend(d_to_mem(*r, *adjust));
            Ok(lines)
        }

        // Writable single-bit destinations (Requirement 2.2/2.7/2.8):
        // only `1`/`0` make sense as the source.
        (Imm(imm), Df) => one_or_zero(imm, "DF", "SMI 0", "ADI 0"),
        (Imm(imm), Q) => one_or_zero(imm, "Q", "SEQ", "REQ"),
        (Imm(imm), Ei) => one_or_zero(imm, "EI", "RET", "DIS"),

        _ => Err(format!(
            "unsupported assignment {a:?} : {b:?} (no Requirement 3 table row matches)"
        )),
    }
}

/// `(Rn adjust) -> D`: the read half of the `(Rn) <-> D` rows (shared by
/// the `(Rn) <-> Rm.half` rows too, which just add a `PLO`/`PHI` on top).
fn mem_to_d(r: RegRef, adjust: Adjust) -> Vec<String> {
    let name = r.asm_name();
    match adjust {
        Adjust::None => vec![format!("LDN {name}")],
        Adjust::PostInc => vec![format!("LDA {name}")],
        Adjust::PostDec => vec![format!("LDN {name}"), format!("DEC {name}")],
        Adjust::PreInc => vec![format!("INC {name}"), format!("LDN {name}")],
        Adjust::PreDec => vec![format!("DEC {name}"), format!("LDN {name}")],
    }
}

/// `D -> (Rn adjust)`: the write half of the `D <-> (Rn)` rows (shared by
/// the `Rm.half <-> (Rn)` rows too, which just add a `GLO`/`GHI` first).
fn d_to_mem(r: RegRef, adjust: Adjust) -> Vec<String> {
    let name = r.asm_name();
    match adjust {
        Adjust::None => vec![format!("STR {name}")],
        Adjust::PostInc => vec![format!("STR {name}"), format!("INC {name}")],
        Adjust::PostDec => vec![format!("STR {name}"), format!("DEC {name}")],
        Adjust::PreInc => vec![format!("INC {name}"), format!("STR {name}")],
        Adjust::PreDec => vec![format!("DEC {name}"), format!("STR {name}")],
    }
}

/// `(GLO/GHI, PLO/PHI)` for a given half -- the instructions that move a
/// register half into/out of `D`.
fn half_mnemonics(half: Half) -> (&'static str, &'static str) {
    match half {
        Half::Low => ("GLO", "PLO"),
        Half::High => ("GHI", "PHI"),
    }
}

/// `A : B : C : ...` (3+ operands): the same source written into more
/// than one destination, left to right. Computes `A` into `D` once
/// (reusing exactly the byte-sized "X -> D" half of the Requirement 3
/// table), then moves `D` into each destination in turn (reusing the
/// "D -> Y" half) -- `D` is never clobbered by a `PLO`/`PHI`/`STR`, so
/// one computation safely feeds every destination. Deliberately scoped
/// to byte-sized operands only (`D`, `Rn.0`/`Rn.1`, `(Rn...)`): a full
/// `Rn` destination would need its own `HIGH`/`LOW` pair recomputed per
/// destination, a different (and unrequested) feature, so it's an error
/// here rather than a guess. `PORT(n)`/`DF`/`Q`/`EI` are excluded too --
/// none of them are real `D`-mediated transfers (`PORT(n)` needs its own
/// `xtrack` lookback; `DF`/`Q`/`EI` compile from the literal text `1`/`0`,
/// not a `D` value).
pub(crate) fn lower_fanout_chain(operands: &[Operand]) -> Result<Vec<String>, String> {
    let (source, destinations) = operands
        .split_first()
        .expect("a chain always has at least one operand");
    let mut lines = operand_to_d(source)?;
    for dest in destinations {
        lines.extend(d_to_operand(dest)?);
    }
    Ok(lines)
}

/// Computes `source` into `D`: the left half of a fan-out chain.
fn operand_to_d(source: &Operand) -> Result<Vec<String>, String> {
    match source {
        Operand::D => Ok(Vec::new()),
        Operand::Imm(imm) => Ok(vec![format!("LDI {imm}")]),
        Operand::RegHalf(r, half) => Ok(vec![format!(
            "{} {}",
            half_mnemonics(*half).0,
            r.asm_name()
        )]),
        Operand::Mem(r, adjust) => Ok(mem_to_d(*r, *adjust)),
        other => Err(format!(
            "`{}` can't be the source of a multi-destination chain \
             (only D, an immediate, Rn.0/Rn.1, or (Rn...) are)",
            operand_name(other)
        )),
    }
}

/// Moves `D` into `dest`: one step of the right half of a fan-out chain.
fn d_to_operand(dest: &Operand) -> Result<Vec<String>, String> {
    match dest {
        Operand::D => Ok(Vec::new()),
        Operand::RegHalf(r, half) => Ok(vec![format!(
            "{} {}",
            half_mnemonics(*half).1,
            r.asm_name()
        )]),
        Operand::Mem(r, adjust) => Ok(d_to_mem(*r, *adjust)),
        other => Err(format!(
            "`{}` can't be a destination in a multi-destination chain \
             (only D, Rn.0/Rn.1, or (Rn...) are; a full Rn needs its own \
             2-element chain, since each full register needs its own \
             HIGH/LOW load)",
            operand_name(other)
        )),
    }
}

/// A user-facing spelling for an operand, for error messages -- `Rn` for
/// a full register (`r.asm_name()`), the matching keyword for the other
/// single-token operands, and `Debug` as a fallback for the rest.
fn operand_name(op: &Operand) -> String {
    match op {
        Operand::Reg(r) => r.asm_name(),
        Operand::Port(n) => format!("PORT({n})"),
        Operand::Df => "DF".to_string(),
        Operand::Q => "Q".to_string(),
        Operand::Ei => "EI".to_string(),
        Operand::Ef(k) => format!("EF{k}"),
        other => format!("{other:?}"),
    }
}

/// `1 : DF/Q/EI` and `0 : DF/Q/EI` (Requirements 2.2, 2.7, 2.8).
fn one_or_zero(
    imm: &str,
    name: &str,
    when_one: &str,
    when_zero: &str,
) -> Result<Vec<String>, String> {
    match imm.trim() {
        "1" => Ok(vec![when_one.to_string()]),
        "0" => Ok(vec![when_zero.to_string()]),
        other => Err(format!(
            "`{other} : {name}` -- only `1` or `0` is valid here"
        )),
    }
}

/// Recognizes `Rn + k` / `Rn - k` as written in an `Imm`'s raw text
/// (Requirement 8), returning the register, whether it's addition, and
/// the magnitude `k`. `k` is a genuine compile-time constant MOL1802
/// itself must evaluate here (unlike every other `Imm`, which is always
/// opaque text) since the instruction *count* below depends on its size.
fn parse_reg_plus_const(text: &str) -> Option<(RegRef, bool, i64)> {
    let text = text.trim();
    let idx = text.find(['+', '-'])?;
    let reg = crate::parser::parse_register_operand(text[..idx].trim())?;
    let is_add = text.as_bytes()[idx] == b'+';
    let k = parse_const(text[idx + 1..].trim())?;
    Some((reg, is_add, k))
}

/// A plain compile-time integer literal: decimal, `#` + hex digits (this
/// project's convention), or `0x`/`0X` + hex digits.
fn parse_const(text: &str) -> Option<i64> {
    if let Some(hex) = text.strip_prefix('#') {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok();
    }
    text.parse().ok()
}

/// `Rn + k : Rn` / `Rn - k : Rn` (Requirement 8.1/8.2/8.4): repeated
/// `INC`/`DEC` at or below [`INC_DEC_THRESHOLD`], the general 16-bit
/// add/subtract through `D` above it, nothing for `k == 0`.
fn lower_constant_arithmetic(reg: RegRef, is_add: bool, k: i64) -> Vec<String> {
    if k == 0 {
        return Vec::new();
    }
    let name = reg.asm_name();
    if k.unsigned_abs() <= u64::from(INC_DEC_THRESHOLD) {
        let step = if is_add { "INC" } else { "DEC" };
        return vec![format!("{step} {name}"); k.unsigned_abs() as usize];
    }
    let low = (k & 0xFF) as u8;
    let high = ((k >> 8) & 0xFF) as u8;
    let (add_op, carry_op) = if is_add {
        ("ADI", "ADCI")
    } else {
        ("SMI", "SDBI")
    };
    vec![
        format!("GLO {name}"),
        format!("{add_op} {low}"),
        format!("PLO {name}"),
        format!("GHI {name}"),
        format!("{carry_op} {high}"),
        format!("PHI {name}"),
    ]
}

/// `Rm[.h] + Rn[.h] : Rn[.h]` / `Rm[.h] - Rn[.h] : Rn[.h]` (Requirement 9):
/// one `.0`-only (or `.1`-only) step if `dest` is a `RegHalf`, or the
/// full 16-bit carry-propagating pair of steps if `dest` is a plain
/// `Reg`. Uses `xtrack` in *insert, never error* mode for `SEX R6`
/// (Requirement 9.1) -- unlike the `STK` chaining sugar, which never
/// touches `SEX` at all.
fn lower_register_arithmetic(
    left: &Operand,
    is_add: bool,
    dest: &Operand,
    output_so_far: &[EmittedLine],
) -> Vec<EmittedLine> {
    let mut out = sex_if_needed(output_so_far, SCRATCH_REG, &[]);
    let lines: Vec<String> = match dest {
        Operand::RegHalf(dest_reg, half) => {
            let left_reg = register_of(left);
            arithmetic_step(left_reg, *dest_reg, *half, is_add, false)
        }
        Operand::Reg(dest_reg) => {
            let left_reg = register_of(left);
            let mut lines = arithmetic_step(left_reg, *dest_reg, Half::Low, is_add, false);
            lines.extend(arithmetic_step(
                left_reg,
                *dest_reg,
                Half::High,
                is_add,
                true,
            ));
            lines
        }
        _ => Vec::new(),
    };
    out.extend(lines.into_iter().map(EmittedLine::new));
    out
}

fn register_of(op: &Operand) -> RegRef {
    match op {
        Operand::Reg(r) | Operand::RegHalf(r, _) => *r,
        _ => unreachable!("register_arithmetic_chain only matches Reg/RegHalf operands"),
    }
}

/// One stage/operate/store step through the `R6` scratch byte. Addition
/// stages `left` and accumulates into `dest` (order-independent);
/// subtraction stages `dest` and stages `left` into `D` instead, per
/// design.md section 6's note that `SM`/`SMB` compute `D - M(R(X))`, not
/// the reverse. `with_carry` selects `ADC`/`SMB` (the high-byte step of
/// a full 16-bit operation, Requirement 9.4) over `ADD`/`SM`.
fn arithmetic_step(
    left: RegRef,
    dest: RegRef,
    half: Half,
    is_add: bool,
    with_carry: bool,
) -> Vec<String> {
    let (get, put) = half_mnemonics(half);
    let op = match (is_add, with_carry) {
        (true, false) => "ADD",
        (true, true) => "ADC",
        (false, false) => "SM",
        (false, true) => "SMB",
    };
    let (staged, loaded) = if is_add { (left, dest) } else { (dest, left) };
    vec![
        format!("{get} {}", staged.asm_name()),
        format!("STR {}", SCRATCH_REG.asm_name()),
        format!("{get} {}", loaded.asm_name()),
        op.to_string(),
        format!("{put} {}", dest.asm_name()),
    ]
}

/// The one-time `R6` initialization sequence (Requirement 9.2), emitted
/// once at the very top of the output (by the re-merge step, task 9) if
/// any register-to-register arithmetic statement appears anywhere in the
/// source.
pub(crate) fn scratch_register_init() -> Vec<String> {
    vec![
        format!("LDI HIGH({SCRATCH_LABEL})"),
        format!("PHI {}", SCRATCH_REG.asm_name()),
        format!("LDI LOW({SCRATCH_LABEL})"),
        format!("PLO {}", SCRATCH_REG.asm_name()),
    ]
}

/// The scratch byte declaration appended once at the end of the output
/// (Requirement 9.2/9.3, by the re-merge step, task 9), if needed.
pub(crate) fn scratch_byte_declaration() -> String {
    format!("{SCRATCH_LABEL}: DB 0")
}

/// A conditional (`Some`) or bare (`None`) branch (Requirement 5.1/5.2).
/// Branches never touch `X`, so there's no `xtrack` lookback here.
pub(crate) fn lower_branch(
    cond: Option<Condition>,
    len: BranchLength,
    target: &str,
) -> Vec<String> {
    vec![format!("{} {target}", branch_mnemonic(cond, len))]
}

fn branch_mnemonic(cond: Option<Condition>, len: BranchLength) -> String {
    let prefix = if matches!(len, BranchLength::Long) {
        "LB"
    } else {
        "B"
    };
    match cond {
        None => format!("{prefix}R"),
        Some(Condition::EfEq(k, true)) => format!("{prefix}{k}"),
        Some(Condition::EfEq(k, false)) => format!("{prefix}N{k}"),
        Some(Condition::DZero(true)) => format!("{prefix}Z"),
        Some(Condition::DZero(false)) => format!("{prefix}NZ"),
        Some(Condition::DfEq(true)) => format!("{prefix}DF"),
        Some(Condition::DfEq(false)) => format!("{prefix}NF"),
        Some(Condition::QEq(true)) => format!("{prefix}Q"),
        Some(Condition::QEq(false)) => format!("{prefix}NQ"),
    }
}

/// `list:STK` / `STK:list` (the `STK` chaining sugar): pushes or pops
/// `D`/full-register elements left to right, *never* touching `SEX`.
/// `STK` is always assumed to already be `R2`, set by a `SEX R2` the
/// developer writes themselves -- unlike every other `X`-sensitive
/// sugar in this language, this one neither errors nor inserts over a
/// missing `SEX`. The developer is trusted completely here.
pub(crate) fn lower_stack(elements: &[StackElement], direction: StackDirection) -> Vec<String> {
    match direction {
        StackDirection::Push => lower_stack_push(elements),
        StackDirection::Pop => lower_stack_pop(elements),
    }
}

/// Push order, left to right: `D` is one `STXD`; a full register is its
/// high byte then its low byte, each via `G__`/`STXD` (high pushed
/// first, so it ends up deepest; low pushed second, so it's on top).
fn lower_stack_push(elements: &[StackElement]) -> Vec<String> {
    let mut out = Vec::new();
    for el in elements {
        match el {
            StackElement::D => out.push("STXD".to_string()),
            StackElement::Reg(r) => {
                out.push(format!("GHI {}", r.asm_name()));
                out.push("STXD".to_string());
                out.push(format!("GLO {}", r.asm_name()));
                out.push("STXD".to_string());
            }
        }
    }
    out
}

/// One byte read during a `STK` pop: straight into `D` (a `D` element),
/// or into a register's low or high half (a full-register element,
/// which reads low then high -- the mirror image of the push order
/// above, since the low byte is the one that ended up on top).
enum PopRead {
    D,
    Low(RegRef),
    High(RegRef),
}

/// Pop order, left to right, flattened to one read per byte: a single
/// `IRX` first (the push side never left `X` pointing at a free byte to
/// read), then one `LDXA`/`PLO`/`PHI` per byte -- except the very last
/// byte read overall, which uses `LDX` (not `LDXA`), matching this
/// project's "full descending, next-free" stack convention.
fn lower_stack_pop(elements: &[StackElement]) -> Vec<String> {
    let mut reads = Vec::new();
    for el in elements {
        match el {
            StackElement::D => reads.push(PopRead::D),
            StackElement::Reg(r) => {
                reads.push(PopRead::Low(*r));
                reads.push(PopRead::High(*r));
            }
        }
    }
    let mut out = vec!["IRX".to_string()];
    let last = reads.len().saturating_sub(1);
    for (i, read) in reads.iter().enumerate() {
        out.push(if i == last {
            "LDX".to_string()
        } else {
            "LDXA".to_string()
        });
        match read {
            PopRead::D => {}
            PopRead::Low(r) => out.push(format!("PLO {}", r.asm_name())),
            PopRead::High(r) => out.push(format!("PHI {}", r.asm_name())),
        }
    }
    out
}

/// Inserts `SEX <reg>` unless `X` is already known to be `reg`, either
/// because the immediately preceding line is `SEX <reg>` itself, or
/// because it's one of `implied_by` (mnemonics that, in this language's
/// own generated output, only ever appear once `X` is already `reg`).
fn sex_if_needed(
    output_so_far: &[EmittedLine],
    reg: RegRef,
    implied_by: &[&str],
) -> Vec<EmittedLine> {
    if x_still_at(output_so_far, reg, implied_by) {
        Vec::new()
    } else {
        vec![EmittedLine::new(format!("SEX {}", reg.asm_name()))]
    }
}

/// True if `text` is written in this project's established `#NN`
/// convention for an explicit single byte (exactly 1-2 hex digits after
/// `#`) -- the one case MOL1802 can tell is 8-bit-only without evaluating
/// the expression (Requirement 3.2). A bare decimal/symbolic literal
/// (`5`, `FOO`) is deliberately treated as width-ambiguous and allowed
/// through to the 16-bit split.
fn looks_like_an_explicit_byte(text: &str) -> bool {
    let Some(digits) = text.trim().strip_prefix('#') else {
        return false;
    };
    (1..=2).contains(&digits.len()) && digits.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::RegRef;

    fn reg(n: u8) -> RegRef {
        RegRef(n)
    }

    fn imm(s: &str) -> Operand {
        Operand::Imm(s.to_string())
    }

    #[test]
    fn d_to_register_half() {
        assert_eq!(
            lower_chain(&Operand::D, &Operand::RegHalf(reg(3), Half::Low)),
            Ok(vec!["PLO R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::D, &Operand::RegHalf(reg(3), Half::High)),
            Ok(vec!["PHI R3".to_string()])
        );
    }

    #[test]
    fn register_half_to_d() {
        assert_eq!(
            lower_chain(&Operand::RegHalf(reg(3), Half::Low), &Operand::D),
            Ok(vec!["GLO R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::RegHalf(reg(3), Half::High), &Operand::D),
            Ok(vec!["GHI R3".to_string()])
        );
    }

    #[test]
    fn immediate_to_d_and_halves() {
        assert_eq!(
            lower_chain(&imm("5"), &Operand::D),
            Ok(vec!["LDI 5".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("5"), &Operand::RegHalf(reg(3), Half::Low)),
            Ok(vec!["LDI 5".to_string(), "PLO R3".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("5"), &Operand::RegHalf(reg(3), Half::High)),
            Ok(vec!["LDI 5".to_string(), "PHI R3".to_string()])
        );
    }

    #[test]
    fn immediate_16bit_to_full_register() {
        assert_eq!(
            lower_chain(&imm("BUFF"), &Operand::Reg(reg(3))),
            Ok(vec![
                "LDI HIGH(BUFF)".to_string(),
                "PHI R3".to_string(),
                "LDI LOW(BUFF)".to_string(),
                "PLO R3".to_string(),
            ])
        );
    }

    #[test]
    fn explicit_byte_literal_to_full_register_is_rejected() {
        assert!(lower_chain(&imm("#FF"), &Operand::Reg(reg(3))).is_err());
        assert!(lower_chain(&imm("#5"), &Operand::Reg(reg(3))).is_err());
        // A four-digit `#AAAA` is explicitly 16-bit, so it's fine.
        assert!(lower_chain(&imm("#1234"), &Operand::Reg(reg(3))).is_ok());
        // A bare decimal is width-ambiguous, so it's allowed too.
        assert!(lower_chain(&imm("5"), &Operand::Reg(reg(3))).is_ok());
    }

    #[test]
    fn register_to_register() {
        assert_eq!(
            lower_chain(&Operand::Reg(reg(3)), &Operand::Reg(reg(4))),
            Ok(vec![
                "GHI R3".to_string(),
                "PHI R4".to_string(),
                "GLO R3".to_string(),
                "PLO R4".to_string(),
            ])
        );
    }

    #[test]
    fn mem_plain() {
        assert_eq!(
            lower_chain(&Operand::D, &Operand::Mem(reg(3), Adjust::None)),
            Ok(vec!["STR R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::Mem(reg(3), Adjust::None), &Operand::D),
            Ok(vec!["LDN R3".to_string()])
        );
    }

    #[test]
    fn mem_post_inc() {
        assert_eq!(
            lower_chain(&Operand::Mem(reg(3), Adjust::PostInc), &Operand::D),
            Ok(vec!["LDA R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::D, &Operand::Mem(reg(3), Adjust::PostInc)),
            Ok(vec!["STR R3".to_string(), "INC R3".to_string()])
        );
    }

    #[test]
    fn mem_post_dec() {
        assert_eq!(
            lower_chain(&Operand::Mem(reg(3), Adjust::PostDec), &Operand::D),
            Ok(vec!["LDN R3".to_string(), "DEC R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::D, &Operand::Mem(reg(3), Adjust::PostDec)),
            Ok(vec!["STR R3".to_string(), "DEC R3".to_string()])
        );
    }

    #[test]
    fn mem_pre_inc() {
        assert_eq!(
            lower_chain(&Operand::Mem(reg(3), Adjust::PreInc), &Operand::D),
            Ok(vec!["INC R3".to_string(), "LDN R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::D, &Operand::Mem(reg(3), Adjust::PreInc)),
            Ok(vec!["INC R3".to_string(), "STR R3".to_string()])
        );
    }

    #[test]
    fn mem_pre_dec() {
        assert_eq!(
            lower_chain(&Operand::Mem(reg(3), Adjust::PreDec), &Operand::D),
            Ok(vec!["DEC R3".to_string(), "LDN R3".to_string()])
        );
        assert_eq!(
            lower_chain(&Operand::D, &Operand::Mem(reg(3), Adjust::PreDec)),
            Ok(vec!["DEC R3".to_string(), "STR R3".to_string()])
        );
    }

    #[test]
    fn fanout_chain_matches_the_user_example() {
        let operands = vec![
            Operand::Imm("PAGE".to_string()),
            Operand::RegHalf(reg(2), Half::High),
            Operand::RegHalf(reg(4), Half::High),
            Operand::RegHalf(reg(5), Half::High),
        ];
        assert_eq!(
            lower_fanout_chain(&operands),
            Ok(vec![
                "LDI PAGE".to_string(),
                "PHI R2".to_string(),
                "PHI R4".to_string(),
                "PHI R5".to_string(),
            ])
        );
    }

    #[test]
    fn fanout_chain_from_d_needs_no_load_step() {
        let operands = vec![
            Operand::D,
            Operand::RegHalf(reg(3), Half::Low),
            Operand::RegHalf(reg(4), Half::Low),
        ];
        assert_eq!(
            lower_fanout_chain(&operands),
            Ok(vec!["PLO R3".to_string(), "PLO R4".to_string()])
        );
    }

    #[test]
    fn fanout_chain_from_memory_read() {
        let operands = vec![
            Operand::Mem(reg(6), Adjust::PostInc),
            Operand::RegHalf(reg(3), Half::High),
            Operand::RegHalf(reg(4), Half::High),
        ];
        assert_eq!(
            lower_fanout_chain(&operands),
            Ok(vec![
                "LDA R6".to_string(),
                "PHI R3".to_string(),
                "PHI R4".to_string(),
            ])
        );
    }

    #[test]
    fn fanout_chain_rejects_a_full_register_destination() {
        let operands = vec![
            Operand::Imm("5".to_string()),
            Operand::RegHalf(reg(2), Half::High),
            Operand::Reg(reg(4)),
        ];
        assert!(lower_fanout_chain(&operands).is_err());
    }

    #[test]
    fn fanout_chain_rejects_an_unsupported_source() {
        let operands = vec![
            Operand::Reg(reg(3)),
            Operand::RegHalf(reg(2), Half::High),
            Operand::RegHalf(reg(4), Half::High),
        ];
        assert!(lower_fanout_chain(&operands).is_err());
    }

    #[test]
    fn fanout_chain_rejects_port_and_df_q_ei() {
        let good_dest = Operand::RegHalf(reg(2), Half::High);
        assert!(lower_fanout_chain(&[
            Operand::Imm("5".to_string()),
            good_dest.clone(),
            Operand::Port("4".to_string())
        ])
        .is_err());
        assert!(
            lower_fanout_chain(&[Operand::Imm("5".to_string()), good_dest, Operand::Q]).is_err()
        );
    }

    #[test]
    fn mem_to_register_half_reads_through_d() {
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::None),
                &Operand::RegHalf(reg(3), Half::High)
            ),
            Ok(vec!["LDN R6".to_string(), "PHI R3".to_string()])
        );
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PostInc),
                &Operand::RegHalf(reg(3), Half::High)
            ),
            Ok(vec!["LDA R6".to_string(), "PHI R3".to_string()])
        );
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PostInc),
                &Operand::RegHalf(reg(3), Half::Low)
            ),
            Ok(vec!["LDA R6".to_string(), "PLO R3".to_string()])
        );
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PostDec),
                &Operand::RegHalf(reg(3), Half::Low)
            ),
            Ok(vec![
                "LDN R6".to_string(),
                "DEC R6".to_string(),
                "PLO R3".to_string(),
            ])
        );
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PreInc),
                &Operand::RegHalf(reg(3), Half::Low)
            ),
            Ok(vec![
                "INC R6".to_string(),
                "LDN R6".to_string(),
                "PLO R3".to_string(),
            ])
        );
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PreDec),
                &Operand::RegHalf(reg(3), Half::Low)
            ),
            Ok(vec![
                "DEC R6".to_string(),
                "LDN R6".to_string(),
                "PLO R3".to_string(),
            ])
        );
    }

    #[test]
    fn register_half_to_mem_writes_through_d() {
        assert_eq!(
            lower_chain(
                &Operand::RegHalf(reg(3), Half::High),
                &Operand::Mem(reg(6), Adjust::None)
            ),
            Ok(vec!["GHI R3".to_string(), "STR R6".to_string()])
        );
        assert_eq!(
            lower_chain(
                &Operand::RegHalf(reg(3), Half::Low),
                &Operand::Mem(reg(6), Adjust::PostInc)
            ),
            Ok(vec![
                "GLO R3".to_string(),
                "STR R6".to_string(),
                "INC R6".to_string(),
            ])
        );
        assert_eq!(
            lower_chain(
                &Operand::RegHalf(reg(3), Half::Low),
                &Operand::Mem(reg(6), Adjust::PostDec)
            ),
            Ok(vec![
                "GLO R3".to_string(),
                "STR R6".to_string(),
                "DEC R6".to_string(),
            ])
        );
        assert_eq!(
            lower_chain(
                &Operand::RegHalf(reg(3), Half::Low),
                &Operand::Mem(reg(6), Adjust::PreInc)
            ),
            Ok(vec![
                "GLO R3".to_string(),
                "INC R6".to_string(),
                "STR R6".to_string(),
            ])
        );
        assert_eq!(
            lower_chain(
                &Operand::RegHalf(reg(3), Half::Low),
                &Operand::Mem(reg(6), Adjust::PreDec)
            ),
            Ok(vec![
                "GLO R3".to_string(),
                "DEC R6".to_string(),
                "STR R6".to_string(),
            ])
        );
    }

    /// The exact pattern from the SCRT `CALL` trampoline (a real-world
    /// use this generalization unblocks): read the two bytes of a
    /// subroutine address out of inline memory, high byte first, each
    /// auto-advancing the pointer.
    #[test]
    fn scrt_address_unpack_matches_hand_written_ldaphi() {
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PostInc),
                &Operand::RegHalf(reg(3), Half::High)
            ),
            Ok(vec!["LDA R6".to_string(), "PHI R3".to_string()])
        );
        assert_eq!(
            lower_chain(
                &Operand::Mem(reg(6), Adjust::PostInc),
                &Operand::RegHalf(reg(3), Half::Low)
            ),
            Ok(vec!["LDA R6".to_string(), "PLO R3".to_string()])
        );
    }

    #[test]
    fn df_q_ei_writes() {
        assert_eq!(
            lower_chain(&imm("1"), &Operand::Df),
            Ok(vec!["SMI 0".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("0"), &Operand::Df),
            Ok(vec!["ADI 0".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("1"), &Operand::Q),
            Ok(vec!["SEQ".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("0"), &Operand::Q),
            Ok(vec!["REQ".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("1"), &Operand::Ei),
            Ok(vec!["RET".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("0"), &Operand::Ei),
            Ok(vec!["DIS".to_string()])
        );
    }

    #[test]
    fn df_q_ei_reject_anything_but_one_or_zero() {
        assert!(lower_chain(&imm("2"), &Operand::Df).is_err());
        assert!(lower_chain(&imm("FOO"), &Operand::Q).is_err());
        assert!(lower_chain(&Operand::D, &Operand::Ei).is_err());
    }

    #[test]
    fn no_op_assignment_emits_nothing() {
        assert_eq!(lower_chain(&Operand::D, &Operand::D), Ok(Vec::new()));
        assert_eq!(
            lower_chain(&Operand::Reg(reg(3)), &Operand::Reg(reg(3))),
            Ok(Vec::new())
        );
        assert_eq!(
            lower_chain(
                &Operand::RegHalf(reg(3), Half::Low),
                &Operand::RegHalf(reg(3), Half::Low)
            ),
            Ok(Vec::new())
        );
    }

    #[test]
    fn constant_add_at_or_below_threshold_uses_inc() {
        assert_eq!(
            lower_chain(&imm("R3 + 1"), &Operand::Reg(reg(3))),
            Ok(vec!["INC R3".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("R3 + 4"), &Operand::Reg(reg(3))),
            Ok(vec!["INC R3"; 4].into_iter().map(str::to_string).collect())
        );
    }

    #[test]
    fn constant_subtract_at_or_below_threshold_uses_dec() {
        assert_eq!(
            lower_chain(&imm("R3 - 1"), &Operand::Reg(reg(3))),
            Ok(vec!["DEC R3".to_string()])
        );
        assert_eq!(
            lower_chain(&imm("R3 - 4"), &Operand::Reg(reg(3))),
            Ok(vec!["DEC R3"; 4].into_iter().map(str::to_string).collect())
        );
    }

    #[test]
    fn constant_add_just_above_threshold_uses_the_general_form() {
        assert_eq!(
            lower_chain(&imm("R3 + 5"), &Operand::Reg(reg(3))),
            Ok(vec![
                "GLO R3".to_string(),
                "ADI 5".to_string(),
                "PLO R3".to_string(),
                "GHI R3".to_string(),
                "ADCI 0".to_string(),
                "PHI R3".to_string(),
            ])
        );
    }

    #[test]
    fn constant_subtract_just_above_threshold_uses_the_general_form() {
        assert_eq!(
            lower_chain(&imm("R3 - 5"), &Operand::Reg(reg(3))),
            Ok(vec![
                "GLO R3".to_string(),
                "SMI 5".to_string(),
                "PLO R3".to_string(),
                "GHI R3".to_string(),
                "SDBI 0".to_string(),
                "PHI R3".to_string(),
            ])
        );
    }

    #[test]
    fn constant_add_multi_byte_value() {
        assert_eq!(
            lower_chain(&imm("R3 + 0x100"), &Operand::Reg(reg(3))),
            Ok(vec![
                "GLO R3".to_string(),
                "ADI 0".to_string(),
                "PLO R3".to_string(),
                "GHI R3".to_string(),
                "ADCI 1".to_string(),
                "PHI R3".to_string(),
            ])
        );
    }

    #[test]
    fn constant_add_zero_emits_nothing() {
        assert_eq!(
            lower_chain(&imm("R3 + 0"), &Operand::Reg(reg(3))),
            Ok(Vec::new())
        );
    }

    #[test]
    fn constant_arithmetic_on_a_different_register_does_not_match() {
        // `R3 + 5` naming a register other than the destination isn't
        // this sugar at all; it falls through to the generic immediate
        // row (which will try, and fail, to load the literal text).
        assert!(lower_chain(&imm("R3 + 5"), &Operand::Reg(reg(4))).is_ok());
    }

    #[test]
    fn half_only_register_add_through_scratch() {
        let history = [EmittedLine::new("SEX R6")];
        assert_eq!(
            lower_chain_statement(
                &imm("R3.0 + R4.0"),
                &Operand::RegHalf(reg(4), Half::Low),
                &history
            ),
            Ok(vec![
                EmittedLine::new("GLO R3"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GLO R4"),
                EmittedLine::new("ADD"),
                EmittedLine::new("PLO R4"),
            ])
        );
    }

    #[test]
    fn half_only_register_subtract_stages_the_destination() {
        let history = [EmittedLine::new("SEX R6")];
        assert_eq!(
            lower_chain_statement(
                &imm("R3.0 - R4.0"),
                &Operand::RegHalf(reg(4), Half::Low),
                &history
            ),
            Ok(vec![
                EmittedLine::new("GLO R4"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GLO R3"),
                EmittedLine::new("SM"),
                EmittedLine::new("PLO R4"),
            ])
        );
    }

    #[test]
    fn full_16bit_register_add_propagates_carry() {
        let history = [EmittedLine::new("SEX R6")];
        assert_eq!(
            lower_chain_statement(&imm("R3 + R4"), &Operand::Reg(reg(4)), &history),
            Ok(vec![
                EmittedLine::new("GLO R3"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GLO R4"),
                EmittedLine::new("ADD"),
                EmittedLine::new("PLO R4"),
                EmittedLine::new("GHI R3"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GHI R4"),
                EmittedLine::new("ADC"),
                EmittedLine::new("PHI R4"),
            ])
        );
    }

    #[test]
    fn full_16bit_register_subtract_propagates_borrow() {
        let history = [EmittedLine::new("SEX R6")];
        assert_eq!(
            lower_chain_statement(&imm("R3 - R4"), &Operand::Reg(reg(4)), &history),
            Ok(vec![
                EmittedLine::new("GLO R4"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GLO R3"),
                EmittedLine::new("SM"),
                EmittedLine::new("PLO R4"),
                EmittedLine::new("GHI R4"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GHI R3"),
                EmittedLine::new("SMB"),
                EmittedLine::new("PHI R4"),
            ])
        );
    }

    #[test]
    fn register_arithmetic_inserts_sex_r6_when_needed() {
        assert_eq!(
            lower_chain_statement(&imm("R3 + R4"), &Operand::Reg(reg(4)), &[]),
            Ok(vec![
                EmittedLine::new("SEX R6"),
                EmittedLine::new("GLO R3"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GLO R4"),
                EmittedLine::new("ADD"),
                EmittedLine::new("PLO R4"),
                EmittedLine::new("GHI R3"),
                EmittedLine::new("STR R6"),
                EmittedLine::new("GHI R4"),
                EmittedLine::new("ADC"),
                EmittedLine::new("PHI R4"),
            ])
        );
    }

    #[test]
    fn is_register_arithmetic_chain_detects_both_forms() {
        assert!(is_register_arithmetic_chain(
            &imm("R3.0 + R4.0"),
            &Operand::RegHalf(reg(4), Half::Low)
        ));
        assert!(is_register_arithmetic_chain(
            &imm("R3 + R4"),
            &Operand::Reg(reg(4))
        ));
        assert!(!is_register_arithmetic_chain(
            &imm("R3 + 5"),
            &Operand::Reg(reg(3))
        ));
        assert!(!is_register_arithmetic_chain(
            &Operand::D,
            &Operand::Reg(reg(4))
        ));
    }

    #[test]
    fn scratch_init_and_byte_declaration_text() {
        assert_eq!(
            scratch_register_init(),
            vec![
                "LDI HIGH(MOL1802_SCRATCH)".to_string(),
                "PHI R6".to_string(),
                "LDI LOW(MOL1802_SCRATCH)".to_string(),
                "PLO R6".to_string(),
            ]
        );
        assert_eq!(scratch_byte_declaration(), "MOL1802_SCRATCH: DB 0");
    }

    #[test]
    fn unsupported_shape_is_an_error() {
        assert!(lower_chain(&Operand::Df, &Operand::D).is_err());
        assert!(lower_chain(&Operand::Ef(1), &Operand::D).is_err());
    }

    fn port(n: &str) -> Operand {
        Operand::Port(n.to_string())
    }

    #[test]
    fn inp_with_a_preceding_sex() {
        let history = [EmittedLine::new("SEX R3")];
        assert_eq!(
            lower_chain_statement(&port("4"), &Operand::D, &history),
            Ok(vec![EmittedLine::new("INP 4")])
        );
    }

    #[test]
    fn out_with_a_preceding_sex() {
        let history = [EmittedLine::new("SEX R3")];
        assert_eq!(
            lower_chain_statement(&Operand::D, &port("4"), &history),
            Ok(vec![EmittedLine::new("OUT 4")])
        );
    }

    #[test]
    fn inp_out_without_a_preceding_sex_is_an_error() {
        assert!(lower_chain_statement(&port("4"), &Operand::D, &[]).is_err());
        let history = [EmittedLine::new("LDI 5")];
        assert!(lower_chain_statement(&port("4"), &Operand::D, &history).is_err());
    }

    #[test]
    fn inp_out_without_a_preceding_sex_two_lines_back_is_still_an_error() {
        let history = [EmittedLine::new("SEX R3"), EmittedLine::new("LDI 5")];
        assert!(lower_chain_statement(&port("4"), &Operand::D, &history).is_err());
    }

    #[test]
    fn port_paired_with_anything_but_d_is_an_error() {
        let history = [EmittedLine::new("SEX R3")];
        assert!(lower_chain_statement(&port("4"), &Operand::Reg(reg(3)), &history).is_err());
        assert!(
            lower_chain_statement(&Operand::RegHalf(reg(3), Half::Low), &port("4"), &history)
                .is_err()
        );
    }

    #[test]
    fn non_port_chains_route_through_the_requirement_3_table() {
        assert_eq!(
            lower_chain_statement(&Operand::D, &Operand::RegHalf(reg(3), Half::Low), &[]),
            Ok(vec![EmittedLine::new("PLO R3")])
        );
    }

    #[test]
    fn branch_table_covers_every_condition_both_lengths() {
        let cases: &[(Option<Condition>, &str, &str)] = &[
            (None, "BR", "LBR"),
            (Some(Condition::EfEq(1, false)), "BN1", "LBN1"),
            (Some(Condition::EfEq(1, true)), "B1", "LB1"),
            (Some(Condition::EfEq(2, false)), "BN2", "LBN2"),
            (Some(Condition::EfEq(2, true)), "B2", "LB2"),
            (Some(Condition::EfEq(3, false)), "BN3", "LBN3"),
            (Some(Condition::EfEq(3, true)), "B3", "LB3"),
            (Some(Condition::EfEq(4, false)), "BN4", "LBN4"),
            (Some(Condition::EfEq(4, true)), "B4", "LB4"),
            (Some(Condition::DZero(true)), "BZ", "LBZ"),
            (Some(Condition::DZero(false)), "BNZ", "LBNZ"),
            (Some(Condition::DfEq(false)), "BNF", "LBNF"),
            (Some(Condition::DfEq(true)), "BDF", "LBDF"),
            (Some(Condition::QEq(false)), "BNQ", "LBNQ"),
            (Some(Condition::QEq(true)), "BQ", "LBQ"),
        ];
        for (cond, short, long) in cases {
            assert_eq!(
                lower_branch(*cond, BranchLength::Short, "LOOP"),
                vec![format!("{short} LOOP")],
                "short form of {cond:?}"
            );
            assert_eq!(
                lower_branch(*cond, BranchLength::Long, "LOOP"),
                vec![format!("{long} LOOP")],
                "long form of {cond:?}"
            );
        }
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn single_register_stack_push_and_pop() {
        assert_eq!(
            lower_stack(&[StackElement::Reg(reg(10))], StackDirection::Push),
            strs(&["GHI R10", "STXD", "GLO R10", "STXD"])
        );
        assert_eq!(
            lower_stack(&[StackElement::Reg(reg(10))], StackDirection::Pop),
            strs(&["IRX", "LDXA", "PLO R10", "LDX", "PHI R10"])
        );
    }

    #[test]
    fn single_d_stack_push_and_pop() {
        // Never a SEX, unlike the old PUSH D/POP D -- STK is always
        // assumed to already be R2.
        assert_eq!(
            lower_stack(&[StackElement::D], StackDirection::Push),
            strs(&["STXD"])
        );
        assert_eq!(
            lower_stack(&[StackElement::D], StackDirection::Pop),
            strs(&["IRX", "LDX"])
        );
    }

    #[test]
    fn comma_chained_stack_push_matches_the_worked_example() {
        let elements = [
            StackElement::Reg(reg(10)),
            StackElement::Reg(reg(9)),
            StackElement::Reg(reg(8)),
            StackElement::Reg(reg(7)),
        ];
        assert_eq!(
            lower_stack(&elements, StackDirection::Push),
            strs(&[
                "GHI R10", "STXD", "GLO R10", "STXD", "GHI R9", "STXD", "GLO R9", "STXD", "GHI R8",
                "STXD", "GLO R8", "STXD", "GHI R7", "STXD", "GLO R7", "STXD",
            ])
        );
    }

    #[test]
    fn comma_chained_stack_pop_matches_the_worked_example() {
        // Mirrors the push order above: R7 (pushed last, so on top) is
        // popped first.
        let elements = [
            StackElement::Reg(reg(7)),
            StackElement::Reg(reg(8)),
            StackElement::Reg(reg(9)),
            StackElement::Reg(reg(10)),
        ];
        assert_eq!(
            lower_stack(&elements, StackDirection::Pop),
            strs(&[
                "IRX", "LDXA", "PLO R7", "LDXA", "PHI R7", "LDXA", "PLO R8", "LDXA", "PHI R8",
                "LDXA", "PLO R9", "LDXA", "PHI R9", "LDXA", "PLO R10", "LDX", "PHI R10",
            ])
        );
    }

    #[test]
    fn mixed_d_and_register_stack_list() {
        let elements = [StackElement::Reg(reg(3)), StackElement::D];
        assert_eq!(
            lower_stack(&elements, StackDirection::Push),
            strs(&["GHI R3", "STXD", "GLO R3", "STXD", "STXD"])
        );
        assert_eq!(
            lower_stack(&elements, StackDirection::Pop),
            strs(&["IRX", "LDXA", "PLO R3", "LDXA", "PHI R3", "LDX"])
        );
    }
}
