I had a netronics elf II with giant board + VID-1 and the ASCII keyboard.
Here is how they we're all connected and what I/O lines and interrupts were used.

When you paired the Netronics ELF II + Giant Board with the ASCII Keyboard + VID-1 Video Display Board, the system split into two separate units connected by a serial link. The ASCII keyboard and VID-1 board sat together in the external keyboard chassis to act as a standalone RS-232 / 20mA serial terminal, rather than connecting directly to the 1802's data bus.

1. Physical Architecture (How the Boards Connected)

• Inside the Keyboard Enclosure: The ASCII Keyboard mounted on top, and the VID-1 Video Display Board mounted directly underneath it inside the same blue metal case.
• Keyboard to VID-1 (Parallel): A short internal ribbon cable connected the keyboard's 7-bit parallel ASCII output and strobe line directly into the parallel input header on the VID-1 board.
• VID-1 to Composite Monitor: An RCA jack on the back of the keyboard case sent standard composite video (RS-170) out to a monochrome CRT monitor.
• VID-1 to Giant Board (Serial): A serial cable ran from the back of the keyboard enclosure (usually a DB-25 or Molex connector) to the serial I/O pins on the Giant Board, which sat in Slot 5 of the ELF II backplane.


2. RCA 1802 I/O Lines & Signals Used

Because the VID-1 had its own onboard UART and display controller, the ELF II did not use the CDP1861 Pixie video chip or parallel ports for the terminal. Instead, the Giant Board's ROM monitor and Tiny BASIC communicated with the VID-1 terminal via bit-banged serial on the 1802's dedicated flag and output lines:

1802 CPU Line	Direction	Connection & Function in This Setup
Q	Output	Serial Transmit (TX): Sent asynchronous serial characters from the Giant Board to the VID-1's UART to be drawn on the CRT screen. (Also shared with the Giant Board's cassette save audio circuit).
EF3 or EF4	Input	Serial Receive (RX): Received asynchronous serial keystrokes coming from the VID-1's UART. (Note: On a stock ELF II, EF4 is wired to the onboard IN push-button, so Giant Board jumpers allowed routing serial RX to either EF3 or EF4 depending on the monitor/BASIC build).
EF2	Input	Cassette Tape Load: Used strictly by the Giant Board's tape interface to read audio data from a cassette recorder, leaving it free from terminal duties.
EF1	Input	Unused / Available: Normally used by the CDP1861 Pixie chip for end-of-frame timing; left unused when running a serial terminal.
N0, N1, N2	I/O Select	Unused by Terminal: The Giant Board's parallel CDP1852 ports (INP 7 / OUT 7) were bypassed in this configuration since all keyboard and screen traffic went over serial Q and EF.


3. Interrupts (/INT and /DMA)

• No Hardware Interrupts Used: In the Giant Board + VID-1 configuration, neither /INT nor /DMA-IN / /DMA-OUT was used for the keyboard or display.
• Why Interrupts Weren't Needed: The VID-1 board contained its own screen RAM and video timing circuitry, so it refreshed the CRT independently without stealing CPU cycles via DMA. Meanwhile, the Giant Board's serial routines used software polling loops—watching the EF serial receive pin for a start bit and timing each bit in software—so keyboard input never triggered a hardware /INT on the 1802.
(Note: Hardware /INT on the ELF II was only active if you disabled the VID-1 serial terminal and switched back to the onboard CDP1861 Pixie video chip, or if you explicitly jumpered the Giant Board's parallel CDP1852 Service Request pin to /INT for parallel keyboard input.)
For full schematics and restoration notes on this exact hardware combination, see the COSMAC ELF Groups.io VID-1 discussion and the Netronics ELF II Restoration log.
