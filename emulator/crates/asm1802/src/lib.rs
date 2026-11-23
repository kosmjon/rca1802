//! Cross-assembler for the RCA CDP1802, accepting the source dialect of the
//! QELFEXE assembler (as used by the Netronics Tiny BASIC recreation).
//!
//! # Source format
//!
//! - A line that starts with `;` or `*` is a comment. Blank lines are ignored.
//! - Anything starting in column 1 is a label (no colon needed; a trailing
//!   colon is accepted and dropped). Labels may share names with mnemonics,
//!   e.g. a routine called `CALL`.
//! - The mnemonic follows whitespace. The operand field is the next
//!   whitespace-delimited token; whatever follows it is a comment, with or
//!   without `;`. Whitespace is allowed inside quotes, after a comma, and
//!   after the `LOW` / `HIGH` keywords (`LDI LOW $+3  comment`).
//! - Mnemonics without an operand treat the rest of the line as a comment.
//! - Symbols and mnemonics are case-insensitive. `R0`-`R15` and `RA`-`RF`
//!   are predefined as 0-15 unless the source defines them.
//!
//! # Directives and macros
//!
//! | Form | Meaning |
//! |---|---|
//! | `name EQU expr` | define a constant |
//! | `ORG expr` | set the location counter |
//! | `name = expr` | same as `EQU` (RCA style; `STACK =*-1` works too) |
//! | `DB a,'text',...` | bytes (values are truncated to 8 bits). `DC` is the same. An item of more than two hex digits after `#` is that many bytes: `DC #01020300` is `01 02 03 00` |
//! | `DW a,b,...` | 16-bit words, high byte first |
//! | `DS n` | reserve `n` bytes (nothing is written) |
//! | `ALIGN n` | reserve bytes (like `DS`) up to the next multiple of `n`; a no-op if already aligned |
//! | `END` | stop assembling |
//! | `INCLUDE "file"` | assemble another file's lines here (quotes optional; relative to the including file) |
//! | `DEFINE name` | mark `name` as defined, for `IFDEF`/`IFNDEF` (no value; distinct from `EQU`) |
//! | `IFDEF name` / `IFNDEF name` ... `ELSE` ... `ENDIF` | keep or drop lines depending on whether `name` is defined |
//! | `IF expr` ... `ELSE` ... `ENDIF` | keep or drop lines depending on a constant expression (non-zero is true) |
//! | `name MACRO p1, p2` ... `ENDM` | define a macro with parameters (see "Macros" below) |
//! | `REPT count` ... `ENDR` | repeat the lines in between `count` times |
//! | `CALL addr` | `SEP R4` then `DW addr` (standard call technique) |
//! | `RETURN` / `RETN` | `SEP R5` |
//! | `SEP R7 x` | `SEP R7` then `DB x` (Tiny BASIC "FECH" macro; written with one space) |
//!
//! Immediate operands are truncated to 8 bits, so `LDI BUFF` loads the low
//! byte of `BUFF`. Short branches must target the page holding their
//! operand byte. Expressions are described in the `expr` module docs.
//!
//! # Tiny BASIC IL
//!
//! Tom Pittman's interpretive-language mnemonics (from the *Tiny BASIC
//! Experimenter's Kit*) can be mixed with 1802 code:
//!
//! - A mnemonic that exists only in the IL (`PC`, `BC`, `JS`, `SX`, ...)
//!   switches to IL mode; one that exists only for the 1802 (`LDI`, `CALL`,
//!   ...) switches back. `BR` and `DS` follow the current mode. `DB`, `DW`,
//!   `EQU`, `ORG` and `END` work in both and don't switch.
//! - `J` / `JS` targets are encoded relative to the first IL instruction in
//!   the file (the IL start Tiny BASIC keeps at `AIL`), in 11 bits.
//! - `BR` reaches 31 bytes either way from the byte after the opcode; `BC`,
//!   `BV`, `BN` and `BE` reach 1-31 bytes forward. An operand of `*` encodes
//!   the error-stop branch (offset 0).
//! - `PC` and `BC` strings may use any delimiter except `^`. `X^` stands for
//!   `X` minus 40 hex (`J^` is a line feed). The last byte gets bit 7 set.
//! - Remarks such as `BC * !20 '='` (the error number) are skipped.
//! - `LB` takes a byte, `LN` a word, `SX` a depth 0-7.
//!
//! # Macros and repetition
//!
//! ```text
//! ALU_PBYTE .MACRO OP, OFFSET
//!         LDI     LOW(OFFSET)
//!         PLO     R7
//!         SEX     R7
//!         OP                      ; D = D <OP> M(R7)
//!         SEX     R2
//!         .ENDM
//!
//!         ALU_PBYTE ADD, 7        ; expands to the five lines above
//!
//!         .REPT 4
//!         NOP
//!         .ENDR
//! ```
//!
//! Like conditional assembly, both are plain text expansion done before
//! anything is parsed as an instruction. The leading dot on `MACRO`, `ENDM`,
//! `REPT` and `ENDR` is optional, and they are not case sensitive.
//!
//! - **Definition.** The macro name is the column-1 label of the `MACRO`
//!   line, followed by comma-separated parameter names. A macro has to be
//!   defined before it is used. Defining a name twice is an error.
//! - **Call.** Write the macro name where a mnemonic goes, with its
//!   arguments separated by commas (spaces are allowed after a comma). An
//!   argument may contain spaces, and commas inside parentheses or quotes
//!   do not split it. The argument count must match. Unlike an instruction
//!   line, text after the last argument is not a comment unless it starts
//!   with `;`. A label in front of the call labels the first expanded line.
//!   A macro name wins over a mnemonic of the same name.
//! - **Substitution.** In the body, each parameter is replaced by its
//!   argument wherever it appears as a whole word, in code only (not in a
//!   `;` comment, a quoted string, or a `#` hex literal such as `#F0`).
//!   Arguments are text, not values, so an argument can be a mnemonic
//!   (`OP` above), a register, or any expression.
//! - **Local labels.** `\@` in a body is replaced by `_` and a number that
//!   is different for every expansion, so `LOOP\@:` and `BNZ LOOP\@` make a
//!   label that is unique to each use of the macro.
//! - **Nesting.** A macro can call other macros, use `REPT`, and contain
//!   `IFDEF`/`IFNDEF`. Nesting (including a macro that calls itself) is
//!   limited to 16 levels, and all expansions together to a million lines.
//! - **`REPT count`.** The count is a constant expression (numbers and
//!   operators, with no symbols, since no symbol has a value yet) from 0 to
//!   65536. Parameters of an enclosing macro may be used in it. `REPT`
//!   blocks nest, and `\@` is different in every iteration.
//! - **Listing and errors.** The call (or `REPT`) line is shown in the
//!   listing as a comment, followed by the expanded lines, which all carry
//!   the call's line number, as do any errors in them.
//!
//! # Conditional assembly
//!
//! `DEFINE`, `IFDEF`, `IFNDEF`, `IF`, `ELSE` and `ENDIF` (each may be written
//! with a leading dot, as `.IF`) are resolved as text, in
//! one pass over the file (the same pass that expands `INCLUDE`), before
//! anything is parsed as an instruction. This lets one shared, `INCLUDE`d
//! file adapt to different callers: the including file `DEFINE`s a name
//! before the `INCLUDE`, and the included file branches on it with `IFDEF`.
//! `ELSE` and nesting work as usual; a name only has to be defined, its
//! value (if any, from a separate `EQU`) doesn't matter. Names given to
//! [`assemble_with`] / [`assemble_file`] are defined from the start, as a
//! command-line `-D` would be.

