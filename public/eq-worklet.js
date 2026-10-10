// Audition only. The system audio adapters do not use Web Audio.
class EasyEqAuditionProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.current = this.make({ ears: [[], []], matrix: [1, 0, 0, 1], gain: 1 });
    this.previous = null;
    this.fadeRemaining = 0;
    this.port.onmessage = ({ data }) => {
      this.previous = this.current;
      this.current = this.make(data);
      this.fadeRemaining = 240; // 5 ms at 48 kHz; no lookahead or delayed samples.
    };
  }
  make(data) {
    return { matrix: data.matrix, gain: data.gain, ears: data.ears.map(bands => bands.map(c => ({ c, z1: 0, z2: 0 }))) };
  }
  sample(graph, ear, left, right) {
    let x = (graph.matrix[ear * 2] * left + graph.matrix[ear * 2 + 1] * right) * graph.gain;
    const bands = graph.ears[ear];
    for (let i = 0; i < bands.length; i++) {
      const b = bands[i], c = b.c;
      const y = c.b0 * x + b.z1;
      b.z1 = c.b1 * x - c.a1 * y + b.z2;
      b.z2 = c.b2 * x - c.a2 * y;
      x = y;
    }
    return Number.isFinite(x) ? x : 0;
  }
  process(inputs, outputs) {
    const source = inputs[0], target = outputs[0];
    if (!target || !target[0]) return true;
    const left = source && source[0], right = source && (source[1] || source[0]);
    for (let i = 0; i < target[0].length; i++) {
      const l = left ? left[i] : 0, r = right ? right[i] : 0;
      const blend = this.fadeRemaining > 0 ? 1 - this.fadeRemaining / 240 : 1;
      for (let ear = 0; ear < Math.min(target.length, 2); ear++) {
        const next = this.sample(this.current, ear, l, r);
        target[ear][i] = this.previous && this.fadeRemaining > 0
          ? this.sample(this.previous, ear, l, r) * (1 - blend) + next * blend : next;
      }
      if (this.fadeRemaining > 0 && --this.fadeRemaining === 0) this.previous = null;
    }
    return true;
  }
}
registerProcessor('easy-eq-audition', EasyEqAuditionProcessor);
