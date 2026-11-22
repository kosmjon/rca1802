//! Line classifier (`design.md`, section "1. Line classifier").
//!
//! A shallow, syntactic pass-through-vs-MOL1802-statement check per line.
//! It does not validate that a statement-shaped line is actually valid
//! MOL1802 -- a line that merely *looks* like a chain but has a typo is
//! still handed to the parser (task 4), which produces a proper
//! diagnostic (Requirement 1.4) rather than this classifier silently
//! letting it through as broken `asm1802` text.

use crate::ast::CONDITION_TOKENS;

/// The result of classifying one source line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Classification<'a> {
    /// An `asm1802` directive, label, comment, blank line, or raw
    /// instruction line (including `CALL`/`RETURN`/`RETN` and any
    /// developer-written `SEX Rn`): copied to the output unchanged.
    PassThrough,
    /// A MOL1802 statement, with its leading label (if any) already
    /// split off and its trailing comment (if any, Requirement 10.1)
    /// already split off `body` too.
    Statement {
        label: Option<&'a str>,
        body: &'a str,
        comment: Option<&'a str>,
    },
}

/// Classify one source line (Requirement 1.2).
pub(crate) fn classify(line: &str) -> Classification<'_> {
    let trimmed_start = line.trim_start();
    if trimmed_start.is_empty() || trimmed_start.starts_with(';') || trimmed_start.starts_with('*')
    {
        return Classification::PassThrough;
    }

    let (label, rest) = split_label(line);
    let (before, comment) = split_comment(rest);
    let body = before.trim();
    if body.is_empty() || !is_mol1802_statement_body(body) {
        return Classification::PassThrough;
    }
    Classification::Statement {
        label,
        body,
        comment,
    }
}

/// Splits a leading column-1 label off `line`, the same rule `asm1802`
/// itself uses (re-implemented minimally here since that logic is
/// private to the `asm1802` crate): if the line doesn't start with
/// whitespace, its first whitespace-delimited token is a label, with an
/// optional trailing `:` dropped. Returns `(label, remainder)`; the
/// remainder still has its own leading whitespace (if any).
fn split_label(line: &str) -> (Option<&str>, &str) {
    let bytes = line.as_bytes();
    if bytes.is_empty() || is_ws(bytes[0]) {
        return (None, line);
    }
    let mut i = 0;
    while i < bytes.len() && !is_ws(bytes[i]) && bytes[i] != b';' {
        i += 1;
    }
    let mut label = &line[..i];
    if label.len() > 1 && label.ends_with(':') {
        label = &label[..label.len() - 1];
    }
    (Some(label), &line[i..])
}