mod expr;
mod macros;
mod opcodes;

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use expr::{eval, is_symbol_char, is_symbol_start, parse_string, Symbols};
use macros::{BlockLine, Macro};
use opcodes::Kind;

const MAX_PASSES: usize = 20;
const LISTING_BYTES_PER_LINE: usize = 4;

/// An error tied to a source line (1-based; 0 means the whole file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Source file name (empty for [`assemble`] input).
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.file.is_empty() {
            write!(f, "line {}: {}", self.line, self.message)
        } else {
            write!(f, "{}:{}: {}", self.file, self.line, self.message)
        }
    }
}

/// One source line as it appears in the listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingLine {
    /// Source file name (empty for [`assemble`] input).
    pub file: String,
    pub line: usize,
    /// Address of the first byte, for lines that generate code or define a label.
    pub addr: Option<u16>,
    pub bytes: Vec<u8>,
    /// Value of an `EQU`.
    pub value: Option<i64>,
    pub source: String,
    pub errors: Vec<String>,
}

/// A defined symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Spelling at the point of definition.
    pub name: String,
    pub value: i64,
    /// Defining file and line.
    pub file: String,
    pub line: usize,
}

/// Result of assembling a source file.
#[derive(Debug, Clone)]
pub struct Assembly {
    memory: Vec<Option<u8>>,
    pub errors: Vec<Diagnostic>,
    pub listing: Vec<ListingLine>,
    /// Sorted by upper-case name.
    pub symbols: Vec<Symbol>,
}

impl Assembly {
    /// True when there were no errors.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Byte at `addr`, if the program wrote one there.
    pub fn byte(&self, addr: u16) -> Option<u8> {
        self.memory[usize::from(addr)]
    }

    /// Contiguous runs of written bytes, in address order.
    pub fn segments(&self) -> Vec<(u16, Vec<u8>)> {
        let mut out: Vec<(u16, Vec<u8>)> = Vec::new();
        let mut cur: Option<(u16, Vec<u8>)> = None;
        for (addr, b) in self.memory.iter().enumerate() {
            match (b, cur.as_mut()) {
                (Some(b), Some((_, data))) => data.push(*b),
                (Some(b), None) => cur = Some((addr as u16, vec![*b])),
                (None, Some(_)) => out.extend(cur.take()),
                (None, None) => {}
            }
        }
        out.extend(cur);
        out
    }

    /// Lowest and highest written addresses.
    pub fn range(&self) -> Option<(u16, u16)> {
        let lo = self.memory.iter().position(Option::is_some)?;
        let hi = self.memory.iter().rposition(Option::is_some)?;
        Some((lo as u16, hi as u16))
    }

    /// Flat image from the lowest to the highest written address, with gaps
    /// filled with `00`. Returns the start address and the bytes.
    pub fn to_bin(&self) -> Option<(u16, Vec<u8>)> {
        let (lo, hi) = self.range()?;
        let data = self.memory[usize::from(lo)..=usize::from(hi)]
            .iter()
            .map(|b| b.unwrap_or(0))
            .collect();
        Some((lo, data))
    }

    /// Intel HEX: 16-byte data records per segment and an end-of-file record.
    pub fn to_hex(&self) -> String {
        let mut out = String::new();
        for (base, data) in self.segments() {
            for (k, chunk) in data.chunks(16).enumerate() {
                let addr = base.wrapping_add((k * 16) as u16);
                let mut sum = chunk.len() as u8;
                sum = sum.wrapping_add((addr >> 8) as u8).wrapping_add(addr as u8);
                let _ = write!(out, ":{:02X}{addr:04X}00", chunk.len());
                for b in chunk {
                    let _ = write!(out, "{b:02X}");
                    sum = sum.wrapping_add(*b);
                }
                let _ = writeln!(out, "{:02X}", sum.wrapping_neg());
            }
        }
        out.push_str(":00000001FF\n");
        out
    }

