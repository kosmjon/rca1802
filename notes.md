Building an RCA 1802 (COSMAC) emulator in Rust is worthwhile if you want a modular, embeddable, or web-ready core that avoids the legacy baggage of older C/C++ implementations. While classic tools like Emma 02 cover nearly every historical system, a modern Rust approach solves specific architectural and deployment problems that older emulators do not address well.

Key Reasons to Build It in Rust

• Embeddable library design. Most existing 1802 emulators are monolithic desktop applications tightly coupled to specific UI toolkits. Designing the 1802 CPU as a clean, no_std-compatible Rust crate allows you or others to drop the core into custom hardware projects, automated test harnesses, or entirely different host systems.
• First-class WebAssembly (Wasm) compilation. Rust compiles directly to WebAssembly with minimal overhead. You can build a fast, zero-install browser version for sharing custom ROMs, Forth environments, or educational demos without rewriting logic in JavaScript.
• Explicit state and trait-based I/O. The RCA 1802 relies heavily on hardware-level signals like the four external flags (EF1–EF4), the Q output line, N0–N2 I/O lines, and built-in DMA (DMA-IN/DMA-OUT). Rust’s trait system lets you model the bus and peripheral pins cleanly, making it easy to swap out a simple bit-banged serial terminal for a CDP1861 Pixie video chip without touching the CPU execution loop.
• Cycle-accurate timing without global state. Because the 1802 uses a fixed 8-clock machine cycle (mostly two machine cycles per instruction, three for long branches), modeling exact cycle states in Rust using enums and explicit state machines prevents the subtle timing bugs common in older C codebases.
• Zero-dependency cross-platform distribution. Rust’s Cargo toolchain makes it straightforward to compile single-binary releases for Linux, macOS, and Windows, eliminating the dependency headaches often found when compiling older C++ or wxWidgets-based retro tools.

When You Might Skip It

• Broad system coverage is your main goal. If you just want to run existing COSMAC VIP, Netronics ELF II, or COMX-35 software immediately, mature projects like Emma 02 already emulate dozens of obscure hardware variations and peripherals out of the box.
• Rapid, loose prototyping. As noted in discussions on Hackaday, C can be faster for quick, unpolished hacks if you do not want to satisfy the borrow checker while wiring up messy memory-mapped hardware.

High-Value Niches for a New Rust 1802 Project

If you decide to build it, focusing on one of these gaps will give your project clear value in the retrocomputing community:
1. A headless rca1802-core crate. Publish a well-tested, cycle-stepped CPU library on crates.io with clean traits for memory, I/O ports, DMA, and interrupts so other developers can build custom frontends.
2. Modern terminal and TUI debugging. Pair the core with a terminal UI library like ratatui to offer live register inspection (especially tracking which of the sixteen 16-bit registers R0–RF is currently designated as P, X, or the DMA pointer), memory breakpoints, and reverse-stepping.
3. Interactive web playground. Compile the core to Wasm and pair it with a clean web interface for writing 1802 assembly or testing interactive interpreters directly in a browser.
If you're considering starting this project, what target setup or workflow do you have in mind?
• A standalone desktop or TUI app for debugging and running custom ROMs
• A reusable Rust crate for embedding the 1802 into other projects
• A browser-based Wasm tool for easy sharing
I can help sketch out a clean trait architecture for the 1802's bus, DMA, and EF/Q lines to get you started.

sketch out a clean trait architecture for the 1802's bus, DMA, and EF/Q lines to get me started

Separating the RCA 1802's signals into distinct, composable Rust traits lets you model its unique pin-level architecture without coupling the CPU core to any specific machine. Because the 1802 handles I/O through dedicated control pins (N0–N2, EF1–EF4, Q) and built-in R0-driven DMA rather than standard memory-mapped peripherals, a clean trait boundary mirrors the actual hardware datasheet.

Core System Bus Trait

The Bus trait acts as the motherboard. Passing a mutable reference to Bus during each CPU step avoids Rc<RefCell<...>> overhead and satisfies the borrow checker cleanly.
rust
#![no_std]

/// Represents the external hardware environment connected to the CDP1802.
pub trait Bus {
    /// Standard 16-bit memory read (TPA/TPB multiplexed address).
    fn read_mem(&mut self, addr: u16) -> u8;

    /// Standard 16-bit memory write.
    fn write_mem(&mut self, addr: u16, val: u8);

    /// Hardware I/O triggered by `OUT 1` through `OUT 7` (N0-N2 lines = 1..=7).
    /// Note: On real hardware, the CPU places M(R(X)) onto the data bus during an OUT.
    fn io_out(&mut self, port: u8, val: u8);

