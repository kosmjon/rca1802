//! Disassembler for the RCA CDP1802.
//!
//! Produces an `asm1802`-reassemblable listing from a memory image. On top of
//! the single-instruction decoder in `cdp1802::disasm`, it:
//!
//! - discovers code by recursive traversal from entry points (following
//!   branches), so bytes that are never reached as instructions are treated as
//!   data and emitted as `DB`;
//! - names every branch (and SCRT call) target that lands on an instruction
//!   boundary with an `Lxxxx` label, and rewrites the operand to use it;
//! - optionally recognises the standard call technique (`SEP R4` followed by an
//!   inline `DW target`, returning with `SEP R5`), so call targets become
//!   labels and the inline address word is treated as data, not code (the
//!   `scrt` option, on by default).
//!
//! Perfect code/data recovery is impossible in general (computed jumps, data
//! that happens to decode as instructions). The traversal is conservative:
//! it only calls a byte "code" when some reachable path decodes it as an
//! instruction, and it marks places where control flow becomes unknowable
//! (such as `SEP Rn` to a register it can't resolve) with a comment.
//!
//! The intended correctness bar is a round trip: assembling the output with
//! `asm1802` reproduces the input bytes.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;

use cdp1802::{disasm, Instruction, Operand};

/// The input image: `bytes` loaded at address `org`.
#[derive(Debug, Clone)]
pub struct Program {
    pub org: u16,
    pub bytes: Vec<u8>,
}

impl Program {
    pub fn new(org: u16, bytes: Vec<u8>) -> Self {
        Program { org, bytes }
    }

    /// Address one past the last byte.
    fn end(&self) -> u32 {
        u32::from(self.org) + self.bytes.len() as u32
    }

    /// True if `addr` is within the image.
    fn contains(&self, addr: u16) -> bool {
        let a = u32::from(addr);
        a >= u32::from(self.org) && a < self.end()
    }

    /// The byte at `addr`, if inside the image.
    fn at(&self, addr: u16) -> Option<u8> {
        if self.contains(addr) {
            Some(self.bytes[usize::from(addr) - usize::from(self.org)])
        } else {
            None
        }
    }

    /// Up to three bytes starting at `addr` (for decoding).
    fn window(&self, addr: u16) -> Vec<u8> {
        (0..3)
            .filter_map(|k| self.at(addr.wrapping_add(k)))
            .collect()
    }
}

/// How the disassembler should behave.
#[derive(Debug, Clone)]
pub struct Options {
    /// Extra entry points to start tracing from (besides the load address).
    pub entries: Vec<u16>,
    /// Recognise the standard call technique (`SEP R4` / inline `DW` / `SEP R5`).
    pub scrt: bool,
    /// The register used as the program counter for calls (`SEP Rc`), default 4.
    pub call_reg: u8,
    /// The register used to return (`SEP Rr`), default 5.
    pub return_reg: u8,
    /// The register used as the stack pointer, default 2. A full-word load of
    /// it is labelled as the stack pointer's initial value.
    pub stack_reg: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            entries: Vec::new(),
            scrt: true,
            call_reg: 4,
            return_reg: 5,
            stack_reg: 2,
        }
    }
}

/// What a byte was classified as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Not yet visited.
    Unknown,
    /// First byte of an instruction.
    CodeStart,
    /// A continuation byte of an instruction (operand).
    CodeInner,
    /// The `SEP R4` opcode of a call collapsed to a `CALL` directive (3 bytes).
    Call,
    /// A byte that is part of an SCRT inline address word.
    CallWord,
    /// Reached as data (never decoded as an instruction).
    Data,
}

/// A line of the output listing.
#[derive(Debug, Clone)]
pub struct Line {
    pub addr: u16,
    /// Raw bytes this line covers.
    pub bytes: Vec<u8>,
    /// Label defined at this address, if any (without the trailing colon).
    pub label: Option<String>,
    /// The instruction or directive text (operand already using labels).
    pub text: String,
    /// A trailing comment (without the leading `;`), if any.
    pub comment: Option<String>,
}

