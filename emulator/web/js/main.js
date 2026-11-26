// Page entry point: starts the emulator worker and wires up the panels.
//
// URL parameters:
//   ?load=FILE[@ADDR]   load programs/FILE (project test programs) after start-up;
//                       .bin files need @ADDR, .hex files carry their own addresses
//   &run=1              then set RUN
//   ?fresh=1            ignore the saved machine state (ROMs are still loaded)

import { FrontPanel } from './panel.js';
import { PixieScreen } from './pixie.js';
import { Vid1Terminal } from './terminal.js';
import { hex4 } from './storage.js';
import { Debugger } from './debugger.js';

// Cache-busting build version, taken from this script's own ?v=... (stamped
// into index.html at build time). The worker is loaded from a versioned URL so
// a new build fetches a fresh worker (and, through it, fresh wasm) instead of a
// cached one. main.js and its static imports are busted by index.html's ?v=.
const V = new URL(import.meta.url).search; // e.g. "?v=20261125…" or ""

const CONFIG_KEY = 'elf2.config';
const params = new URLSearchParams(location.search);

const status = document.getElementById('status');
const worker = new Worker(new URL('./worker.js' + V, import.meta.url), { type: 'module' });
const send = (msg, transfer) => worker.postMessage(msg, transfer || []);

const panel = new FrontPanel(send);
const pixie = new PixieScreen(document.getElementById('pixie'));
const term = new Vid1Terminal(document.getElementById('vid1'), send);
const debug = new Debugger(send);

const $ = (id) => document.getElementById(id);
const speedLabel = $('speed');
const fileMsg = $('file-msg');
const warn = $('cpu-warning');
const romsLabel = $('rom-list');
let textRequests = 0;
const textWaiters = new Map();

function setStatus(text, error = false) {
  status.textContent = text;
  status.classList.toggle('error', error);
}

function message(text, error = false) {
  fileMsg.textContent = text;
  fileMsg.classList.toggle('error', error);
  if (error) fileMsg.scrollIntoView({ block: 'nearest' });
}

// ------------------------------------------------------------------ worker messages
let latest = null;
let readyText = 'running';
worker.onmessage = (ev) => {
  const m = ev.data;
  switch (m.type) {
    case 'ready':
      readyText = `running · core ${m.version}`;
      setStatus(readyText);
      $('restore-note').textContent = m.restored
        ? 'Resumed the machine saved on your last visit.'
        : 'Fresh machine.';
      afterReady();
      break;
    case 'update': latest = m; break;
    case 'loaded': message(m.msg); break;
    case 'error': message(m.msg, true); break;
    case 'roms': showRomTable(m.roms, m.missing); break;
    case 'config-ok':
      if (m.config.trim() === '' || m.config.trim() === '{}') localStorage.removeItem(CONFIG_KEY);
      else localStorage.setItem(CONFIG_KEY, m.config);
      break;
    case 'download': download(m.name, m.data, m.mime); break;
    case 'text':
    case 'reply': {
      const done = textWaiters.get(m.id);
      textWaiters.delete(m.id);
      if (done) done(m);
      break;
    }
    default: break;
  }
};
worker.onerror = (e) => setStatus(`worker failed: ${e.message}`, true);

let romsShown = null;
function showRoms(json) {
  if (json === romsShown) return;
  romsShown = json;
  const roms = JSON.parse(json);
  romsLabel.textContent = roms.length
    ? 'ROMs: ' + roms.map((r) => `${r.name} ${hex4(r.base)}-${hex4(r.base + r.len - 1)}`).join(', ')
    : 'ROMs: none loaded';
}

