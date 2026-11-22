# cassette-loader

Tools and recordings for recovering RCA 1802 (COSMAC) programs from cassette
tape audio, for loading into an emulator such as Emma 02.

## Layout

| Folder | Contents |
|---|---|
| `tools/` | Current decoders and shared modules (see below). |
| `experiments/` | Earlier one-off scripts from the investigation. Kept for reference; several have hard-coded file names from the original flat layout and will not run as-is. |
| `recordings/tinybasic/` | Netronics Elf II Tiny BASIC tape: takes 1-4 (`TinyBasic*.wav`) plus polarity-inverted copies (`*_inv.wav`). |
| `recordings/quest-edasm/` | Quest EDASM tape (Super Elf format), two copies on one recording. |
| `recordings/emma02-reference/` | `giant.wav`: Emma 02 saving the Giant Board ROM (F000-F0FF). Clean reference signal. |
| `reference/` | Known-good data: `giantboard.hex` (Intel HEX, F000-F0FF), `jktinybasic.bin` (Emma 02 64K memory dump; contains Netronics Level III BASIC, not the tape). |
| `output/` | Best results so far. |
| `work/` | Intermediate files (pickles, arrays, logs, plots). Safe to delete. |

## Tools

Run from any directory; outputs are written to the current directory.
Use the venv: `.venv\Scripts\python tools\<script>.py ...`

| Script | Purpose |
|---|---|
| `clean_decode.py WAV [-inv]` | Zero-crossing decoder for clean (emulator-generated) Netronics Elf II WAVs. Verified 256/256 bytes on `giant.wav` with `-inv`. |
| `decode2.py WAV [nbytes] [known-hex]` | Elf II format, channel-model (MLSE) bit decoder bootstrapped from the leader and known first bytes. |
| `pipe.py WAV` | Elf II format, equalizer + edge/frame-template decoder with decision-directed retraining. |
| `quest.py WAV t_from t_to [inv] [expected-hex]` | Quest Super Elf format, channel-model bit decoder. |
| `framedec_lib.py`, `mlse_core.py`, `wavload.py` | Shared modules (frame templates, channel model, 16/24-bit WAV loading). |

## Tape formats (confirmed)

**Netronics Elf II (Giant Board monitor)**: one bit = one square-wave cycle;
'1' short (~420-450 us), '0' long (~1215-1250 us). Frame = start bit 0, 8 data
bits MSB first, parity bit = 1 when the byte has an even number of 1s. Start-bit
cycle slightly longer. No address/length header. Emma 02 writes the timed half
as the negative half; the real tapes here have it positive (hence `*_inv.wav`).

**Quest Super Elf**: leader of 1s, a single 0, then 9-bit bytes (8 data MSB
first + parity, no start bit). First 4 bytes: start address, byte count (hi/lo).
On `QuestEdasm.wav` the header decodes as address 0000, count 0C26, and the
parity is **odd** (the Questdata article says even).

## Status

- `output/giantboard/giant_decoded.bin`: exact match with `reference/giantboard.hex`.
- `output/tinybasic/TinyBasic_partial.bin`: take 1 decode, 0000-0F77, first 14
  bytes patched from known values. Text and BASIC program read correctly; some
  1802 code regions still contain bit errors. Take 1 was clipped and cut off
  before 0x0FFF.
- Takes 2-4 and QuestEdasm: not decodable yet. The playback chain smears short
  '1' cycles next to long '0' cycles. Likely fix is a different deck or head
  azimuth adjustment.

Known first bytes:
- Tiny BASIC @0000: `C0 80 00 C0 F0 00 D0 00 00 00 00 00 A0 9A`
- Quest EDASM @0000: `F8 0D C8 F8 00 A6 F8 0D A3 F8`

## Next steps

1. Emma 02 Super Elf reference WAV with known bytes, to confirm Quest timing and parity.
2. Re-record tapes with better playback (azimuth, flat tone, no ALC, no clipping).
3. Find a reference Netronics Tiny BASIC image to verify the interpreter region.
