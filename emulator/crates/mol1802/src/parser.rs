//! MOL1802 statement/operand parsing (`design.md`, section "3. Parser").
//!
//! Each operand token is matched against the Requirement 2 grammar in a
//! fixed order: `(Rn...)` forms, then `PORT(...)`, then the
//! `EF1`-`EF4`/`Q`/`D`/`DF`/`EI` literal keywords, then `Rn[.0/.1/']`, then
//! "anything else is an `Imm` expression". Shapes that look
//! almost-but-not-quite like a known form (`R16`, `PORT(8)`, `PORT(0)`, a
//! malformed `(Rn)`) produce an error naming the token -- they commit
//! structurally to that form and so never fall through to the generic
//! `Imm` catch-all.

use crate::ast::{
    Adjust, BranchLength, Half, Operand, RegRef, StackDirection, StackElement, Statement,
    CONDITION_TOKENS,
};

/// Parse one operand token (already split out of its chain/statement by
/// the caller).
pub(crate) fn parse_operand(token: &str) -> Result<Operand, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("empty operand".to_string());
    }

    if token.starts_with('(') {
        return parse_mem(token);
    }

    if let Some(result) = parse_port(token) {
        return result;
    }

    let upper = token.to_ascii_uppercase();
    match upper.as_str() {
        "D" => return Ok(Operand::D),
        "DF" => return Ok(Operand::Df),
        "Q" => return Ok(Operand::Q),
        "EI" => return Ok(Operand::Ei),
        "EF1" => return Ok(Operand::Ef(1)),
        "EF2" => return Ok(Operand::Ef(2)),
        "EF3" => return Ok(Operand::Ef(3)),
        "EF4" => return Ok(Operand::Ef(4)),
        _ => {}
    }

    if let Some(result) = parse_reg_or_half(&upper, token) {
        return result;
    }

    Ok(Operand::Imm(token.to_string()))
}

/// `(Rn)` and its four adjust variants (Requirement 2.5). Reached only once
/// the token structurally commits to this shape (a leading `(`), so any
/// failure here is a hard error, never an `Imm` fallback.
pub(crate) fn parse_mem(token: &str) -> Result<Operand, String> {
    let inner = token
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| format!("unrecognized memory operand `{token}`"))?;
    let upper = inner.to_ascii_uppercase();
    let (adjust, reg_part) = if let Some(r) = upper.strip_prefix("++") {
        (Adjust::PreInc, r)
    } else if let Some(r) = upper.strip_prefix("--") {
        (Adjust::PreDec, r)
    } else if let Some(r) = upper.strip_suffix("++") {
        (Adjust::PostInc, r)
    } else if let Some(r) = upper.strip_suffix("--") {
        (Adjust::PostDec, r)
    } else {
        (Adjust::None, upper.as_str())
    };
    let Some(reg_part) = reg_part.strip_prefix('R') else {
        return Err(format!("unrecognized memory operand `{token}`"));
    };
    match classify_reg_part(reg_part) {
        RegShape::Valid(n) => Ok(Operand::Mem(RegRef(n), adjust)),
        RegShape::OutOfRange | RegShape::NotRegister => {
            Err(format!("unrecognized memory operand `{token}`"))
        }
    }
}

/// `PORT(n)`, `n` = 1-7 (Requirement 2.6). Returns `None` if the token
/// doesn't structurally look like `PORT(...)` at all, so the caller keeps
/// trying other operand shapes; once it does look like `PORT(...)`, a bad
/// `n` is an error, never an `Imm` fallback.
fn parse_port(token: &str) -> Option<Result<Operand, String>> {
    let upper = token.to_ascii_uppercase();
    let inner = upper.strip_prefix("PORT(")?.strip_suffix(')')?;
    let inner = inner.trim();
    let valid = inner.len() == 1
        && inner
            .chars()
            .next()
            .is_some_and(|c| ('1'..='7').contains(&c));
    if valid {
        Some(Ok(Operand::Port(inner.to_string())))
    } else {
        Some(Err(format!(
            "PORT number must be 1-7 (the only ports INP/OUT can address): `{token}`"
        )))
    }
}

