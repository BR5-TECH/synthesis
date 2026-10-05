/**
 * Coalescing the reloads a run's own progress asks for
 * (`../../../specifications/core/GRD-graduation.md` GRD-FR-EFAU).
 *
 * One turn emits many `"graduation run changed"` and `"graduation queue
 * changed"` events — a stage entered, a state moved, a checkpoint written — and
 * each one is durable before it is emitted, so each is a real change. Reading
 * the whole queue once per event reads every run record the project has ever
 * made, several times a second, for a listing that cannot be seen to change
 * that fast.
 *
 * The reader reloads its own listing off an event rather than reading a queue
 * out of the payload, so a reload that stands for three events answers all
 * three (GRD-FR-EFAU).
 */

/** How long a burst of events is gathered before the listing is read. */
export const COALESCE_MS = 120;

/** A reload that runs at most once per window, with a way to stop it. */
export interface Coalesced {
  /** Ask for a reload. A call inside the window joins the one already armed. */
  ask: () => void;
  /** Drop whatever is armed. Safe to call more than once. */
  cancel: () => void;
}

/**
 * Gather the reloads asked for inside one window into a single call.
 *
 * The **first** ask arms the window rather than restarting it, so a stream of
 * events reloads steadily instead of being starved until the events stop —
 * which is the failure a plain debounce has here, where a working run emits
 * events for as long as it works.
 */
export function coalesce(reload: () => void, windowMs = COALESCE_MS): Coalesced {
  let armed: ReturnType<typeof setTimeout> | null = null;
  // Both callers unsubscribe asynchronously, so an event can arrive after the
  // cleanup ran. Cancelling therefore closes the coalescer for good rather than
  // only dropping what is armed: a later ask would otherwise reload an
  // unmounted surface.
  let closed = false;
  return {
    ask: () => {
      if (closed || armed !== null) return;
      armed = setTimeout(() => {
        armed = null;
        if (!closed) reload();
      }, windowMs);
    },
    cancel: () => {
      closed = true;
      if (armed === null) return;
      clearTimeout(armed);
      armed = null;
    },
  };
}