    /// Listing (line, address, bytes, source, errors) followed by the symbol table.
    pub fn listing_text(&self) -> String {
        let mut out = String::new();
        out.push_str(" LINE  ADDR  CODE         SOURCE\n");
        let mut file = self.listing.first().map(|l| l.file.as_str());
        for l in &self.listing {
            if file != Some(l.file.as_str()) {
                file = Some(&l.file);
                let _ = writeln!(out, "                          ---- {} ----", l.file);
            }
            let addr = l.addr.map(|a| format!("{a:04X}")).unwrap_or_default();
            let mut chunks = l.bytes.chunks(LISTING_BYTES_PER_LINE);
            let code = match (l.value, chunks.next()) {
                (Some(v), _) => format!("= {:04X}", v & 0xFFFF),
                (None, Some(c)) => hex_bytes(c),
                (None, None) => String::new(),
            };
            let _ = writeln!(out, "{:5}  {addr:4}  {code:<11}  {}", l.line, l.source);
            for (k, c) in chunks.enumerate() {
                let a = l.addr.unwrap_or(0) as usize + (k + 1) * LISTING_BYTES_PER_LINE;
                let _ = writeln!(out, "       {:04X}  {}", a & 0xFFFF, hex_bytes(c));
            }
            for e in &l.errors {
                let _ = writeln!(out, "*****  error: {e}");
            }
        }
        let _ = writeln!(out, "\n{} error(s)\n\nSYMBOLS\n", self.errors.len());
        for s in &self.symbols {
            let _ = writeln!(out, "{:<16} {:04X}", s.name, s.value & 0xFFFF);
        }
        out
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads an included file: given the name written in the `INCLUDE` line and
/// the name of the file containing it, returns the included file's name (as
/// it should appear in messages) and its text.
pub type Loader<'a> = dyn FnMut(&str, &str) -> Result<(String, String), String> + 'a;

const MAX_INCLUDE_DEPTH: usize = 16;

/// Assemble `source`. `INCLUDE` is not available (use [`assemble_with`] or
/// [`assemble_file`]). Always returns a result; check [`Assembly::ok`].
pub fn assemble(source: &str) -> Assembly {
    assemble_with("", source, &[], &mut |_, _| {
        Err("INCLUDE needs a file to include from".into())
    })
}

/// Assemble the file at `path`. Included files are looked up relative to the
/// including file, then in each of `include_dirs`. `defines` are the names
/// `IFDEF` sees as defined from the start (a command-line `-D`). Fails only
/// if `path` itself can't be read.
pub fn assemble_file(
    path: &Path,
    include_dirs: &[PathBuf],
    defines: &[&str],
) -> Result<Assembly, String> {
    let name = path.display().to_string();
    let text = std::fs::read(path)
        .map(decode_source)
        .map_err(|e| format!("{name}: {e}"))?;
    Ok(assemble_with(&name, &text, defines, &mut |req, from| {
        let req_path = Path::new(req);
        let mut candidates = Vec::new();
        if req_path.is_absolute() {
            candidates.push(req_path.to_path_buf());
        } else {
            let dir = Path::new(from).parent().unwrap_or(Path::new(""));
            candidates.push(dir.join(req_path));
            candidates.extend(include_dirs.iter().map(|d| d.join(req_path)));
        }
        for c in &candidates {
            if let Ok(bytes) = std::fs::read(c) {
                return Ok((c.display().to_string(), decode_source(bytes)));
            }
        }
        let tried: Vec<String> = candidates.iter().map(|c| c.display().to_string()).collect();
        Err(format!("can't find `{req}` (tried {})", tried.join(", ")))
    }))
}

/// Source text from file bytes: UTF-8, or else Latin-1 (old sources are often
/// Latin-1 or CP437).
pub fn decode_source(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => e.into_bytes().iter().map(|&b| b as char).collect(),
    }
}

/// Assemble `source`, named `name` in messages, reading `INCLUDE`d files
/// through `load`. `defines` are the names `IFDEF` sees as defined from the
/// start (a command-line `-D`).
pub fn assemble_with(name: &str, source: &str, defines: &[&str], load: &mut Loader) -> Assembly {
    let mut files = vec![name.to_string()];
    let mut lines = Vec::new();
    let mut stack = vec![name.to_string()];
    let mut defined: std::collections::HashSet<String> =
        defines.iter().map(|d| d.to_ascii_uppercase()).collect();
    let mut cond = Vec::new();
    let mut mac = MacroState::default();
    expand(
        0,
        source,
        &mut files,
        &mut stack,
        load,
        &mut lines,
        &mut defined,
        &mut cond,
        &mut mac,
        0,
        None,
    );
    let mut asm = assemble_lines(&files, &lines);
    if !cond.is_empty() {
        asm.errors.push(Diagnostic {
            file: files[0].clone(),
            line: 0,
            message: format!("{} unterminated IFDEF/IFNDEF (missing ENDIF)", cond.len()),
        });
    }
    asm
}

/// One line of the program after includes are expanded.
struct SrcLine {
    file: usize,
    line: usize,
    text: String,
    /// Problem with an `INCLUDE` on this line.
    error: Option<String>,
}

/// One open `IFDEF`/`IFNDEF` in [`expand`].
struct CondFrame {
    /// Whether this frame's current branch (considered alone) is kept.
    taking: bool,
    seen_else: bool,
}

/// The deepest nesting of macro calls and `REPT` blocks.
const MAX_MACRO_DEPTH: usize = 16;
/// The most lines all macro calls and `REPT`s together may generate.
const MAX_GENERATED_LINES: usize = 1_000_000;
/// The most iterations one `REPT` may ask for.
const MAX_REPT_COUNT: i64 = 0x10000;

/// Macro definitions and expansion bookkeeping, shared across all files.
#[derive(Default)]
struct MacroState {
    defs: std::collections::HashMap<String, std::rc::Rc<Macro>>,
    /// Source of the number `\@` expands to (one per expansion).
    next_id: usize,
    /// Lines generated so far.
    generated: usize,
}

impl MacroState {
    /// Checks the nesting depth and size limits before one more expansion
    /// of `lines` lines.
    fn begin(&mut self, depth: usize, lines: usize) -> Result<(), String> {
        if depth >= MAX_MACRO_DEPTH {
            return Err(format!(
                "macros and REPT nested more than {MAX_MACRO_DEPTH} deep (recursive macro?)"
            ));
        }
        self.generated += lines.max(1);
        if self.generated > MAX_GENERATED_LINES {
            return Err(format!(
                "macros and REPT generated more than {MAX_GENERATED_LINES} lines"
            ));
        }
        Ok(())
    }
}

/// A listing-only line (a comment) standing in for a directive that was
/// consumed, carrying `error` if there is one.
fn note_line(file: usize, line: usize, text: &str, error: Option<String>) -> SrcLine {
    SrcLine {
        file,
        line,
        text: format!("; {}", text.trim()),
        error,
    }
}

/// Collects the body of a `MACRO` (`is_macro`) or `REPT` whose first body
/// line is `lines[start]`, up to the matching `ENDM` / `ENDR`. Returns the
/// body, the index of the line after the terminator, and whether the
/// terminator was found.
fn collect_block(lines: &[&str], start: usize, is_macro: bool) -> (Vec<String>, usize, bool) {
    let mut depth = 1usize;
    let mut body = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let l = lines[i];
        i += 1;
        match (macros::block_line(l), is_macro) {
            (Some(BlockLine::MacroDef { .. }), true) | (Some(BlockLine::Rept(_)), false) => {
                depth += 1;
            }
            (Some(BlockLine::EndM), true) | (Some(BlockLine::EndR), false) => {
                depth -= 1;
                if depth == 0 {
                    return (body, i, true);
                }
            }
            _ => {}
        }
        body.push(l.to_string());
    }
    (body, i, false)
}

/// Registers a macro, or says what is wrong with the definition.
fn define_macro(
    mac: &mut MacroState,
    name: Option<String>,
    params: Vec<String>,
    body: Vec<String>,
    closed: bool,
) -> Result<(), String> {
    let name = name.ok_or("MACRO needs a name (write `NAME MACRO params`)")?;
    if !macros::is_name(&name) {
        return Err(format!("bad macro name `{name}`"));
    }
    if !closed {
        return Err(format!("MACRO `{name}` has no matching ENDM"));
    }
    for (i, p) in params.iter().enumerate() {
        if !macros::is_name(p) {
            return Err(format!("bad parameter `{p}` for macro `{name}`"));
        }
        if params[..i].iter().any(|q| q.eq_ignore_ascii_case(p)) {
            return Err(format!("parameter `{p}` repeated in macro `{name}`"));
        }
    }
    let key = name.to_ascii_uppercase();
    if mac.defs.contains_key(&key) {
        return Err(format!("macro `{name}` is already defined"));
    }
    mac.defs
        .insert(key, std::rc::Rc::new(Macro { params, body }));
    Ok(())
}

/// The body lines with parameters and `\@` substituted, as one text.
fn join_lines(body: &[String], params: &[String], args: &[String], id: usize) -> String {
    body.iter()
        .map(|l| macros::substitute(l, params, args, id))
        .collect::<Vec<_>>()
        .join("\n")
}

struct NoSymbols;

impl Symbols for NoSymbols {
    fn lookup(&self, _: &str) -> Option<i64> {
        None
    }
}

/// The value of a `REPT` count. It is evaluated while the text is being
/// expanded, before any symbol has a value, so it has to be a constant.
fn rept_count(text: &str) -> Result<usize, String> {
    let v = eval(text, 0, &NoSymbols).map_err(|e| format!("REPT count: {e}"))?;
    if let Some(name) = v.undefined {
        return Err(format!(
            "REPT count must be a constant expression (`{name}` is not known yet)"
        ));
    }
    if !(0..=MAX_REPT_COUNT).contains(&v.value) {
        return Err(format!(
            "REPT count {} out of range (0 to {MAX_REPT_COUNT})",
            v.value
        ));
    }
    Ok(v.value as usize)
}

