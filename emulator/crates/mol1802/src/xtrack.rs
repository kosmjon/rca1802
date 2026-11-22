//! The `xtrack` one-line lookback (`design.md`, section "5. The `xtrack`
//! one-line lookback").
//!
//! One small helper, [`preceding_sex`], used in two modes by its callers:
//!
//! - `INP`/`OUT` (Requirement 4.4, `lower.rs`'s `PORT(n)` case): if
//!   [`preceding_sex`] returns `None`, it's an error. The register it
//!   finds is whatever the developer wrote -- this module doesn't care
//!   which one, only that one is there.
//! - `PUSH`/`POP` (Requirement 7.3) and register-to-register arithmetic
//!   (Requirement 9.1): if [`preceding_sex`] doesn't return the expected
//!   fixed register (`R2` or `R6`), the caller inserts a `SEX` for it;
//!   otherwise it inserts nothing. Never an error there, since the
//!   target register is fixed by the sugar itself, not supplied by the
//!   developer.

use crate::ast::RegRef;
use crate::parser::parse_register_operand;

/// One line of already-emitted output text, pass-through or lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmittedLine {
    pub text: String,
}

impl EmittedLine {
    pub fn new(text: impl Into<String>) -> Self {
        EmittedLine { text: text.into() }
    }
}

/// Does the immediately preceding emitted output line read exactly
/// `SEX <reg>`? Looks back exactly one line; never further -- a `SEX Rn`
/// two lines back, with something unrelated in between, does not count.
pub(crate) fn preceding_sex(output_so_far: &[EmittedLine]) -> Option<RegRef> {
    parse_sex_line(&output_so_far.last()?.text)
}

/// Like [`preceding_sex`], but for the *insert, never error* callers
/// (`PUSH`/`POP`, Requirement 7.3; register-to-register arithmetic,
/// Requirement 9.1): also treats the immediately preceding line as
/// confirming `X == reg` if its text exactly matches one of
/// `implied_by` -- mnemonics that, in this language's own generated
/// output, only ever appear once `X` is already `reg` (e.g.
/// `STXD`/`IRX`/`LDX`, which only `PUSH`/`POP` themselves emit, and only
/// after ensuring `X` is `R2`). Still only ever looks back one line.
pub(crate) fn x_still_at(output_so_far: &[EmittedLine], reg: RegRef, implied_by: &[&str]) -> bool {
    if preceding_sex(output_so_far) == Some(reg) {
        return true;
    }
    output_so_far
        .last()
        .is_some_and(|line| implied_by.contains(&line.text.trim()))
}

/// Parses one line of text as exactly `SEX Rn` (any surrounding
/// whitespace and trailing `;` comment ignored, same as every other
/// MOL1802-adjacent line), returning the register if it matches.
fn parse_sex_line(text: &str) -> Option<RegRef> {
    let before_comment = text.split(';').next().unwrap_or("").trim();
    let mut tokens = before_comment.split_whitespace();
    let verb = tokens.next()?;
    let operand = tokens.next()?;
    if tokens.next().is_some() || !verb.eq_ignore_ascii_case("SEX") {
        return None;
    }
    parse_register_operand(operand)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> EmittedLine {
        EmittedLine::new(text)
    }

    #[test]
    fn matching_sex_immediately_before() {
        assert_eq!(preceding_sex(&[line("SEX R3")]), Some(RegRef(3)));
        assert_eq!(
            preceding_sex(&[line("LDI 5"), line("SEX R6")]),
            Some(RegRef(6))
        );
    }

    #[test]
    fn any_register_counts_for_the_error_mode_caller() {
        // This module doesn't judge which register; it just reports it.
        assert_eq!(preceding_sex(&[line("SEX R2")]), Some(RegRef(2)));
        assert_eq!(preceding_sex(&[line("sex ra")]), Some(RegRef(10)));
    }

    #[test]
    fn a_different_line_immediately_before_is_none() {
        assert_eq!(preceding_sex(&[line("LDI 5")]), None);
        assert_eq!(preceding_sex(&[line("GLO R3")]), None);
    }

    #[test]
    fn nothing_at_all_before_is_none() {
        assert_eq!(preceding_sex(&[]), None);
    }

    #[test]
    fn lookback_is_exactly_one_line() {
        // `SEX R3` two lines back, with something unrelated between it
        // and here, must not satisfy the lookback.
        assert_eq!(
            preceding_sex(&[line("SEX R3"), line("LDI 5")]),
            None,
            "a SEX two lines back must not count"
        );
    }

    #[test]
    fn comment_and_whitespace_variations_still_match() {
        assert_eq!(preceding_sex(&[line("  SEX R3  ")]), Some(RegRef(3)));
        assert_eq!(
            preceding_sex(&[line("SEX R3 ; point at the UART port")]),
            Some(RegRef(3))
        );
    }

    const STACK_OPS: &[&str] = &["STXD", "IRX", "LDX"];

    #[test]
    fn x_still_at_true_for_a_matching_sex() {
        assert!(x_still_at(&[line("SEX R2")], RegRef(2), STACK_OPS));
    }

    #[test]
    fn x_still_at_true_for_an_implied_by_mnemonic() {
        assert!(x_still_at(&[line("STXD")], RegRef(2), STACK_OPS));
        assert!(x_still_at(&[line("IRX")], RegRef(2), STACK_OPS));
        assert!(x_still_at(&[line("LDX")], RegRef(2), STACK_OPS));
    }

    #[test]
    fn x_still_at_false_for_anything_else() {
        assert!(!x_still_at(&[line("INP 4")], RegRef(2), STACK_OPS));
        assert!(!x_still_at(&[], RegRef(2), STACK_OPS));
        assert!(!x_still_at(&[line("SEX R3")], RegRef(2), STACK_OPS));
    }
}
