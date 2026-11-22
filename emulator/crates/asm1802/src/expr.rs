//! Expression evaluation.
//!
//! Operators, lowest precedence first:
//!
//! | Operator | Meaning |
//! |---|---|
//! | `LOW x`, `HIGH x` | low / high byte of everything that follows |
//! | `\|` | bitwise or |
//! | `^` | bitwise exclusive or |
//! | `&` | bitwise and |
//! | `<<`, `>>` | shifts |
//! | `+`, `-` | add, subtract |
//! | `*`, `/`, `%` | multiply, divide, remainder |
//! | `-x`, `+x`, `~x` | negate, plus, complement |
//!
//! Primaries: numbers (`12`, `0x0F`, `0FH`, `#0F`, `$0F`, `0b1010`),
//! characters (`'A'`, `"A"`, `''''`), `$` or `*` for the current location,
//! symbols, and parenthesised expressions.

/// Symbol lookup used during evaluation.
pub(crate) trait Symbols {
    fn lookup(&self, name: &str) -> Option<i64>;
}

/// Result of a successful evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Value {
    pub value: i64,
    /// First undefined symbol encountered (its value was taken as 0).
    pub undefined: Option<String>,
}

/// Evaluate `text` with `here` as the value of `$`.
pub(crate) fn eval(text: &str, here: i64, syms: &dyn Symbols) -> Result<Value, String> {
    let mut p = Parser {
        s: text.as_bytes(),
        text,
        pos: 0,
        here,
        syms,
        undefined: None,
    };
    p.skip_ws();
    if p.at_end() {
        return Err("missing expression".into());
    }
    let value = p.expr()?;
    p.skip_ws();
    if !p.at_end() {
        return Err(format!("unexpected `{}` in expression", &text[p.pos..]));
    }
    Ok(Value {
        value,
        undefined: p.undefined,
    })
}

pub(crate) fn is_symbol_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || matches!(c, b'_' | b'.' | b':' | b'?' | b'@')
}

pub(crate) fn is_symbol_char(c: u8) -> bool {
    is_symbol_start(c) || c.is_ascii_digit()
}

struct Parser<'a> {
    s: &'a [u8],
    text: &'a str,
    pos: usize,
    here: i64,
    syms: &'a dyn Symbols,
    undefined: Option<String>,
}

