The Netronics VID-1 (Video Display Board) was a standalone serial CRT terminal controller designed to mount inside the Netronics ASCII Keyboard enclosure. It converted parallel ASCII keystrokes into serial data and rendered incoming serial characters onto a composite video monitor. [1] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [2] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)

Display & Video Output

• Text Format. Selectable 64 characters × 16 lines or 32 characters × 16 lines display layout.
• Character Set. Displays the full 128-character ASCII set (uppercase and lowercase) with 96 printable characters.
• Video Memory. Includes 1 KB (1024 bytes) of onboard screen RAM.
• Video Signal. Outputs standard 75-ohm RS-170 composite video for direct connection to a monochrome CRT monitor.
• Screen Controls. Supports full hardware cursor control, automatic scrolling, and screen clear functions. [1] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [2] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)

Serial & Keyboard Interfaces

• Serial Standards. Supports both RS-232C voltage levels and 20 mA current loop (TTY) interfaces via onboard jumper settings. [1] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [2] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)
• Baud Rate & Parity. Features jumper-selectable baud rates (commonly 110 to 300 baud for the ELF II Giant Board, up to higher rates for the Explorer/85) along with selectable parity. [1] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [2] (https://www.cosmacvip.com/ELFII/ElfII.html)
• Keyboard Input. Connects via an internal parallel ribbon cable to accept 7-bit parallel ASCII and strobe signals directly from the Netronics ASCII Keyboard. [1] (https://forum.vcfed.org/index.php?threads/netronics-ascii-keyboard-and-video-board-questions.78378/), [2] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)
• Duplex Operation. Operates strictly in full-duplex mode by default. Keystrokes from the parallel keyboard go straight out the serial transmit (TX) line and only appear on screen when echoed back by the host computer on the serial receive (RX) line. [1] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)

Hardware & Power Architecture

• Bus Independence. Requires no CPU bus or I/O port mapping on the host computer because it communicates exclusively over asynchronous serial lines.
• Voltage Regulation. Includes onboard voltage regulators, though it requires a multi-rail AC or DC input supply (including a negative voltage rail) to power its UART, character generator ROM, and RS-232 line drivers.
• Documentation. Original schematic scans and community restoration notes are preserved in the COSMAC ELF Groups.io archives and Rich's Classic Computing Pages. [1] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [2] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)
Would you like the pinout and jumper table for configuring RS-232 vs. 20 mA current loop and baud rates on the VID-1, or are you looking for the control character codes (cursor movement, clear screen) that the VID-1 responds to?

The Netronics VID-1 used single-byte ASCII control characters (rather than multi-byte ANSI/VT100 ESC [ sequences) to move the cursor and clear the screen. Because it was a pre-ANSI microprocessor-controlled terminal board, the host computer triggered display actions simply by sending standard ASCII control codes over the serial line. [1] (https://vt100.net/docs/vt510-rm/chapter11.html), [2] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [3] (http://www.cosmacelf.com/publications/newsletters/ipso-facto/ipso-facto-38.pdf), [4] (https://www.atarimagazines.com/compute/issue11/98_1_CAPUTE_CORRECTIONS_CLARIFICATIONS.php)

VID-1 ASCII Control Codes

Function	Hex	Decimal	ASCII Name	Keyboard Key	Action on Screen
Cursor Left	08H	8	BS (Backspace)	Ctrl-H / Backspace	Moves the cursor one space to the left.
Line Feed	0AH	10	LF	Ctrl-J / Line Feed	Moves the cursor down one line (scrolls up if on line 16).
Clear Screen	0CH	12	FF (Form Feed)	Ctrl-L / Clear	Erases the entire display and returns the cursor to home.
Carriage Return	0DH	13	CR	Ctrl-M / Return	Moves the cursor to column 1 of the current line.
Delete / Rubout	7FH	127	DEL	DEL / Rubout	Erases the character at the cursor or steps back.

Additional Cursor & Erase Functions

• Supported Hardware Commands. Netronics' official specifications list hardware support for Erase, End of Line, Erase of Screen, Form Feed, and Delete, along with directional cursor movement (Up, Down, Left, Right, and Home). [1] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf), [2] (https://www.atarimagazines.com/compute/issue11/98_1_CAPUTE_CORRECTIONS_CLARIFICATIONS.php)
• Firmware Variations. The VID-1 used an onboard microcontroller (such as an 8048/8748 or 3870 variant) to decode incoming serial bytes. While 0CH (FF / Ctrl-L) is documented in COSMAC ELF newsletters as the universal clear-screen command, the exact single-byte control codes for directional arrows and end-of-line erase varied slightly across early Netronics firmware revisions (such as early TTY-style control characters vs. later Smartvid-80 ESC sequences). [1] (http://www.cosmacelf.com/publications/newsletters/ipso-facto/ipso-facto-38.pdf), [2] (http://cini.classiccmp.org/pdf/pe/1980/PE1980-Dec-pg112.pdf)
• Full-Duplex Echo Note. Because the VID-1 operated in full-duplex mode by default, pressing a control key on the Netronics ASCII Keyboard sent that ASCII control byte out the serial TX line to the ELF II. The VID-1 only executed the cursor movement or screen clear when the ELF II echoed that control byte back to the VID-1's serial RX line.
(For restoration discussions and schematic lookups covering the VID-1's onboard controller ROM, check the COSMAC ELF Groups.io VID-1 thread and the Vintage Computer Federation Netronics terminal forum.) [1] (https://forum.vcfed.org/index.php?threads/netronics-ascii-keyboard-and-video-board-questions.78378/), [2] (https://groups.io/g/cosmacelf/topic/netronics_full_basic_level/112220212)
Are you writing 1802 assembly/BASIC output routines to drive a physical VID-1, or configuring terminal emulation mappings for an emulator? If you share what software you're running on the ELF II, I can help verify how it handles screen clearing and line endings.

