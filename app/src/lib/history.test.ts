import { describe, expect, it } from 'vitest';

import { History } from './history';

const same = (a: number, b: number) => a === b;

describe('history', () => {
  it('undoes and redoes in order', () => {
    const history = new History<number>(1, same);
    history.record(2);
    history.record(3);
    expect(history.undo()).toBe(2);
    expect(history.undo()).toBe(1);
    expect(history.undo()).toBeUndefined();
    expect(history.redo()).toBe(2);
    expect(history.redo()).toBe(3);
    expect(history.redo()).toBeUndefined();
    expect(history.present).toBe(3);
  });

  it('ignores a state equal to the present one', () => {
    const history = new History<number>(1, same);
    history.record(1);
    expect(history.canUndo).toBe(false);
  });

  it('forgets the redo trail when something new is recorded', () => {
    const history = new History<number>(1, same);
    history.record(2);
    history.undo();
    expect(history.canRedo).toBe(true);
    history.record(5);
    expect(history.canRedo).toBe(false);
    expect(history.undo()).toBe(1);
  });

  it('keeps no more than its limit', () => {
    const history = new History<number>(0, same, 3);
    for (let i = 1; i <= 10; i++) history.record(i);
    expect([history.undo(), history.undo(), history.undo(), history.undo()]).toEqual([
      9,
      8,
      7,
      undefined
    ]);
  });
});
