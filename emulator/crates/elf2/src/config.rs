//! Machine configuration (requirements 3.1-3.3, 3.8, 3.9, 6.6, 7, 8.3).
//!
//! Serialised as JSON. Every field has a default, so a partial JSON document (or
//! `{}`) gives the owner's default Elf II.

use crate::error::ConfigError;
use serde::{Deserialize, Serialize};

/// Default CPU clock: 3.579545 MHz NTSC colour-burst crystal / 2.
pub const DEFAULT_CLOCK_HZ: u32 = 1_789_773;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// CPU clock in Hz. Machine cycle = 8 clocks.
    pub clock_hz: u32,
    /// RAM size in bytes, from address 0000. 256..=65536.
    pub ram_size: u32,
    /// Value read from addresses with no RAM or ROM (floating bus).
    pub unmapped_read: u8,
    /// ROM images the configuration needs. Bytes are supplied separately.
    pub roms: Vec<RomSlot>,
    /// Giant Board start-up jumper: leaving reset jumps to F000 instead of running 0000.
    pub start_at_f000: bool,
    /// CDP1861 Pixie video; `None` = not fitted.
    pub pixie: Option<PixieConfig>,
    /// Giant Board bit-banged serial port; `None` = not fitted.
    pub serial: Option<SerialConfig>,
    /// VID-1 terminal settings.
    pub vid1: Vid1Config,
    /// Giant Board CDP1852 parallel ports.
    pub parallel_1852: bool,
    /// Jumper: CDP1852 input service request drives /INT.
    pub parallel_sr_to_int: bool,
    /// Cassette interface (later release; a stub in v1).
    pub cassette: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            clock_hz: DEFAULT_CLOCK_HZ,
            ram_size: 24 * 1024,
            unmapped_read: 0xFF,
            roms: Vec::new(),
            start_at_f000: false,
            pixie: Some(PixieConfig::default()),
            serial: Some(SerialConfig::default()),
            vid1: Vid1Config::default(),
            parallel_1852: false,
            parallel_sr_to_int: false,
            cassette: false,
        }
    }
}

/// A ROM image the configuration expects, identified by name and SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RomSlot {
    pub name: String,
    /// Lower-case hex SHA-256 of the image.
    pub sha256: String,
    /// Load address.
    pub base: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PixieConfig {
    /// `true`: INP 1 turns the display on and OUT 1 turns it off (RCA COSMAC VIP
    /// convention). `false`: the reverse. [verify] against the CDP1861 datasheet.
    pub on_is_inp: bool,
}

