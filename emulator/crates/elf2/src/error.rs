//! Error types.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// JSON could not be parsed.
    Json(String),
    /// A field has an unusable value.
    Invalid { field: &'static str, msg: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Json(e) => write!(f, "configuration JSON: {e}"),
            ConfigError::Invalid { field, msg } => write!(f, "configuration {field}: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachineError {
    /// ROM bytes do not match the checksum the configuration expects.
    RomChecksum {
        name: String,
        expected: String,
        actual: String,
    },
    /// ROM or program does not fit in the 64K address space.
    TooLarge { base: u16, len: usize },
    /// ROM overlaps another ROM.
    RomOverlap { name: String, other: String },
    /// Intel HEX file could not be read.
    Hex(crate::hexfile::HexError),
    /// A file would write to an address with no RAM or ROM.
    OutsideMemory { addr: u16, ram_size: u32 },
    /// A snapshot could not be restored.
    Snapshot(String),
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MachineError::RomChecksum {
                name,
                expected,
                actual,
            } => write!(
                f,
                "ROM '{name}' has SHA-256 {actual}, but the configuration expects {expected}"
            ),
            MachineError::TooLarge { base, len } => write!(
                f,
                "{len} bytes at {base:04X} run past FFFF (only {} fit)",
                0x10000 - usize::from(*base)
            ),
            MachineError::RomOverlap { name, other } => {
                write!(f, "ROM '{name}' overlaps ROM '{other}'")
            }
            MachineError::Hex(e) => write!(f, "{e}"),
            MachineError::OutsideMemory { addr, ram_size } => write!(
                f,
                "address {addr:04X} has no memory (RAM is 0000-{:04X})",
                ram_size - 1
            ),
            MachineError::Snapshot(msg) => write!(f, "snapshot: {msg}"),
        }
    }
}

impl std::error::Error for MachineError {}