/// The result of disassembling a program.
#[derive(Debug, Clone)]
pub struct Listing {
    pub org: u16,
    pub lines: Vec<Line>,
    /// Addresses referenced as labels but outside the image (emitted as EQU).
    pub externs: BTreeMap<u16, String>,
}

impl Listing {
    /// The listing as `asm1802` source text that reassembles to the input.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (addr, name) in &self.externs {
            let _ = writeln!(out, "{name:<8} EQU 0x{addr:04X}");
        }
        if !self.externs.is_empty() {
            out.push('\n');
        }
        let _ = writeln!(out, "        ORG 0x{:04X}", self.org);
        for l in &self.lines {
            if let Some(label) = &l.label {
                // Label on its own column; the instruction follows, padded.
                let _ = write!(out, "{label}:");
                if label.len() < 7 {
                    for _ in 0..(7 - label.len()) {
                        out.push(' ');
                    }
                } else {
                    out.push(' ');
                }
            } else {
                out.push_str("        ");
            }
            out.push_str(&l.text);
            if let Some(c) = &l.comment {
                // Pad to a comment column.
                let col = l.text.len() + 8;
                for _ in col..32 {
                    out.push(' ');
                }
                let _ = write!(out, " ; {c}");
            }
            out.push('\n');
        }
        out
    }
}

/// Label name for an address inside the image.
fn label_for(addr: u16) -> String {
    format!("L{addr:04X}")
}

/// An unconditional transfer ends a linear trace (nothing falls through).
fn is_unconditional_end(ins: &Instruction) -> bool {
    matches!(ins.mnemonic, "LBR" | "BR" | "RET" | "DIS" | "IDL")
}

/// A branch instruction whose target we can follow (short or long, any
/// condition). `SKP`/`LSKP`/long skips are not address branches.
fn branch_target(ins: &Instruction) -> Option<u16> {
    match (ins.mnemonic, ins.operand) {
        (m, Operand::Addr(a)) if m.starts_with('B') || m.starts_with("LB") => Some(a),
        _ => None,
    }
}

/// Whether a discovered SCRT handler is the call dispatcher or the return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandlerKind {
    Call,
    Return,
}

/// Abstract register state for resolving the SCRT setup. Only the pieces built
/// from immediates (`LDI`) and register-half moves (`GHI`/`GLO`/`PHI`/`PLO`)
/// are tracked; anything else makes the touched register unknown.
/// A byte of a register, with the address of the `LDI` that set it (if it was
/// set from an immediate, so the operand can be rewritten to `HIGH`/`LOW name`).
type Half = (Option<u8>, Option<u16>);

#[derive(Debug, Clone)]
struct Regs {
    /// Known high and low byte of each register (`(hi, lo)`), each `Some` only
    /// when that half is known. A register has a known 16-bit value only when
    /// both bytes are known.
    r: [(Half, Half); 16],
    /// Known value of D, and the address of the `LDI` that set it (if any).
    d: Option<u8>,
    d_src: Option<u16>,
}

impl Regs {
    fn unknown() -> Self {
        Regs {
            r: [((None, None), (None, None)); 16],
            d: None,
            d_src: None,
        }
    }

    /// At reset the 1802 clears R0 and P/X select R0, so R0 is 0. Other
    /// registers are unspecified. `GHI R0` early in a program therefore yields
    /// a known high byte of 0.
    fn at_reset() -> Self {
        let mut s = Regs::unknown();
        s.r[0] = ((Some(0), None), (Some(0), None));
        s
    }

    /// The full 16-bit value of `reg`, only when both bytes are known.
    fn value(&self, reg: u8) -> Option<u16> {
        match self.r[usize::from(reg)] {
            ((Some(hi), _), (Some(lo), _)) => Some(u16::from(hi) << 8 | u16::from(lo)),
            _ => None,
        }
    }

