// Reproduce: Giant Board monitor ROM, key in LBR F000, RUN, enter 04 + IN, expect the
// display to show 04. Uses real (trusted) mouse and key events through CDP.
//   node web/test/giant.mjs
import { spawn } from 'node:child_process';
import { readFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const romText = readFileSync(join(here, '..', '..', '..', 'cassette-loader', 'reference', 'giantboard.hex'));
const PORT = 9334;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const chrome = spawn('C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', [
  '--headless=new', '--disable-gpu', '--no-first-run',
  `--user-data-dir=${mkdtempSync(join(tmpdir(), 'elf2-g-'))}`, `--remote-debugging-port=${PORT}`, 'about:blank',
], { stdio: 'ignore' });
let url;
for (let i = 0; i < 50 && !url; i++) {
  try { url = (await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json()).find((t) => t.type === 'page')?.webSocketDebuggerUrl; } catch { }
  await sleep(200);
}
const ws = new WebSocket(url);
await new Promise((r) => ws.addEventListener('open', r));
let id = 1; const pend = new Map();
ws.addEventListener('message', (ev) => { const m = JSON.parse(ev.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } });
const cmd = (method, params = {}) => new Promise((r) => { const i = id++; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (e) => (await cmd('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true })).result.result?.value;
async function center(sel) {
  return ev(`(() => { const el = document.querySelector(${JSON.stringify(sel)}); el.scrollIntoView({block: 'center'}); const r = el.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; })()`);
}
async function mouseClick(sel, hold = 80) {
  const [x, y] = await center(sel);
  await cmd('Input.dispatchMouseEvent', { type: 'mousePressed', x, y, button: 'left', clickCount: 1 });
  await sleep(hold);
  await cmd('Input.dispatchMouseEvent', { type: 'mouseReleased', x, y, button: 'left', clickCount: 1 });
  await sleep(50);
}
async function key(k, code, keyCode) {
  await cmd('Input.dispatchKeyEvent', { type: 'keyDown', key: k, code, windowsVirtualKeyCode: keyCode, text: k.length === 1 ? k : undefined });
  await sleep(80);
  await cmd('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: keyCode });
  await sleep(50);
}
const state = () => ev(`[document.getElementById('cpu-state').textContent, elf2.panel.shown]`);

await cmd('Page.navigate', { url: 'http://127.0.0.1:1802/' });
for (let i = 0; i < 100 && !(await ev(`document.getElementById('status')?.textContent.startsWith('running')`)); i++) await sleep(100);

// Load the ROM as a user would.
await ev(`(() => { const dt = new DataTransfer(); dt.items.add(new File([new Uint8Array(${JSON.stringify(Array.from(romText))})], 'giantboard.hex')); document.getElementById('file').files = dt.files; document.getElementById('file-rom').checked = ${process.env.ASROM !== '0'}; })()`);
await mouseClick('#file-load');
await sleep(200);
await sleep(500); console.log('load msg:', await ev(`document.getElementById('file-msg').textContent`), '| status:', await ev(`document.getElementById('status').textContent`), '| rom checked:', await ev(`document.getElementById('file-rom').checked`), '| files:', await ev(`document.getElementById('file').files.length`));
await mouseClick('#boot-f000');
await mouseClick('#sw-run');
await sleep(500);
console.log('after RUN:', await state(), '| warning:', await ev(`document.getElementById('cpu-warning').hidden ? '(none)' : document.getElementById('cpu-warning').textContent`), '|', await ev(`document.getElementById('rom-list').textContent`));

// A. mouse: keypad 0, 4, then IN button
await mouseClick('#keypad button:nth-child(13)'); // 0
await mouseClick('#keypad button:nth-child(9)');  // 4
await mouseClick('#btn-in', Number(process.env.HOLD || 150));
await sleep(300);
console.log('A (mouse) after 04 + IN:', await state());

// B. keyboard: 0, 5, Enter (focus is in the front panel after the clicks)
await key('0', 'Digit0', 48);
await key('5', 'Digit5', 53);
await cmd('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 }); await cmd('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
await sleep(300);
console.log('B (keyboard) after 05 + Enter:', await state(), 'focus:', await ev('document.activeElement.id || document.activeElement.tagName'));

ws.close(); chrome.kill(); process.exit(0);
