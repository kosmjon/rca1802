# Requirements: Netronics Elf II browser emulator

## Introduction

A zero-install emulator for the Netronics Elf II (RCA CDP1802), hosted as a static
site on GitHub Pages. It emulates the base Elf II board (hex keypad, two-digit hex
display, Q LED, EF1-EF4, CDP1861 "Pixie" video) and the owner's expansions:

- 24K RAM by default.
- Giant Board in slot 5: monitor ROM, bit-banged serial, cassette interface, and
  CDP1852 parallel ports.
- A serial terminal made of the Netronics ASCII keyboard and the VID-1 video board,
  housed together in the keyboard case. The keyboard feeds the VID-1 over a short
  parallel ribbon; the VID-1 has its own UART, screen RAM and composite video, and
  talks to the Giant Board over a serial cable. It does not use the CPU bus, DMA or
  interrupts.

References in this folder: `pixie1861.md` (CDP1861), `video-keyboard.md`
(Giant Board + VID-1 + ASCII keyboard wiring), `vid1.md` (VID-1 terminal) and
`netronics-elf2.md` (front-panel switches and keypad).

Per `notes.md`, the CPU core is a Rust crate with trait-based bus and I/O, compiled
to WebAssembly. The same core also runs headless (native tests, CLI).

Hardware details marked **[verify]** must be checked against the owner's Netronics
manuals and schematics before implementation.

### Decisions

- Implementation: Rust core compiled to WebAssembly, browser front end.
- Terminal: VID-1 + ASCII keyboard as a serial terminal on the Giant Board's
  bit-banged port (TX = Q, RX = EF3 or EF4 by jumper).
- Pixie (CDP1861) video: emulated as well, for software that uses the on-board video.
- Cassette WAV load/save: out of scope for v1 (planned later).
- ROMs and programs: the owner may use them all but the site will not ship them.
  Users load ROMs from their own files; the site keeps them in browser storage.

## Glossary

- **Core**: the Rust CDP1802 CPU crate, with no browser dependencies.
- **Machine**: the Elf II system model: memory map, I/O ports, peripherals.
- **Front end**: the browser UI (HTML/JS/Canvas) that drives the Wasm build.
- **Pixie**: CDP1861 video display controller, fed by 1802 DMA-OUT.
- **VID-1**: Netronics video display board acting as a serial terminal screen.
- **Machine cycle**: 8 clock periods; most instructions take 2, long branches 3.

## Signal map (default configuration)

| 1802 line | Used by | Notes |
|---|---|---|
| Q | Q LED, serial TX to VID-1, cassette out | Q LED always shows Q. |
| EF1 | CDP1861 DISP STATUS | Unused when Pixie is off. |
| EF2 | Cassette in | Cassette is a later release; reads idle until then. |
| EF3 | Serial RX from VID-1 (default) | Giant Board jumper selects EF3 or EF4. |
| EF4 | IN push-button | Shared with serial RX if the jumper selects EF4. |
| /INT, DMA-OUT | CDP1861 | Not used by the terminal. |
| DMA-IN | Front-panel LOAD mode | **[verify]** |
| INP/OUT 1 | CDP1861 on/off | **[verify]** direction. |
| INP/OUT 4 | Hex keypad / two-digit display | **[verify]** |
| INP/OUT 7 | Giant Board CDP1852 parallel ports | Not used by the terminal. |

## Requirements

### 1. Zero-install hosting

**User story:** As a user, I want to open a URL and run the emulator without
installing anything.

1. The site SHALL be fully static (HTML, JS, CSS, Wasm) and deployable to GitHub Pages.
2. The site SHALL run in current Chrome, Edge, Firefox and Safari on desktop.
3. The site SHALL NOT require a server-side component, sign-in or network access after load.
4. A GitHub Actions workflow SHALL build the Wasm package and publish the site on push to the main branch.
5. Total initial download SHOULD be under 2 MB, excluding user-loaded files.

### 2. CDP1802 CPU core

**User story:** As a developer, I want an accurate, reusable 1802 core.

