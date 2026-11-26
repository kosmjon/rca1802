// Exercises the parallel keyboard + printer through the WebAssembly Emulator,
// without a browser. Run: node web/test/parallel.mjs  (after building web/pkg)
import { readFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, '..', 'pkg');
const mod = await import(pathToFileURL(join(pkg, 'elf2_wasm.js')).href);
await mod.default({ module_or_path: await readFile(join(pkg, 'elf2_wasm_bg.wasm')) });
const { Emulator } = mod;

let fails = 0;
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? '  ' + detail : ''}`);
  if (!ok) fails++;
};

const cfg = JSON.stringify({
  parallel_keyboard: { ef: 2 },
  printer: { ef: 1, width: 80, page_lines: 66, cr_feeds: true, busy_ms: 5 },
});

// --- Printer: OUT 3 prints "HI", EF1 ready, tear off clears. ---
{
  const e = new Emulator(cfg);
  // M(20)='H', M(21)='I'; LDI 20, PLO R2, SEX R2, OUT 3, OUT 3, IDL
  e.poke(0x20, new Uint8Array([0x48, 0x49]));
  e.poke(0, new Uint8Array([0xF8, 0x20, 0xA2, 0xE2, 0x63, 0x63, 0x00]));
  e.set_switch(0, true);
  e.run(2000);
  check('printer prints HI', e.printer_text().split('\n')[0] === 'HI', JSON.stringify(e.printer_text()));
  check('printer head is fitted', e.printer_head() >= 0);
  e.printer_tear_off();
  check('tear off clears paper', e.printer_text() === '');
}

// --- Keyboard: parallel_key raises EF2; INP 5 reads it and clears. ---
{
  const e = new Emulator(cfg);
  check('EF2 low before key', (e.ef_bits() & 0b0010) === 0);
  e.parallel_key('Q'.charCodeAt(0));
  check('EF2 high after key', (e.ef_bits() & 0b0010) !== 0);
  // LDI 20, PLO R2, SEX R2, INP 5, STR R2, IDL -> stores key at 0020
  e.poke(0, new Uint8Array([0xF8, 0x20, 0xA2, 0xE2, 0x6D, 0x52, 0x00]));
  e.set_switch(0, true);
  e.run(2000);
  check('INP 5 reads the key', e.peek(0x20, 1)[0] === 'Q'.charCodeAt(0));
  check('EF2 clears after read', (e.ef_bits() & 0b0010) === 0);
}

// --- Not fitted by default: no printer, parallel_key is a no-op. ---
{
  const e = new Emulator('{}');
  check('no printer by default', Number(e.printer_head()) === -1 && e.printer_text() === '');
  e.parallel_key(0x41); // must not throw
  check('parallel_key no-op when unfitted', true);
}

// --- PrinterChar event is emitted on OUT 3 (kind 8). ---
{
  const e = new Emulator(cfg);
  e.poke(0x20, new Uint8Array([0x41]));
  e.poke(0, new Uint8Array([0xF8, 0x20, 0xA2, 0xE2, 0x63, 0x00]));
  e.set_switch(0, true);
  e.run(2000);
  const kinds = [...e.events()].map((v) => v >>> 24);
  check('PrinterChar event (kind 8) seen', kinds.includes(8), JSON.stringify(kinds));
}

console.log(fails ? `\n${fails} failure(s)` : '\nall passed');
process.exitCode = fails ? 1 : 0;
