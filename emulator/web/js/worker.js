// Web Worker that owns the emulator and its saved state. All timing is in emulated
// machine cycles; wall time only decides how many cycles to run per pass.
//
// Persistence (IndexedDB, see storage.js):
//   * ROMs added with "as ROM" are stored by SHA-256 and loaded at every power-on.
//   * The whole machine is snapshotted every few seconds and when the page is hidden;
//     the next visit resumes from it. "Power cycle" discards it.
//
// Messages from the page:
//   {type: 'init', config, fresh}          start; fresh = ignore the saved snapshot
//   {type: 'switch', sw, on}               0 RUN, 1 LOAD, 2 MP
//   {type: 'in', down}                     IN button
//   {type: 'hex', nibble}                  hex key 0-F
//   {type: 'key', code} / {type: 'paste', text} / {type: 'term-clear'}   VID-1
//   {type: 'speed', max}                   max-speed mode
//   {type: 'load', name, base, bytes, hex} load a program into RAM
//   {type: 'rom-add', name, base, bytes}   add a ROM (loaded now, stored for later)
//   {type: 'rom-enable', sha256, enabled}  {type: 'rom-remove', sha256}
//   {type: 'power-cycle', config?}         new machine with the enabled ROMs
//   {type: 'boot-f000'}                    key in C0 F0 00 at 0000
//   {type: 'save-now'}                     snapshot now (page being hidden)
//   {type: 'save-range', start, end, format}   reply {type: 'download', ...}
//   {type: 'continue'}                     resume after a breakpoint
//   {type: 'get-text', id}                 reply {type: 'text', id, text}
//
// Messages to the page:
//   {type: 'ready', version, cps, restored}
//   {type: 'update', ...}                  about 60 per second
//   {type: 'roms', roms, missing}          stored ROMs and ROMs the config lacks
//   {type: 'config-ok', config}            a new configuration was accepted
//   {type: 'loaded', msg} / {type: 'error', msg} / {type: 'text', id, text}
//   {type: 'download', name, data, mime}

import init, { Emulator, version, assemble_files, assemble_files_from } from '../pkg/elf2_wasm.js';
import { openDb, getAllRoms, getRom, putRom, deleteRom, getState, putState, deleteState, sha256Hex, hex4 } from './storage.js';

const SLICE = 20000;          // max cycles per run() call
const MAX_CATCH_UP_MS = 50;   // never try to catch up more than this after a stall
const POST_MS = 16;           // UI update interval
const AUTOSAVE_MS = 5000;

let wasm = null;
let emu = null;
let db = null;
let config = '{}';
let cps = 223721;
let maxSpeed = false;
let stoppedAt = -1;
let last = 0;
let lastPost = 0;
let lastSave = 0;
let pixieDirty = true;
let rateCycles = 0;
let rateStart = 0;
let rate = 0;

function post(msg, transfer) {
  self.postMessage(msg, transfer || []);
}

function errorText(e) {
  return String(e && e.message ? e.message : e);
}

function fail(e) {
  post({ type: 'error', msg: errorText(e) });
}

function copy(ptr, len) {
  if (!ptr) return null;
  // A fresh view each time: wasm memory can grow and detach old views.
  return new Uint8Array(wasm.memory.buffer, ptr, len).slice();
}

function sendUpdate() {
  const pixie = pixieDirty ? copy(emu.pixie_frame_ptr(), 1024) : null;
  pixieDirty = false;
  const cols = emu.vid1_columns();
  const screen = copy(emu.vid1_screen_ptr(), 16 * cols);
  const state = JSON.parse(emu.state_json());
  const transfer = [];
  if (pixie) transfer.push(pixie.buffer);
  if (screen) transfer.push(screen.buffer);
  post({
    type: 'update',
    pixie, screen, cols,
    cursor: emu.vid1_cursor(),
    state,
    display: emu.hex_display(),
    q: emu.q(),
    ef: emu.ef_bits(),
    pixieOn: emu.pixie_on(),
    pending: emu.term_pending(),
    // Running from an address with no RAM or ROM: the CPU is executing FF bytes.
    pcUnmapped: (state.state === 'running' || state.state === 'idle') && !emu.is_mapped(state.pc),
    roms: emu.roms_json(),
    stoppedAt,
    paused,
    debug: debugInfo(state),
    cps,
    rate,
  }, transfer);
}

