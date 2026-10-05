/**
 * The editing-activity observer
 * (`specifications/ui/DFI-draft-information.md` DFI-FR-XKRM, DFI-FR-UKFR).
 *
 * What produces the time the Information modal shows. It runs whether or not the
 * modal is open, holds at most one open interval **per draft**, and reports each
 * settled interval whole — it knows nothing of a draft's telemetry activation
 * boundary and applies none, the clip being the backend fold's (per
 * `specifications/core/DSS-draft-statistics-storage.md` DSS-FR-KYTB).
 *
 * An interval opens when all three hold: the application has focus, a **related
 * surface** of that draft is visible, and the author has interacted with the
 * application. It settles when the application loses focus, when no related
 * surface is visible any longer, or when **five minutes pass with no author
 * interaction**. The interval stays open through the whole of that idle period
 * and ends **at the five-minute mark** rather than at the last interaction
 * before it: an author reading what is on screen without touching anything is
 * working, so the threshold is what decides they have stopped.
 *
 * It reconstructs nothing (DFI-FR-UKFR): no interval is ever derived from the
 * number of times a tab or the modal was opened, from a surface's mount, or from
 * the age of a draft, and an interval it never settled — the application closing
 * mid-interval among them — is reported not at all rather than guessed at.
 */
import { recordDraftEditingInterval } from "./api";
import { logWarn } from "./logging";

/** DFI-FR-XKRM: the inactivity threshold, and the whole of what settles an idle interval. */
export const IDLE_THRESHOLD_MS = 5 * 60 * 1000;

/** One draft's open interval. */
interface OpenInterval {
  startedAt: number;
  /** When the last author interaction landed, which is what the threshold runs from. */
  lastInteractionAt: number;
}

export interface EditingObserverOptions {
  /** Report one settled interval. Fire-and-forget on the caller's side too. */
  report?: (draftId: string, startedAt: string, endedAt: string) => void;
  /** The clock, so a test can drive the threshold without waiting five minutes. */
  now?: () => number;
}

/**
 * The observer, as an object the shell owns for the life of the window.
 *
 * Deliberately a plain object rather than a hook: the signals it reads —
 * application focus and author interaction — are window-level, and a hook would
 * tie the count to whichever component happened to mount it.
 */
export class DraftEditingObserver {
  private readonly open = new Map<string, OpenInterval>();
  /** Which drafts have a related surface visible right now (DFI-FR-YAOM). */
  private visible = new Set<string>();
  private focused = true;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readonly now: () => number;
  private readonly report: (draftId: string, startedAt: string, endedAt: string) => void;

  constructor(options: EditingObserverOptions = {}) {
    this.now = options.now ?? (() => Date.now());
    this.report =
      options.report ??
      ((draftId, startedAt, endedAt) => {
        // The report is fire-and-forget: a failed or slow call never delays the
        // author, blocks a surface, or produces a visible error.
        void recordDraftEditingInterval(draftId, startedAt, endedAt).catch((e) =>
          logWarn(["frontend"], "a draft editing interval could not be recorded", {
            draftId,
            reason: String(e),
          }),
        );
      });
  }

  /**
   * DFI-FR-JDWS: which drafts have a related surface visible.
   *
   * The whole set at once rather than one surface at a time, because two related
   * surfaces of one draft hold **one** interval however many of them the author
   * has in front of them — the caller collapses its surfaces to the drafts they
   * are about, and this counts drafts.
   */
  setVisibleDrafts(draftIds: Iterable<string>): void {
    this.visible = new Set(draftIds);
    for (const draftId of [...this.open.keys()]) {
      if (!this.visible.has(draftId)) this.settle(draftId);
    }
    this.arm();
  }

  /** DFI-FR-XKRM: the application gained or lost focus. */
  setFocused(focused: boolean): void {
    if (this.focused === focused) return;
    this.focused = focused;
    if (!focused) {
      // A conversation tab counts only while the application has focus: a tab of
      // an unfocused application is something the author is not at.
      for (const draftId of [...this.open.keys()]) this.settle(draftId);
    }
    this.arm();
  }

  /**
   * DFI-FR-XKRM: the author interacted with the application.
   *
   * This is what opens an interval for every visible draft that has none, and
   * what restarts the five minutes for one that is already open — an
   * interaction arriving before the threshold settles nothing.
   */
  interacted(): void {
    if (!this.focused) return;
    const at = this.now();
    for (const draftId of this.visible) {
      const open = this.open.get(draftId);
      if (open) open.lastInteractionAt = at;
      else this.open.set(draftId, { startedAt: at, lastInteractionAt: at });
    }
    this.arm();
  }

  /** Whether a draft has an interval open right now, for a test to read. */
  isOpen(draftId: string): boolean {
    return this.open.has(draftId);
  }

  /**
   * Settle every interval whose author has been idle for the threshold.
   *
   * Driven by the single timer below in production; a test calls it directly
   * after moving its own clock.
   */
  sweep(): void {
    const at = this.now();
    for (const [draftId, open] of [...this.open]) {
      if (at - open.lastInteractionAt >= IDLE_THRESHOLD_MS) {
        // The interval ends **at the threshold**, not at the last interaction
        // before it: the time up to the threshold is counted and the time after
        // it is not.
        this.settle(draftId, open.lastInteractionAt + IDLE_THRESHOLD_MS);
      }
    }
    this.arm();
  }

  /** Release the timer. Any interval still open is reported not at all. */
  dispose(): void {
    if (this.timer !== undefined) clearTimeout(this.timer);
    this.timer = undefined;
    this.open.clear();
  }

  /** Settle one draft's interval and report it whole (DFI-FR-UKFR). */
  private settle(draftId: string, endedAtMs?: number): void {
    const open = this.open.get(draftId);
    if (!open) return;
    this.open.delete(draftId);
    const ended = endedAtMs ?? this.now();
    // An interval of no duration is not an interval: nothing was spent, so
    // nothing is reported.
    if (ended <= open.startedAt) return;
    this.report(draftId, new Date(open.startedAt).toISOString(), new Date(ended).toISOString());
  }

  /** One timer per open interval set, and none at all when nothing is open. */
  private arm(): void {
    if (this.timer !== undefined) {
      clearTimeout(this.timer);
      this.timer = undefined;
    }
    if (this.open.size === 0) return;
    const at = this.now();
    const soonest = Math.min(
      ...[...this.open.values()].map((open) => open.lastInteractionAt + IDLE_THRESHOLD_MS - at),
    );
    this.timer = setTimeout(() => this.sweep(), Math.max(0, soonest));
  }
}