fn is_ws(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Splits `s` on the first `;` (Requirement 10.1 treats `;` as the
/// MOL1802 statement comment delimiter, same as `asm1802`), returning the
/// text before it and the comment text after it (trimmed, `;` dropped).
/// A bare trailing `;` with nothing after it (or only whitespace) is
/// treated the same as no comment at all, rather than an empty one.
fn split_comment(s: &str) -> (&str, Option<&str>) {
    match s.find(';') {
        Some(idx) => {
            let comment = s[idx + 1..].trim();
            let comment = if comment.is_empty() {
                None
            } else {
                Some(comment)
            };
            (&s[..idx], comment)
        }
        None => (s, None),
    }
}

/// The three MOL1802 statement shapes (design.md section 1): a chain
/// (including a `STK` push/pop, which is still just a chain with a
/// comma-separated list on one side -- parsed in `parser.rs`), a
/// condition+branch, or a bare branch.
fn is_mol1802_statement_body(body: &str) -> bool {
    contains_top_level_colon(body) || condition_branch_len(body).is_some() || is_branch_tail(body)
}

fn contains_top_level_colon(body: &str) -> bool {
    body.contains(':')
}

/// If `body` starts with one of Requirement 5.1's condition tokens
/// followed by `/` or `//` and a non-empty target, returns the length of
/// the condition token consumed.
fn condition_branch_len(body: &str) -> Option<usize> {
    let upper = body.to_ascii_uppercase();
    let (token, _) = CONDITION_TOKENS
        .iter()
        .find(|(token, _)| upper.starts_with(token))?;
    is_branch_tail(&body[token.len()..]).then_some(token.len())
}

/// `/LABEL` or `//LABEL` with a non-empty label. Whitespace between a
/// condition and the `/`/`//` (e.g. `EF1=0? /LOOP`) is tolerated here,
/// even though the examples in the guide write it tight against the
/// condition.
fn is_branch_tail(s: &str) -> bool {
    let s = s.trim_start();
    let target = s.strip_prefix("//").or_else(|| s.strip_prefix('/'));
    matches!(target, Some(t) if !t.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_pass_through(line: &str) -> bool {
        classify(line) == Classification::PassThrough
    }

    fn statement(line: &str) -> (Option<&str>, &str) {
        match classify(line) {
            Classification::Statement { label, body, .. } => (label, body),
            Classification::PassThrough => panic!("expected a statement: `{line}`"),
        }
    }

    #[test]
    fn directives_are_pass_through() {
        assert!(is_pass_through("    ORG 0400H"));
        assert!(is_pass_through("FOO EQU 5"));
        assert!(is_pass_through("    DB 1,2,3"));
        assert!(is_pass_through("    DW LABEL"));
        assert!(is_pass_through("    DS 4"));
        assert!(is_pass_through("    ALIGN 4"));
        assert!(is_pass_through("    INCLUDE \"file.asm\""));
        assert!(is_pass_through("    DEFINE FOO"));
        assert!(is_pass_through("    IFDEF FOO"));
        assert!(is_pass_through("    IFNDEF FOO"));
        assert!(is_pass_through("    ELSE"));
        assert!(is_pass_through("    ENDIF"));
    }

    #[test]
    fn blank_and_comment_lines_are_pass_through() {
        assert!(is_pass_through(""));
        assert!(is_pass_through("    "));
        assert!(is_pass_through("; a comment"));
        assert!(is_pass_through("* a comment"));
    }

    #[test]
    fn labels_alone_are_pass_through() {
        assert!(is_pass_through("LOOP"));
        assert!(is_pass_through("LOOP:"));
    }

    #[test]
    fn raw_instruction_lines_are_pass_through() {
        assert!(is_pass_through("    LDI 5"));
        assert!(is_pass_through("    GLO R3"));
        assert!(is_pass_through("    SEP R4"));
    }

    #[test]
    fn call_return_retn_are_pass_through() {
        assert!(is_pass_through("    CALL FOO"));
        assert!(is_pass_through("    RETURN"));
        assert!(is_pass_through("    RETN"));
    }

    #[test]
    fn sex_lines_are_pass_through() {
        assert!(is_pass_through("    SEX R3"));
        assert!(is_pass_through("    SEX R2"));
    }

    #[test]
    fn chain_is_a_statement() {
        let (label, body) = statement("    R3.0 : D");
        assert_eq!(label, None);
        assert_eq!(body, "R3.0 : D");
    }

    #[test]
    fn chain_with_label_is_a_statement() {
        let (label, body) = statement("LOOP: R3.0 : D");
        assert_eq!(label, Some("LOOP"));
        assert_eq!(body, "R3.0 : D");
    }

    #[test]
    fn condition_branch_is_a_statement() {
        let (label, body) = statement("    D=0?/LOOP");
        assert_eq!(label, None);
        assert_eq!(body, "D=0?/LOOP");

        assert!(matches!(
            classify("    D<>0?//LOOP"),
            Classification::Statement { .. }
        ));
        assert!(matches!(
            classify("    EF1=1?/LOOP"),
            Classification::Statement { .. }
        ));
    }

    #[test]
    fn bare_branch_is_a_statement() {
        assert!(matches!(
            classify("    /LOOP"),
            Classification::Statement { .. }
        ));
        assert!(matches!(
            classify("    //LOOP"),
            Classification::Statement { .. }
        ));
    }

    #[test]
    fn push_d_and_pop_d_text_are_pass_through() {
        // The old PUSH D/POP D syntax is gone; this text has no special
        // meaning any more and is left for `asm1802` to reject.
        assert!(is_pass_through("    PUSH D"));
        assert!(is_pass_through("    POP D"));
    }

    #[test]
    fn stk_chain_is_a_statement() {
        let (label, body) = statement("    R10:STK");
        assert_eq!(label, None);
        assert_eq!(body, "R10:STK");

        assert!(matches!(
            classify("    STK:R10"),
            Classification::Statement { .. }
        ));
        assert!(matches!(
            classify("    R10,R9,R8,R7:STK"),
            Classification::Statement { .. }
        ));
    }

    #[test]
    fn stk_chain_with_label_is_a_statement() {
        let (label, body) = statement("SAVE: R10,R9:STK");
        assert_eq!(label, Some("SAVE"));
        assert_eq!(body, "R10,R9:STK");
    }

    #[test]
    fn comment_is_stripped_before_shape_detection() {
        // A raw asm1802 line whose comment happens to contain `:` must not
        // be misclassified as a chain.
        assert!(is_pass_through("    CALL FOO ; result: ok"));
    }

    #[test]
    fn trailing_comment_on_a_statement_is_not_part_of_the_body() {
        let (label, body) = statement("    R3.0 : D ; move low byte");
        assert_eq!(label, None);
        assert_eq!(body, "R3.0 : D");
    }

    #[test]
    fn condition_branch_with_a_space_before_the_slash_is_still_a_statement() {
        let (label, body) = statement("MAIN: EF4=0? /MAIN");
        assert_eq!(label, Some("MAIN"));
        assert_eq!(body, "EF4=0? /MAIN");

        assert!(matches!(
            classify("    D<>0? /OFF"),
            Classification::Statement { .. }
        ));
        assert!(matches!(
            classify("    D=0?  //LOOP"),
            Classification::Statement { .. }
        ));
    }

    #[test]
    fn a_bare_trailing_semicolon_is_not_a_comment() {
        match classify("    1:Q;") {
            Classification::Statement { comment, body, .. } => {
                assert_eq!(comment, None);
                assert_eq!(body, "1:Q");
            }
            Classification::PassThrough => panic!("expected a statement"),
        }
    }

    #[test]
    fn trailing_comment_is_captured_separately() {
        match classify("    R3.0 : D ; move low byte") {
            Classification::Statement { comment, .. } => {
                assert_eq!(comment, Some("move low byte"));
            }
            Classification::PassThrough => panic!("expected a statement"),
        }
        match classify("    R3.0 : D") {
            Classification::Statement { comment, .. } => assert_eq!(comment, None),
            Classification::PassThrough => panic!("expected a statement"),
        }
    }
}
