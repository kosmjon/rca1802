//! Giant Board CDP1852 parallel ports and the cassette slot (requirements 9, 10.3).
//!
//! The Giant Board has one CDP1852 configured as an input port and one as an output
//! port, both selected by N = 7 ([verify] port number against the Giant Board manual):
//!
//! * **Input**: a device strobes a byte into the latch, which asserts the chip's
//!   service request (/SR). `INP 7` reads the latch and clears the request.
//! * **Output**: `OUT 7` latches a byte and asserts the output port's service request
//!   ("data available") until the device takes it.
//!
//! The input service request can be jumpered to /INT. In the owner's configuration
//! nothing is connected to these ports.

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cdp1852Ports {
    pub(crate) input: u8,
    pub(crate) input_ready: bool,
    pub(crate) output: u8,
    pub(crate) output_ready: bool,
}

impl Cdp1852Ports {
    /// A device strobes `byte` into the input port.
    pub fn strobe_input(&mut self, byte: u8) {
        self.input = byte;
        self.input_ready = true;
    }

    /// Input service request: a byte is waiting to be read.
    pub fn input_ready(&self) -> bool {
        self.input_ready
    }

    /// CPU `INP 7`.
    pub fn cpu_read(&mut self) -> u8 {
        self.input_ready = false;
        self.input
    }

    /// CPU `OUT 7`.
    pub fn cpu_write(&mut self, byte: u8) {
        self.output = byte;
        self.output_ready = true;
    }

    /// The device takes the output byte, clearing the output service request.
    pub fn take_output(&mut self) -> Option<u8> {
        let r = self.output_ready.then_some(self.output);
        self.output_ready = false;
        r
    }

    /// Current output latch, whether or not it has been taken.
    pub fn output_latch(&self) -> u8 {
        self.output
    }

    /// /CLEAR resets both service requests (latch contents are kept).
    pub fn reset(&mut self) {
        self.input_ready = false;
        self.output_ready = false;
    }
}

/// Cassette interface slot: Q drives the recorder, EF2 reads it back.
///
/// v1 stub: Q edges are counted and EF2 reads idle (not asserted). The later WAV
/// load/save feature fills this in using the format notes in `cassette-loader/`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cassette {
    q_edges: u64,
}

impl Cassette {
    /// Q changed at machine-cycle time `cycle` (future: record to WAV).
    pub fn q_changed(&mut self, _cycle: u64, _q: bool) {
        self.q_edges += 1;
    }

    /// EF2 as the CPU sees it.
    pub fn ef2(&self, _cycle: u64) -> bool {
        false
    }

    /// Number of Q changes seen (for tests and diagnostics).
    pub fn q_edges(&self) -> u64 {
        self.q_edges
    }
}
