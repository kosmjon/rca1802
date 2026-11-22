// VID-1 terminal: draws the 16-line screen from the emulator's screen memory and sends
// key presses to the emulated keyboard. Full duplex, no local echo: what you type only
// appears if the Elf II echoes it.
//
// The font is the browser's monospace font standing in for the VID-1 character
// generator ROM.

const ROWS = 16;
const CELL_W = 11;
const CELL_H = 20;
const PAD = 8;

export class Vid1Terminal {
  constructor(canvas, send) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.send = send;
    this.cols = 64;
    this.screen = new Uint8Array(ROWS * 64).fill(32);
    this.cursor = 0;
    this.blink = true;
    this.upper = document.getElementById('upper-case');
    this.pendingLabel = document.getElementById('term-pending');
    this.resize(64);

    canvas.addEventListener('keydown', (e) => this.onKey(e));
    canvas.addEventListener('paste', (e) => {
      e.preventDefault();
      this.paste(e.clipboardData.getData('text'));
    });
    canvas.addEventListener('mousedown', () => canvas.focus());
    setInterval(() => { this.blink = !this.blink; this.draw(); }, 530);
  }

  resize(cols) {
    this.cols = cols;
    const dpr = window.devicePixelRatio || 1;
    const w = cols * CELL_W + 2 * PAD;
    const h = ROWS * CELL_H + 2 * PAD;
    this.canvas.width = w * dpr;
    this.canvas.height = h * dpr;
    this.canvas.style.aspectRatio = `${w} / ${h}`;
    this.canvas.style.maxWidth = `${Math.round(w * 1.25)}px`;
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.ctx.font = `${CELL_H - 4}px ui-monospace, "Cascadia Mono", Consolas, monospace`;
    this.ctx.textBaseline = 'top';
  }

  paste(text) {
    if (!text) return;
    this.send({ type: 'paste', text: this.upper.checked ? text.toUpperCase() : text });
  }

  onKey(e) {
    let code = null;
    const k = e.key;
    if (e.ctrlKey && !e.altKey && k.length === 1) {
      const c = k.toUpperCase().charCodeAt(0);
      if (c >= 0x40 && c <= 0x5F) code = c & 0x1F;          // Ctrl-@ .. Ctrl-_
      else if (k === 'v') return;                          // let the paste event happen
    } else if (k.length === 1 && !e.metaKey) {
      const s = this.upper.checked ? k.toUpperCase() : k;
      const c = s.charCodeAt(0);
      if (c < 128) code = c;
    } else {
      code = { Enter: 13, Backspace: 8, Tab: 9, Escape: 27, Delete: 127 }[k] ?? null;
    }
    if (code === null) return;
    e.preventDefault();
    this.send({ type: 'key', code });
  }

  update(u) {
    if (u.cols && u.cols !== this.cols) this.resize(u.cols);
    if (u.screen) this.screen = u.screen;
    this.cursor = u.cursor;
    this.pendingLabel.textContent = u.pending ? `${u.pending} characters waiting to send` : '';
    this.draw();
  }

  draw() {
    const ctx = this.ctx;
    const w = this.cols * CELL_W + 2 * PAD;
    const h = ROWS * CELL_H + 2 * PAD;
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = '#33ff66';
    ctx.shadowColor = 'rgba(51,255,102,0.6)';
    ctx.shadowBlur = 4;
    for (let r = 0; r < ROWS; r++) {
      const line = String.fromCharCode(...this.screen.subarray(r * this.cols, (r + 1) * this.cols));
      for (let c = 0; c < this.cols; c++) {
        const ch = line[c];
        if (ch !== ' ') ctx.fillText(ch, PAD + c * CELL_W, PAD + r * CELL_H + 2);
      }
    }
    ctx.shadowBlur = 0;
    if (this.cursor >= 0 && (this.blink || document.activeElement !== this.canvas)) {
      const row = this.cursor >> 8;
      const col = this.cursor & 0xFF;
      ctx.fillRect(PAD + col * CELL_W, PAD + row * CELL_H + CELL_H - 4, CELL_W - 1, 3);
    }
  }
}