/// The truth of an `IF` condition (non-zero is true). Like a `REPT` count,
/// it is evaluated during expansion, so it can't use symbols.
fn if_value(text: &str) -> Result<bool, String> {
    let v = eval(text, 0, &NoSymbols).map_err(|e| format!("IF: {e}"))?;
    match v.undefined {
        Some(name) => Err(format!(
            "IF needs a constant expression (`{name}` is not known yet; use IFDEF to test a name)"
        )),
        None => Ok(v.value != 0),
    }
}

/// A `DEFINE` / `IFDEF` / `IFNDEF` / `ELSE` / `ENDIF` line.
enum CondLine<'a> {
    Define(&'a str),
    /// `IF expr`: the expression text.
    If(&'a str),
    IfDef(&'a str),
    IfNDef(&'a str),
    Else,
    EndIf,
}

/// If `text` is a conditional-assembly directive, parse it. `None` for any
/// other line, including a plain `DEFINE`/`IFDEF` with a bad or missing name
/// (reported as a normal unknown-mnemonic error instead).
fn cond_line(text: &str) -> Option<CondLine<'_>> {
    // These directives never have a label, so just look at the first word
    // after any indentation.
    let trimmed = text.trim_start();
    let mut parts = trimmed.splitn(2, [' ', '\t']);
    let word = parts.next().unwrap_or("");
    // Like the other directives, each may be written with a leading dot.
    let up = word.strip_prefix('.').unwrap_or(word).to_ascii_uppercase();
    let rest = parts.next().unwrap_or("");
    let arg = rest
        .trim()
        .split(|c: char| c == ';' || is_ws(c as u8))
        .next()
        .unwrap_or("");
    let name_ok =
        !arg.is_empty() && is_symbol_start(arg.as_bytes()[0]) && arg.bytes().all(is_symbol_char);
    match up.as_str() {
        "DEFINE" if name_ok => Some(CondLine::Define(arg)),
        "IF" => Some(CondLine::If(macros::strip_comment(rest).trim())),
        "IFDEF" if name_ok => Some(CondLine::IfDef(arg)),
        "IFNDEF" if name_ok => Some(CondLine::IfNDef(arg)),
        "ELSE" => Some(CondLine::Else),
        "ENDIF" => Some(CondLine::EndIf),
        _ => None,
    }
}

/// Append the lines of `source` (file number `file`) to `out`, replacing
/// each `INCLUDE` line's position with the included file's lines (the
/// `INCLUDE` line itself is kept, for the listing and any label on it).
/// `DEFINE`/`IFDEF`/`IFNDEF`/`ELSE`/`ENDIF` lines are resolved here and never
/// reach `out`; lines inside a false branch are dropped entirely (their
/// `INCLUDE`s are not followed).
#[allow(clippy::too_many_arguments)]
fn expand(
    file: usize,
    source: &str,
    files: &mut Vec<String>,
    stack: &mut Vec<String>,
    load: &mut Loader,
    out: &mut Vec<SrcLine>,
    defines: &mut std::collections::HashSet<String>,
    cond: &mut Vec<CondFrame>,
    mac: &mut MacroState,
    depth: usize,
    fixed_line: Option<usize>,
) {
    let src_lines: Vec<&str> = source.lines().collect();
    let mut idx = 0;
    while idx < src_lines.len() {
        let text = src_lines[idx];
        // Lines produced by a macro or REPT all report the line that
        // started it.
        let line = fixed_line.unwrap_or(idx + 1);
        idx += 1;
        let active = cond.iter().all(|f| f.taking);

        if let Some(block) = macros::block_line(text) {
            match block {
                BlockLine::MacroDef { name, params } => {
                    let (body, next, closed) = collect_block(&src_lines, idx, true);
                    idx = next;
                    if active {
                        if let Err(e) = define_macro(mac, name, params, body, closed) {
                            out.push(note_line(file, line, text, Some(e)));
                        }
                    }
                }
                BlockLine::Rept(count) => {
                    let (body, next, closed) = collect_block(&src_lines, idx, false);
                    idx = next;
                    if active {
                        let count = if closed {
                            rept_count(&count)
                        } else {
                            Err("REPT has no matching ENDR".to_string())
                        };
                        match count {
                            Err(e) => out.push(note_line(file, line, text, Some(e))),
                            Ok(count) => {
                                out.push(note_line(file, line, text, None));
                                for _ in 0..count {
                                    if let Err(e) = mac.begin(depth, body.len()) {
                                        out.push(note_line(file, line, text, Some(e)));
                                        break;
                                    }
                                    let id = mac.next_id;
                                    mac.next_id += 1;
                                    let expanded = join_lines(&body, &[], &[], id);
                                    expand(
                                        file,
                                        &expanded,
                                        files,
                                        stack,
                                        load,
                                        out,
                                        defines,
                                        cond,
                                        mac,
                                        depth + 1,
                                        Some(line),
                                    );
                                }
                            }
                        }
                    }
                }
                BlockLine::EndM | BlockLine::EndR => {
                    if active {
                        let msg = if block == BlockLine::EndM {
                            "ENDM without MACRO"
                        } else {
                            "ENDR without REPT"
                        };
                        out.push(note_line(file, line, text, Some(msg.into())));
                    }
                }
            }
            continue;
        }

        match cond_line(text) {
            Some(CondLine::Define(name)) => {
                if active {
                    defines.insert(name.to_ascii_uppercase());
                }
                continue;
            }
            Some(CondLine::If(expr)) => {
                // Only evaluated where it is live: a false branch may
                // hold anything.
                let mut taking = false;
                if active {
                    match if_value(expr) {
                        Ok(v) => taking = v,
                        Err(e) => out.push(note_line(file, line, text, Some(e))),
                    }
                }
                cond.push(CondFrame {
                    taking,
                    seen_else: false,
                });
                continue;
            }
            Some(CondLine::IfDef(name)) => {
                let taking = defines.contains(&name.to_ascii_uppercase());
                cond.push(CondFrame {
                    taking,
                    seen_else: false,
                });
                continue;
            }
            Some(CondLine::IfNDef(name)) => {
                let taking = !defines.contains(&name.to_ascii_uppercase());
                cond.push(CondFrame {
                    taking,
                    seen_else: false,
                });
                continue;
            }
            Some(CondLine::Else) => {
                match cond.last_mut() {
                    Some(f) if !f.seen_else => {
                        f.taking = !f.taking;
                        f.seen_else = true;
                    }
                    Some(_) => out.push(SrcLine {
                        file,
                        line,
                        text: text.to_string(),
                        error: Some("a second ELSE for the same IFDEF/IFNDEF".into()),
                    }),
                    None => out.push(SrcLine {
                        file,
                        line,
                        text: text.to_string(),
                        error: Some("ELSE without IFDEF/IFNDEF".into()),
                    }),
                }
                continue;
            }
            Some(CondLine::EndIf) => {
                if cond.pop().is_none() {
                    out.push(SrcLine {
                        file,
                        line,
                        text: text.to_string(),
                        error: Some("ENDIF without IFDEF/IFNDEF".into()),
                    });
                }
                continue;
            }
            None => {}
        }
        if !active {
            continue;
        }

        if let Some(f) = macros::fields(text) {
            let found = mac.defs.get(&f.word.to_ascii_uppercase()).cloned();
            if let (false, Some(m)) = (f.word.is_empty(), found) {
                let args = macros::split_args(f.rest);
                let err = if args.len() != m.params.len() {
                    Err(format!(
                        "macro `{}` takes {} argument(s), got {}",
                        f.word,
                        m.params.len(),
                        args.len()
                    ))
                } else {
                    mac.begin(depth, m.body.len())
                };
                // The call itself stays in the listing as a comment; a
                // label on it still defines the label.
                let shown = match f.label {
                    Some(l) => format!("{l} ; {}", text[l.len()..].trim()),
                    None => format!("; {}", text.trim()),
                };
                out.push(SrcLine {
                    file,
                    line,
                    text: shown,
                    error: err.clone().err(),
                });
                if err.is_ok() {
                    let id = mac.next_id;
                    mac.next_id += 1;
                    let expanded = join_lines(&m.body, &m.params, &args, id);
                    expand(
                        file,
                        &expanded,
                        files,
                        stack,
                        load,
                        out,
                        defines,
                        cond,
                        mac,
                        depth + 1,
                        Some(line),
                    );
                }
                continue;
            }
        }

        let target = include_target(text);
        out.push(SrcLine {
            file,
            line,
            text: text.to_string(),
            error: None,
        });
        let req = match target {
            None => continue,
            Some(Err(e)) => {
                out.last_mut().unwrap().error = Some(e);
                continue;
            }
            Some(Ok(req)) => req,
        };
        if stack.len() > MAX_INCLUDE_DEPTH {
            out.last_mut().unwrap().error = Some(format!(
                "includes nested more than {MAX_INCLUDE_DEPTH} deep"
            ));
            continue;
        }
        match load(&req, &files[file]) {
            Err(e) => out.last_mut().unwrap().error = Some(e),
            Ok((name, _)) if stack.iter().any(|s| same_file(s, &name)) => {
                out.last_mut().unwrap().error = Some(format!("`{name}` includes itself"));
            }
            Ok((name, text)) => {
                files.push(name.clone());
                stack.push(name);
                expand(
                    files.len() - 1,
                    &text,
                    files,
                    stack,
                    load,
                    out,
                    defines,
                    cond,
                    mac,
                    depth,
                    None,
                );
                stack.pop();
            }
        }
    }
}

fn same_file(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
    norm(a) == norm(b)
}

/// If `text` is an `INCLUDE` line, the file name it names.
fn include_target(text: &str) -> Option<Result<String, String>> {
    let s = text.as_bytes();
    if s.is_empty() || s[0] == b';' || s[0] == b'*' {
        return None;
    }
    let mut i = 0;
    while i < s.len() && !is_ws(s[i]) {
        i += 1; // label, if any
    }
    while i < s.len() && is_ws(s[i]) {
        i += 1;
    }
    let rest = &text[i..];
    let word_end = rest.find([' ', '\t']).unwrap_or(rest.len());
    if !rest[..word_end].eq_ignore_ascii_case("INCLUDE") {
        return None;
    }
    let arg = rest[word_end..].trim_start_matches([' ', '\t']);
    let name = match arg.chars().next() {
        Some(q @ ('"' | '\'')) => match arg[1..].find(q) {
            Some(end) => arg[1..1 + end].to_string(),
            None => return Some(Err("unterminated file name".into())),
        },
        Some(';') | None => return Some(Err("INCLUDE needs a file name".into())),
        Some(_) => arg
            .split([' ', '\t', ';'])
            .next()
            .unwrap_or_default()
            .to_string(),
    };
    if name.is_empty() {
        return Some(Err("INCLUDE needs a file name".into()));
    }
    Some(Ok(name))
}

fn assemble_lines(files: &[String], lines: &[SrcLine]) -> Assembly {
    let mut il_mode = false;
    let parsed: Vec<Parsed> = lines
        .iter()
        .map(|l| {
            let mut p = parse_line(&l.text, &mut il_mode);
            if l.error.is_some() {
                p.error.clone_from(&l.error);
            }
            p
        })
        .collect();

    // Size passes: repeat until every symbol keeps its value, so forward
    // references in EQU / ORG / DS settle.
    let mut prev = SymTab::new();
    let mut settled = false;
    for _ in 0..MAX_PASSES {
        let cur = Pass::new(&prev, lines, files, None).run(&parsed);
        let same = cur.len() == prev.len()
            && cur
                .iter()
                .all(|(k, s)| prev.get(k).is_some_and(|p| p.value == s.value));
        prev = cur;
        if same {
            settled = true;
            break;
        }
    }

    let mut out = Output {
        memory: vec![None; 0x10000],
        owner: vec![0; 0x10000],
        errors: Vec::new(),
        listing: Vec::new(),
    };
    let syms = Pass::new(&prev, lines, files, Some(&mut out)).run(&parsed);
    if !settled {
        out.errors.push(Diagnostic {
            file: files[0].clone(),
            line: 0,
            message: format!("symbol values did not settle after {MAX_PASSES} passes"),
        });
    }
    let mut symbols: Vec<(String, Symbol)> = syms.into_iter().collect();
    symbols.sort_by(|a, b| a.0.cmp(&b.0));
    Assembly {
        memory: out.memory,
        errors: out.errors,
        listing: out.listing,
        symbols: symbols.into_iter().map(|(_, s)| s).collect(),
    }
}

// ---------------------------------------------------------------------------
// Line parsing

#[derive(Debug, Default)]
struct Parsed<'a> {
    label: Option<&'a str>,
    mnemonic: Option<&'a str>,
    kind: Option<Kind>,
    operand: Option<&'a str>,
    /// Encoded string of an IL `PC` / `BC`.
    data: Vec<u8>,
    /// Instruction from the Tiny BASIC IL.
    il: bool,
    error: Option<String>,
}

fn is_ws(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Parse one line. `il_mode` tracks whether shared mnemonics (`BR`, `DS`)
/// mean their Tiny BASIC IL form; it carries over from line to line.
fn parse_line<'a>(text: &'a str, il_mode: &mut bool) -> Parsed<'a> {
    let s = text.as_bytes();
    let mut p = Parsed::default();
    if s.is_empty() || s[0] == b';' || s[0] == b'*' {
        return p;
    }
    let mut i = 0;
    if !is_ws(s[0]) {
        while i < s.len() && !is_ws(s[i]) && s[i] != b';' {
            i += 1;
        }
        let mut label = &text[..i];
        if label.len() > 1 && label.ends_with(':') {
            label = &label[..label.len() - 1];
        }
        let b = label.as_bytes();
        if !is_symbol_start(b[0]) || !b.iter().all(|&c| is_symbol_char(c)) {
            p.error = Some(format!("bad label `{label}`"));
            return p;
        }
        p.label = Some(label);
    }
    while i < s.len() && is_ws(s[i]) {
        i += 1;
    }
    if i >= s.len() || s[i] == b';' {
        return p;
    }

    // "SEP R7" (one space) is a macro name in this dialect.
    let rest = &text[i..];
    let (mnemonic, kind, after) = if rest.len() > 6
        && rest[..6].eq_ignore_ascii_case("SEP R7")
        && is_ws(rest.as_bytes()[6])
    {
        *il_mode = false;
        (&rest[..6], Some(Kind::Fetch), i + 6)
    } else {
        let start = i;
        while i < s.len() && !is_ws(s[i]) && s[i] != b';' {
            i += 1;
        }
        let m = &text[start..i];
        // RCA/FIG style: directives may be written with a leading dot
        // (`.ORG`, `.DB`, ...). No real mnemonic starts with one, so this
        // is unambiguous; the mnemonic text kept for messages is unchanged.
        let up = m.strip_prefix('.').unwrap_or(m).to_ascii_uppercase();
        // Mnemonics only one language has switch modes; shared ones (BR, DS)
        // follow the current mode.
        let kind = if m.starts_with('=') {
            // RCA style `NAME = expr`, also written `NAME =expr`.
            i = start + 1;
            Some(Kind::Equ)
        } else {
            match (opcodes::lookup(&up), opcodes::il_lookup(&up)) {
                (Some(a), Some(b)) => Some(if *il_mode { b } else { a }),
                (Some(a), None) => {
                    if !a.is_neutral() {
                        *il_mode = false;
                    }
                    Some(a)
                }
                (None, Some(b)) => {
                    *il_mode = true;
                    Some(b)
                }
                (None, None) => None,
            }
        };
        p.il = *il_mode && kind.is_some_and(|k| !k.is_neutral());
        (&text[start..i], kind, i)
    };
    p.mnemonic = Some(mnemonic);
    p.kind = kind;
    let Some(kind) = kind else {
        p.error = Some(format!("unknown mnemonic `{mnemonic}`"));
        return p;
    };
    if !kind.takes_operand() {
        return p;
    }
    let mut tail = &text[after..];
    if kind != Kind::IlPc {
        let operand = take_operand(tail);
        if operand.is_empty() {
            p.error = Some(format!("`{mnemonic}` needs an operand"));
            return p;
        }
        p.operand = Some(operand);
        let end = operand.as_ptr() as usize - tail.as_ptr() as usize + operand.len();
        tail = &tail[end..];
    }
    if matches!(kind, Kind::IlPc | Kind::IlBc) {
        match il_string(tail) {
            Ok(bytes) => p.data = bytes,
            Err(e) => p.error = Some(format!("`{mnemonic}`: {e}")),
        }
    }
    p
}

