# MOL1802 Language Guide

MOL1802 is a higher-level notation for the RCA 1802 that transpiles to
`asm1802` source. It adds a `SOURCE:DESTINATION` register-transfer
assignment, plus sugar for conditionals, branches, the stack, and small
arithmetic, on top of plain `asm1802` text. Anything MOL1802 doesn't have
sugar for, you write as ordinary `asm1802` -- the two freely mix in the
same file.

Source files use the extension **`.m18`**.

This guide is a practical reference for the syntax. For the full
rationale behind each decision, see `.kiro/specs/mol1802/requirements.md`
and `design.md` in this repository.

## Contents

- [Quick example](#quick-example)
- [Pass-through lines](#pass-through-lines)
- [Operands](#operands)
- [Register-transfer assignment](#register-transfer-assignment)
- [`INP`/`OUT`](#inpout)
- [Branches and conditions](#branches-and-conditions)
- [Call and return](#call-and-return)
- [Stack: `STK`](#stack-stk)
- [Arithmetic with a constant](#arithmetic-with-a-constant)
- [Register-to-register arithmetic](#register-to-register-arithmetic)
- [Comments](#comments)
- [What MOL1802 doesn't do](#what-mol1802-doesnt-do)
- [Running the compiler](#running-the-compiler)

## Quick example

```
        ORG 0000H
BYTE:   DB 0
SEND:   SEX R2
        D:STK
        BYTE:R3
        #55:D
        D:(R3)
        SEX R3
        D:PORT(4)
        SEX R2
        STK:D
        RETN
```

transpiles to:

```
        ORG 0000H
BYTE:   DB 0
SEND:   SEX R2
        STXD
        LDI HIGH(BYTE) ; BYTE:R3
        PHI R3
        LDI LOW(BYTE)
        PLO R3
        LDI #55
        STR R3
        SEX R3
        OUT 4
        SEX R2
        IRX ; STK:D
        LDX
        RETN
```

Generated lines line up with the rest of the file (more on this in the
next section) -- they don't just always indent by a fixed amount.

## Pass-through lines

A line is a MOL1802 statement only if it's a chain (`A:B`, including a
`STK` push/pop chain) or a branch (`cond?/LABEL` or `/LABEL`). Everything
else -- `asm1802` directives (`ORG`, `EQU`, `DB`, `DW`, `DS`, `ALIGN`,
`INCLUDE`, `DEFINE`/`IFDEF`/`IFNDEF`/`ELSE`/`ENDIF`), labels, comments,
and raw instruction lines you write yourself (including `CALL`,
`RETURN`/`RETN`, and every `SEX Rn`) -- is copied straight through,
completely unchanged. This is MOL1802's escape hatch: anything the sugar
below doesn't cover, write as ordinary `asm1802`.

A leading label works exactly as it does in `asm1802`: a line starting in
column 1, with an optional trailing `:`, is a label. It can sit on its own
line or in front of a MOL1802 statement on the same line:

```
LOOP:                   ; label alone
        R3.0:D
LOOP2:  R3.0:D          ; label and statement on one line
```

Everything else -- every MOL1802 statement, and every raw `asm1802`
instruction you write -- **must be indented**. A bare mnemonic starting in
column 1 is read as a label, same as in hand-written `asm1802`.

Because a line is only recognized as MOL1802 syntax by its *shape*, a
line that merely looks like a chain but has a typo (an unknown operand, a
second `:`, ...) is still handed to the parser and reported as an error,
not silently passed through as broken `asm1802`.

### Including `.m18` files

`INCLUDE` works for MOL1802 source too. `INCLUDE "x.m18"` (the extension
is matched case-insensitively) makes `mol1802` transpile `x.m18` to a
sibling `x.asm` (same folder), and the `INCLUDE` line in the generated
output is rewritten to `INCLUDE "x.asm"`, so the output assembles with
plain `asm1802`. Included `.m18` files may include others; paths resolve
relative to the including file, include cycles are an error, and nesting
is limited to 16 levels. Includes of anything else (`.asm`, ...) are left
alone. Nothing is written for a file until all its includes succeeded, and
a diagnostic from an included file names that file.

Each file is transpiled **on its own**, with no checks across files.
Notably, a file that uses register-to-register arithmetic gets its own `R6`
scratch setup at its start and its own `MOL1802_SCRATCH` byte at its end,
and `SEX` tracking restarts at the top of each file. Put such an `INCLUDE`
where that is safe (and don't use register arithmetic in two files that
end up in one program, since the label would be defined twice).

## Alignment

Generated mnemonics line up with whatever column the rest of your file
already starts mnemonics at -- they don't always indent by a fixed
amount regardless of the file's own style. The compiler looks at every
line in your source (both pass-through and MOL1802 alike) and finds the
column mnemonics most often start at there, whether that's from plain
indentation or from a label padded out with spaces:

```
        ORG 0x0000          ; column 9, from 8 spaces of indentation
PAGE    EQU 0                ; column 9, from "PAGE" + padding
SCRT:   PAGE:R2.1            ; a MOL1802 statement counts too
```

transpiles to:

```
        ORG 0x0000
PAGE    EQU 0
SCRT:   LDI PAGE ; PAGE:R2.1
        PHI R2
```

-- the generated `PHI R2` lines up at column 9, same as everything else
in the file, and a label in front of a generated statement (`SCRT:`
here) gets padded out to the same column rather than just one space.
Labels longer than that column still get at least one separating space,
never running into the mnemonic.

If a file has no established convention at all (no indented lines and no
padded labels anywhere), generated lines fall back to a plain 4-space
indent.

## Operands

| Write | Means | Notes |
|---|---|---|
| `D` | the accumulator | |
| `DF` | the data flag | readable only as a branch condition; writable (below) |
| `Rn` | a full 16-bit register | `R0`-`R15`, or `RA`-`RF` for 10-15 |
| `Rn.0` | the low byte of `Rn` | |
| `Rn.1` or `Rn'` | the high byte of `Rn` | `'` is a synonym for `.1` |
| `(Rn)` | the byte `Rn` points at | direct memory, **never** goes through `X` |
| `(Rn++)` | `(Rn)`, then increment `Rn` | |
| `(Rn--)` | `(Rn)`, then decrement `Rn` | |
| `(++Rn)` | increment `Rn`, then `(Rn)` | |
| `(--Rn)` | decrement `Rn`, then `(Rn)` | |
| `PORT(n)` | I/O port `n` | `n` = 1-7; only valid in an `INP`/`OUT` chain, see below |
| `EF1`-`EF4` | a flag input line | readable only as a branch condition |
| `Q` | the `Q` output line | readable as a condition; writable (below) |
| `EI` | interrupt enable/disable | write-only, see below |
| anything else | an opaque expression | a number, a label, `LOW x`, `HIGH x`, ... copied through to `asm1802` untouched and unevaluated |

`(Rn)` is deliberately the common, simple case: it's just "the memory
`Rn` points at," compiling to `LDN`/`STR`/`LDA`/`INC`/`DEC` as needed, and
it never needs a `SEX`. The one place `X` matters at all is `INP`/`OUT`
(next section) -- that's a real hardware restriction, not a MOL1802
invention, and MOL1802 confines all of the bookkeeping around it to that
one spot.

Numbers and labels are never evaluated by MOL1802 itself -- whatever you
write is copied verbatim into the generated `asm1802` line, so every
expression form `asm1802` supports (`#FF`, `0x0F`, `0FH`, `LOW x`,
`HIGH x`, arithmetic, forward references, ...) works exactly as it does
in hand-written `asm1802`.

## Register-transfer assignment

`SOURCE:DESTINATION` -- data flows left to right, with no space on
either side of the `:`. The compiler picks the real instruction(s):

| Source | Destination | Emits |
|---|---|---|
| `D` | `Rn.0` | `PLO Rn` |
| `D` | `Rn.1` | `PHI Rn` |
| `Rn.0` | `D` | `GLO Rn` |
| `Rn.1` | `D` | `GHI Rn` |
| immediate | `D` | `LDI <imm>` |
| immediate | `Rn.0` / `Rn.1` | `LDI <imm>` then `PLO Rn` / `PHI Rn` |
| immediate | `Rn` (full) | `LDI HIGH(<imm>)` / `PHI Rn` / `LDI LOW(<imm>)` / `PLO Rn` |
| `Rm` | `Rn` (full) | `GHI Rm` / `PHI Rn` / `GLO Rm` / `PLO Rn` |
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
| `(Rn...)` | `Rm.0` / `Rm.1` | the matching `(Rn) <-> D` row above, then `PLO Rm` / `PHI Rm` |
| `Rm.0` / `Rm.1` | `(Rn...)` | `GLO Rm` / `GHI Rm`, then the matching `D <-> (Rn)` row above |
| `1` | `DF` | `SMI 0` (forces `DF`=1) |
| `0` | `DF` | `ADI 0` (forces `DF`=0) |
| `1` | `Q` | `SEQ` |
| `0` | `Q` | `REQ` |
| `1` | `EI` | `RET` (also pops `X`,`P` from the `R2` stack -- a real hardware side effect) |
| `0` | `EI` | `DIS` (same side effect) |

### Fan-out: one source, several destinations

`A:B:C:...` writes the same source into every destination after it, left
to right -- useful for things like setting several registers' high byte
to the same page number in one statement:

```
PAGE    EQU 00
        PAGE:R2.1:R4.1:R5.1
```
```
    LDI PAGE ; PAGE:R2.1:R4.1:R5.1
    PHI R2
    PHI R4
    PHI R5
```

The source is computed into `D` once; each destination after it is then
written from `D` in turn (`PLO`/`PHI`/`STR`, none of which change `D`, so
the same value safely reaches every destination). Because of that, fan-out
is restricted to the operands that are actually `D`-sized transfers:

- **Source:** `D`, an immediate, `Rn.0`/`Rn.1`, or `(Rn...)`.
- **Each destination after it:** `D`, `Rn.0`/`Rn.1`, or `(Rn...)`.

A full register (`Rn`, no `.0`/`.1`) isn't allowed as a destination in a
fan-out chain -- it needs its own `HIGH`/`LOW` pair, which doesn't
generalize the same way across several destinations, so write it as its
own ordinary two-element chain instead. `PORT(n)`, `DF`, `Q`, and `EI`
aren't allowed anywhere in a fan-out chain either: none of them are real
`D`-mediated transfers (`PORT(n)` has its own `SEX` lookback requirement;
`DF`/`Q`/`EI` compile from the literal text `1`/`0`, not a `D` value).

Rules:

- **An 8-bit immediate going to a full register is an error.** `#FF:R3`
  is rejected (use `R3.0` or `R3.1`, or a genuinely 16-bit literal like
  `#1234`). This only catches literals written in the project's own `#NN`
  (exactly two hex digits) convention; a bare decimal like `5` or a label
  name is width-ambiguous and goes through the 16-bit form.
- **A no-op assignment compiles to nothing.** `D:D`, `R3:R3`,
  `R3.0:R3.0`, ... are accepted and emit no instructions (useful for
  generated or templated source).
- Anything not in this table and not one of the special cases below is an
  error naming the unsupported shape.
- **No 1802 instruction moves memory directly into or out of a register
  half** -- `D` is always the unavoidable intermediate, on real hardware
  as much as here. `(R6++):R3.1` is genuinely two instructions
  (`LDA R6` / `PHI R3`); MOL1802 just saves you writing the `D` step
  yourself, the same way it already does for `(Rn) <-> D`. This is the
  one place in the table where a single chain can expand into more real
  instructions than it looks like at a glance, so it's worth remembering
  when reading generated output.

```
    (R6++):R3.1     ; LDA R6 / PHI R3 -- classic SCRT address unpacking
    (R6++):R3.0     ; LDA R6 / PLO R3
```

## `INP`/`OUT`

`INP` and `OUT` are the only two 1802 instructions that can *only*
address memory through whatever register `X` currently designates --
there's no way to name a different register in the opcode. MOL1802
doesn't invent new syntax to work around this; it's honest about it:

| Write | Emits |
|---|---|
| `PORT(n):D` | `INP n` |
| `D:PORT(n)` | `OUT n` |

`INP n` really does write to both `D` *and* memory at once -- that's a
real, unavoidable side effect of the hardware, not something MOL1802
adds. `OUT n` reads whatever is already staged in memory at `X`; neither
chain names a register, because the memory they touch is always whatever
`X` currently is.

**The line immediately before a `PORT(n)` chain must be a plain `SEX
Rn`.** This is the one place MOL1802 enforces something beyond "no safety
net": getting `X` wrong here would silently read or write the wrong
memory -- potentially corrupting your `R2` stack -- with no other
symptom. If the line right before isn't `SEX Rn`, it's a compile error:

```
    SEX R3
    D:PORT(4)       ; OK -- SEX R3 is the line right before this
```
```
    SEX R3
    LDI 0
    D:PORT(4)       ; ERROR -- SEX R3 is two lines back, not one
```

The lookback is exactly one line and never further, and it doesn't care
*which* register you named -- only that one was named right there. If you
have a polling loop between the `SEX` and the `OUT`/`INP` (a common real
1802 idiom: `SEX` once, then `WAIT: BNx WAIT`, then `OUT`), write the
`OUT`/`INP` itself as a plain pass-through instruction instead of using
the `PORT(n)` sugar -- the loop's own branch line sits between the `SEX`
and the `OUT`, which the one-line rule doesn't allow.

Pairing `PORT(n)` with anything other than exactly `D` is an error too:

```
    R3.0:PORT(4)     ; ERROR -- move it through D first:
    R3.0:D
    D:PORT(4)
```

## Branches and conditions

A condition, followed by `/LABEL` (short branch) or `//LABEL` (long
branch):

| Condition | Short | Long |
|---|---|---|
| `EFk=0?` (`k`=1-4) | `BNk LABEL` | `LBNk LABEL` |
| `EFk=1?` | `Bk LABEL` | `LBk LABEL` |
| `D=0?` | `BZ LABEL` | `LBZ LABEL` |
| `D<>0?` | `BNZ LABEL` | `LBNZ LABEL` |
| `DF=0?` | `BNF LABEL` | `LBNF LABEL` |
| `DF=1?` | `BDF LABEL` | `LBDF LABEL` |
| `Q=0?` | `BNQ LABEL` | `LBNQ LABEL` |
| `Q=1?` | `BQ LABEL` | `LBQ LABEL` |

A bare `/LABEL` or `//LABEL` (no condition) is an unconditional `BR
LABEL` or `LBR LABEL`.

```
WAIT:   EF1=0?/WAIT       ; BN1 WAIT
        D<>0?//DONE       ; LBNZ DONE
        /LOOP             ; BR LOOP
```

Whitespace between the condition and the `/`/`//` is fine (`EF1=0? /WAIT`
works the same as `EF1=0?/WAIT`) -- the examples just write it tight by
convention.

**`<>0?` only exists for `D`.** `DF` and `Q` are single bits, so `=0?`
and `=1?` already cover both states -- there's no `DF<>0?` or `Q<>0?`.
Writing one of those is a plain unrecognized-mnemonic error (the line
doesn't match any condition, so it isn't treated as MOL1802 syntax at
all, and gets passed straight through to `asm1802` broken). If you want
"is `Q` currently on," write `Q=1?`.

MOL1802 never chooses short vs. long for you -- `/` vs. `//` is exactly
what you wrote, matching how `asm1802` already requires this choice.
(Whether a short branch's target is actually on the same page is still
`asm1802`'s own existing check, surfaced the same way any other
`--assemble` error is.)

## Call and return

`CALL LABEL`, `RETURN`, and `RETN` pass straight through to `asm1802`'s
existing pseudo-ops (`SEP R4`/`DW addr` and `SEP R5`). MOL1802 adds no
logic of its own here.

## Stack: `STK`

`list:STK` pushes; `STK:list` pops. `list` is one or more `D`/`Rn`
elements, comma-separated with no spaces, moved left to right:

```
    R10:STK        ; GHI R10 / STXD / GLO R10 / STXD
    STK:R10        ; IRX / LDXA / PLO R10 / LDX / PHI R10
```

A full register pushes its high byte then its low byte (high ends up
deepest, low ends up on top); popping back into the same register reads
low then high, the mirror image. `D` alone is a single byte each way:

```
    D:STK          ; STXD
    STK:D          ; IRX / LDX
```

Chain more than one element, comma-separated, to save/restore a whole
group in one statement -- the push list and the pop list are mirror
images of each other, since it's a stack:

```
    R10,R9,R8,R7:STK
    STK:R7,R8,R9,R10
```

pushes `R10` first (ends up deepest) through `R7` last (ends up on top),
and the pop reads `R7` first (it's on top) back up through `R10` last.
Every byte read uses `LDXA` except the very last one overall, which uses
`LDX` -- never `LDXA` for that one -- matching the "full descending,
points-to-next-free" stack convention used throughout this project's
hand-written 1802 code.

**The comma list is reserved for `STK`.** Writing a comma anywhere else
(`R3,R4 : D`) is a parse error, not a general chain feature.

**`STK` is always assumed to already be `R2`, set by a `SEX R2` you write
yourself. Unlike everything else in this guide that touches `X`, a `STK`
chain never checks for a `SEX` and never inserts one** -- not even the
fixed-register insert-if-needed behavior `PUSH`/`POP` used to have. You
are fully responsible for `X` being `R2` before a `STK` chain:

```
    SEX R2
    R10:STK
    ... (anything that might change X) ...
    SEX R2          ; you must re-assert this yourself; STK won't
    STK:R10
```

`STK` on a register other than `R2` isn't supported -- it's always `R2`
by convention, never named in the generated instructions (which is also
why MOL1802 can't check it for you).

## Arithmetic with a constant

`Rn+k:Rn` or `Rn-k:Rn`, where `k` is a literal number MOL1802 itself
reads (not an opaque expression -- this is the one place besides widths
above that it looks inside an immediate):

- `k` = 0 emits nothing.
- `1 <= |k| <= 4` emits `k` repeated `INC Rn` (add) or `DEC Rn` (subtract).
- `|k| > 4` emits the general form through `D`:
  `GLO Rn` / `ADI low(k)` / `PLO Rn` / `GHI Rn` / `ADCI high(k)` / `PHI Rn`
  (subtract uses `SMI`/`SDBI` in place of `ADI`/`ADCI`).

```
    R3+1:R3       ; INC R3
    R3+4:R3       ; INC R3 (x4)
    R3+5:R3       ; GLO R3 / ADI 5 / PLO R3 / GHI R3 / ADCI 0 / PHI R3
    R3+0x100:R3   ; carries into the high byte
```

## Register-to-register arithmetic

The 1802 has no direct register-to-register ALU op, so `Rm+Rn:Rn` (or
`Rm.0+Rn.0:Rn.0`, or the subtract forms) routes one operand through a
single compiler-reserved scratch byte, addressed via a compiler-reserved
register, `R6`:

```
    R3.0+R4.0:R4.0    ; 8-bit only, no carry
    R3+R4:R4          ; full 16-bit, DF carries from the low byte
    R3-R4:R4          ; subtract: result is R3 - R4, stored into R4
```

- The right-hand operand of the arithmetic expression must be exactly the
  destination (`Rn.0+Rn.0:...` doesn't make sense any other way).
- Using `.0`/`.1` on both sides gives the single-byte form; using plain
  `Rm`/`Rn` (no half selector) gives the full 16-bit form, with `DF`
  correctly carrying from the low-byte step into the high-byte step
  (`ADD`/`ADC` or `SM`/`SMB`).
- `R6` is reserved for this. If any register-to-register arithmetic
  statement appears anywhere in your file, the compiler prepends a
  one-time initialization to the very top of the output (pointing `R6` at
  a scratch byte) and appends the scratch byte declaration
  (`MOL1802_SCRATCH: DB 0`) at the very end. This assumes the first line
  of your file is the real entry point. If no such statement appears,
  neither is emitted.
- Because `R6` is reserved this way, don't use it for anything else in a
  file that has register-to-register arithmetic in it.
- `X` is managed the lazy, never-error way: `SEX R6` is inserted only if
  the line right before doesn't already guarantee `X` is `R6` (unlike
  `STK`, which never checks or inserts at all).

## Comments

A `;` comment on a MOL1802 statement line is preserved, attached to the
last instruction the statement expands to:

```
    R3.0:D ; grab the low byte
```
```
    GLO R3 ; grab the low byte
```

When a statement expands to more than one instruction, the first line of
the expansion gets a `; <original statement>` comment, so the generated
file stays traceable back to what you wrote:

```
    BUFF:R4
```
```
    LDI HIGH(BUFF) ; BUFF:R4
    PHI R4
    LDI LOW(BUFF)
    PLO R4
```

## What MOL1802 doesn't do

- **No short/long branch selection.** You always choose `/` vs. `//`.
- **No `STK` on anything but `R2`.**
- **No `X` checking at all for `STK`.** It's the one place in this
  language with no net whatsoever -- not even the auto-insert kind.
- **No `X` tracking beyond the one-line lookback** described above, for
  `PORT(n)` and register-to-register arithmetic. There's no whole-program
  or whole-subroutine analysis of what `X` might be at any other point --
  write plain `SEX`/instructions yourself everywhere else, exactly as you
  would in `asm1802`.
- **No evaluation of your expressions.** Numbers, labels, `LOW`/`HIGH`,
  and anything else that isn't one of the forms above is handed to
  `asm1802` untouched.

## Running the compiler

```
mol1802 <source.m18> [options]

  -o <file>            transpiled asm1802 output path (default: <source>
                       with .asm)
  --assemble           also assemble the transpiled output with asm1802
  --assemble-out <f>   assembled output path (default: the transpiled
                       .asm path with .hex, or .bin with --bin)
  --bin                assemble to a flat binary instead of Intel HEX
  -l, --listing        with --assemble, also write a listing (default:
                       <assemble-out> with .lst)
  --max-errors <n>     errors to print before stopping (default 50, 0 = all)
  -h, --help           show this help
```

Errors from either stage (transpiling or, with `--assemble`, assembling)
print in the same `file:line: error: message` shape:

```
$ mol1802 bad.m18
bad.m18:3: error: register number out of range (0-15): `R16`
1 error(s); no output written
```

Example, transpiling and assembling in one step:

```
$ mol1802 send.m18 --assemble -l
send.asm: transpiled
send.hex: 14 bytes in 0000-000D
```
