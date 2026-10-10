import test from 'node:test';
import assert from 'node:assert/strict';
import { Biquad, coefficients, exportApo, exportPipeWire, flatProfile, headroomDb, magnitudeDb, panMatrix, validateProfile } from '../src/core.ts';

test('flat EQ is identity and needs no headroom', () => {
  const p = flatProfile();
  assert.equal(headroomDb(p), 0);
  for (const f of [20, 60, 1000, 10000, 20000]) for (const b of p.ears.left) {
    assert.ok(Math.abs(magnitudeDb(coefficients(b), f)) < 1e-6);
  }
});
test('manual gain is the default and EQ boosts do not silently attenuate either ear', () => {
  const p = flatProfile(); p.gainDb = -6; p.ears.right[2].gain = 9;
  assert.equal(p.autoHeadroom, false);
  assert.equal(headroomDb(p), 0);
  const text = exportApo(p);
  assert.equal((text.match(/Preamp: -6.00 dB/g) || []).length, 2);
  assert.match(text, /Gain 9.00 dB/);
});
test('peaking filter has its requested gain at the center at all supported rates', () => {
  const b = { ...flatProfile().ears.left[2], kind: 'peak' as const, frequency: 2000, gain: 9, q: 3 };
  for (const rate of [22050, 44100, 48000, 96000, 192000]) assert.ok(Math.abs(magnitudeDb(coefficients(b, rate), 2000, rate) - 9) < 1e-7);
});
test('shelves match half the shelf gain at the midpoint', () => {
  for (const kind of ['lowShelf', 'highShelf'] as const) {
    const b = { ...flatProfile().ears.left[0], kind, frequency: 1000, gain: 6, q: 0.7071 };
    assert.ok(Math.abs(magnitudeDb(coefficients(b), 1000) - 3) < 1e-7);
  }
});
test('bypass filters pass the current sample with no delay', () => {
  const b = { ...flatProfile().ears.left[0], enabled: false, gain: 24 };
  const filter = new Biquad(coefficients(b));
  assert.deepEqual([1, 0, -0.5, 0.25].map(x => filter.process(x)), [1, 0, -0.5, 0.25]);
});
test('IIR processes the first sample immediately and remains finite over an impulse tail', () => {
  const b = { ...flatProfile().ears.left[2], gain: -12, q: 3 };
  const c = coefficients(b), filter = new Biquad(c);
  assert.equal(filter.process(1), c.b0);
  let tail = 0;
  for (let i = 0; i < 48000; i++) { tail = filter.process(0); assert.ok(Number.isFinite(tail)); }
  assert.ok(Math.abs(tail) < 1e-20);
});
test('automatic headroom uses the louder ear without equalizing the ear gains', () => {
  const p = flatProfile(); p.autoHeadroom = true; p.ears.right[2].gain = 9;
  assert.ok(headroomDb(p) <= -10);
  const text = exportApo(p);
  assert.equal((text.match(/Preamp: -10.00 dB/g) || []).length, 2);
  assert.match(text, /Gain 9.00 dB/);
  p.autoHeadroom = false; assert.equal(headroomDb(p), 0);
});
test('balance attenuates one ear; true pan retains both inputs at the destination', () => {
  assert.deepEqual(panMatrix(0, 'balance'), [1, 0, 0, 1]);
  assert.deepEqual(panMatrix(-1, 'balance'), [1, 0, 0, 0]);
  assert.deepEqual(panMatrix(1, 'pan'), [0, 0, 1, 1]);
  assert.deepEqual(panMatrix(-1, 'pan'), [1, 1, 0, 0]);
});
test('headroom accounts for correlated channels summed by true pan', () => {
  const p = flatProfile(); p.autoHeadroom = true; p.pan = 1; p.panMode = 'pan';
  assert.ok(headroomDb(p) <= -7);
});
test('exports scope channels correctly and omit all processing on bypass', () => {
  const p = flatProfile(); p.ears.left[1].gain = -3; p.ears.right[1].gain = 4;
  const text = exportApo(p);
  assert.match(text, /Channel: L\nPreamp:.*\nFilter 1: ON LSC/);
  assert.match(text, /Channel: R\nPreamp:/);
  assert.ok(text.endsWith('Channel: ALL\n'));
  assert.doesNotMatch(exportApo(p, false), /Filter \d|Preamp:|Copy:/);
  assert.match(exportPipeWire(p), /libpipewire-module-filter-chain/);
  assert.match(exportPipeWire(p), /eq_l_1/);
  assert.doesNotMatch(exportPipeWire(p, false), /bq_/);
});
test('profile import rejects invalid numbers, duplicates, and unbounded input', () => {
  const p = flatProfile(); assert.deepEqual(validateProfile(p), p);
  p.ears.left[0].gain = NaN; assert.throws(() => validateProfile(p));
  p.ears.left[0].gain = 0; p.ears.left[1].id = p.ears.left[0].id; assert.throws(() => validateProfile(p));
  assert.throws(() => validateProfile({ ...flatProfile(), name: 'x'.repeat(81) }));
});
