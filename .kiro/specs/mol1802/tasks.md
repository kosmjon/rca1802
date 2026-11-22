# Implementation plan: MOL1802

Each task builds on the previous ones and ends with passing tests. Requirement
numbers refer to `requirements.md`; section numbers refer to `design.md`.

- [x] 1. Set up the `mol1802` crate
  - Create `emulator/crates/mol1802/` (`lib.rs` + `main.rs` binary) and add it
    to the workspace `Cargo.toml`. Depend on `asm1802` as a normal dependency
    (not dev-only), per design section "Overview".
  - Re-export/reuse `asm1802`'s `Diagnostic { file, line, message }` type
    rather than redefining it.
  - `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` clean
    for the new (near-empty) crate, matching the project-wide bar.
  - _Requirements: 1.1, 1.4_

- [x] 2. AST and operand grammar
  - [x] 2.1 Define `Operand`, `Adjust`, `Condition`, `BranchLength`, and
    `Statement` exactly as in design section 2 (chains are exactly two
    operands; no `XIndirect` variant -- `(RX)` doesn't exist in this
    language).
    - _Requirements: 2.1-2.8_
  - [x] 2.2 Operand parser: `D`, `DF`, `Rn` (built-in `asm1802` register
    names, `R0`-`R15`/`RA`-`RF`), `Rn.0`/`Rn.1`/`Rn'`, `PORT(n)`, `EF1`-`EF4`,
    `Q`, `EI`, and "anything else is an opaque `Imm` expression" (never
    evaluated, copied through verbatim per Requirement 1.3).
    - Unit tests: one per operand form, including that an unrecognized shape
      produces a diagnostic naming the token (Requirement 1.4), not a panic
      or a silent `Imm` fallback for things that look almost-but-not-quite
      like a known form (e.g. `R16`, `PORT(8)`, `PORT(0)`).
    - _Requirements: 2.1-2.4, 2.6, 2.7, 1.4_
  - [x] 2.3 `(Rn)` memory-operand parser: the plain form and its four adjust
    variants (`(Rn++)`, `(Rn--)`, `(++Rn)`, `(--Rn)`) -- all four are valid in
    both directions (Requirement 2.5), no rejection case needed since none of
    them touch `X`.
    - _Requirements: 2.5_

- [x] 3. Line classifier
  - Implement pass-through vs. MOL1802-statement classification per design
    section "1. Line classifier": label-stripping, then the four statement
    shapes (chain, condition+branch, bare branch, `PUSH`/`POP`).
  - Unit tests: directives (`ORG`, `EQU`, `DB`, `INCLUDE`, `DEFINE`/`IFDEF`),
    labels alone, raw `asm1802` instruction lines, `CALL`/`RETURN`/`RETN`
    (Requirement 6.1), and plain `SEX Rn` lines (the ones a developer writes
    ahead of an `INP`/`OUT` chain) all classify as pass-through; each of the
    four statement shapes classifies as MOL1802, including with a leading
    label.
  - _Requirements: 1.2, 6.1_

- [x] 4. Statement parser
  - [x] 4.1 Chain parser: split on a single top-level `:`, producing
    `Statement::Chain([a, b])` via the operand parser from task 2. A chain
    with more than one `:` is a parse error (Requirement 1.4) -- nothing in
    the grammar needs a third element now.
    - _Requirements: 3.1_
  - [x] 4.2 Branch parser: the condition-token table (Requirement 5.1),
    `/LABEL` vs `//LABEL`, and bare unconditional branches.
    - _Requirements: 5.1, 5.2_
  - [x] 4.3 `PUSH D` / `POP D` parser; any other operand after `PUSH`/`POP`
    is a diagnostic (Requirement 7's grammar is `D`-only in v1), not a
    generalized-but-untested form.
    - _Requirements: 7.1, 7.2_
  - [x] 4.4 Comment capture: the trailing `;` comment on a statement line is
    attached to the parsed `Statement` for use during lowering (Requirement
    10.1).
    - _Requirements: 10.1_

- [x] 5. Lowering: register-transfer assignment table
  - Implement the full Requirement 3.1 table (design section 4, step 2): one
    `match` arm (or table lookup) per row, including all eight `(Rn...)`
    adjust combinations with the pre/post `INC`/`DEC` ordering from the
    table. None of these rows touch `X` or need the `xtrack` lookback at all.
  - Implement Requirement 3.2 (8-bit immediate into a full `Rn` is an error)
    and 3.3 (`D:D`/no-op assignment emits nothing).
  - Unit tests: one test per table row, asserting the exact emitted
    instruction text (not just "it compiled").
  - _Requirements: 3.1, 3.2, 3.3_

- [x] 6. Lowering: `INP`/`OUT` and the `xtrack` lookback
  - [x] 6.1 Implement the `PORT(n)` special case (design section 4, step 1):
    `PORT(n):D` -> `INP n`, `D:PORT(n)` -> `OUT n`; any other operand paired
    with `PORT(n)` is the Requirement 4.5 error.
    - _Requirements: 4.1, 4.2, 4.5_
  - [x] 6.2 Implement `xtrack.rs`'s `preceding_sex` helper (design section 5)
    and wire it into the `PORT(n)` case in **error** mode: missing or
    non-matching `SEX` immediately before is the Requirement 4.4 error naming
    the statement and suggesting `SEX Rn`.
    - Unit tests covering all three outcomes: `SEX Rn` immediately before (no
      error, `Rn` used as-is regardless of which register it is), a
      different pass-through line immediately before (error), and nothing at
      all immediately before (error) -- confirm the lookback is exactly one
      line, i.e. a `SEX Rn` two lines back (with something unrelated in
      between) still errors.
    - _Requirements: 4.4_

- [x] 7. Lowering: branches, call/return, stack
  - [x] 7.1 Branch lowering per Requirement 5.1/5.2's table (short and long
    forms for every condition, plus bare `BR`/`LBR`).
    - _Requirements: 5.1, 5.2, 5.3_
  - [x] 7.2 Confirm (with a test, not just by inspection) that `CALL`/`RETURN`
    /`RETN` lines are classified as pass-through (task 3) and reach the
    output completely unchanged, since Requirement 6.1 means MOL1802 adds no
    lowering logic for them at all.
    - _Requirements: 6.1_
  - [x] 7.3 `PUSH`/`POP` lowering (Requirement 7.1/7.2: `STXD` and `IRX`/`LDX`
    in that order, not `LDXA`), using `xtrack`'s `preceding_sex` helper in
    **insert, never error** mode (Requirement 7.3): `SEX R2` inserted only if
    not already shown by the lookback.
    - Unit tests: `POP` emits `IRX` then `LDX` specifically (regression test
      against accidentally emitting `LDXA`); a `PUSH` immediately after
      another `PUSH`/`POP` emits no redundant `SEX R2`; a `PUSH` after an
      `INP`/`OUT` chain (which left `X` pointed elsewhere) does emit one.
    - _Requirements: 7.1, 7.2, 7.3_