impl Default for PixieConfig {
    fn default() -> Self {
        PixieConfig { on_is_inp: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Parity {
    None,
    Even,
    Odd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SerialConfig {
    /// EF line receiving serial data from the VID-1: 3 (default) or 4 (Giant Board jumper).
    pub rx_ef: u8,
    /// Invert the TX line (Q).
    pub tx_inverted: bool,
    /// Invert the RX line (EF).
    pub rx_inverted: bool,
    pub baud: u32,
    /// 7 or 8.
    pub data_bits: u8,
    pub parity: Parity,
    /// 1 or 2.
    pub stop_bits: u8,
    /// Pause between pasted characters, milliseconds of emulated time.
    pub paste_gap_ms: u32,
    /// Pause after a pasted carriage return (the monitor needs time to process a line).
    pub paste_cr_gap_ms: u32,
}

impl Default for SerialConfig {
    fn default() -> Self {
        // Baud rate, framing and polarity are [verify] items (Giant Board manual).
        SerialConfig {
            rx_ef: 3,
            tx_inverted: false,
            rx_inverted: false,
            baud: 300,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            paste_gap_ms: 40,
            paste_cr_gap_ms: 200,
        }
    }
}

/// What a control code does on the VID-1 screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vid1Action {
    /// Cursor one position left (stops at the start of the line).
    CursorLeft,
    /// Cursor one position right (stops at the end of the line).
    CursorRight,
    /// Cursor one line up (stops at the top line).
    CursorUp,
    /// Cursor one line down; at the bottom line the screen scrolls.
    LineFeed,
    /// Cursor one line down, stopping at the bottom line (no scroll).
    CursorDown,
    /// Cursor to column 1 of the current line.
    CarriageReturn,
    /// Cursor to the top-left corner.
    Home,
    /// Clear the screen and home the cursor.
    ClearScreen,
    /// Erase from the cursor to the end of the line.
    EraseToEol,
    /// Erase from the cursor to the end of the screen.
    EraseToEos,
    /// Erase the character at the cursor; the cursor does not move.
    Delete,
    /// Step the cursor back and erase that character.
    Rubout,
}

/// A control code and its action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlCode {
    /// 00-1F or 7F.
    pub code: u8,
    pub action: Vid1Action,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vid1Config {
    /// 64 (default) or 32 characters per line.
    pub columns: u8,
    /// Control-code table. Codes not listed are ignored (nothing is displayed).
    /// Later entries override earlier ones for the same code. The defaults are the
    /// five codes documented for the VID-1; the codes for cursor up/down/right/home
    /// and erase-to-end are [verify] against the VID-1 manual, so they are not bound
    /// until you add them here.
    pub codes: Vec<ControlCode>,
}

impl Default for Vid1Config {
    fn default() -> Self {
        let bind = |code, action| ControlCode { code, action };
        Vid1Config {
            columns: 64,
            codes: vec![
                bind(0x08, Vid1Action::CursorLeft),
                bind(0x0A, Vid1Action::LineFeed),
                bind(0x0C, Vid1Action::ClearScreen),
                bind(0x0D, Vid1Action::CarriageReturn),
                bind(0x7F, Vid1Action::Delete),
            ],
        }
    }
}

impl Config {
    /// Parse JSON; missing fields take their defaults. The result is validated.
    pub fn from_json(text: &str) -> Result<Config, ConfigError> {
        let cfg: Config =
            serde_json::from_str(text).map_err(|e| ConfigError::Json(e.to_string()))?;
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("config serialises")
    }

    /// Machine cycles per second (clock / 8).
    pub fn cycles_per_second(&self) -> u32 {
        self.clock_hz / 8
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let bad = |field: &'static str, msg: String| Err(ConfigError::Invalid { field, msg });
        if self.clock_hz < 8 {
            return bad("clock_hz", format!("{} Hz is too low", self.clock_hz));
        }
        if !(256..=65536).contains(&self.ram_size) {
            return bad(
                "ram_size",
                format!("{} bytes; must be 256..=65536", self.ram_size),
            );
        }
        for r in &self.roms {
            if r.sha256.len() != 64 || !r.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return bad(
                    "roms.sha256",
                    format!("ROM '{}': not a SHA-256 hex digest", r.name),
                );
            }
        }
        if let Some(s) = &self.serial {
            if !(3..=4).contains(&s.rx_ef) {
                return bad("serial.rx_ef", format!("EF{}; must be 3 or 4", s.rx_ef));
            }
            if s.baud == 0 || s.baud > self.cycles_per_second() / 4 {
                return bad("serial.baud", format!("{} baud is out of range", s.baud));
            }
            if !(7..=8).contains(&s.data_bits) {
                return bad(
                    "serial.data_bits",
                    format!("{}; must be 7 or 8", s.data_bits),
                );
            }
            if !(1..=2).contains(&s.stop_bits) {
                return bad(
                    "serial.stop_bits",
                    format!("{}; must be 1 or 2", s.stop_bits),
                );
            }
        }
        if self.vid1.columns != 32 && self.vid1.columns != 64 {
            return bad(
                "vid1.columns",
                format!("{}; must be 32 or 64", self.vid1.columns),
            );
        }
        for c in &self.vid1.codes {
            if c.code >= 0x20 && c.code != 0x7F {
                return bad(
                    "vid1.codes",
                    format!(
                        "{:02X} is a printable character, not a control code",
                        c.code
                    ),
                );
            }
        }
        Ok(())
    }
}