    /// The `LDI` source addresses of `reg`'s high and low bytes, if each came
    /// from an immediate (for rewriting those operands to `HIGH`/`LOW name`).
    fn sources(&self, reg: u8) -> (Option<u16>, Option<u16>) {
        let (hi, lo) = self.r[usize::from(reg)];
        (hi.1, lo.1)
    }

    /// Update the state for `ins` (located at `addr`). Returns the register
    /// whose value this instruction just set (for the caller to check), if any.
    fn apply(&mut self, ins: &Instruction, addr: u16) -> Option<u8> {
        match (ins.mnemonic, ins.operand) {
            ("LDI", Operand::Imm(v)) => {
                self.d = Some(v);
                self.d_src = Some(addr);
                None
            }
            ("GHI", Operand::Reg(r)) => {
                self.d = self.r[usize::from(r)].0 .0;
                self.d_src = None; // value came from a register, not an LDI
                None
            }
            ("GLO", Operand::Reg(r)) => {
                self.d = self.r[usize::from(r)].1 .0;
                self.d_src = None;
                None
            }
            ("PHI", Operand::Reg(r)) => {
                self.set_half(r, true);
                Some(r)
            }
            ("PLO", Operand::Reg(r)) => {
                self.set_half(r, false);
                Some(r)
            }
            // INC/DEC change the register value; we stop tracking it.
            ("INC" | "DEC", Operand::Reg(r)) => {
                self.r[usize::from(r)] = ((None, None), (None, None));
                None
            }
            // LDA increments its register and loads D; drop both.
            ("LDA", Operand::Reg(r)) => {
                self.r[usize::from(r)] = ((None, None), (None, None));
                self.d = None;
                self.d_src = None;
                None
            }
            // Anything that writes D with a value we don't model: D unknown.
            _ if writes_d(ins) => {
                self.d = None;
                self.d_src = None;
                None
            }
            _ => None,
        }
    }

    /// Set the high (`hi`) or low byte of register `r` from D, carrying the
    /// `LDI` source address so the operand can later be rewritten to a symbol.
    fn set_half(&mut self, r: u8, hi: bool) {
        let idx = usize::from(r);
        let half = (self.d, self.d_src);
        if hi {
            self.r[idx].0 = half;
        } else {
            self.r[idx].1 = half;
        }
    }
}

/// Instructions that load D with a value this tracker does not model, so after
/// them D must be treated as unknown (`LDI`, `GHI`, `GLO` are modelled above
/// and excluded).
fn writes_d(ins: &Instruction) -> bool {
    matches!(
        ins.mnemonic,
        "LDN"
            | "LDX"
            | "LDXA"
            | "INP"
            | "OR"
            | "AND"
            | "XOR"
            | "ADD"
            | "SD"
            | "SM"
            | "SHR"
            | "SHL"
            | "SHRC"
            | "SHLC"
            | "ADC"
            | "SDB"
            | "SMB"
            | "ORI"
            | "ANI"
            | "XRI"
            | "ADI"
            | "SDI"
            | "SMI"
            | "ADCI"
            | "SDBI"
            | "SMBI"
    )
}

/// Core analysis: classify every byte, collect label targets and externs.
struct Analysis<'a> {
    prog: &'a Program,
    opts: &'a Options,
    kind: Vec<Kind>,
    /// Addresses that are branch/call targets (candidate labels).
    targets: BTreeSet<u16>,
    /// A note to attach to the instruction starting at an address.
    notes: BTreeMap<u16, String>,
    /// Discovered SCRT handler addresses (call dispatcher / return).
    handlers: BTreeMap<u16, HandlerKind>,
    /// The stack pointer's initial value, once a full-word load of the stack
    /// register is seen.
    stack_init: Option<u16>,
    /// Comments to attach to the instruction starting at an address (hints and
    /// role notes; several may join with "; ").
    hints: BTreeMap<u16, Vec<String>>,
    /// A symbolic name for an address (handlers, stack), overriding `Lxxxx`.
    names: BTreeMap<u16, String>,
    /// `LDI` operands to rewrite as `HIGH name` / `LOW name` (address -> rewrite).
    rewrites: BTreeMap<u16, (ByteHalf, u16)>,
}

