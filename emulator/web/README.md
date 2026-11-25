# Elf II web front end

Static site: `index.html`, `css/`, `js/` and the generated `pkg/` (WebAssembly build).

## Run locally

```
cd emulator
wasm-pack build crates/elf2-wasm --target web --release --out-dir ../../web/pkg
python -m http.server 1802 --bind 127.0.0.1 --directory web
```

Open http://127.0.0.1:1802/. A local web server is needed: browsers do not load
module workers or WebAssembly from `file://`.

## Using it

- **Front panel**: click it, then type `0`-`F` for the hex keypad, `Enter` for IN
  (held while the key is down), `R` / `L` / `M` to toggle RUN / LOAD / MP. The
  on-screen buttons do the same.
- **VID-1 terminal**: click the screen and type. Ctrl+letter sends control codes;
  Ctrl+V or the Paste button pastes text (sent at the configured pace). The
  terminal does not echo locally: text appears only if the Elf II sends it back.
- **Load and save**: choose a `.bin` (with a hex address) or `.hex` file, or drop it on
  the page. Tick *as ROM* for a ROM image (for example the Giant Board monitor at
  F000). *Key in LBR F000* writes `C0 F0 00` at 0000 so RUN starts the monitor.
  *Save memory* downloads an address range as Intel HEX or binary.
  *Sample program* loads one of the bundled programs in `programs/` (listed in
  `programs/samples.json`) into RAM; it does not start it, so press RUN after.
- **Assemble and save**: add CDP1802 source. `.asm` is assembled directly;
  `.m18` is MOL1802 and is transpiled to `asm1802` source first (a `.m18`
  `INCLUDE` resolves to the transpiled `.asm`). *Add files…* and *Add folder…*
  accumulate into one list (shown below the buttons, each removable; *Clear*
  empties it), so a program and its `INCLUDE`s can come from different folders.
  *Add folder…* keeps each file's path under the chosen folder, so an
  `INCLUDE "sub/dir/file"` resolves by that path; otherwise `INCLUDE "name"`
  matches by base name. The main file is the one no other file includes; when
  that is ambiguous, a *main file* chooser appears. *Assemble* shows the listing
  (or the errors, as `file:line: message`). On success,
  *Load* puts the result in RAM (then press RUN), and *Download .hex* / *.lst*
  save the output. Assembling never runs the program.
- **ROMs and machine**:
  - ROMs loaded with *as ROM* are saved in this browser (IndexedDB, keyed by
    SHA-256) and loaded at every power-on. Untick *use* to leave one out, or remove it.
  - The machine (memory, registers, switches, screens) is saved every 5 seconds and
    when the page is hidden, and resumed on the next visit.
  - *Power cycle* discards the saved state and starts a fresh machine with the ticked
    ROMs.
  - *Machine configuration (JSON)* sets RAM size, clock, serial and VID-1 options; it
    is kept in `localStorage` and applied with a power cycle.
- **URL parameters**: `?load=FILE[@ADDR]` loads `programs/FILE` (the project test
  programs) after start-up, `&run=1` then sets RUN, `?fresh=1` ignores the saved state.
  Example: `?load=pixie64x128.hex&run=1`.
- **Max speed** runs the emulator as fast as the browser allows.
- **Debugger** (click to open):
  - *Stop*, *Continue* (F5), *Step* (F10 or F8: one instruction, stepping through any
    DMA or interrupt cycles), *Reset CPU* (RUN off and on; memory kept).
  - Registers R0-RF with P, X and the DMA pointer (R0) marked; D, DF, P, X, T, IE, Q,
    EF lines and the cycle count. Click a register or flag to edit it.
  - Disassembly that follows the PC (or starts at an address you give). Click a line
    to set or clear a breakpoint; breakpoints are listed and survive power cycles
    (not page reloads). Scroll with the mouse wheel, the ▲/▼ buttons, or Page Up/Down
    (click the panel first) to reveal code at other addresses; this turns off
    "follow PC", the same as typing an address in *Go*.
  - Memory view of 256 bytes from any address; click a byte to edit it (several
    bytes separated by spaces are written in sequence). The PC byte is highlighted
    and M(R(X)) underlined. Scroll with the mouse wheel (one row of 16 bytes per
    notch, or one byte with Shift held), the ▲/▼ buttons (Shift-click for one byte),
    or Page Up/Down.
  - Serial line monitor: every character the VID-1 sends (blue) and receives from
    the Elf II (green), with times, and TX framing errors (red), which usually mean
    the baud rate or polarity setting is wrong.

## Files

| File | Purpose |
|---|---|
| `js/main.js` | Page entry point; starts the worker, routes updates to the panels. |
| `js/worker.js` | Web Worker that owns the emulator and runs it in time slices. |
| `js/panel.js` | Front panel, seven-segment display, LEDs. |
| `js/pixie.js` | CDP1861 screen (64 x 128 bitmap). |
| `js/terminal.js` | VID-1 screen and keyboard. |
| `js/storage.js` | IndexedDB helpers (ROMs, saved state) and SHA-256. |
| `js/debugger.js` | Debugger panel. |
| `programs/` | Programs served to the page: the `?load=` test programs (copied by `tests/programs/build.py`), plus the *Sample program* list in `samples.json` and its `.hex` files. |
| `test/e2e.mjs` | End-to-end test in headless Chrome (see below). |
| `test/assemble.mjs` | Node test of the WebAssembly assembler bindings (no browser). |

## End-to-end test

With the local server running:

```
node web/test/e2e.mjs ["path\to\chrome.exe"]
```

It loads the project test programs through the page's file input, drives the front
panel and terminal with synthetic events, and checks the display, LEDs, Pixie
frame, VID-1 screen and real-time speed. Set `ELF2_SCREENSHOT=file.png` to save a
screenshot.

`node web/test/assemble.mjs` checks the "Assemble and save" tab's WebAssembly
bindings (main-file auto-detection, `INCLUDE` resolution across uploaded files,
`.m18` MOL1802 transpilation including the `sample-code/m18` sample set, and
error reporting) directly, without a browser or the local server. Build
`web/pkg` first.

`node web/test/giant.mjs` runs the owner's Giant Board monitor ROM
(`cassette-loader/reference/giantboard.hex`, not part of the emulator) with real
mouse and key events: keys `04` + IN and `05` + Enter and checks the display echoes
each byte. `HOLD=0` makes the IN click instantaneous.

Note on IN: the worker keeps IN down for at least 30 ms of emulated time, like a real
push button, so a quick tap or an Enter press and release that arrive together are
not missed by software polling EF4.
