/**
 * Single-flight runs of one read, with one trailing run.
 *
 * STB-FR-GAYM, CHG-FR-RUAG, DFV-FR-HTXT: a `"changes updated"` burst, such as
 * the one a submodule clone makes, must not start one backend read for each
 * event. At most one run of the task is in flight. A request that arrives
 * during a run marks the runner dirty, and exactly one more run starts after
 * the current one ends, however many requests arrived. The trailing run starts
 * after the last request, so its result reflects every change that was asked
 * about.
 */
export interface CoalescedRun {
  /** Start a run now, or mark one to follow the run in flight. */
  run(): void;
  /** Start no more runs. A run in flight ends, and nothing follows it. */
  dispose(): void;
}

export function coalescedRun(task: () => Promise<unknown> | unknown): CoalescedRun {
  let running = false;
  let dirty = false;
  let disposed = false;

  const start = () => {
    running = true;
    dirty = false;
    // The task starts now, in the call that asked for it. A task that throws,
    // or rejects, still ends the run: the caller's task owns its own error
    // handling, and the runner must never stay "running".
    let outcome: Promise<unknown> | unknown;
    try {
      outcome = task();
    } catch {
      outcome = undefined;
    }
    void Promise.resolve(outcome)
      .catch(() => undefined)
      .then(() => {
        running = false;
        if (dirty && !disposed) start();
      });
  };

  return {
    run() {
      if (disposed) return;
      if (running) {
        dirty = true;
        return;
      }
      start();
    },
    dispose() {
      disposed = true;
      dirty = false;
    },
  };
}