// ------------------------------------------------------------------ IN button
// The IN button must stay down long enough for software polling EF4 to see it, as a
// real (debounced) push button does. Releases are held back until the button has
// been down for at least MIN_IN_MS of emulated time.
const MIN_IN_MS = 30;
let totalCycles = 0;
let inDownAt = -1;
let inReleasePending = false;

function pressIn(down) {
  if (down) {
    inReleasePending = false;
    if (inDownAt < 0) inDownAt = totalCycles;
    emu.press_in(true);
  } else if (inDownAt >= 0 && totalCycles - inDownAt < MIN_IN_MS * cps / 1000) {
    inReleasePending = true;
  } else {
    inDownAt = -1;
    emu.press_in(false);
  }
}

function releaseInIfDue() {
  if (inReleasePending && totalCycles - inDownAt >= MIN_IN_MS * cps / 1000) {
    inReleasePending = false;
    inDownAt = -1;
    emu.press_in(false);
  }
}

// ------------------------------------------------------------------ run loop
function runCycles(n) {
  while (n > 0 && stoppedAt < 0 && !paused) {
    const c = Math.min(n, inReleasePending ? 1000 : SLICE);
    const count = emu.run(c);
    if (count) {
      for (const e of emu.events()) noteEvent(e);
    }
    stoppedAt = emu.stopped_at();
    n -= c;
    rateCycles += c;
    totalCycles += c;
    releaseInIfDue();
  }
}

function loop() {
  const now = performance.now();
  try {
    if (stoppedAt < 0 && !paused) {
      if (maxSpeed) {
        const end = now + 12;
        while (performance.now() < end && stoppedAt < 0 && !paused) runCycles(SLICE);
      } else {
        const dt = Math.min(now - last, MAX_CATCH_UP_MS);
        runCycles(Math.round(dt * cps / 1000));
      }
    }
    last = now;
    if (now - rateStart >= 1000) {
      rate = rateCycles / ((now - rateStart) / 1000) / cps;
      rateCycles = 0;
      rateStart = now;
    }
    if (now - lastPost >= POST_MS) {
      lastPost = now;
      sendUpdate();
    }
    if (now - lastSave >= AUTOSAVE_MS) {
      lastSave = now;
      saveSnapshot();
    }
  } catch (e) {
    fail(e);
    return;
  }
  setTimeout(loop, maxSpeed ? 0 : 4);
}

// ------------------------------------------------------------------ debugger
let paused = false;            // stopped by the debugger's Stop button
let debugOpen = false;         // send disassembly/memory with updates
let followPc = true;
let disStart = 0;
let memStart = 0;
const DIS_LINES = 18;
const breakpoints = new Set();
const serialLog = [];          // {dir: 'tx'|'rx'|'err', code, cycle}
const SERIAL_LOG_MAX = 300;
let serialSeq = 0;

function logSerial(dir, code) {
  serialLog.push({ dir, code, cycle: totalCycles, seq: ++serialSeq });
  if (serialLog.length > SERIAL_LOG_MAX) serialLog.shift();
}

function applyBreakpoints() {
  for (const a of breakpoints) emu.set_breakpoint(a, true);
}

function debugInfo(state) {
  if (!debugOpen) return null;
  const pc = state.pc;
  let lines = JSON.parse(emu.disasm_json(disStart, DIS_LINES));
  if (followPc && !lines.slice(0, DIS_LINES - 3).some((l) => l.addr === pc)) {
    disStart = pc;
    lines = JSON.parse(emu.disasm_json(disStart, DIS_LINES));
  }
  return {
    lines,
    disStart,
    memStart,
    mem: emu.peek(memStart, 256),
    breakpoints: [...breakpoints].sort((a, b) => a - b),
    serial: serialLog.slice(-60),
    serialSeq,
    paused,
  };
}

// One instruction: DMA, interrupt and idle cycles (1 cycle each) are stepped through
// until an instruction (2 or 3 cycles) has executed, or the CPU cannot execute one.
function stepInstruction() {
  for (let i = 0; i < 64; i++) {
    const c = emu.step();
    for (const e of emu.events()) noteEvent(e);
    totalCycles += c;
    if (c >= 2) return;
    const s = JSON.parse(emu.state_json()).state;
    if (s !== 'running') return;
  }
}

function noteEvent(e) {
  const kind = e >>> 24;
  if (kind === 1) pixieDirty = true;
  else if (kind === 2) logSerial('tx', e & 0xFF);
  else if (kind === 3) logSerial('err', 0);
}

