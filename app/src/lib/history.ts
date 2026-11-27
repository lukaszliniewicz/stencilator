/**
 * Undo and redo over whole snapshots. Recording a state equal to the present one does
 * nothing, so callers may record freely; recording anything new forgets what could have
 * been redone.
 */
export class History<T> {
  #past: T[] = [];
  #future: T[] = [];
  #present: T;
  readonly #equal: (a: T, b: T) => boolean;
  readonly #limit: number;

  constructor(present: T, equal: (a: T, b: T) => boolean, limit = 200) {
    this.#present = present;
    this.#equal = equal;
    this.#limit = limit;
  }

  get present(): T {
    return this.#present;
  }

  get canUndo(): boolean {
    return this.#past.length > 0;
  }

  get canRedo(): boolean {
    return this.#future.length > 0;
  }

  some(matches: (snapshot: T) => boolean): boolean {
    return matches(this.#present) || this.#past.some(matches) || this.#future.some(matches);
  }

  record(next: T): void {
    if (this.#equal(next, this.#present)) return;
    this.#past.push(this.#present);
    if (this.#past.length > this.#limit) this.#past.shift();
    this.#present = next;
    this.#future = [];
  }

  /** Steps back and returns the state to show, or nothing when there is no earlier one. */
  undo(): T | undefined {
    const previous = this.#past.pop();
    if (previous === undefined) return undefined;
    this.#future.push(this.#present);
    this.#present = previous;
    return previous;
  }

  redo(): T | undefined {
    const next = this.#future.pop();
    if (next === undefined) return undefined;
    this.#past.push(this.#present);
    this.#present = next;
    return next;
  }
}