/// `Rn` / `Rn.0` / `Rn.1` / `Rn'` (Requirement 2.3/2.4). Returns `None` if
/// the token isn't register-shaped at all, so the caller falls back to
/// `Imm`; a register-shaped token with an invalid number (`R16`) is an
/// error, never an `Imm` fallback.
fn parse_reg_or_half(upper: &str, original: &str) -> Option<Result<Operand, String>> {
    let rest = upper.strip_prefix('R')?;
    if rest.is_empty() {
        return None;
    }
    let (reg_part, half) = strip_half(rest);
    match classify_reg_part(reg_part) {
        RegShape::NotRegister => None,
        RegShape::OutOfRange => Some(Err(format!(
            "register number out of range (0-15): `{original}`"
        ))),
        RegShape::Valid(n) => {
            let reg = RegRef(n);
            Some(Ok(match half {
                Some(h) => Operand::RegHalf(reg, h),
                None => Operand::Reg(reg),
            }))
        }
    }
}

/// Strips a trailing `.0` (low), `.1` or `'` (high) half-selector.
fn strip_half(s: &str) -> (&str, Option<Half>) {
    if let Some(r) = s.strip_suffix(".0") {
        (r, Some(Half::Low))
    } else if let Some(r) = s.strip_suffix(".1") {
        (r, Some(Half::High))
    } else if let Some(r) = s.strip_suffix('\'') {
        (r, Some(Half::High))
    } else {
        (s, None)
    }
}

enum RegShape {
    /// Doesn't look like a register number/letter at all.
    NotRegister,
    /// Looks exactly like a register reference, value 0-15.
    Valid(u8),
    /// Looks like a register reference (all digits) but outside 0-15.
    OutOfRange,
}

/// Classifies the part of a token between `R` (or `(`/adjust markers) and
/// an optional half-selector: all-digits (numeric register, range
/// checked), a single hex letter `A`-`F` (10-15), or neither.
fn classify_reg_part(s: &str) -> RegShape {
    if s.is_empty() {
        return RegShape::NotRegister;
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return match s.parse::<u16>() {
            Ok(n) if n <= 15 => RegShape::Valid(n as u8),
            _ => RegShape::OutOfRange,
        };
    }
    if s.len() == 1 {
        let c = s.chars().next().expect("len == 1");
        if ('A'..='F').contains(&c) {
            return RegShape::Valid(10 + (c as u8 - b'A'));
        }
    }
    RegShape::NotRegister
}

/// Parses `token` as a bare full-register reference (`R3`, `RA`, ...),
/// returning `None` for anything else (a half-register, a memory operand,
/// an immediate, ...). Used by `xtrack` to read the register named by a
/// pass-through `SEX Rn` line.
pub(crate) fn parse_register_operand(token: &str) -> Option<RegRef> {
    match parse_operand(token) {
        Ok(Operand::Reg(r)) => Some(r),
        _ => None,
    }
}

/// A fully parsed MOL1802 statement line: its leading label (if any), the
/// parsed statement, and its trailing comment (if any -- Requirement
/// 10.1, captured here for lowering/re-merge to attach to the generated
/// output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParsedLine {
    pub label: Option<String>,
    pub statement: Statement,
    pub comment: Option<String>,
}

/// Parses one line already classified as
/// [`crate::classify::Classification::Statement`].
pub(crate) fn parse_statement_line(
    label: Option<&str>,
    body: &str,
    comment: Option<&str>,
) -> Result<ParsedLine, String> {
    let statement = parse_statement_body(body)?;
    Ok(ParsedLine {
        label: label.map(str::to_string),
        statement,
        comment: comment.map(str::to_string),
    })
}

/// Dispatches to the chain or branch parser based on which of the three
/// MOL1802 statement shapes `body` has. Callers only ever pass a `body`
/// the line classifier (task 3) already confirmed is one of these
/// shapes.
fn parse_statement_body(body: &str) -> Result<Statement, String> {
    if body.contains(':') {
        return parse_chain(body);
    }
    parse_branch(body)
}