1. The core SHALL implement all 1802 instructions (including IDL, the 68 opcode as 1802, not 1804/1805 extensions).
2. The core SHALL model registers R0-RF, D, DF, P, X, T, IE, Q, and N/I.
3. The core SHALL count machine cycles per instruction (2 or 3) so timing-dependent software (Pixie, cassette, bit-banged serial) behaves correctly.
4. The core SHALL handle DMA-IN, DMA-OUT and interrupt requests at the end of instruction execution, with priority DMA-IN > DMA-OUT > INT, and IE cleared on interrupt entry.
5. The core SHALL expose its I/O through traits (memory, OUT/INP ports 1-7, EF1-EF4, Q, control signals) as sketched in `notes.md`.
6. The core SHALL be `no_std`-compatible and contain no browser-specific code.
7. The core SHALL pass an instruction-level test suite covering every opcode, including DF behaviour for arithmetic and shift instructions.

### 3. Elf II machine model

**User story:** As an Elf II owner, I want the emulator to behave like my machine.

1. The default configuration SHALL provide 24K RAM at 0000-5FFF.
2. RAM size SHALL be configurable (256 bytes up to 64K), and unmapped reads SHALL return a configurable value (default FF) **[verify]**.
3. The clock SHALL default to 1.7898 MHz (3.579545 MHz crystal / 2) and be adjustable, including a "max speed" mode. Serial and video timing SHALL follow the emulated clock.
4. The machine SHALL support optional ROM images (for example the Giant Board monitor at F000-F0FF) loaded by the user.
5. The front panel SHALL provide latching RUN, LOAD and MP (memory protect) switches and a momentary IN button, with the behaviour in `netronics-elf2.md`:

   | RUN | LOAD | /CLEAR, /WAIT | State |
   |---|---|---|---|
   | off | off | 0, 1 | Reset: R0 = 0, P = 0, X = 0, Q = 0, IE = 1. |
   | off | on | 0, 0 | Load: CPU idle, R0 = 0000. Each IN press performs one DMA-IN cycle. |
   | on | off | 1, 1 | Run: execution starts at M(0000). |
   | on | on | 1, 0 | Pause: CPU frozen, registers kept. |

6. In Load mode, IN SHALL put the keypad byte on the bus, show it on the hex display, write it to M(R0) unless MP is on, and increment R0. With MP on, writes SHALL be blocked so memory can be stepped through without change, and the display SHALL show the memory byte **[verify]**.
7. MP SHALL block all memory writes (CPU and DMA) while it is on.
8. The Giant Board has no start-up jump: RUN always starts at 0000, and the owner keys in `C0 F0 00` (LBR F000) at 0000 to start the monitor ROM at F000 (confirmed by the owner). The emulator SHALL offer a one-click convenience that writes these three bytes to 0000 (and the CLI option `--boot-f000`); the configuration option `start_at_f000` stays available but off by default.
9. Peripherals (Pixie, Giant Board serial, Giant Board parallel ports, cassette) SHALL be individually enabled in the machine configuration, and their EF/port assignments SHALL be configurable, with the signal map above as the default.

### 4. Hex keypad and two-digit display

1. The front end SHALL show a 16-key hex keypad, the IN button and the RUN, LOAD and MP switches, operable by mouse, touch and the PC keyboard (0-9, A-F, Enter for IN, configurable keys for the switches).
2. The keypad SHALL latch the last two hex keys pressed as one byte (high digit first), as the 74C923 encoder and 74C173 latches do.
3. INP 4 SHALL read the latched keypad byte.
4. In Run mode, IN SHALL assert EF4 for as long as it is held.
5. OUT 4 SHALL latch a byte shown on the two-digit seven-segment hex display. In Load mode the display SHALL show the byte transferred by each DMA-IN.

### 5. Q LED and EF1-EF4

1. The front end SHALL show a Q LED that reflects the Q output.
2. Each EF line SHALL be driven by the peripheral assigned to it in the signal map. When two sources share a line (EF4: IN button and serial RX), the line SHALL be the logical OR of their asserted states.
3. A debug panel SHALL show all four EF lines and allow forcing each one.
4. When Q toggles at audio rates, the front end SHOULD offer optional Q-to-speaker audio through Web Audio.

### 6. CDP1861 Pixie video

Reference: `pixie1861.md` in this spec folder.

