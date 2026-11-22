// End-to-end test of the web front end in headless Chrome, using the DevTools protocol
// directly (Node.js 22+, no npm packages).
//
//   python -m http.server 1802 --bind 127.0.0.1 --directory emulator/web
//   node emulator/web/test/e2e.mjs [chrome-path]
//
// Loads the project test programs through the page's own file input, drives the front
// panel and the VID-1 keyboard with synthetic events, and checks the results.

import { spawn } from 'node:child_process';
import { readFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const programs = join(here, '..', '..', 'tests', 'programs');
const URL_ = process.env.ELF2_URL || 'http://127.0.0.1:1802/';
const chromePath = process.argv[2] || 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe';
const PORT = 9333;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
function check(name, ok, detail = '') {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  ' + detail : ''}`);
  if (!ok) failures++;
}

const chrome = spawn(chromePath, [
  '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
  `--user-data-dir=${mkdtempSync(join(tmpdir(), 'elf2-e2e-'))}`,
  `--remote-debugging-port=${PORT}`, 'about:blank',
], { stdio: 'ignore' });

async function cdpTarget() {
  for (let i = 0; i < 50; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = list.find((t) => t.type === 'page');
      if (page) return page.webSocketDebuggerUrl;
    } catch { /* not up yet */ }
    await sleep(200);
  }
  throw new Error('Chrome did not start');
}