/// Which byte of an address an `LDI` immediate represents.
#[derive(Debug, Clone, Copy)]
enum ByteHalf {
    High,
    Low,
}

impl<'a> Analysis<'a> {
    fn new(prog: &'a Program, opts: &'a Options) -> Self {
        Analysis {
            prog,
            opts,
            kind: vec![Kind::Unknown; prog.bytes.len()],
            targets: BTreeSet::new(),
            notes: BTreeMap::new(),
            handlers: BTreeMap::new(),
            stack_init: None,
            hints: BTreeMap::new(),
            names: BTreeMap::new(),
            rewrites: BTreeMap::new(),
        }
    }

    /// Record that `reg`'s two `LDI` sources (if immediate) should render as
    /// `HIGH name` / `LOW name`, and name `target`.
    fn name_and_rewrite(&mut self, target: u16, name: &str, regs: &Regs, reg: u8) {
        self.names.entry(target).or_insert_with(|| name.to_string());
        let (hi_src, lo_src) = regs.sources(reg);
        if let Some(a) = hi_src {
            self.rewrites.insert(a, (ByteHalf::High, target));
        }
        if let Some(a) = lo_src {
            self.rewrites.insert(a, (ByteHalf::Low, target));
        }
    }

    /// Add a trailing comment to the line starting at `addr`.
    fn add_hint(&mut self, addr: u16, text: impl Into<String>) {
        self.hints.entry(addr).or_default().push(text.into());
    }

    fn idx(&self, addr: u16) -> Option<usize> {
        if self.prog.contains(addr) {
            Some(usize::from(addr) - usize::from(self.prog.org))
        } else {
            None
        }
    }

    /// Record a branch/call target. In-image, on a boundary → a label later.
    fn note_target(&mut self, addr: u16) {
        self.targets.insert(addr);
    }

