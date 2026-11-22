//! MOL1802: a machine-oriented language that transpiles to `asm1802` source.
//! Source files use the `.m18` extension.
//!
//! This crate is a straight-line text-to-text transpiler: classify each
//! input line as either a pass-through line or a MOL1802 statement, parse
//! the MOL1802 statements into a small AST, lower each AST node to the real
//! `asm1802` instruction(s) that realize it, and re-merge everything back
//! into one assembly-text output in original order. See `GUIDE.md` in this
//! crate for the syntax, and `.kiro/specs/mol1802/{requirements,design}.md`
//! for the full design rationale.

/// Re-exported from `asm1802` rather than redefined, so MOL1802 diagnostics
/// print identically to `asm1802` diagnostics.
pub use asm1802::Diagnostic;

mod ast;
mod classify;
mod includes;
mod lower;
mod parser;
mod xtrack;

pub use includes::rewrite_m18_includes;

use ast::Statement;
use classify::Classification;
use xtrack::EmittedLine;

/// Transpiles MOL1802 `source` into `asm1802` source text (`design.md`,
/// "Pipeline stages"). Returns the generated text, or every diagnostic
/// collected across the whole file (classifier/parser/lowering errors;
/// `Diagnostic::file` is left empty, same convention as `asm1802::assemble`
/// for in-memory source -- callers with a real file name set it on each
/// diagnostic afterward).
pub fn transpile(source: &str) -> Result<String, Vec<Diagnostic>> {
    let mnemonic_col = detect_mnemonic_column(source);
    let mut output: Vec<EmittedLine> = Vec::new();
    let mut errors: Vec<Diagnostic> = Vec::new();
    let mut needs_scratch = false;

    for (idx, raw_line) in source.lines().enumerate() {
        let line = idx + 1;
        match classify::classify(raw_line) {
            Classification::PassThrough => {
                output.push(EmittedLine::new(raw_line.to_string()));
            }
            Classification::Statement {
                label,
                body,
                comment,
            } => match parser::parse_statement_line(label, body, comment) {
                Ok(parsed) => {
                    if let Statement::Chain(ops) = &parsed.statement {
                        if let [a, b] = ops.as_slice() {
                            needs_scratch |= lower::is_register_arithmetic_chain(a, b);
                        }
                    }
                    match lower_statement(&parsed.statement, &output) {
                        Ok(lines) => {
                            let lines = attach_label_and_comments(
                                lines,
                                parsed.label.as_deref(),
                                body,
                                parsed.comment.as_deref(),
                                mnemonic_col,
                            );
                            output.extend(lines);
                        }
                        Err(message) => errors.push(Diagnostic {
                            file: String::new(),
                            line,
                            message,
                        }),
                    }
                }
                Err(message) => errors.push(Diagnostic {
                    file: String::new(),
                    line,
                    message,
                }),
            },
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    if needs_scratch {
        let indent = " ".repeat(mnemonic_col - 1);
        let init: Vec<EmittedLine> = lower::scratch_register_init()
            .into_iter()
            .map(|text| EmittedLine::new(format!("{indent}{text}")))
            .collect();
        output.splice(0..0, init);
        output.push(EmittedLine::new(lower::scratch_byte_declaration()));
    }

    let mut text = String::new();
    for line in &output {
        text.push_str(&line.text);
        text.push('\n');
    }
    Ok(text)
}

/// The 1-based column generated mnemonics should start at, so they line
/// up with however the rest of this particular file is already written
/// (classic 1802 listings commonly use column 9; a file with no
/// indentation convention established at all falls back to column 5).
/// Measured from every line in `source` that has a mnemonic after either
/// plain indentation or a column-1 label, whichever column shows up most
/// often.
fn detect_mnemonic_column(source: &str) -> usize {
    use std::collections::HashMap;
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for line in source.lines() {
        if let Some(col) = mnemonic_column_of(line) {
            *counts.entry(col).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(col, _)| col)
        .unwrap_or(5)
        .max(2)
}

/// The 1-based column the mnemonic starts at on `line`, or `None` for a
/// blank line, a full-line comment, or a label with nothing after it.
fn mnemonic_column_of(line: &str) -> Option<usize> {
    let trimmed_start = line.trim_start();
    if trimmed_start.is_empty() || trimmed_start.starts_with(';') || trimmed_start.starts_with('*')
    {
        return None;
    }
    let bytes = line.as_bytes();
    if is_indent_ws(bytes[0]) {
        return Some(line.len() - trimmed_start.len() + 1);
    }
    // Column 1: skip the label token, then any whitespace after it.
    let mut i = 0;
    while i < bytes.len() && !is_indent_ws(bytes[i]) && bytes[i] != b';' {
        i += 1;
    }
    let mut j = i;
    while j < bytes.len() && is_indent_ws(bytes[j]) {
        j += 1;
    }
    if j >= bytes.len() || bytes[j] == b';' {
        return None;
    }
    Some(j + 1)
}

fn is_indent_ws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Lowers one parsed `Statement` to `asm1802` instruction text, given the
/// output already emitted so far (for `xtrack`'s one-line lookback).
fn lower_statement(
    stmt: &Statement,
    output_so_far: &[EmittedLine],
) -> Result<Vec<EmittedLine>, String> {
    match stmt {
        Statement::Chain(ops) => match ops.as_slice() {
            [a, b] => lower::lower_chain_statement(a, b, output_so_far),
            ops if ops.len() > 2 => Ok(lower::lower_fanout_chain(ops)?
                .into_iter()
                .map(EmittedLine::new)
                .collect()),
            _ => Err("a chain must have at least two operands".to_string()),
        },
        Statement::Branch { cond, len, target } => Ok(lower::lower_branch(*cond, *len, target)
            .into_iter()
            .map(EmittedLine::new)
            .collect()),
        Statement::Stack {
            elements,
            direction,
        } => Ok(lower::lower_stack(elements, *direction)
            .into_iter()
            .map(EmittedLine::new)
            .collect()),
    }
}

/// Attaches the leading `; <original statement text>` traceability
/// comment (Requirement 10.2, only when the statement expanded to more
/// than one instruction), the developer's own trailing comment on the
/// last line (Requirement 10.1), and the statement's label (if any) on
/// the first line -- or, for a zero-instruction no-op statement that
/// still has a label, a bare label line so the label keeps pointing
/// somewhere sensible.
fn attach_label_and_comments(
    mut lines: Vec<EmittedLine>,
    label: Option<&str>,
    body: &str,
    comment: Option<&str>,
    mnemonic_col: usize,
) -> Vec<EmittedLine> {
    // Every generated line needs to start with whitespace, the same rule
    // `asm1802` itself follows: a bare mnemonic starting in column 1 would
    // otherwise be read as a label (and the real mnemonic after it as an
    // unrecognized one). A label, below, replaces this indent on the
    // first line instead of sitting in front of it. The indent matches
    // whatever column the rest of this file already starts mnemonics at
    // (`detect_mnemonic_column`), so generated and hand-written lines
    // line up instead of generated ones always sitting at a fixed
    // 4-space indent regardless of the file's own convention.
    let indent = " ".repeat(mnemonic_col - 1);
    for line in &mut lines {
        line.text = format!("{indent}{}", line.text);
    }
    if lines.len() > 1 {
        let first = &mut lines[0];
        first.text = format!("{} ; {body}", first.text);
    }
    if let Some(c) = comment {
        if let Some(last) = lines.last_mut() {
            last.text = format!("{} ; {c}", last.text);
        }
    }
    if lines.is_empty() {
        return match label {
            Some(l) => vec![EmittedLine::new(l.to_string())],
            None => Vec::new(),
        };
    }
    if let Some(l) = label {
        let prefix = format!("{l}:");
        // At least one space, even if the label itself already reaches
        // or passes the mnemonic column.
        let pad = (mnemonic_col - 1).saturating_sub(prefix.len()).max(1);
        let first = &mut lines[0];
        first.text = format!("{prefix}{}{}", " ".repeat(pad), first.text.trim_start());
    }
    lines
}

#[cfg(test)]
mod tests;
