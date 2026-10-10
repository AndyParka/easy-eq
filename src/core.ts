// Portable, minimum-phase EQ model. The visualizer never sits in the audio path.
export type Ear = 'left' | 'right';
export type FilterKind = 'peak' | 'lowShelf' | 'highShelf';
export type Band = { id: string; kind: FilterKind; frequency: number; gain: number; q: number; enabled: boolean };
export type Profile = {
  version: 1;
  id: string;
  name: string;
  ears: Record<Ear, Band[]>;
  gainDb: number;
  pan: number;
  panMode: 'balance' | 'pan';
  autoHeadroom: boolean;
};
export type Coefficients = { b0: number; b1: number; b2: number; a1: number; a2: number };
export const LIMITS = { frequency: [10, 20000], gain: [-24, 24], q: [0.1, 30], bands: 31 } as const;
export const clamp = (n: number, low: number, high: number) => Math.max(low, Math.min(high, n));
export const dbToGain = (db: number) => 10 ** (db / 20);
export const uid = () => crypto.randomUUID();

function numeric(value: unknown, low: number, high: number, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < low || value > high) {
    throw new Error(`${label} must be a finite number between ${low} and ${high}.`);
  }
  return value;
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid profile structure.');
  return value as Record<string, unknown>;
}
export function validateProfile(value: unknown): Profile {
  const p = object(value);
  if (p.version !== 1 || typeof p.id !== 'string' || !p.id || p.id.length > 100 ||
      typeof p.name !== 'string' || !p.name.trim() || p.name.length > 80) throw new Error('Invalid profile identity or version.');
  if (typeof p.autoHeadroom !== 'boolean' || !['balance', 'pan'].includes(String(p.panMode))) throw new Error('Invalid output settings.');
  const ears = object(p.ears);
  const readBands = (ear: Ear): Band[] => {
    const list = ears[ear];
    if (!Array.isArray(list) || list.length > LIMITS.bands) throw new Error(`Invalid ${ear} bands.`);
    const ids = new Set<string>();
    return list.map(value => {
      const b = object(value);
      if (typeof b.id !== 'string' || !b.id || b.id.length > 100 || ids.has(b.id) ||
          !['peak', 'lowShelf', 'highShelf'].includes(String(b.kind)) || typeof b.enabled !== 'boolean') throw new Error('Invalid or duplicate EQ band.');
      ids.add(b.id);
      return {
        id: b.id, kind: b.kind as FilterKind, enabled: b.enabled,
        frequency: numeric(b.frequency, ...LIMITS.frequency, 'Frequency'),
        gain: numeric(b.gain, ...LIMITS.gain, 'Band gain'), q: numeric(b.q, ...LIMITS.q, 'Q'),
      };
    });
  };
  return {
    version: 1, id: p.id, name: p.name.trim(), ears: { left: readBands('left'), right: readBands('right') },
    gainDb: numeric(p.gainDb, -60, 0, 'Output gain'), pan: numeric(p.pan, -1, 1, 'Pan'),
    panMode: p.panMode as Profile['panMode'], autoHeadroom: p.autoHeadroom,
  };
}
export function flatProfile(name = 'My headphones'): Profile {
  const bands: Band[] = [60, 180, 550, 1600, 4500, 10000].map((frequency, i) => ({
    id: `band-${i + 1}`, kind: i === 0 ? 'lowShelf' : i === 5 ? 'highShelf' : 'peak',
    frequency, gain: 0, q: i === 0 || i === 5 ? Math.SQRT1_2 : 1.4, enabled: true,
  }));
  return { version: 1, id: uid(), name, ears: { left: structuredClone(bands), right: structuredClone(bands) },
    gainDb: 0, pan: 0, panMode: 'balance', autoHeadroom: false };
}

