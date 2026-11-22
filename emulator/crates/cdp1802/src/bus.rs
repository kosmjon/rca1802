//! The interface between the CPU and the machine it is plugged into.

/// Request lines sampled by the CPU between instructions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Signals {
    /// /DMA-IN asserted: the CPU will store a byte from the peripheral at M(R0).
    pub dma_in: bool,
    /// /DMA-OUT asserted: the CPU will send M(R0) to the peripheral.
    pub dma_out: bool,
    /// /INT asserted. Taken only when IE = 1.
    pub interrupt: bool,
}

/// Everything outside the CPU: memory, I/O ports, flags, Q and request lines.
///
/// The machine implements this trait. All methods are called by the CPU; the CPU
/// never calls them in any other order than the hardware would.
pub trait Bus {
    /// Memory read.
    fn read(&mut self, addr: u16) -> u8;
    /// Memory write. Memory protect, ROM and unmapped addresses are the machine's job.
    fn write(&mut self, addr: u16, val: u8);
    /// `OUT n` (opcodes 61-67), `port` = 1..=7. `val` is M(R(X)), which the CPU puts on the bus.
    fn out(&mut self, port: u8, val: u8);
    /// `INP n` (opcodes 69-6F), `port` = 1..=7. The CPU stores the result in M(R(X)) and D.
    fn inp(&mut self, port: u8) -> u8;
    /// External flag `line` = 1..=4. `true` means asserted, i.e. `B1`..`B4` would branch.
    fn ef(&self, line: u8) -> bool;
    /// Q output changed (called on every SEQ/REQ and on reset).
    fn set_q(&mut self, q: bool);
    /// DMA-IN cycle: the peripheral supplies the byte.
    fn dma_in(&mut self) -> u8;
    /// DMA-OUT cycle: the peripheral receives the byte at M(R0).
    fn dma_out(&mut self, val: u8);
    /// Current request lines.
    fn signals(&self) -> Signals;
    /// `cycles` machine cycles (8 clocks each) have elapsed.
    fn tick(&mut self, cycles: u32);
    /// The CPU is executing an interrupt-acknowledge (S3) cycle. On the chip this is
    /// visible on the state-code lines SC1/SC0 = 11; the CDP1861 uses it to align its
    /// DMA timing to the interrupt routine. Called before `tick` for that cycle.
    fn interrupt_ack(&mut self) {}
}
