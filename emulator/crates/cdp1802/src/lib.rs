//! RCA CDP1802 (COSMAC) CPU core.
//!
//! `no_std`, with no knowledge of any particular machine. A machine connects to the
//! CPU by implementing [`Bus`] and calls [`Cpu::step`] repeatedly.

#![no_std]

mod bus;
mod cpu;
mod disasm;
mod exec;

pub use bus::{Bus, Signals};
pub use cpu::{Cpu, CpuInternals, CpuState};
pub use disasm::{disasm, Instruction, Operand};

/// Crate version, exposed for the front end's "about" box.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
extern crate std;

#[cfg(test)]
pub(crate) mod test_bus;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_isa;

#[cfg(test)]
mod tests_disasm;
