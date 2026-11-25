// Exercises the WebAssembly assembler bindings (assemble_files /
// assemble_files_from) the "Assemble and save" tab uses, without a browser.
// Run: node web/test/assemble.mjs   (after building web/pkg)
import { readFile, readdir } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join, relative } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const pkg = join(here, '..', 'pkg');

const mod = await import(pathToFileURL(join(pkg, 'elf2_wasm.js')).href);
// The web build is a web-target module: initialise it with the .wasm bytes.
const wasmBytes = await readFile(join(pkg, 'elf2_wasm_bg.wasm'));
await mod.default({ module_or_path: wasmBytes });
const { assemble_files, assemble_files_from } = mod;

let failures = 0;
function check(name, cond, detail = '') {
  if (cond) { console.log(`ok   ${name}`); }
  else { console.log(`FAIL ${name} ${detail}`); failures++; }
}

// 1) Multi-file with an INCLUDE; main file auto-detected.
{
  const files = [
    { name: 'main.asm', text: '        INCLUDE "bits.inc"\n        ORG 0\n        LDI THREE\n' },
    { name: 'bits.inc', text: 'THREE   EQU 3\n' },
  ];
  const r = JSON.parse(assemble_files(JSON.stringify(files)));
  check('auto main = main.asm', r.main === 'main.asm', r.main);
  check('assembled ok', r.ok === true, JSON.stringify(r.errors));
  check('hex has LDI 3 (F803)', r.hex.includes('F803'), r.hex);
}

// 2) Missing include reported as an error, not a crash.
{
  const files = [{ name: 'a.asm', text: '        INCLUDE "gone.inc"\n' }];
  const r = JSON.parse(assemble_files(JSON.stringify(files)));
  check('missing include -> not ok', r.ok === false);
  check('missing include message', r.errors.some((e) => /not among the uploaded/.test(e.message)));
}

// 3) Ambiguous main asks to choose; choosing resolves it.
{
  const files = [
    { name: 'one.asm', text: '        ORG 0\n        NOP\n' },
    { name: 'two.asm', text: '        ORG 0\n        NOP\n' },
  ];
  const r = JSON.parse(assemble_files(JSON.stringify(files)));
  check('ambiguous -> main null', r.main === null, JSON.stringify(r));
  const r2 = JSON.parse(assemble_files_from(JSON.stringify(files), 'two.asm'));
  check('chosen main ok', r2.ok === true && r2.main === 'two.asm');
}

// 4) A real source error carries file:line.
{
  const files = [{ name: 'bad.asm', text: '        ORG 0\n        LDI\n' }];
  const r = JSON.parse(assemble_files(JSON.stringify(files)));
  check('source error -> not ok', r.ok === false);
  check('error has a line number', r.errors.length > 0 && r.errors[0].line > 0, JSON.stringify(r.errors));
}

// 5) MOL1802: a .m18 main is transpiled, and its .m18 / .asm includes resolve.
{
  const files = [
    { name: 'main.m18', text: '        ORG 0\n        INCLUDE "lib.m18"\n        0x1234:R3\n        SEP R3\n' },
    { name: 'lib.m18', text: 'HELPER: R3.0:D\n        SEP R5\n' },
  ];
  const r = JSON.parse(assemble_files(JSON.stringify(files)));
  check('m18 assembles ok', r.ok === true, JSON.stringify(r.errors) + ' msg=' + r.message);
  check('m18 main reported as .asm', r.main === 'main.asm', r.main);
  check('m18 produced hex', typeof r.hex === 'string' && r.hex.length > 0);
}

// 6) The real sample set: sample.m18 + its INCLUDEs, which may live in
//    different folders under sample-code/. Finds each file by name so it keeps
//    working as the samples are reorganised; skips (not fails) if any are
//    absent. Files are passed with a relative path, exercising cross-folder
//    include resolution.
{
  const root = join(here, '..', '..', '..', 'sample-code');
  const needed = ['sample.m18', 'scrt.m18', 'baud_serial.asm'];
  const found = {};
  async function walk(dir) {
    let entries;
    try { entries = await readdir(dir, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      const full = join(dir, e.name);
      if (e.isDirectory()) await walk(full);
      else if (needed.includes(e.name) && !(e.name in found)) found[e.name] = full;
    }
  }
  await walk(root);
  const missing = needed.filter((n) => !(n in found));
  if (missing.length) {
    console.log(`skip real sample.m18 set (not found under sample-code/: ${missing.join(', ')})`);
  } else {
    const files = await Promise.all(needed.map(async (n) => ({
      name: relative(root, found[n]).replaceAll('\\', '/'),
      text: await readFile(found[n], 'latin1'),
    })));
    const r = JSON.parse(assemble_files(JSON.stringify(files)));
    check('real sample.m18 set assembles', r.ok === true,
      'main=' + r.main + ' msg=' + r.message + ' errors=' + JSON.stringify(r.errors?.slice(0, 5)));
    check('real sample main is sample.asm', /(^|\/)sample\.asm$/.test(r.main || ''), r.main);
  }
}

console.log(failures ? `\n${failures} failure(s)` : '\nall passed');
process.exitCode = failures ? 1 : 0;