/// `A : B` (Requirement 3.1), a fan-out chain `A : B : C : ...` (the
/// same source written into more than one destination, left to right),
/// or the `STK` push/pop chaining sugar: a comma-separated list of
/// `D`/`Rn` elements on one side and exactly `STK` on the other
/// (`list:STK` pushes, `STK:list` pops). A comma anywhere outside a
/// `STK` chain is a parse error: the comma-list is special syntax
/// reserved for `STK`, not a general chain feature, and `STK` itself is
/// only valid in a plain two-element chain, never as one destination
/// among several in a fan-out.
pub(crate) fn parse_chain(body: &str) -> Result<Statement, String> {
    let parts: Vec<&str> = body.split(':').map(str::trim).collect();

    if parts.len() == 2 {
        let (left, right) = (parts[0], parts[1]);
        if right.eq_ignore_ascii_case("STK") {
            return Ok(Statement::Stack {
                elements: parse_stack_list(left)?,
                direction: StackDirection::Push,
            });
        }
        if left.eq_ignore_ascii_case("STK") {
            return Ok(Statement::Stack {
                elements: parse_stack_list(right)?,
                direction: StackDirection::Pop,
            });
        }
    }

    if parts.iter().any(|p| p.eq_ignore_ascii_case("STK")) {
        return Err(format!(
            "`STK` is only valid in a two-element chain (`list:STK` or \
             `STK:list`), not as one destination among several: `{body}`"
        ));
    }
    if parts.iter().any(|p| p.contains(',')) {
        return Err(format!(
            "`,` is only valid next to `STK` (e.g. `R10,R9:STK`): `{body}`"
        ));
    }

    let operands: Vec<Operand> = parts
        .into_iter()
        .map(parse_operand)
        .collect::<Result<_, _>>()?;
    Ok(Statement::Chain(operands))
}

/// The comma-separated list on the non-`STK` side of a `STK` chain. Each
/// element must be exactly `D` or a full register `Rn` -- nothing else
/// has a defined push/pop byte sequence.
fn parse_stack_list(list: &str) -> Result<Vec<StackElement>, String> {
    let mut elements = Vec::new();
    for token in list.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(format!("empty element in `STK` list: `{list}`"));
        }
        match parse_operand(token)? {
            Operand::D => elements.push(StackElement::D),
            Operand::Reg(r) => elements.push(StackElement::Reg(r)),
            _ => {
                return Err(format!(
                    "`{token}` isn't valid next to `STK` (only `D` or a full register `Rn`)"
                ))
            }
        }
    }
    if elements.is_empty() {
        return Err(format!("empty `STK` list: `{list}`"));
    }
    Ok(elements)
}

/// The condition-token table (Requirement 5.1) plus `/LABEL` / `//LABEL`
/// (Requirement 5.2), conditional or bare.
pub(crate) fn parse_branch(body: &str) -> Result<Statement, String> {
    let upper = body.to_ascii_uppercase();
    if let Some((token, cond)) = CONDITION_TOKENS.iter().find(|(t, _)| upper.starts_with(t)) {
        let rest = &body[token.len()..];
        let (len, target) = parse_branch_tail(rest)
            .ok_or_else(|| format!("expected `/LABEL` or `//LABEL` after `{token}`: `{body}`"))?;
        return Ok(Statement::Branch {
            cond: Some(*cond),
            len,
            target,
        });
    }
    let (len, target) =
        parse_branch_tail(body).ok_or_else(|| format!("unrecognized branch: `{body}`"))?;
    Ok(Statement::Branch {
        cond: None,
        len,
        target,
    })
}

