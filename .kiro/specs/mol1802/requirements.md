# Requirements: MOL1802 (Machine-Oriented Language for the RCA 1802)

## Introduction

MOL1802 is a higher-level, more expressive notation that transpiles to `asm1802`
source (the project's existing RCA 1802 cross-assembler). It follows the general
1960s-70s "Machine Oriented Language" tradition (a substitute for writing raw
assembly that compiles down to it, e.g. MOL-360, MOL-940) rather than any specific
historical 1802 tool -- there is no known prior "MOL1802"; this is a new design,
sketched first in `notes.md` and expanded through design discussion captured here.

The central idea, from `notes.md`: a register-transfer assignment
`SOURCE : DESTINATION` (data flows left to right), plus sugar for conditionals,
branches, and small arithmetic, that the compiler lowers to the real 1802
instruction(s) that realize it.

### Decisions

- **Transpile, don't re-implement.** MOL1802 emits `asm1802` source text and hands
  off to it for everything else: expression evaluation, labels, `ORG`, `EQU`,
  `INCLUDE`, conditional assembly, and HEX/listing/binary output. MOL1802's own
  grammar covers only the register-transfer/condition/branch sugar; literal
  numeric/label expressions are passed through to `asm1802`'s evaluator unchanged.
- **Directives and labels pass through unchanged.** Lines that are already
  `asm1802` directives (`ORG`, `EQU`, `DB`, `DW`, `DS`, `ALIGN`, `INCLUDE`,
  `DEFINE`/`IFDEF`/`IFNDEF`/`ELSE`/`ENDIF`, `TITLE`/`PAGE`, comments) and label
  definitions are not part of MOL1802's own grammar -- they are recognized and
  copied straight into the output. A source file can freely mix MOL1802
  statements with raw `asm1802` lines (escape hatch for anything not covered by
  sugar, matching how a "higher-level" language in this tradition is meant to work).
- **`(Rn)` means direct memory, nothing to do with `X`.** `(Rn)` and its
  increment/decrement forms are plain register-indexed memory access
  (`LDN`/`STR`/`LDA`/`INC`/`DEC`) and never touch `X` or need a `SEX`. This is
  the common case and stays simple and free of any register-plumbing concerns.
- **`X` only matters for `INP`/`OUT`.** Those are the only two instructions
  that can *only* address memory through whatever `X` currently designates, with
  no way to name a different register in the opcode. MOL1802 puts all of the
  "does `X` already point where I need it to" complexity there, and nowhere
  else -- see Requirement 4. The register-to-register arithmetic sugar also
  needs `X`, but internally and invisibly, always to one fixed register
  (`R6`) the developer never has to name. `STK` (Requirement 7) needs `X` to
  already be `R2` too, but unlike everything else here, it never checks or
  manages this itself -- the developer sets `X` and is fully responsible for
  it.
- **No semantic safety net beyond what's written**, except where getting it
  wrong would silently corrupt memory with no other symptom (`INP`/`OUT`'s `X`
  requirement, Requirement 4.4) -- that one case is a compile error, not a
  guess, same posture as `asm1802` itself everywhere else.
- **Register roles** (fixed by convention, from `notes.md`, reused by sugar that
  needs a dedicated register): `R1` interrupt PC, `R2` stack pointer, `R3` normal
  PC, `R4`/`R5` SCRT `CALL`/`RETURN` (already native `asm1802` mnemonics), `R6`
  MOL1802's own register-to-register arithmetic scratch pointer (Requirement 9).

## Glossary

- **MOL1802 statement**: one register-transfer assignment, conditional/branch,
  or sugar form (`STK`/`CALL`/`RETURN`/etc.) in the new grammar.
- **Pass-through line**: an `asm1802` directive, label, or raw instruction line,
  copied to the output unchanged.
- **Chain**: a statement with more than one `:`-separated destination
  (`A : B : C`), read left to right as a pipeline.
- **Hardware idiom match**: a chain that exactly matches what one real 1802
  instruction already does (e.g. `INP n` writing to both `D` and memory as a
  single real instruction), compiled to that one instruction.
- **`(Rn)`**: memory addressed directly by register `Rn` (`LDN`/`STR`/`LDA`),
  never through `X`.

## Requirements

### 1. Toolchain integration

**User story:** As a developer, I want MOL1802 source to become a normal
`asm1802` input, so the existing assembler, tests, and tooling keep working.

1. The compiler SHALL take a MOL1802 source file and produce `asm1802`-compatible
   assembly text.
2. Lines recognized as `asm1802` directives, labels, or raw instructions SHALL be
   copied to the output unchanged (pass-through).
3. Any numeric or label expression appearing inside a MOL1802 statement (an
   immediate, a port number, a branch target) SHALL be copied into the generated
   operand text unevaluated, so `asm1802`'s own expression evaluator resolves it
   (constants, `LOW`/`HIGH`, arithmetic, forward references, all work exactly as
   they do in hand-written `asm1802` source).
4. The compiler SHALL report an error naming the source line and reason for any
   MOL1802 statement it cannot lower to valid `asm1802` output, rather than
   emitting invalid or silently-wrong assembly.
5. The generated assembly SHALL assemble with `asm1802` without errors whenever
   the MOL1802 input is well-formed per these requirements (verified by the test
   suite assembling every generated example, not merely parsing the MOL1802
   source).

### 2. Operands

**User story:** As a developer, I want to name every value a 1802 instruction
can read or write, consistently, so the assignment grammar (Requirement 3) has
a complete set of things to assign between.

1. `D` SHALL denote the accumulator.
2. `DF` SHALL denote the data flag; readable as a condition (Requirement 6) and
   writable as `1:DF` (compiles to `SMI 0`, forcing `DF=1`) or `0:DF` (compiles
   to `ADI 0`, forcing `DF=0`).
3. `Rn` (`R0`-`R15`, or `RA`-`RF` for 10-15, matching `asm1802`'s existing
   built-in register names) SHALL denote a full 16-bit register.
4. `Rn.0` and `Rn.1` SHALL denote the low and high byte of `Rn` respectively.
   `Rn'` SHALL be accepted as a synonym for `Rn.1` (the `notes.md` spelling).
5. `(Rn)` SHALL denote memory addressed directly by register `Rn`, compiling to
   `LDN`/`STR`/`LDA`/`INC`/`DEC` as needed (Requirement 3) -- never through `X`,
   never requiring a `SEX`. `(Rn++)` and `(Rn--)` SHALL denote the same with
   `Rn` incremented/decremented after the access; `(++Rn)` and `(--Rn)` SHALL
   denote the same with `Rn` adjusted before the access. All four
   adjustment forms are valid in both directions (as a source or a
   destination) since every one of them lowers to ordinary direct-register
   instructions with no hardware restriction to work around.
6. `PORT(n)` (`n` = 1-7) SHALL denote an I/O port, valid only in the two chain
   shapes defined by Requirement 4.
7. `EF1`, `EF2`, `EF3`, `EF4`, and `Q` SHALL denote the corresponding flag/output
   lines, readable as conditions (Requirement 6). `Q` SHALL additionally be a
   valid assignment destination: `1:Q` compiles to `SEQ`, `0:Q` to `REQ`.
8. `1:EI` and `0:EI` SHALL compile to `RET` and `DIS` respectively. The language
   reference SHALL document that both also pop/restore `X`,`P` from the stack
   pointed to by `R2` as a real hardware side effect (matching how every
   interrupt handler already transcribed into this project's sample code uses
   them) -- this is not hidden or special-cased away.

### 3. Register-transfer assignment

**User story:** As a developer, I want to write `SOURCE : DESTINATION` and have
the compiler choose the real instruction(s), instead of hand-tracking which of
`GLO`/`GHI`/`PLO`/`PHI`/`LDI`/etc. applies.

1. For each `(SOURCE, DESTINATION)` combination listed in the table below, the
   compiler SHALL emit exactly the instruction(s) shown:

   | SOURCE | DESTINATION | Emits |
   |---|---|---|
   | `D` | `Rn.0` | `PLO Rn` |
   | `D` | `Rn.1` | `PHI Rn` |
   | `Rn.0` | `D` | `GLO Rn` |
   | `Rn.1` | `D` | `GHI Rn` |
   | immediate (8-bit range) | `D` | `LDI <imm>` |
   | immediate (8-bit range) | `Rn.0` / `Rn.1` | `LDI <imm>` / `PLO Rn` or `PHI Rn` |
   | immediate (16-bit range) | `Rn` | `LDI HIGH(<imm>)` / `PHI Rn` / `LDI LOW(<imm>)` / `PLO Rn` |
   | `Rm` | `Rn` | `GHI Rm` / `PHI Rn` / `GLO Rm` / `PLO Rn` |
   | `D` | `(Rn)` | `STR Rn` |
   | `(Rn)` | `D` | `LDN Rn` |
   | `(Rn++)` | `D` | `LDA Rn` |
   | `D` | `(Rn++)` | `STR Rn` / `INC Rn` |
   | `(Rn--)` | `D` | `LDN Rn` / `DEC Rn` |
   | `D` | `(Rn--)` | `STR Rn` / `DEC Rn` |
   | `(++Rn)` | `D` | `INC Rn` / `LDN Rn` |
   | `D` | `(++Rn)` | `INC Rn` / `STR Rn` |
   | `(--Rn)` | `D` | `DEC Rn` / `LDN Rn` |
   | `D` | `(--Rn)` | `DEC Rn` / `STR Rn` |
   | `(Rn)` / `(Rn++)` / `(Rn--)` / `(++Rn)` / `(--Rn)` | `Rm.0` / `Rm.1` | the matching row above for that memory form into `D`, then `PLO Rm` / `PHI Rm` |
   | `Rm.0` / `Rm.1` | `(Rn)` / `(Rn++)` / `(Rn--)` / `(++Rn)` / `(--Rn)` | `GLO Rm` / `GHI Rm`, then the matching row above for `D` into that memory form |

2. An 8-bit immediate going to a full 16-bit `Rn` (not `.0`/`.1`) SHALL be
   rejected with an error asking for `.0`/`.1` or a 16-bit literal, rather than
   silently zero-extending or guessing which half was meant.
3. `D : D` or any other no-op assignment SHALL be accepted and compile to no
   instructions (not an error), since it may arise from generated/templated
   source.
4. The last two table rows generalize the `(Rn...) <-> D` rows rather than
   adding a new mechanism: no 1802 instruction moves memory directly into or
   out of a register half, so `D` is the unavoidable intermediate here exactly
   as it already is for `(Rn...) <-> D`. A chain using one of these rows SHALL
   still compile (the extra `PLO`/`PHI`/`GLO`/`GHI` is added automatically),
   not be rejected as unsupported.
5. A chain MAY have more than two operands (`A : B : C : ...`), fanning the
   same source out to every operand after it, left to right (e.g.
   `PAGE : R2.1 : R4.1 : R5.1` compiles to `LDI PAGE` / `PHI R2` / `PHI R4` /
   `PHI R5`). The compiler SHALL compute the first operand into `D` once
   (whichever of this table's `X -> D` rows applies: `D` itself, an
   immediate, `Rn.0`/`Rn.1`, or `(Rn...)`), then apply the matching `D -> X`
   row for each subsequent operand in turn, since none of `PLO`/`PHI`/`STR`
   change `D`. A full register (not `.0`/`.1`) SHALL be rejected as a
   destination in such a chain (it needs its own `HIGH`/`LOW` pair, which
   doesn't generalize across several destinations the way a single-byte
   write does); `PORT(n)`, `DF`, `Q`, and `EI` SHALL be rejected anywhere in
   such a chain too, since none of them are real `D`-mediated transfers.
   (Supersedes the original "chains are at most two operands" decision,
   which predated this fan-out form; a plain two-operand chain keeps all of
   its existing behavior, including the `PORT(n)`, constant-arithmetic, and
   register-to-register-arithmetic special cases, none of which generalize
   to 3+ operands and so remain exclusive to the two-operand form.)

### 4. `INP`/`OUT`

**User story:** As a developer, I want `INP`/`OUT` to be honest about only
being able to address memory through `X`, without MOL1802 inventing new
syntax for the common case to pay for it.

1. `PORT(n) : D` SHALL compile to the single instruction `INP n`. The language
   reference SHALL document that `INP n` writes to **both** `D` and
   `M(R(X))` as a real, unavoidable hardware side effect -- this is not spelled
   out as a third chain element; it's documented prose, the same treatment as
   `1:EI`/`0:EI` (Requirement 2.8).
2. `D : PORT(n)` SHALL compile to the single instruction `OUT n`.
3. Neither chain names a register: the memory `INP`/`OUT` touch is always
   whatever `X` currently designates, which the developer sets with an
   ordinary pass-through `SEX Rn` line.
4. Both chains SHALL require that the statement immediately before them (a
   one-line lookback, nothing further back) is a pass-through `SEX Rn` line.
   If it is not, the compiler SHALL report an error naming the statement and
   suggesting `SEX Rn` be added immediately before it, rather than assuming
   `X` is already correct or guessing a register -- getting this wrong would
   silently read or write the wrong memory (potentially the `R2` stack) with
   no other symptom, which is why this is the one place MOL1802 enforces
   something beyond "no safety net."
5. A chain combining `PORT(n)` with anything other than exactly `D` on the
   other side (e.g. attempting `Rn.0 : PORT(n)` directly) SHALL be rejected
   with an error naming the unsupported shape and suggesting the developer
   move the value into `D` first (`Rn.0 : D` then `D : PORT(n)`, two ordinary
   statements), since `INP`/`OUT` only ever move data through `D` and memory,
   never a named register directly.

### 5. Branches and conditions

**User story:** As a developer, I want to test a flag and branch in one
readable statement, with explicit control over short vs. long branches (since
`asm1802` doesn't auto-select between them).

1. The following condition forms SHALL be recognized, each paired with a label
   reference (`/LABEL` for a short branch, `//LABEL` for a long branch):

   | Condition | Short form emits | Long form emits |
   |---|---|---|
   | `EFk=0?` (`k`=1-4) | `BNk LABEL` | `LBNk LABEL` |
   | `EFk=1?` | `Bk LABEL` | `LBk LABEL` |
   | `D=0?` | `BZ LABEL` | `LBZ LABEL` |
   | `D<>0?` | `BNZ LABEL` | `LBNZ LABEL` |
   | `DF=0?` | `BNF LABEL` | `LBNF LABEL` |
   | `DF=1?` | `BDF LABEL` | `LBDF LABEL` |
   | `Q=0?` | `BNQ LABEL` | `LBNQ LABEL` |
   | `Q=1?` | `BQ LABEL` | `LBQ LABEL` |

2. A bare `/LABEL` or `//LABEL` (no condition) SHALL compile to `BR LABEL` or
   `LBR LABEL` respectively (unconditional).
3. The compiler SHALL NOT attempt to infer whether a branch target is in the
   same 256-byte page; the short/long choice is exactly what the developer
   wrote (`/` vs `//`), matching how hand-written `asm1802` source already
   requires this choice today. (Verifying same-page placement for short
   branches remains `asm1802`'s own existing error, surfaced through to the
   MOL1802 user via Requirement 1.4/1.5.)

### 6. Call and return

**User story:** As a developer, I want subroutine calls to use this project's
existing SCRT convention without MOL1802 reinventing it.

1. `CALL LABEL` and `RETURN` (or `RETN`) SHALL pass through unchanged to
   `asm1802`'s existing `CALL`/`RETURN`/`RETN` pseudo-ops (`SEP R4`/`DW addr` and
   `SEP R5` respectively); MOL1802 defines no separate call/return mechanism.

### 7. Stack operations

**User story:** As a developer, I want to push/pop `D` or full registers
(singly or chained in one statement) on the designated stack register
without re-deriving the `STXD`/`IRX`/`LDXA`/`LDX` sequence each time, with no
automatic `SEX` behavior at all -- unlike every other `X`-sensitive sugar in
this language, I want full, unchecked control here.

(Supersedes an earlier revision of this requirement, which defined `PUSH D`/
`POP D` with an auto-inserted `SEX R2`. Retired in favor of the `STK`
chaining syntax below at the developer's request, after `PUSH`/`POP` proved
too narrow -- it only ever moved `D`, one byte at a time.)

1. `list:STK` SHALL push each element of `list` onto `R2`, left to right;
   `STK:list` SHALL pop each element of `list` off `R2`, left to right.
   `list` is one or more comma-separated elements (no spaces around the
   commas), each either `D` or a full register `Rn`.
2. Pushing `D` SHALL compile to `STXD`. Pushing `Rn` SHALL compile to
   `GHI Rn` / `STXD` / `GLO Rn` / `STXD` (high byte pushed first, so it ends
   up deepest; low byte second, so it ends up on top).
3. Popping SHALL compile to one `IRX` followed by one read per byte across
   the whole `list` (not per element): `LDXA` for every read except the very
   last one overall, which SHALL be `LDX` (never `LDXA`). A popped `D`
   element reads straight into `D`; a popped `Rn` element reads its low byte
   via `PLO Rn` then its high byte via `PHI Rn` (the mirror image of the
   push order in 7.2, since the low byte ended up on top).
4. A comma-separated list SHALL be valid only directly next to `STK`; a
   comma anywhere else in a chain SHALL be a parse error (Requirement 1.4),
   not a general chain feature.
5. `STK` SHALL always be assumed to already be `R2`, via a `SEX R2` the
   developer writes themselves. Unlike Requirement 4.4 (`PORT(n)`, which
   errors on a missing `SEX`) and the fixed-register insert-if-needed
   sugar (Requirement 9.1), a `STK` chain SHALL neither check for a
   preceding `SEX` nor insert one, ever -- the developer is trusted
   completely here, on the understanding that getting it wrong is the
   developer's own responsibility to catch.
6. `STK` targeting a register other than `R2` is out of scope for v1
   (default stack register only, and never named in the generated
   instructions, which is also why it can't be checked).

### 8. Register arithmetic with a constant

**User story:** As a developer, I want `Rn + k : Rn` to pick a reasonable
instruction sequence automatically, whether `k` is small or large.

1. For `Rn + k : Rn` or `Rn - k : Rn` where `k` is a compile-time constant with
   `0 < |k| <= 4`, the compiler SHALL emit `k` repeated `INC Rn` (for `+`) or
   `DEC Rn` (for `-`) instructions.
2. For `|k| > 4`, the compiler SHALL emit the general 16-bit add/subtract
   through `D`: `GLO Rn` / `ADI LOW(k)` / `PLO Rn` / `GHI Rn` / `ADCI HIGH(k)` /
   `PHI Rn` (subtract: `SMI`/`SDBI` in place of `ADI`/`ADCI`).
3. The threshold in 8.1/8.2 (4) SHALL be a named constant in the implementation
   (not scattered magic numbers), so it can be tuned later without a design
   change.
4. `Rn + 0 : Rn` SHALL be accepted and compile to no instructions.

### 9. Register-to-register arithmetic

**User story:** As a developer, I want to add one register's value into
another, even though the 1802 has no direct register-to-register ALU op, and
without having to think about `X` or a scratch byte myself.

1. The compiler SHALL support `Rm.0 + Rn.0 : Rn.0` (and the `.1`/subtract
   equivalents) by routing one operand through a single compiler-reserved
   scratch byte in memory, addressed via `X` pointed at a single
   compiler-reserved register (`R6`) that persistently holds that byte's
   address: store one operand to the scratch byte, load the other into `D`,
   `ADD`/`ADC`/`SM`/`SMB` against the scratch byte, store the result. `R6` is
   a fixed, known target, so inserting `SEX R6` when needed (the same
   insert-if-needed `xtrack` lookback as Requirement 7.3 used to use) is
   never a guess and never an error.
2. The compiler SHALL emit a one-time initialization sequence (at a point
   specified by the design, e.g. program start) that points `R6` at the
   reserved scratch byte, so later register-to-register arithmetic statements
   don't repeat that setup.
3. The exact scratch byte's placement and the precise instruction sequence are
   design-level decisions, deferred to `design.md`; this requirement fixes
   only the approach (real memory round-trip, not an invented opcode, `R6` as
   the reserved pointer per the Decisions section) and that initialization
   happens once, not per statement.
4. Full 16-bit register-to-register arithmetic (both halves, with carry
   propagation from the low-byte add into the high-byte add) SHALL be supported
   using the same mechanism, carrying `DF` from the low-byte `ADD`/`SM` into the
   high-byte `ADC`/`SMB` exactly as hand-written 1802 code would.

### 10. Comments and formatting

**User story:** As a developer, I want generated assembly to still be readable,
for debugging and for the kind of transcription-verification work already done
elsewhere in this project.

1. The compiler SHALL preserve comments attached to a MOL1802 statement (same
   line) by attaching them to the last instruction generated for that statement
   in the output.
2. The compiler MAY emit a comment on multi-instruction expansions noting which
   source statement produced them, to make generated output auditable against
   the MOL1802 source (not required to annotate every single instruction, but
   SHALL be possible to trace a generated block back to its source statement).
3. Generated mnemonics SHALL be indented to line up with the column mnemonics
   already most commonly start at elsewhere in the same source file (whether
   established by plain indentation or by a label padded with spaces), rather
   than always using one fixed indent regardless of the file's own
   convention. A label attached to a generated statement SHALL be padded the
   same way, with at least one separating space even if the label itself
   reaches or passes that column. A file with no established convention
   SHALL fall back to a plain 4-space indent.

### 11. Non-goals for v1 (explicitly deferred)

1. Short/long branch auto-selection (relaxation) is deferred; the developer's
   `/` vs `//` choice is authoritative (Requirement 5.3).
2. `STK` on a register other than `R2` is deferred (Requirement 7.6).
3. Any notion of "`X` tracking" beyond the one-line lookback used for
   `INP`/`OUT` (Requirement 4.4) and register-to-register arithmetic
   (Requirement 9.1) is out of scope -- there is no whole-program or
   whole-subroutine analysis of what `X` might be. `STK` (Requirement 7.5)
   doesn't use this lookback at all, in either direction.
