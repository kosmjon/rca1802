//! Netronics Elf II machine model built on the `cdp1802` core.
//!
//! [`Machine`] owns the CPU and the system around it: memory map, front panel,
//! hex keypad and display, CDP1861, and (from tasks 8-10) the Giant Board serial
//! and parallel ports, VID-1 terminal and the cassette slot.

pub use cdp1802;

mod config;
mod error;
mod event;
pub mod hexfile;
mod keypad;
mod machine;
mod memory;
mod panel;
pub mod parallel;
pub mod pixie;
pub mod pkeyboard;
pub mod printer;
pub mod serial;
mod snapshot;
pub mod vid1;

pub use snapshot::{SNAPSHOT_MAGIC, SNAPSHOT_VERSION};

pub use config::{
    Config, ControlCode, Parity, PixieConfig, RomSlot, SerialConfig, Vid1Action, Vid1Config,
    DEFAULT_CLOCK_HZ,
};
pub use error::{ConfigError, MachineError};
pub use event::{Event, RunResult, StopReason};
pub use keypad::{HexDisplay, HexKeypad};
pub use machine::Machine;
pub use memory::{sha256_hex, LoadedRom};
pub use panel::{FrontPanel, Switch};
pub use pixie::Cdp1861;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_keypad;

#[cfg(test)]
mod tests_pixie;

#[cfg(test)]
mod tests_serial;

#[cfg(test)]
mod tests_vid1;

#[cfg(test)]
mod tests_parallel;

#[cfg(test)]
mod tests_pio;

#[cfg(test)]
mod tests_files;

#[cfg(test)]
mod tests_giant;
