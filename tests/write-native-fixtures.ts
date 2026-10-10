// Generate independent TypeScript reference values for the Rust parity tests.
import { writeFileSync } from 'node:fs';
import { coefficients, headroomDb, type Band, type Profile } from '../src/core.ts';
const values = [];
for (const rate of [22050, 44100, 48000, 96000, 192000, 384000]) {
  for (const kind of ['peak', 'lowShelf', 'highShelf'] as const) {
    for (const [frequency, gain, q] of [[10, 24, 30], [1000, -12, 0.1], [20000, 9, 0.7071]]) {
      const band: Band = { id: 'reference', kind, frequency, gain, q, enabled: true };
      values.push({ rate, band, coefficients: coefficients(band, rate) });
    }
  }
}
const profile: Profile = { version: 1, id: 'reference-profile', name: 'Reference',
  ears: { left: [{ id: 'l', kind: 'lowShelf', frequency: 60, gain: -3, q: 0.7071, enabled: true }],
    right: [{ id: 'r', kind: 'peak', frequency: 2000, gain: 9, q: 3, enabled: true }] },
  gainDb: -3, pan: 0.15, panMode: 'balance', autoHeadroom: true };
writeFileSync(new URL('../crates/easy-eq-core/tests/reference.json', import.meta.url),
  JSON.stringify({ coefficients: values, profile, headroom: headroomDb(profile) }, null, 2) + '\n');
