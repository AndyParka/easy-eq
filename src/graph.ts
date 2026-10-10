import { clamp, coefficients, magnitudeDb, type Band, type Ear, type Profile } from './core';
type GraphOptions = { profile: () => Profile; rate: () => number; ear: () => Ear; view: () => string; selected: () => string | null; spectrum: () => { bins: Float32Array; rate: number } | null; onSelect: (ear: Ear, id: string) => void; onDrag: (frequency: number, gain: number) => void; onQ: (delta: number) => void; onAdd: (frequency: number, gain: number) => void };
export class EqGraph {
  canvas: HTMLCanvasElement;
  options: GraphOptions;
  width = 900;
  height = 390;
  dragging = false;
  dirty = true;
  constructor(canvas: HTMLCanvasElement, options: GraphOptions) {
    this.canvas = canvas; this.options = options;
    new ResizeObserver(() => this.resize()).observe(canvas);
    canvas.addEventListener('pointerdown', e => this.down(e));
    canvas.addEventListener('pointermove', e => { if (this.dragging) this.move(e); });
    const end = () => { this.dragging = false; };
    canvas.addEventListener('pointerup', end); canvas.addEventListener('pointercancel', end);
    canvas.addEventListener('dblclick', e => { const p = this.point(e); this.options.onAdd(this.frequency(p.x), this.gain(p.y)); });
    canvas.addEventListener('wheel', e => { if (this.options.selected()) { e.preventDefault(); this.options.onQ(e.deltaY > 0 ? -0.08 : 0.08); } }, { passive: false });
    const loop = () => { if (this.dirty || this.options.spectrum()) { this.draw(); this.dirty = false; } requestAnimationFrame(loop); }; loop();
  }
  resize() {
    const r = this.canvas.getBoundingClientRect(), scale = devicePixelRatio || 1;
    this.width = r.width; this.height = r.height;
    this.canvas.width = Math.round(r.width * scale); this.canvas.height = Math.round(r.height * scale);
    this.dirty = true;
  }
  x(frequency: number) { return 46 + Math.log(frequency / 10) / Math.log(2000) * (this.width - 74); }
  y(gain: number) { return 22 + (24 - gain) / 48 * (this.height - 58); }
  frequency(x: number) { return clamp(10 * 2000 ** ((x - 46) / (this.width - 74)), 10, 20000); }
  gain(y: number) { return clamp(24 - (y - 22) / (this.height - 58) * 48, -24, 24); }
  point(e: MouseEvent) { const r = this.canvas.getBoundingClientRect(); return { x: e.clientX - r.left, y: e.clientY - r.top }; }
  down(e: PointerEvent) {
    const p = this.point(e), ear = this.options.ear();
    const ears: Ear[] = this.options.view() === 'overlay' ? [ear === 'left' ? 'right' : 'left', ear] : [ear];
    let best: { ear: Ear; b: Band; distance: number } | null = null;
    for (const side of ears) for (const b of this.options.profile().ears[side]) {
      const distance = Math.hypot(p.x - this.x(b.frequency), p.y - this.y(b.gain));
      if (distance < 20 && (!best || distance <= best.distance)) best = { ear: side, b, distance };
    }
    if (best) { this.options.onSelect(best.ear, best.b.id); this.dragging = true; this.canvas.setPointerCapture(e.pointerId); this.dirty = true; }
  }
  move(e: PointerEvent) { const p = this.point(e); this.options.onDrag(Math.round(this.frequency(p.x)), Math.round(this.gain(p.y) * 10) / 10); this.dirty = true; }
  draw() {
    const ctx = this.canvas.getContext('2d')!, scale = devicePixelRatio || 1;
    ctx.setTransform(scale, 0, 0, scale, 0, 0); ctx.clearRect(0, 0, this.width, this.height);
    ctx.font = '11px ui-monospace, monospace';
    for (const g of [-24, -18, -12, -6, 0, 6, 12, 18, 24]) {
      ctx.strokeStyle = g === 0 ? '#414a51' : '#242b31'; ctx.lineWidth = 1;
      ctx.beginPath(); ctx.moveTo(46, this.y(g)); ctx.lineTo(this.width - 28, this.y(g)); ctx.stroke();
      ctx.fillStyle = '#7f8b95'; ctx.fillText(`${g > 0 ? '+' : ''}${g}`, 11, this.y(g) + 4);
    }
    for (const f of [10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
      const x = this.x(f); ctx.strokeStyle = '#242b31'; ctx.beginPath(); ctx.moveTo(x, 22); ctx.lineTo(x, this.height - 36); ctx.stroke();
      ctx.fillStyle = '#7f8b95'; ctx.fillText(f < 1000 ? String(f) : `${f / 1000}k`, x - 10, this.height - 13);
    }
    const spectrum = this.options.spectrum();
    if (spectrum) {
      ctx.beginPath(); ctx.moveTo(46, this.height - 36);
      for (let x = 46; x <= this.width - 28; x += 2) {
        const bin = Math.round(this.frequency(x) / spectrum.rate * spectrum.bins.length * 2);
        const db = spectrum.bins[Math.min(bin, spectrum.bins.length - 1)];
        const y = this.height - 36 - clamp((db + 100) / 100, 0, 1) * (this.height - 58);
        ctx.lineTo(x, y);
      }
      ctx.lineTo(this.width - 28, this.height - 36); ctx.closePath();
      const gradient = ctx.createLinearGradient(0, 22, 0, this.height - 36); gradient.addColorStop(0, '#a6afbf50'); gradient.addColorStop(1, '#66728308');
      ctx.fillStyle = gradient; ctx.fill(); ctx.strokeStyle = '#7e8b9d66'; ctx.stroke();
    }
    const p = this.options.profile(), active = this.options.ear();
    const ears: Ear[] = this.options.view() === 'overlay' ? [active === 'left' ? 'right' : 'left', active] : [active];
    for (const ear of ears) {
      const color = ear === 'left' ? '#79dabd' : '#bc9ff2';
      const rate = this.options.rate();
      const cs = p.ears[ear].map(b => coefficients(b, rate));
      ctx.beginPath();
      for (let x = 46; x <= this.width - 28; x += 2) {
        const y = this.y(cs.reduce((sum, c) => sum + magnitudeDb(c, this.frequency(x), rate), 0));
        if (x === 46) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      }
      ctx.save(); ctx.rect(46, 22, this.width - 74, this.height - 58); ctx.clip();
      ctx.strokeStyle = color; ctx.lineWidth = ear === active ? 2.6 : 1.5; ctx.globalAlpha = ear === active ? 1 : 0.7; ctx.stroke(); ctx.restore();
      p.ears[ear].forEach((b, index) => {
        const selected = ear === active && b.id === this.options.selected();
        const x = this.x(b.frequency), y = this.y(b.gain);
        ctx.beginPath(); ctx.arc(x, y, selected ? 11 : 8, 0, Math.PI * 2);
        ctx.fillStyle = b.enabled ? color : '#505861'; ctx.fill();
        if (selected) { ctx.strokeStyle = '#ffffffaa'; ctx.lineWidth = 2; ctx.stroke(); }
        ctx.fillStyle = '#141a1d'; ctx.font = 'bold 10px ui-monospace, monospace'; ctx.textAlign = 'center'; ctx.fillText(String(index + 1), x, y + 3); ctx.textAlign = 'left';
      });
    }
  }
}