// ------------------------------------------------------------------ persistence
async function saveSnapshot() {
  if (!db || !emu) return;
  try {
    await putState(db, 'snapshot', emu.snapshot());
  } catch (e) {
    post({ type: 'error', msg: `Could not save the machine state: ${errorText(e)}` });
  }
}

async function sendRoms() {
  const stored = db ? await getAllRoms(db) : [];
  const loaded = JSON.parse(emu.roms_json());
  post({
    type: 'roms',
    roms: stored.map((r) => ({
      sha256: r.sha256, name: r.name, base: r.base, size: r.bytes.length, enabled: r.enabled,
      loaded: loaded.some((l) => l.name === r.name && l.base === r.base),
    })),
    missing: JSON.parse(emu.missing_roms_json()),
  });
}

/// A new machine from `config` plus the enabled stored ROMs, or the saved snapshot.
async function powerOn(useSnapshot) {
  emu = new Emulator(config);
  stoppedAt = -1;
  inDownAt = -1;
  inReleasePending = false;
  pixieDirty = true;
  let restored = false;
  if (useSnapshot && db) {
    const snap = await getState(db, 'snapshot');
    if (snap) {
      try {
        emu.restore(snap);
        restored = true;
      } catch (e) {
        post({ type: 'error', msg: `The saved machine state could not be restored (${errorText(e)}); starting fresh.` });
        await deleteState(db, 'snapshot');
      }
    }
  }
  if (!restored && db) {
    for (const r of await getAllRoms(db)) {
      if (!r.enabled) continue;
      try {
        emu.load_rom(r.name, r.base, r.bytes);
      } catch (e) {
        post({ type: 'error', msg: `ROM ${r.name}: ${errorText(e)}` });
      }
    }
  }
  cps = emu.cycles_per_second();
  applyBreakpoints();
  await sendRoms();
  return restored;
}

// ------------------------------------------------------------------ messages
function parseAddr(s) {
  const v = parseInt(String(s).replace(/^0x/i, ''), 16);
  if (!(v >= 0 && v <= 0xFFFF)) throw new Error(`'${s}' is not a hex address`);
  return v;
}

function loadProgram(m) {
  try {
    if (m.hex) {
      const low = emu.load_hex(new TextDecoder().decode(m.bytes));
      post({ type: 'loaded', msg: `${m.name}: loaded from ${hex4(low)}` });
    } else {
      const base = parseAddr(m.base);
      const n = emu.load_bin(base, m.bytes);
      post({ type: 'loaded', msg: `${m.name}: ${n} bytes at ${hex4(base)}` });
    }
  } catch (e) {
    let msg = `${m.name}: ${errorText(e)}`;
    if (/has no memory/.test(msg)) {
      msg += '. Nothing was loaded. To load a ROM image (for example the Giant Board monitor at F000), tick "as ROM" and load it again.';
    }
    post({ type: 'error', msg });
  }
}

async function addRom(m) {
  const base = parseAddr(m.base);
  const sha256 = await sha256Hex(m.bytes);
  try {
    emu.load_rom(m.name, base, m.bytes);
  } catch (e) {
    post({ type: 'error', msg: `ROM ${m.name}: ${errorText(e)}` });
    return;
  }
  if (db) await putRom(db, { sha256, name: m.name, base, bytes: m.bytes, enabled: true });
  post({ type: 'loaded', msg: `ROM ${m.name}: ${m.bytes.length} bytes at ${hex4(base)}, saved in this browser for next time.` });
  await sendRoms();
}

function saveRange(m) {
  const start = parseAddr(m.start);
  const end = parseAddr(m.end);
  if (end < start) throw new Error('the end address is below the start address');
  const name = `elf2-${hex4(start)}-${hex4(end)}`;
  if (m.format === 'hex') {
    post({ type: 'download', name: `${name}.hex`, data: emu.save_hex(start, end), mime: 'text/plain' });
  } else {
    const data = emu.save_bin(start, end);
    post({ type: 'download', name: `${name}.bin`, data, mime: 'application/octet-stream' }, [data.buffer]);
  }
}