    /// Recursive-descent trace of code from every entry point.
    fn trace(&mut self) {
        // Each queued trace carries the register state it starts with, so the
        // SCRT setup (LDI/PHI/PLO building R(call_reg) and R(return_reg)) can
        // be resolved to the call/return handler addresses.
        let mut queue: VecDeque<(u16, Regs)> = VecDeque::new();
        queue.push_back((self.prog.org, Regs::at_reset()));
        for &e in &self.opts.entries {
            queue.push_back((e, Regs::unknown()));
        }

        while let Some((mut addr, mut regs)) = queue.pop_front() {
            // Walk straight-line code until we leave the image, hit an already
            // decoded start, or reach an unconditional transfer.
            while let Some(i) = self.idx(addr) {
                match self.kind[i] {
                    Kind::CodeStart | Kind::Call => break, // already traced from here
                    Kind::CodeInner | Kind::CallWord => {
                        // We landed in the middle of something already decoded;
                        // flag it and stop this trace.
                        self.notes
                            .entry(addr)
                            .or_insert_with(|| "branch into the middle of an instruction".into());
                        break;
                    }
                    Kind::Unknown | Kind::Data => {}
                }

                let win = self.prog.window(addr);
                if win.is_empty() {
                    break;
                }
                let ins = disasm(&win, addr);
                // Not enough bytes for the operand: treat as data tail.
                if usize::from(ins.len) > win.len() {
                    break;
                }

                // Mark the instruction's bytes.
                self.kind[i] = Kind::CodeStart;
                for k in 1..ins.len {
                    if let Some(j) = self.idx(addr.wrapping_add(u16::from(k))) {
                        self.kind[j] = Kind::CodeInner;
                    }
                }

                // Track register values across the setup. When the call/return
                // pointer or the stack pointer becomes fully known, label its
                // target and (for the handlers) trace it as code.
                if let Some(reg) = regs.apply(&ins, addr) {
                    if let Some(target) = regs.value(reg) {
                        if self.opts.scrt
                            && (reg == self.opts.call_reg || reg == self.opts.return_reg)
                        {
                            let (kind, name, role) = if reg == self.opts.call_reg {
                                (HandlerKind::Call, "SCRTCALL", "SCRT call handler")
                            } else {
                                (HandlerKind::Return, "SCRTRET", "SCRT return handler")
                            };
                            self.add_hint(addr, format!("set up R{reg:X} -> {role}"));
                            self.name_and_rewrite(target, name, &regs, reg);
                            if self.handlers.insert(target, kind).is_none() {
                                self.note_target(target);
                                if self.prog.contains(target) {
                                    queue.push_back((target, Regs::unknown()));
                                }
                            }
                        } else if reg == self.opts.stack_reg {
                            // A full-word load of the stack register sets the
                            // initial stack pointer. Name it; do not trace it as
                            // code (it is data space, often outside the image).
                            self.add_hint(addr, format!("set up R{reg:X} = stack pointer"));
                            self.name_and_rewrite(target, "STACK", &regs, reg);
                            self.stack_init = Some(target);
                            self.note_target(target);
                        }
                    }
                }

                // Follow a branch target.
                if let Some(t) = branch_target(&ins) {
                    self.note_target(t);
                    if self.prog.contains(t) {
                        queue.push_back((t, regs.clone()));
                    }
                }

                // SCRT call: SEP R(call_reg) then an inline DW target.
                if self.opts.scrt && self.is_call(&ins) {
                    let word_at = addr.wrapping_add(1);
                    if let (Some(hi), Some(lo)) =
                        (self.prog.at(word_at), self.prog.at(word_at.wrapping_add(1)))
                    {
                        let target = u16::from_be_bytes([hi, lo]);
                        self.note_target(target);
                        // `asm1802`'s `CALL addr` emits exactly `D4` + DW, so
                        // the three bytes can collapse to one `CALL` line, but
                        // only when the call register is R4 (what `CALL` uses).
                        if self.opts.call_reg == 4 {
                            self.kind[i] = Kind::Call;
                        }
                        // Mark the two address bytes as a call word (data).
                        for k in 0..2u16 {
                            if let Some(j) = self.idx(word_at.wrapping_add(k)) {
                                self.kind[j] = Kind::CallWord;
                            }
                        }
                        if self.prog.contains(target) {
                            queue.push_back((target, Regs::unknown()));
                        }
                        // Execution continues after the address word.
                        addr = word_at.wrapping_add(2);
                        continue;
                    }
                }

                if is_unconditional_end(&ins) {
                    break;
                }
                // A return via SEP R(return_reg) also ends the trace.
                if self.opts.scrt && self.is_return(&ins) {
                    break;
                }

                addr = addr.wrapping_add(u16::from(ins.len));
            }
        }
    }

    /// `SEP R(call_reg)` — the standard call technique's dispatch.
    fn is_call(&self, ins: &Instruction) -> bool {
        ins.mnemonic == "SEP" && ins.operand == Operand::Reg(self.opts.call_reg)
    }

    /// `SEP R(return_reg)` — the standard return.
    fn is_return(&self, ins: &Instruction) -> bool {
        ins.mnemonic == "SEP" && ins.operand == Operand::Reg(self.opts.return_reg)
    }