/// The delimited string of an IL `PC` or `BC`, at the start of `s` (after
/// whitespace). Any non-blank character except `^` may delimit it; a
/// character followed by `^` has 40 hex subtracted (`'J^'` is a line feed);
/// the last byte gets bit 7 set. QELFEXE error-number remarks such as `!20`
/// before the string are skipped.
fn il_string(s: &str) -> Result<Vec<u8>, String> {
    let mut s = s.trim_start_matches([' ', '\t']);
    loop {
        let b = s.as_bytes();
        let n = b.iter().skip(1).take_while(|c| c.is_ascii_digit()).count();
        if b.first() == Some(&b'!') && n > 0 && b.get(1 + n).is_none_or(|&c| is_ws(c)) {
            s = s[1 + n..].trim_start_matches([' ', '\t']);
        } else {
            break;
        }
    }
    let mut chars = s.chars();
    let delim = match chars.next() {
        None | Some(';') => return Err("missing string".into()),
        Some('^') => return Err("`^` can't delimit a string".into()),
        Some(c) => c,
    };
    let mut out: Vec<u8> = Vec::new();
    let mut closed = false;
    for c in chars {
        if c == delim {
            closed = true;
            break;
        }
        if c == '^' {
            let last = out.last_mut().ok_or("`^` with no character before it")?;
            *last = last.wrapping_sub(0x40);
            continue;
        }
        let b = u8::try_from(u32::from(c)).map_err(|_| format!("character `{c}` is not 8-bit"))?;
        out.push(b);
    }
    if !closed {
        return Err(format!("unterminated string (delimiter `{delim}`)"));
    }
    let last = out.last_mut().ok_or("empty string")?;
    *last |= 0x80;
    Ok(out)
}

