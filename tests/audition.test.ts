import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

type Processor = { port: { onmessage: (message: { data: unknown }) => void }; process: (inputs: Float32Array[][], outputs: Float32Array[][]) => boolean };
function createProcessor(): Processor {
  let Constructor: new () => Processor;
  runInNewContext(readFileSync(new URL('../public/eq-worklet.js', import.meta.url), 'utf8'), {
    AudioWorkletProcessor: class { port = { onmessage: null }; },
    registerProcessor: (_name: string, cls: new () => Processor) => { Constructor = cls; },
  });
  return new Constructor!();
}
test('audition stereo identity returns the current samples and preserves channels', () => {
  const p = createProcessor(), l = new Float32Array([1, 0, -0.25, 0]), r = new Float32Array([0, 0.5, 0, -1]);
  const out = [new Float32Array(4), new Float32Array(4)]; p.process([[l, r]], [out]);
  assert.deepEqual(out[0], l); assert.deepEqual(out[1], r);
});
test('audition graph transition settles to the selected balance and does not mix ears', () => {
  const p = createProcessor();
  p.port.onmessage({ data: { ears: [[], []], gain: 0.5, matrix: [0.5, 0, 0, 1] } });
  const input = [new Float32Array(512).fill(1), new Float32Array(512)];
  const output = [new Float32Array(512), new Float32Array(512)]; p.process([input], [output]);
  assert.equal(output[0][511], 0.25); assert.ok(output[1].every(x => x === 0));
  assert.ok(output[0].every(x => x >= 0.25 && x <= 1));
});
test('audition handles mono input and silence without invalid numbers', () => {
  const p = createProcessor(), output = [new Float32Array(128), new Float32Array(128)];
  p.process([[new Float32Array(128).fill(0.1)]], [output]); assert.deepEqual(output[0], output[1]);
  p.process([[]], [output]); assert.ok(output[0].every(x => x === 0));
});
