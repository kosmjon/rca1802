//! Text helpers for `MACRO` / `REPT`: split a line into fields, parse the
//! block directives, split macro arguments, and substitute parameters.
//! Everything here works on text; `lib.rs` drives the expansion.

use crate::expr::{is_symbol_char, is_symbol_start};

/// A defined macro: parameter names and the raw body lines.
#[derive(Debug)]
pub(crate) struct Macro {
    pub params: Vec<String>,
    pub body: Vec<String>,
}

/// The label, mnemonic-position word and the rest of a source line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Fields<'a> {
    /// A column-1 token, trailing `:` kept.
    pub label: Option<&'a str>,
    /// The word after the label (empty if there is none).
    pub word: &'a str,
    /// Everything after `word`, leading blanks removed.
    pub rest: &'a str,
}

fn is_blank(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Splits `text` the way `asm1802` does. `None` for a blank or comment line.
pub(crate) fn fields(text: &str) -> Option<Fields<'_>> {
    let s = text.as_bytes();
    if s.is_empty() || s[0] == b';' || s[0] == b'*' {
        return None;
    }
    let mut i = 0;
    let mut label = None;
    if !is_blank(s[0]) {
        while i < s.len() && !is_blank(s[i]) && s[i] != b';' {
            i += 1;
        }
        label = Some(&text[..i]);
    }
    while i < s.len() && is_blank(s[i]) {
        i += 1;
    }
    if i >= s.len() || s[i] == b';' {
        return label.map(|l| Fields {
            label: Some(l),
            word: "",
            rest: "",
        });
    }
    let start = i;
    while i < s.len() && !is_blank(s[i]) && s[i] != b';' {
        i += 1;
    }
    let word = &text[start..i];
    while i < s.len() && is_blank(s[i]) {
        i += 1;
    }
    Some(Fields {
        label,
        word,
        rest: &text[i..],
    })
}

/// A block directive line.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BlockLine {
    /// `NAME MACRO p1, p2` (`name` is `None` if the label is missing).
    MacroDef {
        name: Option<String>,
        params: Vec<String>,
    },
    EndM,
    /// `REPT count` (the count text, comment removed).
    Rept(String),
    EndR,
}

/// Recognizes `MACRO`, `ENDM`, `REPT` and `ENDR`, each with an optional
/// leading dot, in any case.
pub(crate) fn block_line(text: &str) -> Option<BlockLine> {
    let f = fields(text)?;
    // `.ENDM` / `.ENDR` written in column 1 arrive as the label.
    let (word, label, rest) = if f.word.is_empty() {
        (f.label?, None, "")
    } else {
        (f.word, f.label, f.rest)
    };
    let up = word.strip_prefix('.').unwrap_or(word).to_ascii_uppercase();
    match up.as_str() {
        "MACRO" => Some(BlockLine::MacroDef {
            name: label.map(|l| l.strip_suffix(':').unwrap_or(l).to_string()),
            params: split_args(rest),
        }),
        "ENDM" => Some(BlockLine::EndM),
        "REPT" => Some(BlockLine::Rept(strip_comment(rest).trim().to_string())),
        "ENDR" => Some(BlockLine::EndR),
        _ => None,
    }
}

/// `s` up to the first `;` that is not inside quotes.
pub(crate) fn strip_comment(s: &str) -> &str {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b';' => return &s[..i],
            q @ (b'\'' | b'"') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    s
}

/// Splits a comma-separated list at top level (outside quotes and
/// parentheses), trims each item and drops a trailing `;` comment. An empty
/// list gives no items.
pub(crate) fn split_args(rest: &str) -> Vec<String> {
    let s = strip_comment(rest).trim();
    if s.is_empty() {
        return Vec::new();
    }
    let b = s.as_bytes();
    let mut items = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            q @ (b'\'' | b'"') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
            }
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                items.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    items.push(s[start..].trim().to_string());
    items
}

/// True if `s` is a plain symbol name.
pub(crate) fn is_name(s: &str) -> bool {
    !s.is_empty() && is_symbol_start(s.as_bytes()[0]) && s.bytes().all(is_symbol_char)
}

