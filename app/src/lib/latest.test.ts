import { describe, expect, it } from 'vitest';

import { latest } from './latest';

/** A job that finishes only when the test says so, recording what it ran. */
function controlledJob() {
  const ran: number[] = [];
  const finishers: (() => void)[] = [];
  const job = (input: number) => {
    ran.push(input);
    return new Promise<number>((resolve) =>
      finishers.push(() => {
        resolve(input * 10);
      })
    );
  };
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  const finishNext = async () => {
    finishers.shift()?.();
    await settle();
  };
  return { ran, job, settle, finishNext };
}

describe('latest', () => {
  it('returns the result of a lone run', async () => {
    const run = latest(async (n: number) => Promise.resolve(n + 1));
    await expect(run(1)).resolves.toEqual({ superseded: false, result: 2 });
  });

  it('skips inputs overtaken while waiting, and reports overtaken runs', async () => {
    const { ran, job, settle, finishNext } = controlledJob();
    const run = latest(job);
    const first = run(1);
    await settle();
    const second = run(2);
    const third = run(3);
    await finishNext();
    await finishNext();
    expect(ran).toEqual([1, 3]);
    await expect(first).resolves.toEqual({ superseded: true });
    await expect(second).resolves.toEqual({ superseded: true });
    await expect(third).resolves.toEqual({ superseded: false, result: 30 });
  });

  it('runs only the last of inputs given at once', async () => {
    const { ran, job, finishNext } = controlledJob();
    const run = latest(job);
    const outcomes = [run(1), run(2), run(3)];
    await finishNext();
    await finishNext();
    expect(ran).toEqual([3]);
    await expect(Promise.all(outcomes)).resolves.toEqual([
      { superseded: true },
      { superseded: true },
      { superseded: false, result: 30 }
    ]);
  });

  it('keeps going after a failed run', async () => {
    const run = latest(async (n: number) => {
      if (n === 1) throw new Error('no');
      return Promise.resolve(n);
    });
    await expect(run(1)).rejects.toThrow('no');
    await expect(run(2)).resolves.toEqual({ superseded: false, result: 2 });
  });
});