- [x] 8. Lowering: constant and register-to-register arithmetic
  - [x] 8.1 `Rn +/- k : Rn` for a compile-time-constant `k`: repeated
    `INC`/`DEC` at or below the threshold, the `D`-mediated 16-bit add/
    subtract above it (Requirement 8.1, 8.2), with the threshold as one
    named constant (Requirement 8.3) and `k=0` emitting nothing (8.4).
    - Unit tests at `k` = 1, 4 (boundary), 5 (just above boundary), and a
      multi-byte value like `0x100`, checked against the exact instruction
      sequences in the requirement.
    - _Requirements: 8.1, 8.2, 8.3, 8.4_
  - [x] 8.2 Register-to-register arithmetic (Requirement 9): the `R6`
    scratch-pointer reservation, the auto-prepended one-time init (only
    emitted if any such statement appears in the source), the
    end-of-output `MOL1802_SCRATCH: DB 0`, and the add/subtract sequence
    from design section 6 for both the `.0`-only and full 16-bit
    (carry-propagating) forms, using `xtrack` in insert-never-error mode
    (Requirement 9.1) exactly like `PUSH`/`POP`.
    - Unit test: a source with **no** register-to-register arithmetic gets
      no init block and no scratch byte at all (confirms it's conditional,
      not unconditionally emitted).
    - Integration test: a 16-bit register-to-register add transpiled and
      then actually run through `cdp1802`'s CPU (or at minimum assembled
      and inspected), confirming the `DF` carry from the low-byte step
      correctly propagates into the high-byte step for a case that
      actually carries (e.g. `0x00FF + 0x0001`).
    - _Requirements: 9.1, 9.2, 9.3, 9.4_
    - Note: the per-statement lowering (`lower_register_arithmetic`,
      `scratch_register_init`, `scratch_byte_declaration`) and the
      detection predicate (`is_register_arithmetic_chain`) are implemented
      and unit-tested now; the two pipeline-level tests above need
      `lib::transpile` (task 9) to exist and are added there.

- [x] 9. Re-merge, comment preservation, and output
  - Implement design section 7: original line order preserved except the
    Requirement 9 init block (first) and scratch byte (last); comments
    attached per Requirement 10.1; a leading `; <source statement>` comment
    on multi-instruction expansions per Requirement 10.2.
  - Integration test: a short multi-statement MOL1802 source transpiled and
    the output checked line-by-line against a hand-written expected
    `asm1802` listing (both instructions and attached comments).
  - _Requirements: 10.1, 10.2_

- [x] 10. CLI
  - Implement `mol1802 <source.mol> [-o file] [--assemble] [-l]
    [--max-errors N]` as in design section 8, reusing `asm1802::assemble_with`
    for `--assemble` and printing diagnostics from either stage in the same
    `file:line: error: message` shape `asm1802`'s own CLI already uses.
  - `-h`/`--help` text.
  - _Requirements: 1.1, 1.4, 1.5_

- [x] 11. End-to-end verification against real programs
  - Pick one non-trivial routine already transcribed into `docs/scans/` or
    `sample-code/` (or a short new example) and hand-write a MOL1802 version
    of it alongside the existing hand-written `asm1802` version. Prefer one
    that includes at least one `INP`/`OUT` (to exercise the `SEX`/lookback
    rule against a real-world register choice) and one stack push/pop.
  - Transpile, assemble both, and confirm they produce **identical bytes**
    (not just "both assemble") -- the strongest available check that the
    lowering tables in `design.md` match real 1802 semantics, following the
    same verify-by-assembling discipline used throughout this project
    (`fig_forth_matches_the_reference_hex_exactly`-style, adapted to compare
    MOL1802 output against hand-written `asm1802` output instead of a scan).
  - _Requirements: 1.5_

- [x] 12. Workspace integration
  - Add `mol1802` to whatever top-level CI/test invocation already covers
    `asm1802`/`cdp1802`/`elf2`, so `cargo fmt --all --check`, `cargo clippy
    --workspace --all-targets -- -D warnings`, and `cargo test --workspace`
    all cover the new crate automatically.
  - _Requirements: 1.4, 1.5_