impl Parser<'_> {
    fn at_end(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.s.get(self.pos + 1).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    /// Consume `op` if it comes next (after whitespace).
    fn eat(&mut self, op: &str) -> bool {
        self.skip_ws();
        if self.s[self.pos..].starts_with(op.as_bytes()) {
            self.pos += op.len();
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<i64, String> {
        self.or()
    }

    fn or(&mut self) -> Result<i64, String> {
        let mut v = self.xor()?;
        while self.eat("|") {
            v |= self.xor()?;
        }
        Ok(v)
    }

    fn xor(&mut self) -> Result<i64, String> {
        let mut v = self.and()?;
        while self.eat("^") {
            v ^= self.and()?;
        }
        Ok(v)
    }

    fn and(&mut self) -> Result<i64, String> {
        let mut v = self.shift()?;
        while self.eat("&") {
            v &= self.shift()?;
        }
        Ok(v)
    }

    fn shift(&mut self) -> Result<i64, String> {
        let mut v = self.add()?;
        loop {
            let left = if self.eat("<<") {
                true
            } else if self.eat(">>") {
                false
            } else {
                return Ok(v);
            };
            let n = self.add()?;
            if !(0..64).contains(&n) {
                return Err(format!("shift count {n} out of range"));
            }
            v = if left { v << n } else { v >> n };
        }
    }

    fn add(&mut self) -> Result<i64, String> {
        let mut v = self.mul()?;
        loop {
            if self.eat("+") {
                v = v.wrapping_add(self.mul()?);
            } else if self.eat("-") {
                v = v.wrapping_sub(self.mul()?);
            } else {
                return Ok(v);
            }
        }
    }

    fn mul(&mut self) -> Result<i64, String> {
        let mut v = self.unary()?;
        loop {
            self.skip_ws();
            let op = match self.peek() {
                Some(c @ (b'*' | b'/' | b'%')) => c,
                _ => return Ok(v),
            };
            self.pos += 1;
            let r = self.unary()?;
            v = match op {
                b'*' => v.wrapping_mul(r),
                _ if r == 0 => {
                    // Undefined symbols read as 0 in early passes; don't fail there.
                    if self.undefined.is_some() {
                        0
                    } else {
                        return Err("division by zero".into());
                    }
                }
                b'/' => v.wrapping_div(r),
                _ => v.wrapping_rem(r),
            };
        }
    }

    fn unary(&mut self) -> Result<i64, String> {
        self.skip_ws();
        if let Some(kw) = self.byte_keyword() {
            let v = self.expr()?;
            return Ok(if kw == ByteKw::Low {
                v & 0xFF
            } else {
                (v >> 8) & 0xFF
            });
        }
        match self.peek() {
            Some(b'-') => {
                self.pos += 1;
                Ok(self.unary()?.wrapping_neg())
            }
            Some(b'+') => {
                self.pos += 1;
                self.unary()
            }
            Some(b'~') => {
                self.pos += 1;
                Ok(!self.unary()?)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<i64, String> {
        self.skip_ws();
        let Some(c) = self.peek() else {
            return Err("expression ends unexpectedly".into());
        };
        match c {
            b'(' => {
                self.pos += 1;
                let v = self.expr()?;
                if !self.eat(")") {
                    return Err("missing `)`".into());
                }
                Ok(v)
            }
            b'0'..=b'9' => self.number(),
            b'#' => {
                self.pos += 1;
                self.radix_digits(16, "#")
            }
            b'H' | b'h' if self.peek2() == Some(b'\'') => {
                // RCA/PL/M style: `H'4000` (no closing quote).
                self.pos += 2;
                self.radix_digits(16, "H'")
            }
            b'$' if self.peek2().is_some_and(|d| d.is_ascii_hexdigit()) => {
                self.pos += 1;
                self.radix_digits(16, "$")
            }
            b'$' | b'*' => {
                self.pos += 1;
                Ok(self.here)
            }
            b'\'' | b'"' => self.char_literal(c),
            c if is_symbol_start(c) => {
                let start = self.pos;
                while self.peek().is_some_and(is_symbol_char) {
                    self.pos += 1;
                }
                let name = &self.text[start..self.pos];
                match self.syms.lookup(name) {
                    Some(v) => Ok(v),
                    None => {
                        if self.undefined.is_none() {
                            self.undefined = Some(name.to_string());
                        }
                        Ok(0)
                    }
                }
            }
            _ => Err(format!("unexpected `{}` in expression", c as char)),
        }
    }

    /// `LOW` or `HIGH` followed by whitespace or `(`.
    fn byte_keyword(&mut self) -> Option<ByteKw> {
        for (word, kw) in [("LOW", ByteKw::Low), ("HIGH", ByteKw::High)] {
            let end = self.pos + word.len();
            if self.s.len() > end
                && self.s[self.pos..end].eq_ignore_ascii_case(word.as_bytes())
                && matches!(self.s[end], b' ' | b'\t' | b'(')
            {
                self.pos = end;
                return Some(kw);
            }
        }
        None
    }

    fn number(&mut self) -> Result<i64, String> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            self.pos += 1;
        }
        let tok = &self.text[start..self.pos];
        let digits = tok.replace('_', "");
        let lower = digits.to_ascii_lowercase();
        let (body, radix) = if let Some(h) = lower.strip_prefix("0x") {
            (h, 16)
        } else if let Some(h) = lower.strip_suffix('h') {
            (h, 16)
        } else if let Some(b) = lower.strip_prefix("0b") {
            (b, 2)
        } else {
            (lower.as_str(), 10)
        };
        if body.is_empty() {
            return Err(format!("bad number `{tok}`"));
        }
        i64::from_str_radix(body, radix).map_err(|_| format!("bad number `{tok}`"))
    }

    fn radix_digits(&mut self, radix: u32, prefix: &str) -> Result<i64, String> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        let body = &self.text[start..self.pos];
        if body.is_empty() {
            return Err(format!("missing digits after `{prefix}`"));
        }
        i64::from_str_radix(body, radix).map_err(|_| format!("bad number `{prefix}{body}`"))
    }

    fn char_literal(&mut self, q: u8) -> Result<i64, String> {
        let start = self.pos;
        let bytes = parse_string(self.s, &mut self.pos)
            .ok_or_else(|| format!("unterminated {} quote", q as char))?;
        match bytes.as_slice() {
            [b] => Ok(i64::from(*b)),
            [] => Err("empty character literal".into()),
            _ => Err(format!(
                "string {} where one value is expected",
                &self.text[start..self.pos]
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ByteKw {
    Low,
    High,
}

/// Parse a quoted string starting at `s[*pos]` (the opening quote). A doubled
/// quote stands for one quote character. Advances `pos` past the closing quote.
pub(crate) fn parse_string(s: &[u8], pos: &mut usize) -> Option<Vec<u8>> {
    let q = s[*pos];
    let mut i = *pos + 1;
    let mut out = Vec::new();
    loop {
        let c = *s.get(i)?;
        if c == q {
            if s.get(i + 1) == Some(&q) {
                out.push(q);
                i += 2;
                continue;
            }
            *pos = i + 1;
            return Some(out);
        }
        out.push(c);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Map(HashMap<&'static str, i64>);
    impl Symbols for Map {
        fn lookup(&self, name: &str) -> Option<i64> {
            self.0.get(name).copied()
        }
    }

    fn ev(text: &str) -> i64 {
        let m = Map(HashMap::from([("BUFF", 0xDB0), ("X", 5)]));
        let v = eval(text, 0x100, &m).unwrap();
        assert_eq!(v.undefined, None, "{text}");
        v.value
    }

    #[test]
    fn numbers() {
        assert_eq!(ev("12"), 12);
        assert_eq!(ev("0x0F"), 15);
        assert_eq!(ev("0D7H"), 0xD7);
        assert_eq!(ev("9dh"), 0x9D);
        assert_eq!(ev("#80"), 0x80);
        assert_eq!(ev("$1F"), 0x1F);
        assert_eq!(ev("0b101"), 5);
        assert_eq!(ev("'A'"), 65);
        assert_eq!(ev("\"!\""), 33);
        assert_eq!(ev("''''"), 39);
        assert_eq!(ev("' '"), 32);
    }

    #[test]
    fn operators() {
        assert_eq!(ev("$+3"), 0x103);
        assert_eq!(ev("*-2"), 0xFE);
        assert_eq!(ev("BUFF<<3"), 0xDB0 << 3);
        assert_eq!(ev("BUFF+344"), 0xDB0 + 344);
        assert_eq!(ev("2+3*4"), 14);
        assert_eq!(ev("(2+3)*4"), 20);
        assert_eq!(ev("-X+1"), -4);
        assert_eq!(ev("7/2"), 3);
        assert_eq!(ev("7%2"), 1);
        assert_eq!(ev("0xF0|0x0F&0x3C"), 0xFC);
        assert_eq!(ev("LOW BUFF"), 0xB0);
        assert_eq!(ev("HIGH BUFF"), 0x0D);
        assert_eq!(ev("LOW $+3"), 0x03);
        assert_eq!(ev("HIGH BUFF-0x100"), 0x0C);
        assert_eq!(ev("low(BUFF)"), 0xB0);
    }

    #[test]
    fn undefined_symbol_reads_as_zero() {
        let m = Map(HashMap::new());
        let v = eval("FOO+1", 0, &m).unwrap();
        assert_eq!(v.value, 1);
        assert_eq!(v.undefined.as_deref(), Some("FOO"));
    }

    #[test]
    fn errors() {
        let m = Map(HashMap::new());
        for bad in ["", "1+", "(1", "1/0", "'AB'", "'A", "0xZZ", "1 2", "#"] {
            assert!(eval(bad, 0, &m).is_err(), "{bad:?}");
        }
    }
}
