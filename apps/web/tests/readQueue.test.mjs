import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createReadQueue } from '../src/readQueue.ts';

test('serializes reads and continues after a failed request', async () => {
  const queue = createReadQueue(1);
  let active = 0;
  let peak = 0;
  const results = await Promise.allSettled([0, 1, 2].map(index => queue(async () => {
    peak = Math.max(peak, ++active);
    await new Promise(resolve => setTimeout(resolve, 5));
    active--;
    if (index === 1) throw new Error('offline');
    return index;
  })));
  assert.equal(peak, 1);
  assert.deepEqual(results.map(result => result.status), ['fulfilled', 'rejected', 'fulfilled']);
  assert.equal(results[2].value, 2);
});