/// The operand field at the start of `s` (after optional whitespace). It ends
/// at whitespace or `;` outside quotes, except that whitespace after a comma
/// or after `LOW` / `HIGH` is skipped.
fn take_operand(s: &str) -> &str {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && is_ws(b[i]) {
        i += 1;
    }
    let start = i;
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' if !is_h_prefix_quote(b, i) => {
                let mut j = i;
                if parse_string(b, &mut j).is_some() {
                    i = j;
                } else {
                    // Unterminated: take the rest; evaluation reports it.
                    i = b.len();
                }
            }
            b';' => break,
            c if is_ws(c) => {
                let so_far = &s[start..i];
                if !(so_far.ends_with(',')
                    || ends_with_word(so_far, "LOW")
                    || ends_with_word(so_far, "HIGH"))
                {
                    break;
                }
                while i < b.len() && is_ws(b[i]) {
                    i += 1;
                }
                if i < b.len() && b[i] == b';' {
                    break;
                }
            }
            _ => i += 1,
        }
    }
    s[start..i].trim_end()
}

/// Whether the quote at `b[i]` is actually the `'` of an RCA/PL/M style
/// `H'4000` radix prefix (no closing quote), not the start of a quoted
/// string or character literal.
fn is_h_prefix_quote(b: &[u8], i: usize) -> bool {
    b[i] == b'\'' && i > 0 && matches!(b[i - 1], b'H' | b'h')
}

fn ends_with_word(s: &str, word: &str) -> bool {
    let n = word.len();
    s.len() >= n
        && s[s.len() - n..].eq_ignore_ascii_case(word)
        && !s[..s.len() - n].bytes().last().is_some_and(is_symbol_char)
}

/// Split a `DB` / `DW` operand at commas outside quotes.
fn split_items(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut items = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' if !is_h_prefix_quote(b, i) => {
                let mut j = i;
                i = if parse_string(b, &mut j).is_some() {
                    j
                } else {
                    b.len()
                };
            }
            b',' => {
                items.push(s[start..i].trim());
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    items.push(s[start..].trim());
    items
}

// ---------------------------------------------------------------------------
// Passes

/// Symbols keyed by upper-case name.
type SymTab = BTreeMap<String, Symbol>;

struct Output {
    memory: Vec<Option<u8>>,
    /// Line (index into the expanded lines, plus 1) that wrote each byte; 0 = none.
    owner: Vec<usize>,
    errors: Vec<Diagnostic>,
    listing: Vec<ListingLine>,
}

