import { describe, expect, it } from 'vitest';
import { CoverQueue } from './coverQueue';

/** A queue whose starts are recorded and finished by hand. */
function harness(limit: number) {
  const queue = new CoverQueue(limit);
  const started: number[] = [];
  const finish = new Map<number, () => void>();
  const ask = (priority: number) => queue.request(priority, (done) => {
    started.push(priority);
    finish.set(priority, done);
  });
  return { queue, started, finish, ask };
}

describe('cover queue', () => {
  it('never runs more than its limit and goes on as images settle', () => {
    const { queue, started, finish, ask } = harness(2);
    [5, 1, 9, 3].forEach(ask);
    expect(started).toEqual([5, 1]);
    expect(queue.size).toEqual({ waiting: 2, running: 2 });
    finish.get(5)!();
    expect(started).toEqual([5, 1, 3]);
    finish.get(5)!(); // twice is once
    expect(queue.size).toEqual({ waiting: 1, running: 2 });
    finish.get(1)!();
    finish.get(3)!();
    expect(started).toEqual([5, 1, 3, 9]);
  });

  it('serves the lowest position first, whatever order they were asked in', () => {
    const { started, finish, ask } = harness(1);
    ask(40);
    [12, 30, 2, 7].forEach(ask);
    for (const priority of [40, 2, 7, 12]) finish.get(priority)!();
    expect(started).toEqual([40, 2, 7, 12, 30]);
  });

  it('gives up the place of a cover that left the screen before its turn', () => {
    const { started, finish, ask } = harness(1);
    ask(0);
    const gone = ask(1);
    ask(2);
    gone.cancel();
    finish.get(0)!();
    expect(started).toEqual([0, 2]);
  });

  it('keeps going when a start throws', () => {
    const queue = new CoverQueue(1);
    const started: number[] = [];
    queue.request(0, () => { throw new Error('boom'); });
    queue.request(1, () => { started.push(1); });
    expect(started).toEqual([1]);
  });
});
