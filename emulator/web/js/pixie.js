// CDP1861 display: 64 x 128 pixel bitmap, 8 bytes per line, MSB = leftmost pixel.

export class PixieScreen {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.image = this.ctx.createImageData(64, 128);
    this.scanlines = document.getElementById('scanlines');
    this.label = document.getElementById('pixie-state');
    this.frame = new Uint8Array(1024);
    this.scanlines.addEventListener('change', () => this.draw());
    this.draw();
  }

  update(u) {
    this.label.textContent = u.pixieOn ? 'on' : 'off';
    this.label.classList.toggle('on', u.pixieOn);
    if (u.pixie) {
      this.frame = u.pixie;
      this.draw();
    }
  }

  draw() {
    const d = this.image.data;
    const lines = this.scanlines.checked;
    for (let y = 0; y < 128; y++) {
      const dim = lines && (y & 1) ? 0.75 : 1;
      for (let x = 0; x < 64; x++) {
        const on = (this.frame[y * 8 + (x >> 3)] >> (7 - (x & 7))) & 1;
        const i = (y * 64 + x) * 4;
        const v = on ? 235 * dim : 8;
        d[i] = v; d[i + 1] = v; d[i + 2] = v; d[i + 3] = 255;
      }
    }
    this.ctx.putImageData(this.image, 0, 0);
  }
}