struct Pass<'a, 'o> {
    prev: &'a SymTab,
    cur: SymTab,
    /// Where each symbol in `cur` was defined (index into `lines`).
    defined_at: BTreeMap<String, usize>,
    lines: &'a [SrcLine],
    files: &'a [String],
    pc: u32,
    out: Option<&'o mut Output>,
    /// Errors for the line being assembled.
    errs: Vec<String>,
    /// Address of the first IL instruction; `J` / `JS` are relative to it.
    il_base: Option<u32>,
}

struct Lookup<'p> {
    cur: &'p SymTab,
    prev: &'p SymTab,
}

impl Symbols for Lookup<'_> {
    fn lookup(&self, name: &str) -> Option<i64> {
        let key = name.to_ascii_uppercase();
        if let Some(s) = self.cur.get(&key).or_else(|| self.prev.get(&key)) {
            return Some(s.value);
        }
        builtin_register(&key)
    }
}

fn builtin_register(key: &str) -> Option<i64> {
    let rest = key.strip_prefix('R')?;
    match rest.len() {
        1 => i64::from_str_radix(rest, 16).ok(),
        2 => rest.parse::<i64>().ok().filter(|n| (10..=15).contains(n)),
        _ => None,
    }
}

impl<'a, 'o> Pass<'a, 'o> {
    fn new(
        prev: &'a SymTab,
        lines: &'a [SrcLine],
        files: &'a [String],
        out: Option<&'o mut Output>,
    ) -> Self {
        Pass {
            prev,
            cur: SymTab::new(),
            defined_at: BTreeMap::new(),
            lines,
            files,
            pc: 0,
            out,
            errs: Vec::new(),
            il_base: None,
        }
    }

    fn final_pass(&self) -> bool {
        self.out.is_some()
    }

    fn run(mut self, parsed: &[Parsed]) -> SymTab {
        for (line, p) in parsed.iter().enumerate() {
            let here = self.pc;
            self.errs.clear();
            if let Some(e) = &p.error {
                self.errs.push(e.clone());
            }
            let mut bytes = Vec::new();
            let mut value = None;
            let mut addr = None;
            let mut stop = false;

            match p.kind {
                Some(Kind::Equ) => match p.label {
                    Some(l) => {
                        let v = self.eval(p.operand, here);
                        value = Some(v);
                        self.define(l, v, line);
                    }
                    // RCA/FIG style: `.EQU NAME, expr` names the symbol as
                    // the operand's first item instead of a label.
                    None => {
                        let parts = split_items(p.operand.unwrap_or_default());
                        let name = parts.first().copied().unwrap_or_default();
                        let ok = parts.len() == 2
                            && !name.is_empty()
                            && is_symbol_start(name.as_bytes()[0])
                            && name.bytes().all(is_symbol_char);
                        if ok {
                            let v = self.eval(Some(parts[1]), here);
                            value = Some(v);
                            self.define(name, v, line);
                        } else {
                            self.errs.push("EQU needs a label".into());
                        }
                    }
                },
                Some(Kind::Org) => {
                    let v = self.eval(p.operand, here);
                    if (0..=0xFFFF).contains(&v) {
                        self.pc = v as u32;
                    } else {
                        self.errs.push(format!("ORG address {v} out of range"));
                    }
                    addr = Some(self.pc as u16);
                    if let Some(l) = p.label {
                        self.define(l, i64::from(self.pc), line);
                    }
                }
                kind => {
                    if let Some(l) = p.label {
                        self.define(l, i64::from(here), line);
                    }
                    if p.il && self.il_base.is_none() {
                        self.il_base = Some(here);
                    }
                    if let Some(kind) = kind {
                        let (b, reserve) = self.encode(kind, p, here);
                        bytes = b;
                        self.pc += (bytes.len() as u32) + reserve;
                        stop = kind == Kind::End;
                    }
                    if p.label.is_some() || !bytes.is_empty() || self.pc != here {
                        addr = Some(here as u16);
                    }
                }
            }

            if self.final_pass() {
                self.emit(line, here, bytes, addr, value);
            }
            if stop {
                break;
            }
        }
        self.cur
    }

    /// Where line `idx` is, as seen from line `from`: "line N", or
    /// "file:N" when it's in a different file.
    fn location(&self, idx: usize, from: usize) -> String {
        let l = &self.lines[idx];
        if l.file == self.lines[from].file {
            format!("line {}", l.line)
        } else {
            format!("{}:{}", self.files[l.file], l.line)
        }
    }

    fn define(&mut self, name: &str, value: i64, idx: usize) {
        let key = name.to_ascii_uppercase();
        if let Some(&old) = self.defined_at.get(&key) {
            let at = self.location(old, idx);
            self.errs
                .push(format!("`{name}` is already defined at {at}"));
            return;
        }
        let src = &self.lines[idx];
        self.defined_at.insert(key.clone(), idx);
        self.cur.insert(
            key,
            Symbol {
                name: name.to_string(),
                value,
                file: self.files[src.file].clone(),
                line: src.line,
            },
        );
    }

    /// Evaluate an operand. Errors are recorded (and the value taken as 0).
    fn eval(&mut self, text: Option<&str>, here: u32) -> i64 {
        let Some(text) = text else { return 0 };
        let lookup = Lookup {
            cur: &self.cur,
            prev: self.prev,
        };
        match eval(text, i64::from(here), &lookup) {
            Ok(v) => {
                if let Some(name) = v.undefined {
                    self.errs.push(format!("undefined symbol `{name}`"));
                }
                v.value
            }
            Err(e) => {
                self.errs.push(e);
                0
            }
        }
    }

    fn word(&mut self, text: Option<&str>, here: u32) -> [u8; 2] {
        let v = self.eval(text, here);
        if !(-0x8000..=0xFFFF).contains(&v) {
            self.errs.push(format!("value {v} does not fit in 16 bits"));
        }
        [(v >> 8) as u8, v as u8]
    }

    fn byte(&mut self, text: Option<&str>, here: u32) -> u8 {
        let v = self.eval(text, here);
        if !(-0x8000..=0xFFFF).contains(&v) {
            self.errs.push(format!("value {v} does not fit in 16 bits"));
        }
        v as u8
    }

    fn ranged(&mut self, text: Option<&str>, here: u32, lo: i64, hi: i64, what: &str) -> u8 {
        let v = self.eval(text, here);
        if (lo..=hi).contains(&v) {
            v as u8
        } else {
            self.errs
                .push(format!("{what} {v} out of range ({lo}-{hi})"));
            lo as u8
        }
    }

    /// Offset of an IL relative branch from the byte after the opcode, which
    /// must be in `min..=31` and not 0. `*` gives 0, the error-stop branch.
    fn il_offset(&mut self, operand: Option<&str>, here: u32, min: i64) -> i64 {
        let Some(text) = operand else { return 0 };
        if text.trim() == "*" {
            return 0;
        }
        let target = self.eval(operand, here);
        let d = target - (i64::from(here) + 1);
        if d == 0 || !(min..=31).contains(&d) {
            let range = if min < 0 {
                "31 bytes forward or back"
            } else {
                "1-31 bytes forward"
            };
            self.errs.push(format!(
                "IL branch target {:04X} is out of range (offset {d}; allowed {range} from {:04X})",
                target & 0xFFFF,
                here + 1
            ));
            return 0;
        }
        d
    }