    /// Hardware I/O triggered by `INP 1` through `INP 7` (N0-N2 lines = 1..=7).
    /// Returns the byte placed on the data bus by the peripheral (also written to M(R(X)) and D).
    fn io_in(&mut self, port: u8) -> u8;

    /// Read the 4 external flag lines (EF1..=EF4).
    /// Returns true if the flag line is asserted (active low on the physical pin,
    /// but represented here as logical true when B1-B4 branch conditions are met).
    fn read_ef(&self, flag: ExternalFlag) -> bool;

    /// Notifies external hardware when the Q flip-flop changes state (SEQ / REQ).
    /// Useful for bit-banged serial TX, audio speakers, or cassette tape output.
    fn write_q(&mut self, state: bool);

    /// Polled at the end of every instruction cycle to check for external control requests.
    fn poll_signals(&mut self) -> ControlSignals;
}
Use code with caution.

Pin & Signal Types

Using strict types instead of raw integers prevents invalid flag lookups and makes DMA/interrupt priority clear.
rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalFlag {
    Ef1 = 1,
    Ef2 = 2,
    Ef3 = 3,
    Ef4 = 4,
}

/// Active hardware control lines sampled by the 1802 at the end of an instruction's execute cycle (S1).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ControlSignals {
    /// Maskable interrupt request (/INT pin asserted). Ignored if IE == 0.
    pub interrupt: bool,
    /// DMA-In request (/DMA-IN pin asserted). Higher priority than DMA-Out and Interrupt.
    pub dma_in: bool,
    /// DMA-Out request (/DMA-OUT pin asserted). Higher priority than Interrupt.
    pub dma_out: bool,
}
Use code with caution.

How the CPU Core Interacts With the Traits

Keeping the CPU struct purely focused on internal state (D, DF, B, T, P, X, N, I, IE, Q, and R[0..16]) makes execution straightforward:
• DMA Handling (S2 State): At the end of an instruction (S1), the CPU checks bus.poll_signals(). If dma_in is true, the CPU enters state S2, calls let byte = bus.io_in(0) (or a dedicated dma_read method if you want to distinguish DMA from INP instructions), writes byte to bus.write_mem(self.r[0], byte), and increments self.r[0]. For dma_out, it reads bus.read_mem(self.r[0]), passes it to the peripheral, and increments self.r[0].
• Interrupt Handling (S3 State): If no DMA lines are active, signals.interrupt is true, and self.ie is true, the CPU saves (X, P) into T, sets X = 2, P = 1, disables IE, and enters state S3.
• Serial & Bit-Banging (EF and Q): Because write_q is triggered only on state changes (or every SEQ/REQ), you can pass a cycle counter or timestamp into Bus if you want to decode bit-banged UART serial streams accurately.

Optional: Composable Peripheral Trait

If you plan to emulate systems with multiple expansion cards (like a CDP1861 Pixie video chip alongside a UART), you can build a multiplexer struct that implements Bus and routes calls to smaller Peripheral devices:
rust
pub trait Peripheral {
    /// Claim and handle an N-line I/O port (1..=7), returning Some(data) if handled.
    fn io_in(&mut self, _port: u8) -> Option<u8> { None }
    fn io_out(&mut self, _port: u8, _val: u8) {}

    /// Optionally drive one of the EF1..=EF4 lines.
    fn read_ef(&self, _flag: ExternalFlag) -> Option<bool> { None }

    /// Assert DMA or Interrupt lines back to the system bus.
    fn poll_signals(&mut self) -> ControlSignals { ControlSignals::default() }
    
    /// Advance peripheral internal clocks by `cycles` (e.g., 8 oscillator ticks per machine cycle).
    fn tick(&mut self, _cycles: u8) {}
}
Use code with caution.

Design Trade-offs to Consider

• Cycle-by-cycle vs. Instruction-by-instruction stepping: Stepping by full instructions (2 or 3 machine cycles) is fast and trivial to implement. However, if you want to emulate the CDP1861 Pixie graphics chip, which relies on exact DMA cycle-stealing timed to horizontal scanlines, adding a bus.tick(machine_cycles) call inside your CPU step loop is essential.
• DMA Data Transfer Method: On real hardware, a DMA-In cycle puts the peripheral's byte onto the data bus without asserting N0–N2. You may want explicit dma_in(&mut self) -> u8 and dma_out(&mut self, val: u8) methods on Bus rather than overloading io_in(0) so peripherals know a DMA acknowledgment (SC1=1, SC0=0) occurred.
Would you like to build out the CPU state machine and instruction decoder next, or focus on modeling a specific peripheral (like a bit-banged serial terminal or the CDP1861 Pixie video chip)?
