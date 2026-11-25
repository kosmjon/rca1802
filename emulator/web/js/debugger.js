// Debugger panel: run control, registers, disassembly with breakpoints, memory
// viewer/editor and a serial line monitor.

// storage.js is loaded with this module's own ?v=... so the cache-busting
// version propagates to it too (see main.js).
const { hex4 } = await import(new URL('./storage.js' + new URL(import.meta.url).search, import.meta.url).href);

const hex2 = (v) => v.toString(16).toUpperCase().padStart(2, '0');
const $ = (id) => document.getElementById(id);

// Must match DIS_LINES in worker.js: how many disassembly lines are sent per update,
// used here only to size a "page" for Page Up/Down.
const DIS_LINES = 18;

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

function printable(c) {
  return c >= 0x20 && c < 0x7F ? String.fromCharCode(c) : '·';
}

function charName(c) {
  const names = { 0x08: 'BS', 0x09: 'TAB', 0x0A: 'LF', 0x0C: 'FF', 0x0D: 'CR', 0x1B: 'ESC', 0x7F: 'DEL', 0x20: 'SP' };
  if (names[c]) return names[c];
  if (c < 0x20) return `^${String.fromCharCode(c + 0x40)}`;
  return String.fromCharCode(c);
}

export class Debugger {
  constructor(send) {
    this.send = send;
    this.root = $('debugger');
    this.lastSerialSeq = -1;
    this.lastMem = '';
    this.lastLines = '';

    this.root.addEventListener('toggle', () => send({ type: 'debug-open', open: this.root.open }));
    $('dbg-stop').addEventListener('click', () => send({ type: 'stop' }));
    $('dbg-continue').addEventListener('click', () => send({ type: 'continue' }));
    $('dbg-step').addEventListener('click', () => send({ type: 'step' }));
    $('dbg-reset').addEventListener('click', () => send({ type: 'debug-reset' }));
    $('dbg-follow').addEventListener('change', (e) => send({ type: 'dis-at', follow: e.target.checked }));
    $('dbg-dis-go').addEventListener('click', () => {
      $('dbg-follow').checked = false;
      send({ type: 'dis-at', follow: false, addr: $('dbg-dis-addr').value });
    });
    $('dbg-mem-go').addEventListener('click', () => send({ type: 'mem-at', addr: $('dbg-mem-addr').value }));
    $('dbg-bp-add').addEventListener('click', () => send({ type: 'bp', addr: $('dbg-bp-addr').value, on: true }));
    $('dbg-serial-clear').addEventListener('click', () => send({ type: 'serial-clear' }));

    // Disassembly and memory are each a small window onto the full 64K
    // address space; scroll them (mouse wheel, the ▲/▼ buttons, or
    // Page Up/Down with one focused) to page that window to higher or
    // lower addresses, rather than only being able to jump to one typed
    // in the "Go" box.
    const disList = $('dbg-dis');
    const memGrid = $('dbg-mem');
    disList.addEventListener('wheel', (e) => {
      e.preventDefault();
      this.scrollDis(Math.sign(e.deltaY));
    }, { passive: false });
    disList.addEventListener('keydown', (e) => {
      if (e.key === 'PageDown') this.scrollDis(DIS_LINES - 2);
      else if (e.key === 'PageUp') this.scrollDis(-(DIS_LINES - 2));
      else return;
      e.preventDefault();
    });
    $('dbg-dis-up').addEventListener('click', () => this.scrollDis(-1));
    $('dbg-dis-down').addEventListener('click', () => this.scrollDis(1));
    memGrid.addEventListener('wheel', (e) => {
      e.preventDefault();
      this.scrollMem(Math.sign(e.deltaY) * (e.shiftKey ? 1 : 16));
    }, { passive: false });
    memGrid.addEventListener('keydown', (e) => {
      if (e.key === 'PageDown') this.scrollMem(16 * 16);
      else if (e.key === 'PageUp') this.scrollMem(-16 * 16);
      else return;
      e.preventDefault();
    });
    $('dbg-mem-up').addEventListener('click', (e) => this.scrollMem(e.shiftKey ? -1 : -16));
    $('dbg-mem-down').addEventListener('click', (e) => this.scrollMem(e.shiftKey ? 1 : 16));

    // Keyboard: F10 / F8 step, F5 continue, while the debugger has focus.
    this.root.addEventListener('keydown', (e) => {
      if (e.target.tagName === 'INPUT') return;
      if (e.key === 'F10' || e.key === 'F8') send({ type: 'step' });
      else if (e.key === 'F5') send({ type: 'continue' });
      else return;
      e.preventDefault();
    });
  }

