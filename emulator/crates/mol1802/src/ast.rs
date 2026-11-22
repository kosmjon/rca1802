//! The MOL1802 abstract syntax tree (`design.md`, section "2. AST").
//!
//! Chains are always exactly two operands: retiring `(RX)` (see
//! `requirements.md`, Decisions) means nothing in this grammar needs a
//! third chain element, so there is no `XIndirect` operand variant.

/// A 1802 register reference, `R0`-`R15`. `RA`-`RF` are accepted as input
/// spellings for 10-15 (matching `asm1802`'s own predefined register
/// names), but are normalized to the numeric form on output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegRef(pub u8);

impl RegRef {
    /// The canonical `asm1802` spelling emitted into generated output
    /// (always decimal, e.g. `R10` rather than `RA`).
    pub fn asm_name(self) -> String {
        format!("R{}", self.0)
    }
}

/// Which byte of a 16-bit register, for `Rn.0` / `Rn.1` (`Rn'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Half {
    /// `Rn.0`.
    Low,
    /// `Rn.1` or `Rn'`.
    High,
}

/// How `(Rn)` adjusts the register around the memory access (Requirement
/// 2.5). None of these touch `X`; all four are valid both as a source and
/// as a destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adjust {
    /// `(Rn)`.
    None,
    /// `(Rn++)`: access, then increment.
    PostInc,
    /// `(Rn--)`: access, then decrement.
    PostDec,
    /// `(++Rn)`: increment, then access.
    PreInc,
    /// `(--Rn)`: decrement, then access.
    PreDec,
}

/// A numeric or label expression, copied through to `asm1802` unevaluated
/// (Requirement 1.3): just the source substring, never parsed as
/// arithmetic by MOL1802 itself.
pub type Expr = String;

/// One operand of a chain (Requirement 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// `D`, the accumulator.
    D,
    /// `DF`, the data flag.
    Df,
    /// `Rn`, a full 16-bit register.
    Reg(RegRef),
    /// `Rn.0` / `Rn.1` / `Rn'`.
    RegHalf(RegRef, Half),
    /// `(Rn)` and its adjust variants: direct memory, never through `X`.
    Mem(RegRef, Adjust),
    /// `PORT(n)`, `n` = 1-7 (the only ports `INP`/`OUT` opcodes can encode).
    /// Valid only in the two chain shapes defined by Requirement 4.
    Port(Expr),
    /// `EF1`-`EF4`.
    Ef(u8),
    /// `Q`, the Q output line.
    Q,
    /// `EI`: only valid as a chain tail (`1:EI` / `0:EI`).
    Ei,
    /// Anything else: an opaque expression, handed to `asm1802` verbatim.
    Imm(Expr),
}

/// A branch condition (Requirement 5.1), one variant per table row. The
/// trailing `bool` on each variant is `true` for the `=1?`/`<>0?` row and
/// `false` for the `=0?` row (design.md section 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// `EFk=1?` (`true`) or `EFk=0?` (`false`), `k` = 1-4.
    EfEq(u8, bool),
    /// `D=0?` (`true`) or `D<>0?` (`false`).
    DZero(bool),
    /// `DF=1?` (`true`) or `DF=0?` (`false`).
    DfEq(bool),
    /// `Q=1?` (`true`) or `Q=0?` (`false`).
    QEq(bool),
}

/// Short (`/LABEL`) vs. long (`//LABEL`) branch (Requirement 5.1/5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchLength {
    Short,
    Long,
}

/// `(token, condition)` pairs for Requirement 5.1's left-hand-side table.
/// `=1?`/`<>0?` rows are listed before their `=0?` counterpart purely for
/// readability; token texts never collide as prefixes of one another, so
/// search order doesn't matter functionally.
pub(crate) const CONDITION_TOKENS: &[(&str, Condition)] = &[
    ("EF1=1?", Condition::EfEq(1, true)),
    ("EF1=0?", Condition::EfEq(1, false)),
    ("EF2=1?", Condition::EfEq(2, true)),
    ("EF2=0?", Condition::EfEq(2, false)),
    ("EF3=1?", Condition::EfEq(3, true)),
    ("EF3=0?", Condition::EfEq(3, false)),
    ("EF4=1?", Condition::EfEq(4, true)),
    ("EF4=0?", Condition::EfEq(4, false)),
    ("D<>0?", Condition::DZero(false)),
    ("D=0?", Condition::DZero(true)),
    ("DF=1?", Condition::DfEq(true)),
    ("DF=0?", Condition::DfEq(false)),
    ("Q=1?", Condition::QEq(true)),
    ("Q=0?", Condition::QEq(false)),
];

/// One element of a `STK` chain list: `D` (one byte) or a full register
/// (two bytes, high then low on push). The only things valid in a `STK`
/// chain's comma list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackElement {
    D,
    Reg(RegRef),
}

/// Which way a `STK` chain moves data: `list:STK` pushes, `STK:list`
/// pops. `STK` is always assumed to already be `R2` as set by a `SEX R2`
/// the developer writes themselves -- unlike every other `X`-sensitive
/// sugar in this language, a `STK` chain never inserts a `SEX` and never
/// errors over a missing one. The developer is trusted completely here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackDirection {
    Push,
    Pop,
}

/// A parsed MOL1802 statement (`design.md`, section 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Statement {
    /// `A : B`. Always exactly two operands.
    Chain(Vec<Operand>),
    /// A conditional (`Some`) or bare (`None`) branch.
    Branch {
        cond: Option<Condition>,
        len: BranchLength,
        target: String,
    },
    /// `list:STK` (push) or `STK:list` (pop), `list` a comma-separated
    /// run of `D`/`Rn` elements, pushed or popped left to right.
    Stack {
        elements: Vec<StackElement>,
        direction: StackDirection,
    },
}