// RBJ cookbook coefficients; shelf frequency is the shelf midpoint (APO LSC/HSC).
export function coefficients(b: Band, sampleRate = 48000): Coefficients {
  numeric(sampleRate, 8000, 768000, 'Sample rate');
  if (!b.enabled) return { b0: 1, b1: 0, b2: 0, a1: 0, a2: 0 };
  const w = 2 * Math.PI * Math.min(b.frequency, sampleRate * 0.49) / sampleRate;
  const c = Math.cos(w), s = Math.sin(w), A = 10 ** (b.gain / 40), alpha = s / (2 * b.q);
  let b0: number, b1: number, b2: number, a0: number, a1: number, a2: number;
  if (b.kind === 'peak') {
    b0 = 1 + alpha * A; b1 = -2 * c; b2 = 1 - alpha * A;
    a0 = 1 + alpha / A; a1 = -2 * c; a2 = 1 - alpha / A;
  } else {
    const t = 2 * Math.sqrt(A) * alpha;
    if (b.kind === 'lowShelf') {
      b0 = A * ((A + 1) - (A - 1) * c + t); b1 = 2 * A * ((A - 1) - (A + 1) * c);
      b2 = A * ((A + 1) - (A - 1) * c - t); a0 = (A + 1) + (A - 1) * c + t;
      a1 = -2 * ((A - 1) + (A + 1) * c); a2 = (A + 1) + (A - 1) * c - t;
    } else {
      b0 = A * ((A + 1) + (A - 1) * c + t); b1 = -2 * A * ((A - 1) + (A + 1) * c);
      b2 = A * ((A + 1) + (A - 1) * c - t); a0 = (A + 1) - (A - 1) * c + t;
      a1 = 2 * ((A - 1) - (A + 1) * c); a2 = (A + 1) - (A - 1) * c - t;
    }
  }
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
}
export function magnitudeDb(c: Coefficients, frequency: number, sampleRate = 48000): number {
  const w = 2 * Math.PI * frequency / sampleRate;
  const c1 = Math.cos(w), s1 = Math.sin(w), c2 = Math.cos(2 * w), s2 = Math.sin(2 * w);
  const nr = c.b0 + c.b1 * c1 + c.b2 * c2, ni = -c.b1 * s1 - c.b2 * s2;
  const dr = 1 + c.a1 * c1 + c.a2 * c2, di = -c.a1 * s1 - c.a2 * s2;
  return 10 * Math.log10(Math.max(1e-20, (nr * nr + ni * ni) / (dr * dr + di * di)));
}
export function response(bands: Band[], frequency: number, sampleRate = 48000) {
  return bands.reduce((sum, b) => sum + magnitudeDb(coefficients(b, sampleRate), frequency, sampleRate), 0);
}
// Estimated steady-state headroom with 1 dB margin, not a brickwall limiter.
// A shared reduction preserves the intentional difference between ears.
export function headroomDb(profile: Profile, sampleRate = 48000): number {
  if (!profile.autoHeadroom) return 0;
  let peak = 0;
  const m = panMatrix(profile.pan, profile.panMode);
  const mixBoost = 20 * Math.log10(Math.max(1, m[0] + m[1], m[2] + m[3]));
  const upper = sampleRate * 0.499;
  const points = Array.from({ length: 2049 }, (_, i) => 10 * (upper / 10) ** (i / 2048));
  for (const ear of ['left', 'right'] as const) {
    const cs = profile.ears[ear].map(b => coefficients(b, sampleRate));
    for (const f of [0, sampleRate / 2, ...points, ...profile.ears[ear].filter(b => b.enabled).map(b => Math.min(b.frequency, sampleRate * 0.49))]) {
      peak = Math.max(peak, mixBoost + cs.reduce((sum, c) => sum + magnitudeDb(c, f, sampleRate), 0));
    }
  }
  return peak > 0.01 ? -Math.ceil((peak + 1) * 10) / 10 : 0;
}
// Matrix order [LL, LR, RL, RR]. True pan moves both channels before per-ear EQ.
// True pan can sum correlated inputs; automatic headroom includes that boost.
export function panMatrix(pan: number, mode: Profile['panMode']): [number, number, number, number] {
  const p = clamp(pan, -1, 1);
  if (mode === 'balance') return [p > 0 ? 1 - p : 1, 0, 0, p < 0 ? 1 + p : 1];
  return p < 0 ? [1, -p, 0, 1 + p] : [1 - p, 0, p, 1];
}
export function exportApo(value: Profile, enabled = true, sampleRate = 48000): string {
  const p = validateProfile(value);
  const lines = ['# Easy EQ | minimum-phase EQ | no added processing buffer', '# Generated fragment: include only for your selected stereo output.'];
  if (!enabled) return [...lines, '# Bypassed: identity processing.', 'Channel: ALL', ''].join('\n');
  const matrix = panMatrix(p.pan, p.panMode);
  lines.push('Channel: L R', `Copy: L=${matrix[0].toFixed(8)}*L+${matrix[1].toFixed(8)}*R R=${matrix[2].toFixed(8)}*L+${matrix[3].toFixed(8)}*R`);
  const gain = p.gainDb + headroomDb(p, sampleRate);
  for (const ear of ['left', 'right'] as const) {
    lines.push(`Channel: ${ear === 'left' ? 'L' : 'R'}`, `Preamp: ${gain.toFixed(2)} dB`);
    p.ears[ear].filter(b => b.enabled).forEach((b, i) => {
      const kind = { peak: 'PK', lowShelf: 'LSC', highShelf: 'HSC' }[b.kind];
      lines.push(`Filter ${i + 1}: ON ${kind} Fc ${Math.min(b.frequency, sampleRate * 0.49).toFixed(2)} Hz Gain ${b.gain.toFixed(2)} dB Q ${b.q.toFixed(4)}`);
    });
  }
  return [...lines, 'Channel: ALL', ''].join('\n');
}