1. The 1861 SHALL run from the CPU clock, locked to CPU machine cycles: 14 machine cycles per scanline and 262 scanlines per frame (NTSC, about 60 Hz).
2. Two scanlines before active video, the 1861 SHALL assert /INT and hold it for 28 machine cycles (a level, not an edge), so an ISR that re-enables interrupts too early is interrupted again, as on real hardware.
3. The 1861 SHALL align its line timing to the CPU's interrupt-acknowledge cycle so the first DMA burst starts 29 machine cycles after it (a 28-cycle interrupt prologue is followed directly by the first burst). During each of the 128 active scanlines, the 1861 SHALL assert DMA-OUT for 8 consecutive machine cycles. Each cycle reads M(R0), increments R0, and supplies 8 pixels. The remaining 6 machine cycles per line are available to the CPU.
4. Resolution SHALL come from the software, not a mode setting: the emulator SHALL render whatever bytes the DMA cycles fetch, so 64x128, 64x64 and 64x32 (ISR rewinds R0 to repeat lines) all work.
5. The 1861 SHALL drive EF1 (DISP STATUS) during the four scanlines before the active display and the last four active scanlines (the CDP1861 datasheet behaviour as recalled; `pixie1861.md` mentions only the second window). The exact lines SHALL be confirmed against the datasheet **[verify]**.
6. The display SHALL be switched on and off by INP 1 and OUT 1. `pixie1861.md` says OUT 1 enables and INP 1 disables; RCA COSMAC VIP software uses INP 1 (69) to turn it on. The direction SHALL be confirmed against the CDP1861 datasheet or the Elf II manual **[verify]** and kept configurable.
7. The emulator SHALL reproduce timing artefacts of real hardware (for example a 3-cycle instruction straddling a DMA burst shifts the line), rather than hiding them.
8. The front end SHALL render the display to a Canvas at 60 Hz with integer scaling and an optional phosphor/scanline style.
9. With the display off, the CPU SHALL run without 1861 cycle-stealing or interrupts.

### 7. Giant Board bit-banged serial port

**User story:** As a user, I want the Giant Board monitor and Tiny BASIC to talk to the terminal exactly as they do on my machine.

1. Serial transmit SHALL be taken from **Q**. Serial receive SHALL drive **EF3** by default, or **EF4**, selected by a configuration option matching the Giant Board jumper.
2. Line polarity (idle level of Q and of the RX EF line, and the meaning of the EF branch sense) SHALL follow the Giant Board schematic **[verify]** and be configurable (normal / inverted).
3. The port SHALL use no interrupts or DMA; the monitor polls and times bits in software.
4. The serial line SHALL be timed in machine cycles. Supported settings: 110, 300, 600, 1200, 2400 baud at least; 7 or 8 data bits; parity none/even/odd; 1 or 2 stop bits. Defaults SHALL match the Giant Board monitor and Tiny BASIC as configured on the owner's machine **[verify]**.
5. If the monitor measures the baud rate from a first received character (auto-baud), the terminal SHALL send that character on connect or when the user presses a "sync" button **[verify]**.
6. Transmit decoding SHALL tolerate the bit-time jitter of software timing loops (sample at bit centre, re-synchronise on each start bit).

### 8. VID-1 terminal with ASCII keyboard

**User story:** As a user, I want a terminal window that looks and behaves like the VID-1 screen, and to type on my PC keyboard as if it were the Netronics ASCII keyboard.

Reference: `vid1.md` in this spec folder.

1. The terminal SHALL emulate the VID-1 as the far end of the serial link: characters received from TX (Q) are drawn on its screen; keys typed are sent on RX (EF3/EF4).
2. The terminal SHALL be full duplex with no local echo: a typed key appears on screen only if the Elf II echoes it back.
3. The screen SHALL be 64 columns x 16 lines (default) or 32 x 16, selectable as by the VID-1 jumper, backed by a 1 KB screen buffer.
4. The character set SHALL cover 128 ASCII codes: 96 printable characters including lower case, drawn with a VID-1-style character generator font (a substitute font until the original ROM font is available **[verify]**).
5. The terminal SHALL act on these control codes:

   | Code | Name | Action |
   |---|---|---|
   | 08 | BS | Cursor left one position. |
   | 0A | LF | Cursor down one line; scroll up on line 16. |
   | 0C | FF | Clear screen, cursor home. |
   | 0D | CR | Cursor to column 1. |
   | 7F | DEL | Erase character at cursor **[verify]** exact behaviour. |