  // Moves the disassembly window by `lines` real instructions: forward by
  // exactly the length of the current top instruction (so one notch is
  // one real line), or back by a short guessed step that re-syncs to a
  // real instruction boundary within a line or two (there's no reverse
  // disassembly to get this exact, same tradeoff every such debugger UI
  // makes). Turns off "follow PC", the same as typing an address in "Go".
  scrollDis(lines) {
    if (this.disStart === undefined || !lines) return;
    const dir = Math.sign(lines);
    const count = Math.abs(lines);
    let step;
    if (dir > 0 && this.lines && this.lines.length > 1) {
      step = (this.lines[1].addr - this.lines[0].addr) * count;
    } else {
      step = 2 * count; // best-effort guess when scrolling backward
    }
    $('dbg-follow').checked = false;
    this.send({ type: 'dis-at', follow: false, addr: hex4((this.disStart + dir * step) & 0xFFFF) });
  }

  // Moves the memory window by `bytes` (positive = toward higher addresses).
  scrollMem(bytes) {
    if (this.memStart === undefined || !bytes) return;
    this.send({ type: 'mem-at', addr: hex4((this.memStart + bytes) & 0xFFFF) });
  }

  edit(label, current, apply) {
    const v = prompt(`New value for ${label} (hex):`, current);
    if (v !== null && v.trim() !== '') apply(v.trim());
  }