    /// Flag instructions that are suspicious in context, as a hand to anyone
    /// checking a transcription (a scanned dump may have a misread nibble).
    /// Conservative: only patterns that are almost always a mistake in code.
    fn flag_suspicious(&mut self) {
        let mut addr = self.prog.org;
        let end = self.prog.end();
        while u32::from(addr) < end {
            let Some(i) = self.idx(addr) else { break };
            if self.kind[i] == Kind::CodeStart {
                let win = self.prog.window(addr);
                let ins = disasm(&win, addr);
                // 0x68 is the only opcode with no 1802 instruction: reached as
                // code, it is almost certainly a misread byte or misclassified
                // data.
                if ins.opcode == 0x68 {
                    self.add_hint(addr, "?? illegal opcode 68 in code (possible scan error)");
                }
                // An operand that runs off the end of the image.
                if ins.operand == Operand::Truncated {
                    self.add_hint(addr, "?? operand bytes missing (truncated image)");
                }
                addr = addr.wrapping_add(u16::from(ins.len.max(1)));
            } else {
                addr = addr.wrapping_add(1);
            }
        }
    }
}

/// Disassemble `prog` into a labelled, reassemblable listing.
pub fn disassemble(prog: &Program, opts: &Options) -> Listing {
    let mut an = Analysis::new(prog, opts);
    an.trace();
    an.flag_suspicious();

    // Any byte not reached as code is data.
    for k in an.kind.iter_mut() {
        if *k == Kind::Unknown {
            *k = Kind::Data;
        }
    }

    // Labels: a target inside the image that is on an instruction/data start.
    // Everything else referenced becomes an extern EQU.
    // Name for an address: a role name (SCRTCALL/STACK/...) if assigned, else
    // the address-based Lxxxx.
    let names = an.names.clone();
    let name_at = move |addr: u16| names.get(&addr).cloned().unwrap_or_else(|| label_for(addr));

    let mut labels: BTreeSet<u16> = BTreeSet::new();
    let mut externs: BTreeMap<u16, String> = BTreeMap::new();
    for &t in &an.targets {
        if prog.contains(t) {
            labels.insert(t);
        } else {
            externs.insert(t, name_at(t));
        }
    }
    // The stack pointer's initial value is referenced as a label even though it
    // is data space (usually above the program and outside the image).
    if let Some(s) = an.stack_init {
        if !prog.contains(s) {
            externs.entry(s).or_insert_with(|| name_at(s));
        } else {
            labels.insert(s);
        }
    }

    let name_of = |addr: u16| -> Option<String> {
        if labels.contains(&addr) || externs.contains_key(&addr) {
            Some(name_at(addr))
        } else {
            None
        }
    };

    // Emit lines in address order.
    let mut lines: Vec<Line> = Vec::new();
    let mut addr = prog.org;
    let end = prog.end();
    while u32::from(addr) < end {
        let i = usize::from(addr) - usize::from(prog.org);
        let label = if labels.contains(&addr) {
            Some(name_at(addr))
        } else {
            None
        };
        // Combine a structural note (e.g. "branch into the middle...") with any
        // role/suspicion hints into one trailing comment.
        let mut parts: Vec<String> = Vec::new();
        if let Some(n) = an.notes.get(&addr) {
            parts.push(n.clone());
        }
        if let Some(hs) = an.hints.get(&addr) {
            parts.extend(hs.iter().cloned());
        }
        let note = if parts.is_empty() {
            None
        } else {
            Some(parts.join("; "))
        };

        match an.kind[i] {
            Kind::CodeStart => {
                let win = prog.window(addr);
                let ins = disasm(&win, addr);
                let bytes: Vec<u8> = win[..usize::from(ins.len)].to_vec();
                let rewrite = an.rewrites.get(&addr).map(|&(h, t)| (h, name_at(t)));
                let text = render_instruction(&ins, &name_of, rewrite);
                lines.push(Line {
                    addr,
                    bytes,
                    label,
                    text,
                    comment: note,
                });
                addr = addr.wrapping_add(u16::from(ins.len));
            }
            Kind::Call => {
                // `SEP R4` + inline DW collapsed into one `CALL target` (3 bytes).
                let op = prog.at(addr).unwrap_or(0);
                let hi = prog.at(addr.wrapping_add(1)).unwrap_or(0);
                let lo = prog.at(addr.wrapping_add(2)).unwrap_or(0);
                let target = u16::from_be_bytes([hi, lo]);
                let operand = name_of(target).unwrap_or_else(|| format!("0x{target:04X}"));
                lines.push(Line {
                    addr,
                    bytes: vec![op, hi, lo],
                    label,
                    text: format!("CALL {operand}"),
                    comment: note,
                });
                addr = addr.wrapping_add(3);
            }
            Kind::CallWord => {
                // A bare inline address word (an SCRT call with call_reg != 4,
                // whose `SEP Rn` was rendered as a normal instruction).
                let hi = prog.at(addr).unwrap_or(0);
                let lo = prog.at(addr.wrapping_add(1)).unwrap_or(0);
                let target = u16::from_be_bytes([hi, lo]);
                let operand = name_of(target).unwrap_or_else(|| format!("0x{target:04X}"));
                lines.push(Line {
                    addr,
                    bytes: vec![hi, lo],
                    label,
                    text: format!("DW {operand}"),
                    comment: note,
                });
                addr = addr.wrapping_add(2);
            }
            Kind::Data | Kind::Unknown | Kind::CodeInner => {
                // A run of data bytes, up to the next label or classification change.
                let (bytes, next) = data_run(&an, addr, &labels);
                let text = render_data(&bytes);
                lines.push(Line {
                    addr,
                    bytes,
                    label,
                    text,
                    comment: note,
                });
                addr = next;
            }
        }
    }

    Listing {
        org: prog.org,
        lines,
        externs,
    }
}

