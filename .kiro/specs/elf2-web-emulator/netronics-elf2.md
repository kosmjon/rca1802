The Netronics ELF II used four primary control switches (RUN, LOAD, MP, and IN) along with a 16-key hexadecimal keypad (0–F) to control the RCA 1802 CPU directly in hardware without needing a boot ROM. Unlike the original Popular Electronics COSMAC ELF which used toggle switches for data and control, the ELF II replaced them with push-on/push-off latching switches and a hex keypad. [1] (https://www.emma02.hobby-site.com/elf2_keyboard.html), [2] (http://penguincentral.com/retrocomputing/ElfII/)

Control Switches & Hardware Signals

• RUN Switch (Latching Pushbutton). Controls the 1802's /CLEAR input line to switch between Reset and Execution states. Turning RUN OFF (with LOAD also OFF) resets the CPU, setting registers P, X, and R0 to 0000H and disabling interrupts (IE = 1 on reset). Turning RUN ON releases /CLEAR, causing the 1802 to immediately begin fetching and executing instructions starting at M(0000). [1] (http://www.exemark.com/Microcontrollers/PopularElecwebc.pdf), [2] (http://penguincentral.com/retrocomputing/ElfII/)
• LOAD Switch (Latching Pushbutton). Controls the 1802's /WAIT line alongside /CLEAR to put the CPU into Hardware DMA Load Mode. When RUN is OFF and LOAD is turned ON, the CPU clears R0 to 0000H and pauses execution, waiting for DMA-IN requests from the front panel so you can enter or inspect memory. (Note: If both RUN and LOAD are ON simultaneously, the 1802 enters its hardware PAUSE state, freezing the clock cycle mid-execution). [1] (http://penguincentral.com/retrocomputing/ElfII/), [2] (https://forum.vcfed.org/index.php?threads/cosmac-elf.26750/)
• MP Switch (Memory Protect — Latching Pushbutton). Gates the Memory Write (/MWR) signal going to RAM. When MP is OFF, pressing IN in Load mode writes the hex keypad byte into RAM. When MP is ON, write pulses to RAM are blocked, allowing you to press IN in Load mode to step through and verify memory contents on the two 7-segment LED displays without overwriting data. [1] (http://www.exemark.com/Microcontrollers/PopularElecwebc.pdf), [2] (http://penguincentral.com/retrocomputing/ElfII/), [3] (https://forum.vcfed.org/index.php?threads/cosmac-elf.26750/)
• IN Switch (Input — Momentary Pushbutton). Serves two completely different functions depending on whether the CPU is in Load or Run mode:
	• In Load Mode (RUN OFF, LOAD ON): Triggers a single-byte /DMA-IN cycle via an onboard flip-flop. The last two hex keys pressed on the keypad (latched in a 74C923 encoder and 74C173 flip-flop) are gated onto the data bus, displayed on the two hex LEDs, written to M(R0) (if MP is OFF), and R0 automatically increments by 1.
	• In Run Mode (RUN ON, LOAD OFF): Pulls the 1802's /EF4 external flag line active (LOW) for as long as the button is held down. Software can poll EF4 (using B4 / BN4 branch instructions) to pause a program, step through screens, or read new byte entries from the hex keypad via INP 4. [1] (http://www.cosmacelf.com/publications/books/cosmac-elf-manual.pdf), [2] (https://www.emma02.hobby-site.com/elf2_keyboard.html), [3] (https://forum.vcfed.org/index.php?threads/cosmac-elf.26750/)

Summary of Mode Combinations


RUN	LOAD	1802 Pins (/CLEAR, /WAIT)	CPU State & Front Panel Behavior
OFF	OFF	0, 1	Reset: R0, P, and X reset to 0; data bus floats; displays turn off or hold last latch.
OFF	ON	0, 0	Load Mode: R0 starts at 0000H; pressing IN writes keypad byte (MP OFF) or reads memory (MP ON) and increments R0.
ON	OFF	1, 1	Run Mode: Executes code starting at M(0000); pressing IN asserts EF4 for software input.
ON	ON	1, 0	Pause Mode: Freezes the 1802 at the current machine cycle without resetting registers.

(Note: When a Netronics Giant Board is installed, its monitor ROM is typically mapped at F000H, and a hardware jump/reset circuit on the Giant Board forces the 1802 to start executing from F000H instead of 0000H when RUN is switched ON, unless disabled by jumper. You can see full circuit details on the Emma 02 Elf II Keyboard page and in the COSMAC ELF User's Manual.)
Would you like to see how the Giant Board's monitor ROM alters the RUN startup sequence (the F000H auto-jump circuit), or are you looking for the hex keypad latching schematic (74C923 + 74C173)?