  update(u) {
    if (!this.root.open || !u.debug) return;
    const s = u.state;
    const d = u.debug;

    // Run state.
    const st = $('dbg-status');
    if (u.stoppedAt >= 0) st.textContent = `stopped at breakpoint ${hex4(u.stoppedAt)}`;
    else if (d.paused) st.textContent = 'stopped';
    else st.textContent = s.state === 'running' ? 'running' : s.state;
    st.classList.toggle('stopped', u.stoppedAt >= 0 || d.paused);

    // Registers.
    const regs = $('dbg-regs');
    if (!regs.children.length) {
      for (let i = 0; i < 16; i++) {
        const cell = el('div', 'reg');
        cell.append(el('span', 'name', `R${i.toString(16).toUpperCase()}`), el('span', 'val'), el('span', 'tags'));
        cell.addEventListener('click', () => this.edit(`R${i.toString(16).toUpperCase()}`, hex4(this.regs[i]),
          (v) => this.send({ type: 'set-reg', reg: i, value: v })));
        regs.appendChild(cell);
      }
    }
    this.regs = s.r;
    [...regs.children].forEach((cell, i) => {
      cell.children[1].textContent = hex4(s.r[i]);
      const tags = [];
      if (i === s.p) tags.push('P');
      if (i === s.x) tags.push('X');
      if (i === 0) tags.push('DMA');
      cell.children[2].textContent = tags.join(' ');
      cell.classList.toggle('is-p', i === s.p);
      cell.classList.toggle('is-x', i === s.x);
    });
    const flags = $('dbg-flags');
    const field = (name, value, edit) => {
      const f = el('span', edit ? 'flag editable' : 'flag', `${name} ${value}`);
      if (edit) f.addEventListener('click', () => this.edit(name, value, (v) => this.send({ type: 'set-reg', reg: edit, value: v })));
      return f;
    };
    flags.textContent = '';
    flags.append(
      field('D', hex2(s.d), 'd'), field('DF', s.df ? 1 : 0, 'df'), field('P', s.p.toString(16).toUpperCase(), 'p'),
      field('X', s.x.toString(16).toUpperCase(), 'x'), field('T', hex2(s.t), 't'), field('IE', s.ie ? 1 : 0, 'ie'),
      field('Q', s.q ? 1 : 0), field('EF', s.ef.map((e, i) => (e ? i + 1 : '-')).join('')),
      field('cycles', s.cycles),
    );

    // Keep the scroll state current even when the DOM below doesn't need to be
    // rebuilt (the `key`/`memKey` guards below skip re-rendering when nothing
    // visible changed, but scrollDis/scrollMem always need the latest window).
    this.disStart = d.disStart;
    this.memStart = d.memStart;
    this.lines = d.lines;

    // Disassembly.
    const key = JSON.stringify([d.lines, s.pc, d.breakpoints]);
    if (key !== this.lastLines) {
      this.lastLines = key;
      const list = $('dbg-dis');
      list.textContent = '';
      for (const l of d.lines) {
        const row = el('div', 'dis-line');
        row.classList.toggle('pc', l.addr === s.pc);
        row.classList.toggle('bp', d.breakpoints.includes(l.addr));
        row.append(
          el('span', 'bp-dot', '●'),
          el('span', 'addr', hex4(l.addr)),
          el('span', 'bytes', l.bytes.map(hex2).join(' ')),
          el('span', 'text', l.text),
        );
        row.title = 'Click to set or clear a breakpoint';
        row.dataset.addr = l.addr;
        row.addEventListener('click', () => this.send({ type: 'bp', addr: hex4(l.addr) }));
        list.appendChild(row);
      }
      const bps = $('dbg-bps');
      bps.textContent = '';
      for (const a of d.breakpoints) {
        const b = el('button', 'bp-chip', `${hex4(a)} ✕`);
        b.title = 'Remove breakpoint';
        b.addEventListener('click', () => this.send({ type: 'bp', addr: hex4(a), on: false }));
        bps.appendChild(b);
      }
      if (!d.breakpoints.length) bps.textContent = 'none';
    }

    // Memory.
    const memKey = d.memStart + ':' + d.mem.join(',') + ':' + s.pc;
    if (memKey !== this.lastMem) {
      this.lastMem = memKey;
      const grid = $('dbg-mem');
      grid.textContent = '';
      for (let row = 0; row < 16; row++) {
        const base = (d.memStart + row * 16) & 0xFFFF;
        const line = el('div', 'mem-line');
        line.appendChild(el('span', 'addr', hex4(base)));
        let ascii = '';
        for (let k = 0; k < 16; k++) {
          const v = d.mem[row * 16 + k];
          const a = (base + k) & 0xFFFF;
          const b = el('span', 'byte', hex2(v));
          if (a === s.pc) b.classList.add('pc');
          if (a === s.r[s.x]) b.classList.add('rx');
          b.dataset.addr = a;
          b.addEventListener('click', () => this.edit(`memory ${hex4(a)}`, hex2(v), (val) => {
            // Accept one byte or several separated by spaces.
            const bytes = val.split(/[\s,]+/).map((t) => parseInt(t, 16));
            if (bytes.some((x) => !(x >= 0 && x <= 255))) return alert(`'${val}' is not hex bytes`);
            this.send({ type: 'poke', addr: hex4(a), bytes });
          }));
          line.appendChild(b);
          ascii += printable(v);
        }
        line.appendChild(el('span', 'ascii', ascii));
        grid.appendChild(line);
      }
    }

    // Serial monitor.
    if (d.serialSeq !== this.lastSerialSeq) {
      this.lastSerialSeq = d.serialSeq;
      const log = $('dbg-serial');
      log.textContent = '';
      for (const e of d.serial) {
        const line = el('div', `ser ${e.dir}`);
        const t = `${(e.cycle / u.cps).toFixed(3)}s`;
        if (e.dir === 'err') line.textContent = `${t}  TX framing error (check baud rate and polarity)`;
        else line.textContent = `${t}  ${e.dir === 'tx' ? 'Elf→VID-1' : 'VID-1→Elf'}  ${hex2(e.code)}  ${charName(e.code)}`;
        log.appendChild(line);
      }
      log.scrollTop = log.scrollHeight;
      const errs = d.serial.filter((e) => e.dir === 'err').length;
      $('dbg-serial-errs').textContent = errs ? `${errs} framing error${errs > 1 ? 's' : ''}` : '';
    }
  }
}