/// Collect a run of data bytes starting at `addr`: stop at the next label, the
/// next code byte, or a maximum run length (so `DB` lines stay readable).
fn data_run(an: &Analysis, start: u16, labels: &BTreeSet<u16>) -> (Vec<u8>, u16) {
    const MAX_RUN: usize = 8;
    let prog = an.prog;
    let mut bytes = Vec::new();
    let mut addr = start;
    while let Some(i) = an.idx(addr) {
        if addr != start && labels.contains(&addr) {
            break; // next label starts its own line
        }
        if !matches!(an.kind[i], Kind::Data | Kind::Unknown | Kind::CodeInner) {
            break; // code or a call word starts here
        }
        bytes.push(prog.bytes[i]);
        addr = addr.wrapping_add(1);
        if bytes.len() >= MAX_RUN {
            break;
        }
    }
    (bytes, addr)
}

/// Render an instruction, substituting a label for an in-range `Addr` operand.
/// `rewrite`, when present, replaces an `LDI` immediate with `HIGH name` /
/// `LOW name` (used for the SCRT/stack setup once the symbol is known).
fn render_instruction(
    ins: &Instruction,
    name_of: &impl Fn(u16) -> Option<String>,
    rewrite: Option<(ByteHalf, String)>,
) -> String {
    match ins.operand {
        Operand::Addr(a) => {
            let target = name_of(a).unwrap_or_else(|| format!("0x{a:04X}"));
            format!("{} {target}", ins.mnemonic)
        }
        Operand::None => ins.mnemonic.to_string(),
        Operand::Reg(r) => format!("{} R{r:X}", ins.mnemonic),
        Operand::Port(p) => format!("{} {p}", ins.mnemonic),
        Operand::Imm(v) => match rewrite {
            Some((ByteHalf::High, name)) => format!("{} HIGH {name}", ins.mnemonic),
            Some((ByteHalf::Low, name)) => format!("{} LOW {name}", ins.mnemonic),
            None => format!("{} 0x{v:02X}", ins.mnemonic),
        },
        Operand::Byte(v) => format!("DB 0x{v:02X}"),
        Operand::Truncated => format!("{} ??", ins.mnemonic),
    }
}

/// Render a run of bytes as a `DB` directive with hex byte values.
fn render_data(bytes: &[u8]) -> String {
    let items: Vec<String> = bytes.iter().map(|b| format!("0x{b:02X}")).collect();
    format!("DB {}", items.join(","))
}

#[cfg(test)]
mod tests;