// Standalone PipeWire graph export. No changes to the user's routing or files.
export function exportPipeWire(value: Profile, enabled = true, sampleRate = 48000): string {
  const p = validateProfile(value), m = enabled ? panMatrix(p.pan, p.panMode) : [1, 0, 0, 1];
  const gain = enabled ? dbToGain(p.gainDb + headroomDb(p, sampleRate)) : 1;
  const nodes: string[] = [], links: string[] = [], outputs: string[] = [];
  nodes.push('{ type = builtin name = input_l label = copy }', '{ type = builtin name = input_r label = copy }');
  for (const [i, ear] of (['left', 'right'] as const).entries()) {
    const stem = ear === 'left' ? 'l' : 'r';
    nodes.push(`{ type = builtin name = mix_${stem} label = mixer control = { "Gain 1" = ${(m[i * 2] * gain).toFixed(8)} "Gain 2" = ${(m[i * 2 + 1] * gain).toFixed(8)} } }`);
    links.push(`{ output = "input_l:Out" input = "mix_${stem}:In 1" }`, `{ output = "input_r:Out" input = "mix_${stem}:In 2" }`);
    let previous = `mix_${stem}:Out`;
    (enabled ? p.ears[ear] : []).filter(b => b.enabled).forEach((b, j) => {
      const label = { peak: 'bq_peaking', lowShelf: 'bq_lowshelf', highShelf: 'bq_highshelf' }[b.kind];
      const name = `eq_${stem}_${j}`;
      nodes.push(`{ type = builtin name = ${name} label = ${label} control = { "Freq" = ${Math.min(b.frequency, sampleRate * 0.49).toFixed(2)} "Q" = ${b.q.toFixed(4)} "Gain" = ${b.gain.toFixed(2)} } }`);
      links.push(`{ output = "${previous}" input = "${name}:In" }`); previous = `${name}:Out`;
    });
    outputs.push(`"${previous}"`);
  }
  return `# Easy EQ: PipeWire stereo filter graph. Hardware routing still needs configuration.\ncontext.modules = [\n { name = libpipewire-module-filter-chain\n   args = {\n     node.description = "Easy EQ"\n     filter.graph = {\n       nodes = [\n         ${nodes.join('\n         ')}\n       ]\n       links = [\n         ${links.join('\n         ')}\n       ]\n       inputs = [ "input_l:In" "input_r:In" ]\n       outputs = [ ${outputs.join(' ')} ]\n     }\n     capture.props = { node.name = "easy_eq_input" media.class = "Audio/Sink" audio.channels = 2 audio.position = [ FL FR ] }\n     playback.props = { node.name = "easy_eq_output" node.passive = true audio.channels = 2 audio.position = [ FL FR ] }\n   }\n }\n]\n`;
}

// Reference processor for offline tests. Production DSP belongs in the native audio callback.
export class Biquad {
  c: Coefficients;
  z1 = 0;
  z2 = 0;
  constructor(c: Coefficients) { this.c = c; }
  process(x: number): number {
    const y = this.c.b0 * x + this.z1;
    this.z1 = this.c.b1 * x - this.c.a1 * y + this.z2;
    this.z2 = this.c.b2 * x - this.c.a2 * y;
    return y;
  }
}