6. The VID-1 also supports cursor up, down, right and home, erase to end of line and erase screen. Their codes SHALL be taken from the VID-1 manual **[verify]** and are configurable until then. Unknown control codes SHALL be ignored, not displayed.
7. The terminal's serial settings (baud rate, typically 110 or 300 with the Giant Board; parity) SHALL be selectable like the VID-1 jumpers and SHALL match the Giant Board settings in requirement 7.
8. The screen SHALL be drawn on a Canvas with a monochrome monitor style and a visible cursor; size and colour SHALL be adjustable.
9. PC key presses SHALL map to the Netronics ASCII keyboard's codes: 7-bit ASCII, Ctrl combinations, an upper-case-only option, and keys for any special codes the keyboard sends (for example BREAK, REPEAT) **[verify]**.
10. The terminal SHALL support copy, paste (pasted text sent at a throttled rate the monitor can keep up with, with a configurable delay after CR) and clear.
11. Keyboard input SHALL go to the hex keypad or the terminal depending on which panel has focus, never both.

### 9. Giant Board parallel ports (CDP1852)

1. The machine SHOULD model the Giant Board's CDP1852 input and output ports on INP 7 / OUT 7 **[verify]**, including the service-request output, which MAY be jumpered to /INT.
2. In the default configuration these ports SHALL be present but unconnected.

### 10. Program loading and saving

1. Users SHALL be able to load a binary (`.bin`) or Intel HEX (`.hex`) file at a chosen address, by file picker or drag and drop.
2. Users SHALL be able to save a memory range as `.bin` or Intel HEX.
3. (Later release, not v1.) The emulator will load an Elf II cassette WAV by feeding it to the cassette input (EF2), and save cassette output (Q) as a WAV, using the tape format confirmed in `cassette-loader/README.md`. The v1 design SHALL leave a cassette peripheral slot so this can be added without core changes.
4. Machine state (RAM, registers, configuration) SHALL be persisted in browser storage and restorable on reload.
5. A URL parameter SHALL be able to preload a program shipped with the site.

### 11. Debugging

1. The front end SHALL provide run, stop, single-step and reset.
2. The front end SHALL show registers (marking which register is P and which is X), D, DF, Q, IE, EF lines and the current instruction disassembled.
3. The front end SHALL provide a memory viewer/editor and PC breakpoints.
4. The front end SHOULD show a serial line monitor (raw TX/RX characters with timing errors flagged) to help diagnose baud and polarity settings.

### 12. Performance and accuracy

1. At the default clock, emulation SHALL keep real time on a mid-range laptop, with no more than one dropped video frame per second.
2. Timing SHALL be driven by machine-cycle counts, not wall-clock instruction rates, so a 1-second delay loop takes 1 emulated second.
3. The CPU and peripherals SHALL run off the main UI thread, or in time slices that keep the UI responsive.

### 13. Software images (ROMs and programs)

1. The repository and site SHALL NOT include ROM images or copyrighted programs.
2. The site SHALL let users load ROM images (for example the Giant Board monitor at F000, Netronics BASIC) from local files, store them in browser storage (IndexedDB), and reload them automatically on later visits.
3. Saved machine configurations SHALL refer to ROMs by name and checksum, and SHALL report a clear message when a required ROM is missing.
4. Test programs needed for the automated test suite SHALL be written for the project (or be public domain) and stored with their source.

## Remaining inputs needed

Confirmed by the owner: serial TX = Q; serial RX = EF3 or EF4 by Giant Board jumper;
EF2 = cassette only; keyboard + VID-1 form a serial terminal (no bus, DMA or interrupts);
1861 behaviour per `pixie1861.md`.

Still to verify against the manuals (place scans or text in `docs/hardware/`):

1. Elf II: what the hex display shows in Load mode with MP on.
2. CDP1861: which of INP 1 / OUT 1 turns the display on (conflicting sources), exact EF1 lines.
3. Giant Board: serial idle polarity and EF sense, baud rate(s), auto-baud, CDP1852 port numbers. Note: the owner's Giant Board monitor ROM (F000-F0FF, run under `elf2-cli`) is a hex-keypad monitor with cassette read (EF2) and write (Q) routines and no serial terminal code, so the serial settings must come from the software that uses the VID-1 (for example Tiny BASIC).
4. VID-1: codes for cursor up/down/right/home and erase EOL/EOS, DEL behaviour, original character font.
5. ASCII keyboard: upper case only or not, special keys.

Note: `video-keyboard.md`, `vid1.md` and `netronics-elf2.md` are summaries, not
manual scans. Items taken from them are used as defaults and remain configurable.