    /// Bytes for one statement, plus a count of bytes reserved without data.
    fn encode(&mut self, kind: Kind, p: &Parsed, here: u32) -> (Vec<u8>, u32) {
        let operand = p.operand;
        let bytes = match kind {
            Kind::IlSx => vec![self.ranged(operand, here, 0, 7, "SX depth")],
            Kind::IlLb => vec![0x09, self.byte(operand, here)],
            Kind::IlLn => {
                let [hi, lo] = self.word(operand, here);
                vec![0x0A, hi, lo]
            }
            Kind::IlPc => [&[0x24], p.data.as_slice()].concat(),
            Kind::IlJump(op) => {
                let target = self.eval(operand, here);
                let base = i64::from(self.il_base.unwrap_or(here));
                let rel = target - base;
                if !(0..=0x7FF).contains(&rel) && operand.is_some() {
                    self.errs.push(format!(
                        "IL jump target {:04X} is not within 7FF bytes after the IL start {base:04X}",
                        target & 0xFFFF
                    ));
                }
                vec![op | ((rel >> 8) as u8 & 0x07), rel as u8]
            }
            Kind::IlBr => vec![0x60u8.wrapping_add(self.il_offset(operand, here, -31) as u8)],
            Kind::IlBranch(op) => vec![op + self.il_offset(operand, here, 1) as u8],
            Kind::IlBc => {
                let d = self.il_offset(operand, here, 1) as u8;
                [&[0x80 + d], p.data.as_slice()].concat()
            }
            Kind::Implied(op) => vec![op],
            Kind::Reg(op) => vec![op | self.ranged(operand, here, 0, 15, "register")],
            Kind::RegNotZero(op) => {
                let r = self.eval(operand, here);
                if r == 0 && operand.is_some() {
                    self.errs
                        .push("LDN R0 does not exist (opcode 00 is IDL)".into());
                }
                if !(0..=15).contains(&r) {
                    self.errs.push(format!("register {r} out of range (0-15)"));
                }
                vec![op | (r as u8 & 0x0F)]
            }
            Kind::Port(op) => vec![op + self.ranged(operand, here, 1, 7, "port")],
            Kind::Imm(op) => vec![op, self.byte(operand, here)],
            Kind::Short(op) => {
                let [hi, lo] = self.word(operand, here);
                let operand_page = ((here + 1) >> 8) & 0xFF;
                if u32::from(hi) != operand_page && operand.is_some() {
                    self.errs.push(format!(
                        "short branch target {:04X} is not in page {operand_page:02X}",
                        u16::from_be_bytes([hi, lo])
                    ));
                }
                vec![op, lo]
            }
            Kind::Long(op) => {
                let [hi, lo] = self.word(operand, here);
                vec![op, hi, lo]
            }
            Kind::Call => {
                let [hi, lo] = self.word(operand, here);
                vec![0xD4, hi, lo]
            }
            Kind::Fetch => vec![0xD7, self.byte(operand, here)],
            Kind::Db => {
                let mut out = Vec::new();
                for item in split_items(operand.unwrap_or_default()) {
                    let b = item.as_bytes();
                    if item.is_empty() {
                        self.errs.push("empty DB item".into());
                        continue;
                    }
                    // RCA style `#01020300`: a run of hex digit pairs is
                    // that many bytes.
                    if b[0] == b'#' && b.len() > 3 && b[1..].iter().all(u8::is_ascii_hexdigit) {
                        let digits = &item[1..];
                        let padded = if digits.len() % 2 == 1 {
                            format!("0{digits}")
                        } else {
                            digits.to_string()
                        };
                        for k in (0..padded.len()).step_by(2) {
                            out.push(u8::from_str_radix(&padded[k..k + 2], 16).unwrap());
                        }
                        continue;
                    }
                    if matches!(b[0], b'\'' | b'"') {
                        let mut j = 0;
                        if let Some(s) = parse_string(b, &mut j) {
                            if j == b.len() && s.len() != 1 {
                                out.extend(s);
                                continue;
                            }
                        }
                    }
                    let here_now = here + out.len() as u32;
                    out.push(self.byte(Some(item), here_now));
                }
                out
            }
            Kind::Dw => {
                let mut out = Vec::new();
                for item in split_items(operand.unwrap_or_default()) {
                    if item.is_empty() {
                        self.errs.push("empty DW item".into());
                        continue;
                    }
                    let here_now = here + out.len() as u32;
                    out.extend(self.word(Some(item), here_now));
                }
                out
            }
            Kind::Ds => {
                let n = self.eval(operand, here);
                if !(0..=0x10000).contains(&n) {
                    self.errs.push(format!("DS size {n} out of range"));
                    return (Vec::new(), 0);
                }
                return (Vec::new(), n as u32);
            }
            Kind::Align => {
                let n = self.eval(operand, here);
                if !(1..=0x10000).contains(&n) {
                    self.errs
                        .push(format!("ALIGN size {n} out of range (must be at least 1)"));
                    return (Vec::new(), 0);
                }
                let n = n as u32;
                let rem = here % n;
                let pad = if rem == 0 { 0 } else { n - rem };
                return (Vec::new(), pad);
            }
            Kind::End | Kind::Include | Kind::Page => Vec::new(),
            Kind::Equ | Kind::Org => unreachable!("handled by the caller"),
        };
        (bytes, 0)
    }

    fn emit(
        &mut self,
        line: usize,
        here: u32,
        bytes: Vec<u8>,
        addr: Option<u16>,
        value: Option<i64>,
    ) {
        let Some(out) = self.out.as_mut() else { return };
        let mut overflow = false;
        let mut clash = None;
        for (k, &b) in bytes.iter().enumerate() {
            let a = here as usize + k;
            if a > 0xFFFF {
                overflow = true;
                break;
            }
            if out.owner[a] != 0 && clash.is_none() {
                clash = Some((a, out.owner[a] - 1));
            }
            out.memory[a] = Some(b);
            out.owner[a] = line + 1;
        }
        if overflow || (bytes.is_empty() && self.pc > 0x10000) {
            self.errs.push("code runs past address FFFF".into());
        }
        if let Some((a, other)) = clash {
            let at = self.location(other, line);
            self.errs
                .push(format!("overwrites address {a:04X} (written by {at})"));
        }
        let src = &self.lines[line];
        let file = self.files[src.file].clone();
        let Some(out) = self.out.as_mut() else { return };
        for e in &self.errs {
            out.errors.push(Diagnostic {
                file: file.clone(),
                line: src.line,
                message: e.clone(),
            });
        }
        out.listing.push(ListingLine {
            file,
            line: src.line,
            addr,
            bytes,
            value,
            source: src.text.trim_end().to_string(),
            errors: self.errs.clone(),
        });
    }
}

#[cfg(test)]
mod tests;