/// Tolerates whitespace between a condition and the `/`/`//` (e.g.
/// `EF1=0? /LOOP`), matching `classify.rs`'s `is_branch_tail`.
fn parse_branch_tail(s: &str) -> Option<(BranchLength, String)> {
    let s = s.trim_start();
    if let Some(t) = s.strip_prefix("//") {
        let t = t.trim();
        return (!t.is_empty()).then(|| (BranchLength::Long, t.to_string()));
    }
    if let Some(t) = s.strip_prefix('/') {
        let t = t.trim();
        return (!t.is_empty()).then(|| (BranchLength::Short, t.to_string()));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Adjust, Condition, Half, Operand, RegRef};

    fn reg(n: u8) -> RegRef {
        RegRef(n)
    }

    #[test]
    fn d_and_df() {
        assert_eq!(parse_operand("D"), Ok(Operand::D));
        assert_eq!(parse_operand("d"), Ok(Operand::D));
        assert_eq!(parse_operand("DF"), Ok(Operand::Df));
        assert_eq!(parse_operand("df"), Ok(Operand::Df));
    }

    #[test]
    fn q_and_ei() {
        assert_eq!(parse_operand("Q"), Ok(Operand::Q));
        assert_eq!(parse_operand("EI"), Ok(Operand::Ei));
        assert_eq!(parse_operand("ei"), Ok(Operand::Ei));
    }

    #[test]
    fn ef_lines() {
        for k in 1..=4u8 {
            assert_eq!(parse_operand(&format!("EF{k}")), Ok(Operand::Ef(k)));
        }
    }

    #[test]
    fn full_register() {
        assert_eq!(parse_operand("R0"), Ok(Operand::Reg(reg(0))));
        assert_eq!(parse_operand("R15"), Ok(Operand::Reg(reg(15))));
        assert_eq!(parse_operand("r6"), Ok(Operand::Reg(reg(6))));
        assert_eq!(parse_operand("RA"), Ok(Operand::Reg(reg(10))));
        assert_eq!(parse_operand("rf"), Ok(Operand::Reg(reg(15))));
    }

    #[test]
    fn register_half() {
        assert_eq!(
            parse_operand("R3.0"),
            Ok(Operand::RegHalf(reg(3), Half::Low))
        );
        assert_eq!(
            parse_operand("R3.1"),
            Ok(Operand::RegHalf(reg(3), Half::High))
        );
        assert_eq!(
            parse_operand("R3'"),
            Ok(Operand::RegHalf(reg(3), Half::High))
        );
        assert_eq!(
            parse_operand("RA.1"),
            Ok(Operand::RegHalf(reg(10), Half::High))
        );
    }

    #[test]
    fn mem_operand_plain_and_adjusted() {
        assert_eq!(
            parse_operand("(R3)"),
            Ok(Operand::Mem(reg(3), Adjust::None))
        );
        assert_eq!(
            parse_operand("(R3++)"),
            Ok(Operand::Mem(reg(3), Adjust::PostInc))
        );
        assert_eq!(
            parse_operand("(R3--)"),
            Ok(Operand::Mem(reg(3), Adjust::PostDec))
        );
        assert_eq!(
            parse_operand("(++R3)"),
            Ok(Operand::Mem(reg(3), Adjust::PreInc))
        );
        assert_eq!(
            parse_operand("(--R3)"),
            Ok(Operand::Mem(reg(3), Adjust::PreDec))
        );
        assert_eq!(
            parse_operand("(ra)"),
            Ok(Operand::Mem(reg(10), Adjust::None))
        );
    }

    #[test]
    fn port_1_to_7() {
        for n in 1..=7u8 {
            assert_eq!(
                parse_operand(&format!("PORT({n})")),
                Ok(Operand::Port(n.to_string()))
            );
        }
        assert_eq!(parse_operand("port(4)"), Ok(Operand::Port("4".to_string())));
    }

    #[test]
    fn imm_fallback_for_anything_else() {
        assert_eq!(parse_operand("LABEL"), Ok(Operand::Imm("LABEL".into())));
        assert_eq!(parse_operand("#FF"), Ok(Operand::Imm("#FF".into())));
        assert_eq!(parse_operand("42"), Ok(Operand::Imm("42".into())));
        assert_eq!(parse_operand("RESULT"), Ok(Operand::Imm("RESULT".into())));
        assert_eq!(parse_operand("ROW"), Ok(Operand::Imm("ROW".into())));
    }

    #[test]
    fn out_of_range_register_is_an_error_not_an_imm_fallback() {
        assert!(parse_operand("R16").is_err());
        assert!(parse_operand("R99").is_err());
    }

    #[test]
    fn bad_port_number_is_an_error_not_an_imm_fallback() {
        assert!(parse_operand("PORT(8)").is_err());
        assert!(parse_operand("PORT(0)").is_err());
    }

    #[test]
    fn malformed_mem_operand_is_an_error() {
        assert!(parse_operand("(R16)").is_err());
        assert!(parse_operand("(RG)").is_err());
        assert!(parse_operand("(R3").is_err());
        assert!(parse_operand("()").is_err());
    }

    #[test]
    fn empty_operand_is_an_error() {
        assert!(parse_operand("").is_err());
        assert!(parse_operand("   ").is_err());
    }

    #[test]
    fn chain_parses_two_operands() {
        assert_eq!(
            parse_chain("R3.0 : D"),
            Ok(Statement::Chain(vec![
                Operand::RegHalf(reg(3), Half::Low),
                Operand::D
            ]))
        );
    }

    #[test]
    fn fanout_chain_parses_more_than_two_operands() {
        assert_eq!(
            parse_chain("PAGE:R2.1:R4.1:R5.1"),
            Ok(Statement::Chain(vec![
                Operand::Imm("PAGE".to_string()),
                Operand::RegHalf(reg(2), Half::High),
                Operand::RegHalf(reg(4), Half::High),
                Operand::RegHalf(reg(5), Half::High),
            ]))
        );
        assert_eq!(
            parse_chain("D : R3.0 : R4.0"),
            Ok(Statement::Chain(vec![
                Operand::D,
                Operand::RegHalf(reg(3), Half::Low),
                Operand::RegHalf(reg(4), Half::Low),
            ]))
        );
    }

    #[test]
    fn stk_in_the_middle_of_a_fanout_chain_is_an_error() {
        assert!(parse_chain("R3:STK:R4").is_err());
        assert!(parse_chain("STK:R3:R4").is_err());
    }

    #[test]
    fn chain_with_a_bad_operand_propagates_the_operand_error() {
        assert!(parse_chain("R16 : D").is_err());
        assert!(parse_chain("D : R16 : R4.0").is_err());
    }

    #[test]
    fn branch_conditions_cover_the_whole_table() {
        let cases: &[(&str, Condition)] = &[
            ("EF1=0?/L", Condition::EfEq(1, false)),
            ("EF1=1?/L", Condition::EfEq(1, true)),
            ("EF2=0?/L", Condition::EfEq(2, false)),
            ("EF2=1?/L", Condition::EfEq(2, true)),
            ("EF3=0?/L", Condition::EfEq(3, false)),
            ("EF3=1?/L", Condition::EfEq(3, true)),
            ("EF4=0?/L", Condition::EfEq(4, false)),
            ("EF4=1?/L", Condition::EfEq(4, true)),
            ("D=0?/L", Condition::DZero(true)),
            ("D<>0?/L", Condition::DZero(false)),
            ("DF=0?/L", Condition::DfEq(false)),
            ("DF=1?/L", Condition::DfEq(true)),
            ("Q=0?/L", Condition::QEq(false)),
            ("Q=1?/L", Condition::QEq(true)),
        ];
        for (text, cond) in cases {
            assert_eq!(
                parse_branch(text),
                Ok(Statement::Branch {
                    cond: Some(*cond),
                    len: BranchLength::Short,
                    target: "L".to_string(),
                }),
                "for `{text}`"
            );
        }
    }

    #[test]
    fn branch_short_vs_long() {
        assert_eq!(
            parse_branch("D=0?/LOOP"),
            Ok(Statement::Branch {
                cond: Some(Condition::DZero(true)),
                len: BranchLength::Short,
                target: "LOOP".to_string(),
            })
        );
        assert_eq!(
            parse_branch("D=0?//LOOP"),
            Ok(Statement::Branch {
                cond: Some(Condition::DZero(true)),
                len: BranchLength::Long,
                target: "LOOP".to_string(),
            })
        );
    }

    #[test]
    fn bare_branch_short_and_long() {
        assert_eq!(
            parse_branch("/LOOP"),
            Ok(Statement::Branch {
                cond: None,
                len: BranchLength::Short,
                target: "LOOP".to_string(),
            })
        );
        assert_eq!(
            parse_branch("//LOOP"),
            Ok(Statement::Branch {
                cond: None,
                len: BranchLength::Long,
                target: "LOOP".to_string(),
            })
        );
    }

    #[test]
    fn branch_without_a_target_is_an_error() {
        assert!(parse_branch("D=0?/").is_err());
        assert!(parse_branch("/").is_err());
        assert!(parse_branch("D=0?").is_err());
    }

    #[test]
    fn space_between_a_condition_and_the_slash_is_tolerated() {
        assert_eq!(
            parse_branch("EF4=0? /MAIN"),
            Ok(Statement::Branch {
                cond: Some(Condition::EfEq(4, false)),
                len: BranchLength::Short,
                target: "MAIN".to_string(),
            })
        );
        assert_eq!(
            parse_branch("D<>0?  //LOOP"),
            Ok(Statement::Branch {
                cond: Some(Condition::DZero(false)),
                len: BranchLength::Long,
                target: "LOOP".to_string(),
            })
        );
    }

    #[test]
    fn single_register_stack_push_and_pop() {
        assert_eq!(
            parse_chain("R10:STK"),
            Ok(Statement::Stack {
                elements: vec![StackElement::Reg(reg(10))],
                direction: StackDirection::Push,
            })
        );
        assert_eq!(
            parse_chain("STK:R10"),
            Ok(Statement::Stack {
                elements: vec![StackElement::Reg(reg(10))],
                direction: StackDirection::Pop,
            })
        );
        // Case-insensitive, like every other keyword.
        assert_eq!(
            parse_chain("R10:stk"),
            Ok(Statement::Stack {
                elements: vec![StackElement::Reg(reg(10))],
                direction: StackDirection::Push,
            })
        );
    }

    #[test]
    fn single_d_stack_push_and_pop() {
        assert_eq!(
            parse_chain("D:STK"),
            Ok(Statement::Stack {
                elements: vec![StackElement::D],
                direction: StackDirection::Push,
            })
        );
        assert_eq!(
            parse_chain("STK:D"),
            Ok(Statement::Stack {
                elements: vec![StackElement::D],
                direction: StackDirection::Pop,
            })
        );
    }

    #[test]
    fn comma_chained_stack_push_and_pop() {
        assert_eq!(
            parse_chain("R10,R9,R8,R7:STK"),
            Ok(Statement::Stack {
                elements: vec![
                    StackElement::Reg(reg(10)),
                    StackElement::Reg(reg(9)),
                    StackElement::Reg(reg(8)),
                    StackElement::Reg(reg(7)),
                ],
                direction: StackDirection::Push,
            })
        );
        assert_eq!(
            parse_chain("STK:R7,R8,R9,R10"),
            Ok(Statement::Stack {
                elements: vec![
                    StackElement::Reg(reg(7)),
                    StackElement::Reg(reg(8)),
                    StackElement::Reg(reg(9)),
                    StackElement::Reg(reg(10)),
                ],
                direction: StackDirection::Pop,
            })
        );
    }

    #[test]
    fn comma_without_stk_is_an_error() {
        assert!(parse_chain("R3,R4 : D").is_err());
        assert!(parse_chain("D : R3,R4").is_err());
    }

    #[test]
    fn stk_list_with_an_invalid_element_is_an_error() {
        assert!(parse_chain("R3.0:STK").is_err());
        assert!(parse_chain("(R3):STK").is_err());
        assert!(parse_chain("PORT(4):STK").is_err());
        assert!(parse_chain("R10,,R9:STK").is_err());
    }

    #[test]
    fn statement_dispatcher_routes_each_shape() {
        assert!(matches!(
            parse_statement_body("D : R3.0").unwrap(),
            Statement::Chain(_)
        ));
        assert!(matches!(
            parse_statement_body("D=0?/LOOP").unwrap(),
            Statement::Branch { .. }
        ));
        assert!(matches!(
            parse_statement_body("/LOOP").unwrap(),
            Statement::Branch { .. }
        ));
        assert!(matches!(
            parse_statement_body("R10:STK").unwrap(),
            Statement::Stack { .. }
        ));
        assert!(matches!(
            parse_statement_body("STK:R10").unwrap(),
            Statement::Stack { .. }
        ));
    }

    #[test]
    fn comment_is_captured_on_the_parsed_line() {
        let parsed = parse_statement_line(None, "R3.0 : D", Some("move low byte")).unwrap();
        assert_eq!(parsed.comment, Some("move low byte".to_string()));
        assert_eq!(parsed.label, None);
        assert_eq!(
            parsed.statement,
            Statement::Chain(vec![Operand::RegHalf(reg(3), Half::Low), Operand::D])
        );
    }

    #[test]
    fn no_comment_is_none_on_the_parsed_line() {
        let parsed = parse_statement_line(Some("LOOP"), "D:STK", None).unwrap();
        assert_eq!(parsed.comment, None);
        assert_eq!(parsed.label, Some("LOOP".to_string()));
        assert_eq!(
            parsed.statement,
            Statement::Stack {
                elements: vec![StackElement::D],
                direction: StackDirection::Push,
            }
        );
    }
}