const ws = new WebSocket(await cdpTarget());
await new Promise((r) => ws.addEventListener('open', r));
let nextId = 1;
const pending = new Map();
ws.addEventListener('message', (ev) => {
  const m = JSON.parse(ev.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  }
  if (m.method === 'Runtime.exceptionThrown') console.log('page exception:', m.params.exceptionDetails.text);
  if (m.method === 'Runtime.consoleAPICalled') console.log('console:', m.params.args.map((a) => a.value).join(' '));
});
const cmd = (method, params = {}) => new Promise((r) => {
  const id = nextId++;
  pending.set(id, r);
  ws.send(JSON.stringify({ id, method, params }));
});
async function evaluate(expr) {
  const r = await cmd('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true });
  if (r.result.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || 'eval failed');
  return r.result.result.value;
}
async function waitFor(expr, ms = 5000) {
  const end = Date.now() + ms;
  while (Date.now() < end) {
    if (await evaluate(expr)) return true;
    await sleep(50);
  }
  return false;
}

// Load a file through the page's file input, as a user would.
async function loadFile(name, bytes, addr, rom = false) {
  await evaluate(`(() => {
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array(${JSON.stringify(Array.from(bytes))})], ${JSON.stringify(name)}));
    document.getElementById('file').files = dt.files;
    document.getElementById('file-addr').value = ${JSON.stringify(addr)};
    document.getElementById('file-rom').checked = ${rom};
    document.getElementById('file-load').click();
  })()`);
  return waitFor(`document.getElementById('file-msg').textContent.includes(${JSON.stringify(name.replace(/\.[^.]*$/, ''))})`);
}
const click = (id) => evaluate(`document.getElementById('${id}').click()`);
const pointer = (id, type) => evaluate(`document.getElementById('${id}').dispatchEvent(new PointerEvent('${type}', {bubbles: true}))`);
const keypad = (n) => evaluate(`[...document.querySelectorAll('#keypad button')].find(b => b.textContent === '${n.toString(16).toUpperCase()}').click()`);
const display = () => evaluate(`elf2.panel.shown`);
const screenText = () => evaluate(`(() => { const t = elf2.term; let s = ''; for (let r = 0; r < 16; r++) s += String.fromCharCode(...t.screen.subarray(r*t.cols, (r+1)*t.cols)).trimEnd() + '\\n'; return s; })()`);

// Load the page. `fresh` first clears the site's saved ROMs, state and configuration.
async function reload(fresh = true, query = '') {
  if (fresh) {
    await cmd('Page.navigate', { url: 'about:blank' });
    await sleep(200);
    await cmd('Storage.clearDataForOrigin', { origin: new URL(URL_).origin, storageTypes: 'indexeddb,local_storage' });
  }
  await cmd('Page.navigate', { url: URL_ + query });
  await sleep(300);
  return waitFor(`document.getElementById('status').textContent.startsWith('running')`, 10000);
}
const text = (id) => evaluate(`document.getElementById('${id}').textContent`);

try {
  await cmd('Runtime.enable');
  await cmd('Page.enable');

  // 1. Page loads, worker and wasm start.
  check('page loads and the emulator starts', await reload(), await evaluate(`document.getElementById('status').textContent`));
  check('CPU starts in reset', await evaluate(`document.getElementById('cpu-state').textContent.startsWith('reset')`));

  // 2. Front panel: LOAD, key in 7B 00 (SEQ, IDL) with keypad + IN, then RUN.
  await click('sw-load');
  for (const b of [0x7B, 0x00]) {
    await keypad(b >> 4);
    await keypad(b & 0xF);
    await pointer('btn-in', 'pointerdown');
    await sleep(60);
    await pointer('btn-in', 'pointerup');
    await sleep(60);
    check(`display shows ${b.toString(16).toUpperCase().padStart(2, '0')} after IN`, await waitFor(`elf2.panel.shown === ${b}`), `shown=${await display()}`);
  }
  await click('sw-load');
  await click('sw-run');
  check('program keyed in runs: Q LED on', await waitFor(`document.getElementById('led-q').classList.contains('on')`));
  check('CPU reaches IDL', await waitFor(`document.getElementById('cpu-state').textContent.startsWith('idle')`));

  // 3. Keyboard shortcuts on the front panel: R toggles RUN off (reset clears Q).
  await evaluate(`(() => { const p = document.getElementById('front-panel'); p.focus(); p.dispatchEvent(new KeyboardEvent('keydown', {key: 'r', bubbles: true})); })()`);
  check('R key resets: Q LED off', await waitFor(`!document.getElementById('led-q').classList.contains('on')`));

  // 4. Serial echo program and the VID-1 keyboard.
  await reload();
  check('serial_echo.bin loads through the file input', await loadFile('serial_echo.bin', readFileSync(join(programs, 'serial_echo.bin')), '0000'));
  await click('sw-run');
  for (const k of ['E', 'l', 'f', '!', 'Enter']) {
    await evaluate(`(() => { const c = document.getElementById('vid1'); c.focus(); c.dispatchEvent(new KeyboardEvent('keydown', {key: ${JSON.stringify(k)}, bubbles: true, cancelable: true})); })()`);
    await sleep(120);
  }
  check('typed text is echoed onto the VID-1 screen', await waitFor(`elf2.term.screen && String.fromCharCode(...elf2.term.screen.subarray(0, 4)) === 'Elf!'`, 4000),
    JSON.stringify((await screenText()).split('\n')[0]));
  check('Enter (CR) returns the cursor to column 1', await waitFor(`(elf2.term.cursor & 0xFF) === 0`));
  await click('term-clear');
  check('Clear screen empties the VID-1', await waitFor(`elf2.term.screen.every(b => b === 32)`));

  // 5. Pixie: 64x128 program plus a picture at 0100.
  await reload();
  const pic = Uint8Array.from({ length: 1024 }, (_, i) => (i * 37) & 0xFF);
  check('pixie64x128.bin loads', await loadFile('pixie64x128.bin', readFileSync(join(programs, 'pixie64x128.bin')), '0000'));
  check('picture loads at 0100', await loadFile('picture.bin', pic, '0100'));
  await click('sw-run');
  check('Pixie display turns on', await waitFor(`document.getElementById('pixie-state').textContent === 'on'`));
  check('display tabs switch to Pixie when it turns on', await waitFor(`!document.getElementById('pane-pixie').hidden && document.getElementById('pane-vid1').hidden`));
  await click('tab-vid1');
  check('VID-1 tab can be selected again', await waitFor(`!document.getElementById('pane-vid1').hidden && document.getElementById('pane-pixie').hidden`));
  await click('tab-pixie');
  check('Pixie canvas shows the picture', await waitFor(`elf2.pixie.frame.every((b, i) => b === ((i * 37) & 0xFF))`, 4000));
  if (process.env.ELF2_SCREENSHOT) {
    await cmd('Emulation.setDeviceMetricsOverride', { width: Number(process.env.ELF2_SCREENSHOT_WIDTH || 1280), height: 1000, deviceScaleFactor: 1, mobile: false });
    await sleep(300);
    const shot = await cmd('Page.captureScreenshot', { format: 'png' });
    (await import('node:fs')).writeFileSync(process.env.ELF2_SCREENSHOT, Buffer.from(shot.result.data, 'base64'));
    console.log('screenshot:', process.env.ELF2_SCREENSHOT);
  }

  // 6. ROM loading and the LBR F000 helper.
  await reload();
  check('ROM loads at F000', await loadFile('tiny.bin', [0x7B, 0x00], 'F000', true));
  await click('boot-f000');
  check('LBR F000 is keyed in', await waitFor(`document.getElementById('file-msg').textContent.includes('C0 F0 00')`));
  await click('sw-run');
  check('ROM at F000 runs (Q on)', await waitFor(`document.getElementById('led-q').classList.contains('on')`));
  check('CPU idles at F002', await waitFor(`document.getElementById('cpu-state').textContent.includes('F002')`));

  // 7. Errors are shown, not thrown.
  await loadFile('big.bin', new Uint8Array(16), '5FF8');
  check('loading past RAM reports an error', await waitFor(`document.getElementById('file-msg').textContent.includes('no memory')`),
    await evaluate(`document.getElementById('file-msg').textContent`));

  // 9. Persistence: ROM kept in the browser, machine state resumed after a reload.
  await reload();
  check('a ROM loaded "as ROM" is listed', await loadFile('tiny.bin', [0x7B, 0x00], 'F000', true)
    && await waitFor(`document.querySelectorAll('#rom-table tbody tr').length === 1`));
  await loadFile('serial_echo.bin', readFileSync(join(programs, 'serial_echo.bin')), '0000');
  await click('sw-run');
  for (const k of ['H', 'i']) {
    await evaluate(`(() => { const c = document.getElementById('vid1'); c.focus(); c.dispatchEvent(new KeyboardEvent('keydown', {key: '${k}', bubbles: true, cancelable: true})); })()`);
    await sleep(150);
  }
  await waitFor(`String.fromCharCode(...elf2.term.screen.subarray(0, 2)) === 'Hi'`, 3000);
  await evaluate(`elf2.send({type: 'save-now'})`);
  await sleep(500);
  check('reload resumes the saved machine', await reload(false) && (await text('restore-note')).includes('Resumed'), await text('restore-note'));
  check('VID-1 screen restored', await waitFor(`String.fromCharCode(...elf2.term.screen.subarray(0, 2)) === 'Hi'`), (await screenText()).split('\n')[0]);
  check('RUN switch shown on after restore', await waitFor(`document.getElementById('sw-run').classList.contains('on')`));
  check('ROM still in memory after restore', (await text('rom-list')).includes('tiny F000-F001'), await text('rom-list'));
  check('echo program still running after restore', await (async () => {
    await evaluate(`(() => { const c = document.getElementById('vid1'); c.focus(); c.dispatchEvent(new KeyboardEvent('keydown', {key: '!', bubbles: true, cancelable: true})); })()`);
    return waitFor(`String.fromCharCode(...elf2.term.screen.subarray(0, 3)) === 'Hi!'`, 3000);
  })());

  // 10. Power cycle: fresh machine, stored ROM loaded again.
  await click('power-cycle');
  check('power cycle resets the machine', await waitFor(`document.getElementById('cpu-state').textContent.startsWith('reset') && elf2.term.screen.every(b => b === 32)`));
  check('power cycle keeps the stored ROM', await waitFor(`document.getElementById('rom-list').textContent.includes('tiny F000')`));
  check('RUN switch shown off after power cycle', !(await evaluate(`document.getElementById('sw-run').classList.contains('on')`)));

  // 11. Disabling a ROM leaves it out at the next power cycle.
  await evaluate(`document.querySelector('#rom-table tbody input[type=checkbox]').click()`);
  await sleep(200);
  await click('power-cycle');
  check('disabled ROM is not loaded', await waitFor(`document.getElementById('rom-list').textContent === 'ROMs: none loaded'`), await text('rom-list'));
  check('disabled ROM is still stored', await waitFor(`document.querySelector('#rom-table tbody td:nth-child(2)').textContent.includes('not in memory')`));

  // 12. Configuration: 4K RAM, applied with a power cycle, persists, resets to defaults.
  await evaluate(`document.getElementById('config-text').value = '{"ram_size": 4096}'; document.getElementById('config-apply').click()`);
  await sleep(300);
  await loadFile('x.bin', new Uint8Array(4), '1000');
  check('configured RAM size is used', (await text('file-msg')).includes('RAM is 0000-0FFF'), await text('file-msg'));
  check('configuration is saved', (await evaluate(`localStorage.getItem('elf2.config')`)) === '{"ram_size": 4096}');
  await evaluate(`document.getElementById('config-text').value = '{"ram_size": 12}'; document.getElementById('config-apply').click()`);
  check('invalid configuration is rejected with a message', await waitFor(`document.getElementById('file-msg').textContent.includes('Configuration not applied')`), await text('file-msg'));
  await click('config-default');
  await sleep(300);
  check('reset to defaults', (await evaluate(`localStorage.getItem('elf2.config')`)) === null);

  // 13. Save a memory range as Intel HEX.
  await reload();
  await loadFile('three.bin', [0x02, 0x33, 0x7A], '0100');
  await evaluate(`(() => { window.__saved = null; const orig = URL.createObjectURL; URL.createObjectURL = (b) => { b.text().then(t => window.__saved = t); return orig(b); }; })()`);
  await evaluate(`document.getElementById('save-start').value = '0100'; document.getElementById('save-end').value = '0102'; document.getElementById('save-format').value = 'hex'; document.getElementById('save-range').click()`);
  check('memory range saved as Intel HEX', await waitFor(`window.__saved === ':0301000002337A4D\\n:00000001FF\\n'`), JSON.stringify(await evaluate('window.__saved')));

  // 13b. Drag and drop a file onto the page.
  await evaluate(`(() => {
    document.getElementById('file-addr').value = '0200';
    document.getElementById('file-rom').checked = false;
    const dt = new DataTransfer();
    dt.items.add(new File([new Uint8Array([0xAA, 0xBB])], 'dropped.bin'));
    document.querySelector('main').dispatchEvent(new DragEvent('drop', {dataTransfer: dt, bubbles: true, cancelable: true}));
  })()`);
  check('dropped file is loaded', await waitFor(`document.getElementById('file-msg').textContent === 'dropped: 2 bytes at 0200'`), await text('file-msg'));

  // 14. ?load= preloads a project program and ?run=1 starts it.
  check('?load=pixie64x128.hex&run=1 starts the program', await reload(true, '?load=pixie64x128.hex&run=1')
    && await waitFor(`document.getElementById('pixie-state').textContent === 'on'`, 3000));
  check('?load= rejects paths', await reload(true, '?load=../index.html')
    && await waitFor(`document.getElementById('file-msg').textContent.includes('only plain file names')`));

  // 15. Debugger.
  await reload();
  await evaluate(`document.getElementById('debugger').open = true`);
  // LDI 12, PHI RF, LDI 34, PLO RF, SEQ, BR 0007
  const prog = [0xF8, 0x12, 0xBF, 0xF8, 0x34, 0xAF, 0x7B, 0x30, 0x07];
  await loadFile('dbg.bin', prog, '0000');
  await click('dbg-stop');
  await click('sw-run');
  check('Stop holds the CPU', await waitFor(`document.getElementById('dbg-status').textContent === 'stopped'`), await text('dbg-status'));
  await click('dbg-step');
  check('Step executes one instruction (LDI 12)', await waitFor(`document.getElementById('dbg-flags').textContent.includes('D 12')`), await text('dbg-flags'));
  check('P and X are marked on R0', await waitFor(`document.querySelector('#dbg-regs .reg').textContent.includes('P X')`), await evaluate(`document.querySelector('#dbg-regs .reg').textContent`));
  for (let i = 0; i < 3; i++) await click('dbg-step');
  check('three more steps set RF = 1234', await waitFor(`document.querySelectorAll('#dbg-regs .reg')[15].textContent.includes('1234')`),
    await evaluate(`document.querySelectorAll('#dbg-regs .reg')[15].textContent`));
  check('the PC line is highlighted', await waitFor(`document.querySelector('#dbg-dis .dis-line.pc .text')?.textContent === 'SEQ'`),
    await evaluate(`document.querySelector('#dbg-dis .dis-line.pc')?.textContent`));
  await evaluate(`[...document.querySelectorAll('#dbg-dis .dis-line')].find(l => l.dataset.addr === '7').click()`);
  check('clicking a line sets a breakpoint', await waitFor(`document.getElementById('dbg-bps').textContent.includes('0007')`));
  await click('dbg-continue');
  check('Continue runs to the breakpoint', await waitFor(`document.getElementById('dbg-status').textContent === 'stopped at breakpoint 0007'`), await text('dbg-status'));
  check('instructions before it ran (Q on)', await waitFor(`document.getElementById('led-q').classList.contains('on')`));
  // Edits go through prompt(); answer it from the test.
  await evaluate(`window.prompt = () => '0006'; document.querySelectorAll('#dbg-regs .reg')[0].click()`);
  check('editing R0 moves the PC', await waitFor(`document.querySelector('#dbg-dis .dis-line.pc .text')?.textContent === 'SEQ'`));
  await evaluate(`window.prompt = () => '7F'; [...document.querySelectorAll('#dbg-flags .flag')].find(f => f.textContent.startsWith('D ')).click()`);
  check('editing D', await waitFor(`document.getElementById('dbg-flags').textContent.includes('D 7F')`), await text('dbg-flags'));
  await evaluate(`window.prompt = () => 'AA BB'; [...document.querySelectorAll('#dbg-mem .byte')].find(b => b.dataset.addr === '16').click()`);
  check('editing memory writes the bytes', await waitFor(`[...document.querySelectorAll('#dbg-mem .byte')].slice(16, 18).map(b => b.textContent).join(' ') === 'AA BB'`));
  await evaluate(`document.getElementById('dbg-mem-addr').value = '0100'; document.getElementById('dbg-mem-go').click()`);
  check('memory view moves to 0100', await waitFor(`document.querySelector('#dbg-mem .mem-line .addr').textContent === '0100'`));
  await evaluate(`document.getElementById('dbg-follow').checked = false; document.getElementById('dbg-dis-addr').value = '0003'; document.getElementById('dbg-dis-go').click()`);
  check('disassembly from an address', await waitFor(`document.querySelector('#dbg-dis .dis-line .addr').textContent === '0003'`));

  // Scrolling: the ▲/▼ buttons, mouse wheel and Page Up/Down on both panels.
  await click('dbg-mem-down');
  check('memory ▼ button scrolls one row (16 bytes)', await waitFor(`document.querySelector('#dbg-mem .mem-line .addr').textContent === '0110'`),
    await evaluate(`document.querySelector('#dbg-mem .mem-line .addr').textContent`));
  await click('dbg-mem-up');
  check('memory ▲ button scrolls back one row', await waitFor(`document.querySelector('#dbg-mem .mem-line .addr').textContent === '0100'`));
  await evaluate(`document.querySelector('#dbg-mem-down').dispatchEvent(new PointerEvent('click', {shiftKey: true, bubbles: true}))`);
  check('Shift+▼ scrolls memory by one byte', await waitFor(`document.querySelector('#dbg-mem .mem-line .addr').textContent === '0101'`),
    await evaluate(`document.querySelector('#dbg-mem .mem-line .addr').textContent`));
  await evaluate(`document.getElementById('dbg-mem').dispatchEvent(new WheelEvent('wheel', {deltaY: 120, bubbles: true, cancelable: true}))`);
  check('mouse wheel scrolls memory forward', await waitFor(`document.querySelector('#dbg-mem .mem-line .addr').textContent === '0111'`),
    await evaluate(`document.querySelector('#dbg-mem .mem-line .addr').textContent`));
  await evaluate(`(() => { const m = document.getElementById('dbg-mem'); m.focus(); m.dispatchEvent(new KeyboardEvent('keydown', {key: 'PageDown', bubbles: true, cancelable: true})); })()`);
  check('Page Down scrolls memory by a full page', await waitFor(`document.querySelector('#dbg-mem .mem-line .addr').textContent === '0211'`),
    await evaluate(`document.querySelector('#dbg-mem .mem-line .addr').textContent`));

  await click('dbg-dis-down');
  check('disassembly ▼ button scrolls forward and turns off follow PC',
    await waitFor(`document.querySelector('#dbg-dis .dis-line .addr').textContent !== '0003' && !document.getElementById('dbg-follow').checked`),
    await evaluate(`document.querySelector('#dbg-dis .dis-line .addr').textContent`));
  await evaluate(`document.getElementById('dbg-dis-addr').value = '0000'; document.getElementById('dbg-dis-go').click()`);
  await waitFor(`document.querySelector('#dbg-dis .dis-line .addr').textContent === '0000'`);
  await click('dbg-dis-down');
  check('disassembly ▼ advances by the real length of the top instruction (LDI, 2 bytes)',
    await waitFor(`document.querySelector('#dbg-dis .dis-line .addr').textContent === '0002'`),
    await evaluate(`document.querySelector('#dbg-dis .dis-line .addr').textContent`));
  await evaluate(`document.getElementById('dbg-dis').dispatchEvent(new WheelEvent('wheel', {deltaY: -120, bubbles: true, cancelable: true}))`);
  check('mouse wheel scrolls disassembly backward', await waitFor(`document.querySelector('#dbg-dis .dis-line .addr').textContent !== '0002'`),
    await evaluate(`document.querySelector('#dbg-dis .dis-line .addr').textContent`));
  await evaluate(`document.querySelector('#dbg-bps .bp-chip').click()`);
  check('breakpoint removed', await waitFor(`document.getElementById('dbg-bps').textContent === 'none'`));

  // Serial monitor with the echo program.
  await click('power-cycle');
  await sleep(300);
  await loadFile('serial_echo.bin', readFileSync(join(programs, 'serial_echo.bin')), '0000');
  await click('sw-run');
  await click('dbg-continue');
  await evaluate(`(() => { const c = document.getElementById('vid1'); c.focus(); c.dispatchEvent(new KeyboardEvent('keydown', {key: 'Z', bubbles: true, cancelable: true})); })()`);
  check('serial monitor shows the key sent and the echo', await waitFor(
    `document.getElementById('dbg-serial').textContent.includes('VID-1→Elf  5A  Z') && document.getElementById('dbg-serial').textContent.includes('Elf→VID-1  5A  Z')`, 3000),
  await text('dbg-serial'));
  if (process.env.ELF2_SCREENSHOT_DEBUG) {
    await evaluate(`document.getElementById('dbg-dis-addr').value = '0000'; document.getElementById('dbg-dis-go').click(); document.getElementById('dbg-mem-addr').value = '0000'; document.getElementById('dbg-mem-go').click()`);
    await click('dbg-stop');
    await sleep(300);
    const y = await evaluate(`document.getElementById('debugger').getBoundingClientRect().top + window.scrollY`);
    await cmd('Emulation.setDeviceMetricsOverride', { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
    await evaluate(`window.scrollTo(0, ${Math.max(0, y - 10)})`);
    await sleep(300);
    const shot = await cmd('Page.captureScreenshot', { format: 'png' });
    (await import('node:fs')).writeFileSync(process.env.ELF2_SCREENSHOT_DEBUG, Buffer.from(shot.result.data, 'base64'));
    await click('dbg-continue');
  }

  // 8. Real-time pacing: speed close to 100%.
  await sleep(2500);
  const speed = await evaluate(`document.getElementById('speed').textContent`);
  const pct = parseInt(speed, 10);
  check('runs at real time (95-105%)', pct >= 95 && pct <= 105, speed);
} catch (e) {
  console.log('ERROR', e.message);
  failures++;
} finally {
  ws.close();
  chrome.kill();
}
console.log(failures ? `${failures} check(s) failed` : 'all checks passed');
process.exit(failures ? 1 : 0);
