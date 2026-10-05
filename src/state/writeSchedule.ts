/**
 * EDT-FR-70 / FLO-FR-27 / NAW-FR-13: the rest a document takes after the last
 * edit before it writes itself.
 *
 * One implementation for artifacts, plain text files and Flows, because it is
 * one promise made about three kinds of file: an edit schedules a write a short
 * rest from now, a further edit restarts that rest so a burst of typing is one
 * write, and any route that brings the write forward — a tab, project, or
 * application close, or the File menu's Save — spends the schedule so the rest
 * does not fire again behind it.
 *
 * A schedule belongs to the document's **store** rather than to the surface
 * showing it. Only the active tab's Editor or canvas is mounted (EDT-FR-30,
 * FLO-FR-30), so a timer the component owned would be torn down the moment the
 * author switched tabs and the edit they had just made would sit unwritten
 * until they came back.
 */

/**
 * How long a document rests after the last edit before it is written. Long
 * enough that a sentence is one write rather than thirty, short enough that a
 * pause is a save.
 */
export const AUTOSAVE_DELAY_MS = 700;

/**
 * Every scheduled write currently outstanding, across every store, as the
 * function that drops it.
 *
 * Holds nothing while nothing is scheduled — an entry is removed when its write
 * fires or is cancelled — so this is a registry of pending work rather than of
 * stores. It exists so a test process can put down whatever a store it abandoned
 * had scheduled (`cancelAllScheduledWrites`); the application itself never needs
 * it, because its stores are cleared when the project closes.
 */
const outstanding = new Set<() => void>();

/**
 * Drop every scheduled write, whichever store holds it, without performing any
 * of them.
 *
 * For a test that abandons a store rather than clearing it: the rest is a real
 * timer, so a write left scheduled by one test lands in the middle of the next
 * one and writes through whatever backend mock is installed by then.
 */
export function cancelAllScheduledWrites(): void {
  [...outstanding].forEach((cancel) => cancel());
}

/** The rest-after-editing schedule of one store's documents, keyed by id. */
export class WriteSchedule {
  /**
   * The outstanding writes, each held as the function that drops it. A closure
   * rather than the raw handle, so cancelling by id and cancelling globally are
   * the same operation and neither can leave the other's bookkeeping stale.
   */
  private drops = new Map<string, () => void>();

  /**
   * @param perform what to do when a document's rest elapses — the store's own
   *   write path, so every route to a write goes through the same guards.
   * @param enabled off for a store whose documents are written on someone
   *   else's schedule (a draft file's is its New Artifact tab's, NAW-FR-13).
   *   Two schedules over one buffer would write it twice.
   */
  constructor(
    private readonly perform: (id: string) => void,
    readonly enabled = true,
  ) {}

  /** Schedule `id`'s write for a rest from now, restarting any rest running. */
  schedule(id: string): void {
    if (!this.enabled) return;
    this.cancel(id);
    const timer = setTimeout(() => {
      drop();
      this.perform(id);
    }, AUTOSAVE_DELAY_MS);
    const drop = () => {
      clearTimeout(timer);
      // Only if it is still ours: a later `schedule` for the same id has
      // already replaced the entry, and dropping it would lose that write.
      if (this.drops.get(id) === drop) this.drops.delete(id);
      outstanding.delete(drop);
    };
    this.drops.set(id, drop);
    outstanding.add(drop);
  }

  /** Drop `id`'s scheduled write without performing it. */
  cancel(id: string): void {
    this.drops.get(id)?.();
  }

  /** Whether `id` holds a write whose rest has not yet elapsed. */
  has(id: string): boolean {
    return this.drops.has(id);
  }

  /** Move a scheduled write to a new id, for a document that was renamed. */
  move(from: string, to: string): void {
    if (!this.drops.has(from)) return;
    this.cancel(from);
    this.schedule(to);
  }

  /** Drop every scheduled write this store holds. */
  cancelAll(): void {
    [...this.drops.values()].forEach((drop) => drop());
  }
}
