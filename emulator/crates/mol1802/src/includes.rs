//! `INCLUDE "x.m18"` support (the pure text half): find the `.m18` includes
//! in transpiled output and point them at the sibling `.asm` the CLI writes
//! for each. Line parsing mirrors `asm1802`'s `include_target`.

/// Rewrites every `INCLUDE` line that names a `.m18` file (extension matched
/// case-insensitively) so it names the sibling `.asm` instead. Returns the
/// rewritten text and the original `.m18` names, in order of appearance.
/// Everything else on the line (label, quotes, comment) and all other lines
/// are left untouched.
pub fn rewrite_m18_includes(text: &str) -> (String, Vec<String>) {
    let mut out = String::with_capacity(text.len());
    let mut targets = Vec::new();
    for line in text.split_inclusive('\n') {
        match m18_name_span(line) {
            Some((start, end)) => {
                let name = &line[start..end];
                targets.push(name.to_string());
                out.push_str(&line[..end - 4]);
                out.push_str(".asm");
                out.push_str(&line[end..]);
            }
            None => out.push_str(line),
        }
    }
    (out, targets)
}

/// The byte span of the file name on an `INCLUDE` line whose name ends in
/// `.m18`, or `None` for any other line.
fn m18_name_span(line: &str) -> Option<(usize, usize)> {
    let s = line.as_bytes();
    if s.is_empty() || s[0] == b';' || s[0] == b'*' {
        return None;
    }
    let is_ws = |b: u8| b == b' ' || b == b'\t';
    let mut i = 0;
    while i < s.len() && !is_ws(s[i]) && s[i] != b'\r' && s[i] != b'\n' {
        i += 1; // label, if any
    }
    while i < s.len() && is_ws(s[i]) {
        i += 1;
    }
    let rest = &line[i..];
    let word_end = rest.find([' ', '\t']).unwrap_or(rest.len());
    if !rest[..word_end].eq_ignore_ascii_case("INCLUDE") {
        return None;
    }
    let arg_start = i
        + word_end
        + (rest[word_end..].len() - rest[word_end..].trim_start_matches([' ', '\t']).len());
    let arg = &line[arg_start..];
    let (start, len) = match arg.chars().next()? {
        q @ ('"' | '\'') => (arg_start + 1, arg[1..].find(q)?),
        ';' | '\r' | '\n' => return None,
        _ => (
            arg_start,
            arg.find([' ', '\t', ';', '\r', '\n']).unwrap_or(arg.len()),
        ),
    };
    let end = start + len;
    let name = &line[start..end];
    let bytes = name.as_bytes();
    if bytes.len() >= 4 && bytes[bytes.len() - 4..].eq_ignore_ascii_case(b".m18") {
        Some((start, end))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_and_unquoted() {
        let (t, n) = rewrite_m18_includes("    INCLUDE \"a.m18\"\n    INCLUDE b.m18 ; c\n");
        assert_eq!(t, "    INCLUDE \"a.asm\"\n    INCLUDE b.asm ; c\n");
        assert_eq!(n, vec!["a.m18", "b.m18"]);
    }

    #[test]
    fn case_insensitive_and_single_quotes() {
        let (t, n) = rewrite_m18_includes("    include 'Sub/X.M18'\n");
        assert_eq!(t, "    include 'Sub/X.asm'\n");
        assert_eq!(n, vec!["Sub/X.M18"]);
    }

    #[test]
    fn asm_includes_untouched() {
        let src = "    INCLUDE \"a.asm\"\n    INCLUDE \"m18\"\n    INCLUDE \"a.m18x\"\n";
        let (t, n) = rewrite_m18_includes(src);
        assert_eq!(t, src);
        assert!(n.is_empty());
    }

    #[test]
    fn label_and_comment_preserved() {
        let (t, _) = rewrite_m18_includes("LIB:  INCLUDE \"../x.m18\" ; the lib\r\n");
        assert_eq!(t, "LIB:  INCLUDE \"../x.asm\" ; the lib\r\n");
    }

    #[test]
    fn comment_lines_and_other_text_ignored() {
        let src = "; INCLUDE \"a.m18\"\n* INCLUDE \"a.m18\"\n    DB 'INCLUDE \"a.m18\"'\n        ; INCLUDE \"a.m18\"\n";
        let (t, n) = rewrite_m18_includes(src);
        assert_eq!(t, src);
        assert!(n.is_empty());
    }

    #[test]
    fn multiple_and_no_trailing_newline() {
        let (t, n) = rewrite_m18_includes("  INCLUDE \"a.m18\"\n  NOP\n  INCLUDE \"b.m18\"");
        assert_eq!(t, "  INCLUDE \"a.asm\"\n  NOP\n  INCLUDE \"b.asm\"");
        assert_eq!(n, vec!["a.m18", "b.m18"]);
    }
}
