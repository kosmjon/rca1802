//! Events reported by `Machine::run` to the front end.

/// Something the front end should show or react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// A complete CDP1861 frame is available (task 7).
    PixieFrame,
    /// A character was decoded from the serial TX line and applied to the VID-1 (task 8).
    TermChar(u8),
    /// The serial TX line produced a framing error (task 8).
    TermFramingError,
    /// A byte was printed to the parallel dot-matrix printer via `OUT 3`.
    PrinterChar(u8),
    /// The two-digit hex display changed.
    HexDisplay(u8),
    /// Q changed during the run; only the final level is reported.
    Q(bool),
    /// Execution stopped before the instruction at this address.
    Breakpoint(u16),
    /// The CPU executed IDL.
    Halted,
}

/// Why `Machine::run` returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The cycle budget was used up.
    Budget,
    /// A breakpoint was reached.
    Breakpoint(u16),
}

/// Result of `Machine::run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunResult {
    /// Machine cycles actually run (may exceed the budget by up to 2 cycles,
    /// since an instruction is never split).
    pub cycles: u64,
    pub stop: StopReason,
}
