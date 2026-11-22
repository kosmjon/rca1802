// Front panel: hex keypad, IN button, RUN/LOAD/MP switches, seven-segment display,
// Q LED and EF indicators.

// Segment order a b c d e f g; bit 6 = a ... bit 0 = g.
const DIGITS = [0x7E, 0x30, 0x6D, 0x79, 0x33, 0x5B, 0x5F, 0x70, 0x7F, 0x7B, 0x77, 0x1F, 0x4E, 0x3D, 0x4F, 0x47];
const SEGMENTS = [
  'M8,4 L32,4 L28,10 L12,10 Z',        // a
  'M34,6 L34,32 L30,29 L30,10 Z',      // b
  'M34,36 L34,62 L30,58 L30,39 Z',     // c
  'M8,64 L32,64 L28,58 L12,58 Z',      // d
  'M6,36 L6,62 L10,58 L10,39 Z',       // e
  'M6,6 L6,32 L10,29 L10,10 Z',        // f
  'M8,34 L12,31 L28,31 L32,34 L28,37 L12,37 Z', // g
];

function makeDigit() {
  const ns = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(ns, 'svg');
  svg.setAttribute('viewBox', '0 0 40 68');
  const segs = SEGMENTS.map((d) => {
    const p = document.createElementNS(ns, 'path');
    p.setAttribute('d', d);
    p.setAttribute('class', 'seg');
    svg.appendChild(p);
    return p;
  });
  return { svg, segs };
}

export class FrontPanel {
  constructor(send) {
    this.send = send;
    this.switches = [false, false, false];
    this.inHeld = false;

    const disp = document.getElementById('hex-display');
    this.digits = [makeDigit(), makeDigit()];
    for (const d of this.digits) disp.appendChild(d.svg);
    this.shown = -1;

    const pad = document.getElementById('keypad');
    // Keypad layout as on the Elf II: C D E F / 8 9 A B / 4 5 6 7 / 0 1 2 3
    for (const n of [0xC, 0xD, 0xE, 0xF, 8, 9, 0xA, 0xB, 4, 5, 6, 7, 0, 1, 2, 3]) {
      const b = document.createElement('button');
      b.textContent = n.toString(16).toUpperCase();
      b.addEventListener('click', () => this.send({ type: 'hex', nibble: n }));
      pad.appendChild(b);
    }

    this.swButtons = ['sw-run', 'sw-load', 'sw-mp'].map((id) => document.getElementById(id));
    this.swButtons.forEach((b, i) => b.addEventListener('click', () => this.toggle(i)));

    const inBtn = document.getElementById('btn-in');
    this.inBtn = inBtn;
    inBtn.addEventListener('pointerdown', (e) => { e.preventDefault(); this.setIn(true); });
    for (const ev of ['pointerup', 'pointerleave', 'pointercancel']) {
      inBtn.addEventListener(ev, () => this.setIn(false));
    }

    // Keyboard shortcuts while the front panel has focus.
    const root = document.getElementById('front-panel');
    root.addEventListener('keydown', (e) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const k = e.key.toLowerCase();
      if (/^[0-9a-f]$/.test(k)) {
        this.send({ type: 'hex', nibble: parseInt(k, 16) });
      } else if (k === 'enter') {
        if (!e.repeat) this.setIn(true);
      } else if (k === 'r') this.toggle(0);
      else if (k === 'l') this.toggle(1);
      else if (k === 'm') this.toggle(2);
      else return;
      e.preventDefault();
    });
    root.addEventListener('keyup', (e) => {
      if (e.key === 'Enter') this.setIn(false);
    });
    root.addEventListener('blur', () => this.setIn(false));

    this.ledQ = document.getElementById('led-q');
    this.ledEf = [1, 2, 3, 4].map((n) => document.getElementById(`led-ef${n}`));
    this.cpuState = document.getElementById('cpu-state');
  }

  // Show the switch positions the machine reports (they change on restore and
  // power cycle, not only by clicking).
  syncSwitches(p) {
    [p.run, p.load, p.mp].forEach((on, i) => {
      if (this.switches[i] !== on) {
        this.switches[i] = on;
        this.swButtons[i].classList.toggle('on', on);
      }
    });
  }

  resetSwitches() {
    this.syncSwitches({ run: false, load: false, mp: false });
  }

  toggle(i) {
    this.switches[i] = !this.switches[i];
    this.swButtons[i].classList.toggle('on', this.switches[i]);
    this.send({ type: 'switch', sw: i, on: this.switches[i] });
  }

  setIn(down) {
    if (down === this.inHeld) return;
    this.inHeld = down;
    this.inBtn.classList.toggle('pressed', down);
    this.send({ type: 'in', down });
  }

  showByte(v) {
    if (v === this.shown) return;
    this.shown = v;
    [v >> 4, v & 0xF].forEach((n, i) => {
      const bits = DIGITS[n];
      this.digits[i].segs.forEach((s, k) => s.classList.toggle('on', !!(bits & (0x40 >> k))));
    });
  }

  update(u) {
    this.showByte(u.display);
    this.syncSwitches(u.state.panel);
    this.ledQ.classList.toggle('on', u.q);
    this.ledEf.forEach((l, i) => l.classList.toggle('on', !!(u.ef & (1 << i))));
    const s = u.state;
    const pc = s.pc.toString(16).toUpperCase().padStart(4, '0');
    this.cpuState.textContent = `${s.state}  P=${s.p.toString(16).toUpperCase()}  ${pc}`;
  }
}
