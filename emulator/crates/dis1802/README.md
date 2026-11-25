# dis1802

Disassembler for the RCA CDP1802. Turns a memory image (Intel HEX or raw
binary) into an `asm1802`-reassemblable listing, with labels for branch and
call targets and a best effort at telling code from data.

## How it works

- **Code discovery** is a recursive traversal from entry points (the load
  address, plus any `--entry`). It follows every branch target and falls
  through conditional branches, marking each decoded instruction. Bytes no
  trace ever reaches are treated as data and emitted as `DB`.
- **Labels**: every in-image branch or call target that lands on an instruction
  boundary gets an `Lxxxx` label (hex address); the operand is rewritten to use
  it. Targets outside the image become `EQU` definitions at the top.
- **Standard call technique** (`--scrt`, on by default): `SEP R4` followed by an
  inline `DW target` is recognised as a call. When the call register is R4 the
  two are collapsed into a single `CALL target` (which `asm1802` reassembles to
  the same `D4` + address word); the target becomes a label and execution
  continues after it. `SEP R5` ends a trace as a return. The startup sequence
  that builds the call/return handlers and the stack pointer is tracked, so
  those get named labels (`SCRTCALL`, `SCRTRET`, `STACK`) and the setup loads
  are rewritten to `LDI HIGH/LOW name`. Turn the call handling off with
  `--no-scrt`; change the registers with `--call-reg` / `--return-reg`.

Perfect code/data recovery is impossible in general (computed jumps, data that
decodes as instructions). The traversal is conservative and marks places where
control flow becomes unknowable. The correctness bar is a **round trip**:
reassembling the output with `asm1802` reproduces the input bytes, which is
checked in the tests, including on the project's sample programs.

## Usage

```
dis1802 [options] <input>

  --org ADDR        load address for a raw binary (hex; default 0000)
  --entry ADDR      extra entry point to trace from (hex; may repeat)
  --no-scrt         disassemble SEP R4 + DW as ordinary code, not a call
  --call-reg N      SCRT program-counter register (default 4)
  --return-reg N    SCRT return register (default 5)
  -o FILE           write the listing to FILE (default: standard output)
```

Intel HEX input (lines starting with `:`) carries its own addresses; `--org`
applies to raw binaries only.

### Example

```
dis1802 docs/scans/enhvidmon.hex -o enhvidmon.asm
asm1802 enhvidmon.asm            # reassembles to the original bytes
```