async function handle(m) {
  switch (m.type) {
    case 'switch': emu.set_switch(m.sw, m.on); pixieDirty = true; break;
    case 'in': pressIn(m.down); break;
    case 'hex': emu.hex_key(m.nibble); break;
    case 'key': emu.term_key(m.code); logSerial('rx', m.code & 0x7F); break;
    case 'paste':
      emu.term_paste(m.text);
      for (const ch of m.text.replace(/\r?\n/g, '\r')) logSerial('rx', ch.charCodeAt(0) & 0x7F);
      break;
    case 'term-clear': emu.vid1_clear(); break;
    case 'speed': maxSpeed = !!m.max; break;
    case 'continue': stoppedAt = -1; paused = false; break;
    case 'stop': paused = true; break;
    case 'debug-reset': {
      // RUN off and back on: the CPU resets, memory and peripherals' contents stay.
      const p = JSON.parse(emu.state_json()).panel;
      emu.set_switch(0, false);
      if (p.run) emu.set_switch(0, true);
      stoppedAt = -1;
      break;
    }
    case 'step': paused = true; stoppedAt = -1; stepInstruction(); break;
    case 'debug-open': debugOpen = !!m.open; break;
    case 'dis-at':
      followPc = !!m.follow;
      if (m.addr !== undefined) disStart = parseAddr(m.addr);
      break;
    case 'mem-at': memStart = parseAddr(m.addr); break;
    case 'bp': {
      const a = parseAddr(m.addr);
      const on = m.on === undefined ? !breakpoints.has(a) : m.on;
      if (on) breakpoints.add(a);
      else breakpoints.delete(a);
      emu.set_breakpoint(a, on);
      break;
    }
    case 'set-reg': {
      const v = parseInt(String(m.value).trim().replace(/^0x/i, ''), 16);
      if (!(v >= 0 && v <= 0xFFFF)) throw new Error(`'${m.value}' is not a hex value`);
      if (typeof m.reg === 'number') emu.set_register(m.reg, v);
      else emu.set_cpu_field(m.reg, v);
      break;
    }
    case 'poke': emu.poke(parseAddr(m.addr), Uint8Array.from(m.bytes)); break;
    case 'serial-clear': serialLog.length = 0; break;
    case 'boot-f000':
      emu.boot_f000();
      post({ type: 'loaded', msg: 'C0 F0 00 keyed in at 0000. Set RUN to start the monitor.' });
      break;
    case 'load': loadProgram(m); break;
    case 'rom-add': await addRom(m); break;
    case 'rom-enable': {
      const r = await getRom(db, m.sha256);
      if (r) await putRom(db, { ...r, enabled: m.enabled });
      await sendRoms();
      break;
    }
    case 'rom-remove': await deleteRom(db, m.sha256); await sendRoms(); break;
    case 'power-cycle': {
      if (m.config !== undefined) {
        try {
          new Emulator(m.config).free(); // validate first
        } catch (e) {
          post({ type: 'error', msg: `Configuration not applied: ${errorText(e)}` });
          break;
        }
        config = m.config;
        post({ type: 'config-ok', config });
      }
      if (db) await deleteState(db, 'snapshot');
      await powerOn(false);
      post({ type: 'loaded', msg: 'Power cycled: fresh machine with the enabled ROMs.' });
      break;
    }
    case 'save-now': lastSave = performance.now(); await saveSnapshot(); break;
    case 'save-range': saveRange(m); break;
    case 'get-text': post({ type: 'text', id: m.id, text: emu.vid1_text() }); break;
    case 'assemble': {
      const filesJson = JSON.stringify(m.files);
      const json = m.main ? assemble_files_from(filesJson, m.main) : assemble_files(filesJson);
      post({ type: 'reply', id: m.id, ...JSON.parse(json) });
      break;
    }
    default: break;
  }
  sendUpdate();
}

// Messages are handled one at a time, in order, even though some are asynchronous.
let queue = Promise.resolve();
self.onmessage = (ev) => {
  const m = ev.data;
  queue = queue.then(async () => {
    try {
      if (m.type === 'init') {
        wasm = await init();
        config = m.config || '{}';
        try {
          db = await openDb();
        } catch (e) {
          post({ type: 'error', msg: `Browser storage unavailable (${errorText(e)}); ROMs and state will not be saved.` });
        }
        let restored;
        try {
          restored = await powerOn(!m.fresh);
        } catch (e) {
          // A bad saved configuration must not lock the page: fall back to defaults.
          post({ type: 'error', msg: `Configuration rejected (${errorText(e)}); using the default machine.` });
          config = '{}';
          restored = await powerOn(false);
        }
        last = rateStart = lastPost = lastSave = performance.now();
        post({ type: 'ready', version: version(), cps, restored });
        loop();
        return;
      }
      if (emu) await handle(m);
    } catch (e) {
      fail(e);
    }
  });
};
