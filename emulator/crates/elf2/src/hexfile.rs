//! Intel HEX reading and writing (requirement 10.1, 10.2).
//!
//! Supported records: 00 data, 01 end of file, 02/04 extended address (only a zero
//! segment or upper address is accepted, since the 1802 has a 16-bit address space),
//! 03/05 start address (ignored).

use std::fmt;

/// A parse error, with the 1-based line number it was found on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HexError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for HexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Intel HEX line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for HexError {}

/// A run of bytes at an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub addr: u16,
    pub data: Vec<u8>,
}

fn hex_byte(s: &str, at: usize, line: usize) -> Result<u8, HexError> {
    s.get(at..at + 2)
        .and_then(|h| u8::from_str_radix(h, 16).ok())
        .ok_or_else(|| HexError {
            line,
            msg: format!("bad hex digits at column {}", at + 1),
        })
}

/// Parse Intel HEX text into chunks, in file order. Blank lines are ignored.
pub fn parse(text: &str) -> Result<Vec<Chunk>, HexError> {
    let mut chunks = Vec::new();
    let mut seen_eof = false;
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        if seen_eof {
            return Err(HexError {
                line,
                msg: "data after the end-of-file record".into(),
            });
        }
        let err = |msg: &str| HexError {
            line,
            msg: msg.to_string(),
        };
        let body = s
            .strip_prefix(':')
            .ok_or_else(|| err("record does not start with ':'"))?;
        if body.len() < 10 || body.len() % 2 != 0 {
            return Err(err("record has the wrong length"));
        }
        let mut bytes = Vec::with_capacity(body.len() / 2);
        for k in (0..body.len()).step_by(2) {
            bytes.push(hex_byte(body, k, line)?);
        }
        let count = bytes[0] as usize;
        if bytes.len() != count + 5 {
            return Err(err(&format!(
                "byte count {count} does not match the record length"
            )));
        }
        let sum = bytes.iter().fold(0u8, |a, &b| a.wrapping_add(b));
        if sum != 0 {
            let expected = bytes[..bytes.len() - 1]
                .iter()
                .fold(0u8, |a, &b| a.wrapping_add(b))
                .wrapping_neg();
            return Err(err(&format!(
                "checksum {:02X}, expected {expected:02X}",
                bytes[bytes.len() - 1]
            )));
        }
        let addr = u16::from_be_bytes([bytes[1], bytes[2]]);
        let data = &bytes[4..4 + count];
        match bytes[3] {
            0x00 => {
                if usize::from(addr) + count > 0x10000 {
                    return Err(err("data runs past address FFFF"));
                }
                if !data.is_empty() {
                    chunks.push(Chunk {
                        addr,
                        data: data.to_vec(),
                    });
                }
            }
            0x01 => seen_eof = true,
            0x02 | 0x04 => {
                if data.iter().any(|&b| b != 0) {
                    return Err(err("extended address beyond 64K is not supported"));
                }
            }
            0x03 | 0x05 => {}
            t => return Err(err(&format!("unknown record type {t:02X}"))),
        }
    }
    Ok(chunks)
}

/// Format `data` located at `base` as Intel HEX, 16 bytes per record, with an
/// end-of-file record.
pub fn format(base: u16, data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len() * 3 + 16);
    for (k, chunk) in data.chunks(16).enumerate() {
        let addr = base.wrapping_add((k * 16) as u16);
        let mut rec = vec![chunk.len() as u8, (addr >> 8) as u8, addr as u8, 0x00];
        rec.extend_from_slice(chunk);
        let sum = rec
            .iter()
            .fold(0u8, |a, &b| a.wrapping_add(b))
            .wrapping_neg();
        rec.push(sum);
        out.push(':');
        for b in rec {
            out.push_str(&format!("{b:02X}"));
        }
        out.push('\n');
    }
    out.push_str(":00000001FF\n");
    out
}