/// Rewrites one body line for an expansion: `\@` becomes `_<uid>` (a number
/// unique to this expansion, for local labels), and each whole-word
/// occurrence of a parameter becomes its argument. Comments (after `;`) and
/// quoted text are left alone, as are hex literals such as `#F0`.
pub(crate) fn substitute(line: &str, params: &[String], args: &[String], uid: usize) -> String {
    let line = line.replace("\\@", &format!("_{uid}"));
    let b = line.as_bytes();
    if params.is_empty() || b.is_empty() || b[0] == b';' || b[0] == b'*' {
        return line;
    }
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b';' {
            out.push_str(&line[i..]);
            break;
        }
        if c == b'\'' || c == b'"' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != c {
                i += 1;
            }
            i = (i + 1).min(b.len());
            out.push_str(&line[start..i]);
            continue;
        }
        if is_symbol_char(c) {
            let start = i;
            while i < b.len() && is_symbol_char(b[i]) {
                i += 1;
            }
            let word = &line[start..i];
            let prev = if start > 0 { b[start - 1] } else { b' ' };
            let hex_literal = prev == b'#' || (prev == b'$' && b[start].is_ascii_hexdigit());
            match params.iter().position(|p| p.eq_ignore_ascii_case(word)) {
                Some(k) if !hex_literal => out.push_str(&args[k]),
                _ => out.push_str(word),
            }
            continue;
        }
        // Not ASCII-special: copy the whole character.
        let ch = line[i..].chars().next().unwrap_or(' ');
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|x| (*x).to_string()).collect()
    }

    #[test]
    fn fields_split() {
        let f = fields("LBL:  LDI  5 ; c").unwrap();
        assert_eq!(f.label, Some("LBL:"));
        assert_eq!(f.word, "LDI");
        assert_eq!(f.rest, "5 ; c");
        let f = fields("   NOP").unwrap();
        assert_eq!((f.label, f.word, f.rest), (None, "NOP", ""));
        let f = fields("ONLY").unwrap();
        assert_eq!((f.label, f.word), (Some("ONLY"), ""));
        assert!(fields("; c").is_none());
        assert!(fields("").is_none());
    }

    #[test]
    fn block_lines() {
        assert_eq!(
            block_line("GET .MACRO OFFSET, X ; c"),
            Some(BlockLine::MacroDef {
                name: Some("GET".into()),
                params: s(&["OFFSET", "X"]),
            })
        );
        assert_eq!(
            block_line("GET: macro"),
            Some(BlockLine::MacroDef {
                name: Some("GET".into()),
                params: vec![],
            })
        );
        assert_eq!(block_line("        .ENDM"), Some(BlockLine::EndM));
        assert_eq!(block_line(".ENDM"), Some(BlockLine::EndM));
        assert_eq!(block_line("  endr  ; x"), Some(BlockLine::EndR));
        assert_eq!(
            block_line("  .REPT 2*3 ; six"),
            Some(BlockLine::Rept("2*3".into()))
        );
        assert_eq!(block_line("  LDI 1"), None);
    }

    #[test]
    fn args() {
        assert_eq!(split_args(""), Vec::<String>::new());
        assert_eq!(split_args(" ; only comment"), Vec::<String>::new());
        assert_eq!(split_args("ADD, #10 ; c"), s(&["ADD", "#10"]));
        assert_eq!(split_args("LOW(A,B), 'x,y'"), s(&["LOW(A,B)", "'x,y'"]));
        assert_eq!(split_args("a,"), s(&["a", ""]));
        assert_eq!(split_args("HIGH BUF + 1"), s(&["HIGH BUF + 1"]));
    }

    #[test]
    fn substitution() {
        let p = s(&["OP", "OFFSET"]);
        let a = s(&["ADD", "BUF+1"]);
        assert_eq!(
            substitute("    OP  ; D = <OP>", &p, &a, 1),
            "    ADD  ; D = <OP>"
        );
        assert_eq!(
            substitute("    LDI LOW(OFFSET)", &p, &a, 1),
            "    LDI LOW(BUF+1)"
        );
        assert_eq!(substitute("    DB 'OP',op", &p, &a, 1), "    DB 'OP',ADD");
        assert_eq!(
            substitute("    LDI #OP", &s(&["F0"]), &s(&["9"]), 1),
            "    LDI #OP"
        );
        assert_eq!(
            substitute("    LDI #F0", &s(&["F0"]), &s(&["9"]), 1),
            "    LDI #F0"
        );
        assert_eq!(substitute("    OPX OFFSET2", &p, &a, 1), "    OPX OFFSET2");
        assert_eq!(
            substitute("LOOP\\@: BR LOOP\\@", &[], &[], 7),
            "LOOP_7: BR LOOP_7"
        );
        assert_eq!(substitute("; OP", &p, &a, 1), "; OP");
    }
}
