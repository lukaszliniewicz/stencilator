/**
 * Runs a job for the newest input only. While one run is under way, further inputs wait,
 * and each replaces the one waiting before it; a run whose input has been overtaken
 * reports `superseded` instead of its result. Settings that change faster than the
 * engine can build therefore never queue up stale work, and only the newest input's
 * failure is reported.
 */
export type Outcome<R> = { superseded: false; result: R } | { superseded: true };

export function latest<T, R>(run: (input: T) => Promise<R>): (input: T) => Promise<Outcome<R>> {
  let generation = 0;
  let running: Promise<unknown> = Promise.resolve();

  return (input) => {
    const mine = ++generation;
    const result = running.then(async (): Promise<Outcome<R>> => {
      if (mine !== generation) return { superseded: true };
      try {
        const value = await run(input);
        return mine === generation ? { superseded: false, result: value } : { superseded: true };
      } catch (error) {
        // Only the newest input's failure matters.
        if (mine !== generation) return { superseded: true };
        throw error;
      }
    });
    // A failed run must not stop the next one.
    running = result.catch(() => undefined);
    return result;
  };
}