// Draw the most recent update once per animation frame.
function frame() {
  if (latest) {
    const u = latest;
    latest = null;
    panel.update(u);
    pixie.update(u);
    followPixie(u.pixieOn);
    updatePrinter(u);
    term.update(u);
    debug.update(u);
    speedLabel.textContent = u.rate ? `${(u.rate * 100).toFixed(0)}% speed` : '';
    showRoms(u.roms);
    warn.hidden = !u.pcUnmapped;
    if (u.pcUnmapped) {
      warn.textContent = `The CPU is running at ${hex4(u.state.pc)}, where there is no RAM or ROM (every byte reads FF). `
        + (JSON.parse(u.roms).length ? 'Check the ROM address.' : 'No ROM is loaded: choose the ROM file, tick "as ROM", and click Load.');
    }
    if (u.stoppedAt >= 0) setStatus(`breakpoint at ${hex4(u.stoppedAt)}`);
    else if (u.paused) setStatus('stopped (debugger)');
    else if (/^(breakpoint|stopped)/.test(status.textContent)) setStatus(readyText);
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// ------------------------------------------------------------------ tabs
// A tab group: buttons #tab-NAME switch panes #pane-NAME; the choice is remembered.
function tabGroup(key, names, onShow) {
  const show = (name) => {
    for (const t of names) {
      const on = t === name;
      $(`tab-${t}`).classList.toggle('active', on);
      $(`tab-${t}`).setAttribute('aria-selected', String(on));
      $(`pane-${t}`).hidden = !on;
    }
    localStorage.setItem(key, name);
    if (onShow) onShow(name);
  };
  for (const t of names) $(`tab-${t}`).addEventListener('click', () => show(t));
  const saved = localStorage.getItem(key);
  show(names.includes(saved) ? saved : names[0]);
  return show;
}
const showTab = tabGroup('elf2.tab', ['vid1', 'pixie', 'printer'], (name) => {
  if (name === 'vid1') term.draw();
  else if (name === 'pixie') pixie.draw();
});
tabGroup('elf2.tab.files', ['load', 'asm', 'roms']);
loadSampleList();

// Bring the Pixie tab forward when a program turns the display on.
let pixieWasOn = null;
function followPixie(on) {
  if (pixieWasOn === false && on && $('pixie-auto').checked) showTab('pixie');
  pixieWasOn = on;
}

// Printer tab: shown only when a printer is fitted; its paper mirrors the
// emulated printout, and it comes forward the first time a program prints.
const printerPaper = $('printer-paper');
let printerShown = false;
let printerPrinted = false;
function updatePrinter(u) {
  if (u.printerFitted && !printerShown) {
    $('tab-printer').hidden = false;
    printerShown = true;
  }
  if (u.printerText !== null && u.printerText !== undefined) {
    printerPaper.textContent = u.printerText;
    // Keep the latest line in view, like paper feeding out.
    printerPaper.parentElement.scrollTop = printerPaper.parentElement.scrollHeight;
    if (!printerPrinted) {
      printerPrinted = true;
      if ($('printer-auto').checked) showTab('printer');
    }
  }
}
$('printer-tear').addEventListener('click', () => {
  send({ type: 'printer-tear-off' });
  printerPaper.textContent = '';
});

// ------------------------------------------------------------------ ROM manager
function showRomTable(roms, missing) {
  const body = document.querySelector('#rom-table tbody');
  body.textContent = '';
  for (const r of roms) {
    const tr = document.createElement('tr');
    const use = document.createElement('input');
    use.type = 'checkbox';
    use.checked = r.enabled;
    use.title = 'Load this ROM at power-on';
    use.addEventListener('change', () => {
      send({ type: 'rom-enable', sha256: r.sha256, enabled: use.checked });
      message(`${r.name} will be ${use.checked ? 'loaded' : 'left out'} at the next power cycle.`);
    });
    const del = document.createElement('button');
    del.textContent = 'Remove';
    del.addEventListener('click', () => {
      if (confirm(`Remove ROM ${r.name} from this browser?`)) {
        send({ type: 'rom-remove', sha256: r.sha256 });
        message(`${r.name} removed; it stays in memory until the next power cycle.`);
      }
    });
    const cells = [use, r.name + (r.loaded ? '' : ' (not in memory)'), `${hex4(r.base)}-${hex4(r.base + r.size - 1)}`, `${r.size}`, r.sha256.slice(0, 16) + '…', del];
    for (const c of cells) {
      const td = document.createElement('td');
      if (typeof c === 'string') td.textContent = c;
      else td.appendChild(c);
      tr.appendChild(td);
    }
    tr.children[2].className = 'mono';
    tr.children[4].className = 'mono';
    tr.children[4].title = r.sha256;
    body.appendChild(tr);
  }
  $('rom-table').hidden = roms.length === 0;
  $('rom-empty').hidden = roms.length !== 0;
  const banner = $('missing-roms');
  banner.hidden = !missing.length;
  banner.textContent = missing.length
    ? 'The configuration needs ROMs that are not loaded: '
      + missing.map((r) => `${r.name} at ${hex4(r.base)} (SHA-256 ${r.sha256})`).join('; ')
      + '. Load each one with "as ROM" ticked.'
    : '';
}

$('power-cycle').addEventListener('click', () => {
  panel.resetSwitches();
  send({ type: 'power-cycle' });
});

// ------------------------------------------------------------------ configuration
const configText = $('config-text');
configText.value = localStorage.getItem(CONFIG_KEY) || '';
$('config-apply').addEventListener('click', () => {
  panel.resetSwitches();
  send({ type: 'power-cycle', config: configText.value.trim() || '{}' });
});
$('config-default').addEventListener('click', () => {
  configText.value = '';
  panel.resetSwitches();
  send({ type: 'power-cycle', config: '{}' });
});

// ------------------------------------------------------------------ options, terminal
$('max-speed').addEventListener('change', (e) => send({ type: 'speed', max: e.target.checked }));
$('term-clear').addEventListener('click', () => send({ type: 'term-clear' }));
$('term-paste').addEventListener('click', async () => {
  try {
    term.paste(await navigator.clipboard.readText());
  } catch {
    message('Clipboard not available: click the screen and press Ctrl+V instead.');
  }
});
// Send a request to the worker and resolve with its reply message.
function request(type, extra) {
  const id = ++textRequests;
  return new Promise((resolve) => {
    textWaiters.set(id, resolve);
    send({ type, id, ...extra });
  });
}

$('term-copy').addEventListener('click', () => {
  request('get-text').then((m) => navigator.clipboard.writeText(m.text).then(
    () => message('Screen copied.'),
    () => message('Clipboard not available.'),
  ));
});

// ------------------------------------------------------------------ files
// Intel HEX -> (base, contiguous bytes), for ROM images supplied as .hex.
function hexToBin(bytes) {
  const out = [];
  let start = null;
  for (const line of new TextDecoder().decode(bytes).split(/\r?\n/)) {
    const m = /^:([0-9A-Fa-f]{2})([0-9A-Fa-f]{4})00([0-9A-Fa-f]*)..$/.exec(line.trim());
    if (!m) continue;
    const addr = parseInt(m[2], 16);
    if (start === null) start = addr;
    const off = addr - start;
    for (let k = 0; k < parseInt(m[1], 16); k++) out[off + k] = parseInt(m[3].substr(2 * k, 2), 16);
  }
  return { base: hex4(start ?? 0), bytes: Uint8Array.from(out, (v) => v ?? 0) };
}

async function loadFile(f) {
  const bytes = new Uint8Array(await f.arrayBuffer());
  const rom = $('file-rom').checked;
  const hex = /\.hex$/i.test(f.name);
  const name = f.name.replace(/\.[^.]*$/, '');
  const base = $('file-addr').value;
  if (rom) {
    const img = hex ? hexToBin(bytes) : { base, bytes };
    send({ type: 'rom-add', name, base: img.base, bytes: img.bytes });
  } else {
    send({ type: 'load', name, base, bytes, hex });
  }
}

$('file-load').addEventListener('click', () => {
  const f = $('file').files[0];
  if (f) loadFile(f);
  else message('Choose a file first.');
});
$('boot-f000').addEventListener('click', () => send({ type: 'boot-f000' }));

// Drag and drop anywhere on the page.
let dragDepth = 0;
document.addEventListener('dragenter', (e) => { e.preventDefault(); dragDepth++; document.body.classList.add('dragging'); });
document.addEventListener('dragleave', () => { if (--dragDepth <= 0) { dragDepth = 0; document.body.classList.remove('dragging'); } });
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => {
  e.preventDefault();
  dragDepth = 0;
  document.body.classList.remove('dragging');
  const f = e.dataTransfer.files[0];
  if (f) loadFile(f);
});

$('save-range').addEventListener('click', () => send({
  type: 'save-range', start: $('save-start').value, end: $('save-end').value, format: $('save-format').value,
}));

function download(name, data, mime) {
  const url = URL.createObjectURL(new Blob([data], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  message(`Saved ${name}.`);
}

// ------------------------------------------------------------------ sample programs
// A dropdown of bundled programs from programs/samples.json. Loading one puts
// it in RAM only; the user presses RUN.
let samples = [];
async function loadSampleList() {
  const sel = $('sample-select');
  try {
    const res = await fetch('programs/samples.json');
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    samples = await res.json();
  } catch {
    samples = [];
  }
  sel.textContent = '';
  if (!samples.length) {
    sel.appendChild(new Option('(none available)', ''));
    sel.disabled = true;
    $('sample-load').disabled = true;
    return;
  }
  for (const [i, s] of samples.entries()) {
    const o = new Option(s.name, String(i));
    if (s.description) o.title = s.description;
    sel.appendChild(o);
  }
}
$('sample-load').addEventListener('click', async () => {
  const s = samples[Number($('sample-select').value)];
  if (!s) return;
  try {
    const res = await fetch(`programs/${s.file}`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const bytes = new Uint8Array(await res.arrayBuffer());
    const hex = /\.hex$/i.test(s.file);
    send({ type: 'load', name: s.file.replace(/\.[^.]*$/, ''), base: s.address || '0000', bytes, hex });
    message(`Loaded ${s.name}${s.description ? ' — ' + s.description : '. Press RUN.'}`);
  } catch (e) {
    message(`Could not load ${s.file}: ${e.message}`, true);
  }
});

// ------------------------------------------------------------------ assemble
// Upload one or more .asm/.m18 files (several for INCLUDEs), assemble in the
// worker, show errors or the listing, then Load on demand. Never auto-runs.
let asmResult = null; // last { ok, main, hex, listing } we assembled
// Added source files, keyed by display path (relative path when known, else
// file name). Accumulated across several "Add files…"/"Add folder…" picks so
// a main file and its INCLUDEs can come from different folders.
const asmFiles = new Map();
const ASM_EXT = /\.(asm|m18|inc)$/i;

function renderAsmFileList() {
  const list = $('asm-filelist');
  list.textContent = '';
  for (const path of asmFiles.keys()) {
    const li = document.createElement('li');
    const name = document.createElement('span');
    name.textContent = path;
    const rm = document.createElement('button');
    rm.textContent = '×';
    rm.className = 'asm-file-remove';
    rm.title = `Remove ${path}`;
    rm.addEventListener('click', () => {
      asmFiles.delete(path);
      $('asm-main-wrap').hidden = true; // the main file may have changed
      renderAsmFileList();
    });
    li.append(name, rm);
    list.appendChild(li);
  }
  const any = asmFiles.size > 0;
  $('asm-clear').disabled = !any;
  $('asm-build').disabled = !any;
  if (!any) list.textContent = '';
}

// Add picked files to the set. `pathOf` gives each file's display path.
async function addAsmFiles(fileList, pathOf) {
  let added = 0;
  let skipped = 0;
  for (const f of fileList) {
    const path = pathOf(f);
    if (!ASM_EXT.test(path)) { skipped++; continue; }
    asmFiles.set(path, await f.text());
    added++;
  }
  $('asm-main-wrap').hidden = true; // adding files may change the main file
  renderAsmFileList();
  const extra = skipped ? ` (skipped ${skipped} non-source file${skipped > 1 ? 's' : ''})` : '';
  message(added ? `Added ${added} file${added > 1 ? 's' : ''}${extra}. ${asmFiles.size} in all.`
    : `No .asm/.m18/.inc files in that selection${extra}.`);
}

async function assembleFiles() {
  if (!asmFiles.size) { message('Add one or more source files first.'); return; }
  const payload = [...asmFiles.entries()].map(([name, text]) => ({ name, text }));
  const main = $('asm-main-wrap').hidden ? '' : $('asm-main').value;
  const m = await request('assemble', { files: payload, main });
  asmResult = null;
  $('asm-load').disabled = true;
  $('asm-dl-hex').disabled = true;
  $('asm-dl-lst').disabled = true;
  const out = $('asm-output');

  // Ambiguous main file: offer a chooser and stop.
  if (m.main === null) {
    const sel = $('asm-main');
    sel.textContent = '';
    for (const name of asmFiles.keys()) sel.appendChild(new Option(name, name));
    $('asm-main-wrap').hidden = false;
    out.hidden = false;
    out.textContent = m.message + '\nPick the main file above and assemble again.';
    out.classList.add('error');
    return;
  }

  if (m.errors && m.errors.length) {
    out.hidden = false;
    out.classList.add('error');
    out.textContent = `${m.errors.length} error(s) assembling ${m.main}:\n`
      + m.errors.map((e) => `${e.file || m.main}:${e.line}: ${e.message}`).join('\n');
    message(`Assembly of ${m.main} failed with ${m.errors.length} error(s).`, true);
    return;
  }
  if (!m.ok) { // e.g. bad input, no entry file chosen yet
    out.hidden = false;
    out.classList.add('error');
    out.textContent = m.message || 'Assembly failed.';
    return;
  }

  asmResult = m;
  out.hidden = false;
  out.classList.remove('error');
  out.textContent = m.listing;
  $('asm-load').disabled = false;
  $('asm-dl-hex').disabled = false;
  $('asm-dl-lst').disabled = false;
  const span = (m.lo >= 0) ? ` (${hex4(m.lo)}-${hex4(m.hi)})` : '';
  message(`Assembled ${m.main}${span}. Click Load, then press RUN.`);
}

$('asm-build').addEventListener('click', assembleFiles);
// The hidden inputs do the actual OS file picking; the visible buttons trigger them.
$('asm-add-files').addEventListener('click', () => $('asm-file').click());
$('asm-add-folder').addEventListener('click', () => $('asm-folder').click());
$('asm-file').addEventListener('change', async (e) => {
  await addAsmFiles(e.target.files, (f) => f.name);
  e.target.value = ''; // allow re-picking the same file later
});
$('asm-folder').addEventListener('change', async (e) => {
  // A folder pick gives each file a relative path (folder/sub/file); keep it
  // so INCLUDEs that name a subfolder resolve, and duplicate base names stay distinct.
  await addAsmFiles(e.target.files, (f) => f.webkitRelativePath || f.name);
  e.target.value = '';
});
$('asm-clear').addEventListener('click', () => {
  asmFiles.clear();
  $('asm-main-wrap').hidden = true;
  $('asm-output').hidden = true;
  renderAsmFileList();
  message('Cleared the source file list.');
});
renderAsmFileList();
$('asm-load').addEventListener('click', () => {
  if (!asmResult) return;
  const bytes = new TextEncoder().encode(asmResult.hex);
  send({ type: 'load', name: asmResult.main.replace(/\.[^.]*$/, ''), base: '0000', bytes, hex: true });
});
$('asm-dl-hex').addEventListener('click', () => {
  if (asmResult) download(asmResult.main.replace(/\.[^.]*$/, '') + '.hex', asmResult.hex, 'text/plain');
});
$('asm-dl-lst').addEventListener('click', () => {
  if (asmResult) download(asmResult.main.replace(/\.[^.]*$/, '') + '.lst', asmResult.listing, 'text/plain');
});

// ------------------------------------------------------------------ start-up
async function afterReady() {
  const spec = params.get('load');
  if (spec) {
    const [file, addr] = spec.split('@');
    if (!/^[\w.-]+$/.test(file)) {
      message(`?load=${spec}: only plain file names from the site's programs/ folder are allowed.`, true);
    } else {
      try {
        const res = await fetch(`programs/${file}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const bytes = new Uint8Array(await res.arrayBuffer());
        send({ type: 'load', name: file.replace(/\.[^.]*$/, ''), base: addr || '0000', bytes, hex: /\.hex$/i.test(file) });
        if (params.get('run') === '1' && !panel.switches[0]) panel.toggle(0);
      } catch (e) {
        message(`?load=${spec}: ${e.message}`, true);
      }
    }
  }
}

// Save the machine when the page is hidden (tab switch, close, navigation).
document.addEventListener('visibilitychange', () => {
  if (document.visibilityState === 'hidden') send({ type: 'save-now' });
});
window.addEventListener('pagehide', () => send({ type: 'save-now' }));

// For the browser console and automated tests.
window.elf2 = { send, panel, pixie, term, debug };

setStatus('starting…');
send({ type: 'init', config: localStorage.getItem(CONFIG_KEY) || '{}', fresh: params.get('fresh') === '1' });
$('front-panel').focus();
