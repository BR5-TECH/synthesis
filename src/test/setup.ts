import { afterEach, vi } from "vitest";

import "@testing-library/jest-dom/vitest";

import { cancelAllScheduledWrites } from "../state/writeSchedule";

/**
 * jsdom implements `Element.getClientRects` and **not** `Range.getClientRects`,
 * which is a gap rather than a difference of opinion: the CSSOM view spec
 * defines both, and code that measures a text selection reaches for the Range
 * one.
 *
 * ProseMirror does exactly that. Its `DOMObserver` listens for `selectionchange`
 * on the document, and a flush that decides the selection moved calls
 * `scrollToSelection` → `coordsAtPos` → `singleRect(range)` → `getClientRects()`
 * — which throws `TypeError: target.getClientRects is not a function`. The throw
 * happens inside a DOM event listener rather than inside a test's own call
 * stack, so Vitest cannot attribute it to a test: it surfaces as an **unhandled
 * error**, which fails the run with every assertion passing. That is the worst
 * shape a failure can take, and it is why this is polyfilled rather than
 * suppressed.
 *
 * Zero rects are the honest answer here. jsdom performs no layout at all, so its
 * `Element.getBoundingClientRect` already returns zeros; a Range reporting the
 * same is consistent with the rest of the environment rather than a fiction
 * invented for this one call. ProseMirror reads the zeros, computes a scroll
 * offset of zero, and scrolls nothing — which is the correct outcome in a
 * document that has no viewport.
 *
 * Layout-dependent behaviour is therefore still not testable under jsdom, and
 * this does not pretend otherwise: it is what the browser verification pass
 * exists for. What this buys is that a test which merely *moves a caret* no
 * longer takes the whole suite down.
 */
function installRangeRectPolyfill(): void {
  if (typeof Range === "undefined") return;
  const proto = Range.prototype as Range & {
    getClientRects?: () => DOMRectList;
    getBoundingClientRect?: () => DOMRect;
  };
  const zeroRect = (): DOMRect =>
    typeof DOMRect === "function"
      ? new DOMRect(0, 0, 0, 0)
      : ({
          x: 0,
          y: 0,
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          width: 0,
          height: 0,
          toJSON: () => ({}),
        } as DOMRect);

  if (typeof proto.getClientRects !== "function") {
    // Array-like and empty: `singleRect` reads `.length`, indexes it, and calls
    // `Array.prototype.find` on it, all of which an empty array satisfies — and
    // an empty list is what a Range in an unlaid-out document genuinely has.
    proto.getClientRects = () =>
      Object.assign([], { item: () => null }) as unknown as DOMRectList;
  }
  if (typeof proto.getBoundingClientRect !== "function") {
    proto.getBoundingClientRect = zeroRect;
  }
}

installRangeRectPolyfill();

/**
 * The short timers a finished test left armed, so the one kind that fires
 * **after** its environment is gone can be let run while it is still there.
 *
 * `@tiptap/react` does not destroy an editor when the component unmounts: it
 * schedules the destruction on a 1 ms timer and destroys only if the component
 * has not remounted by then (`EditorInstanceManager.scheduleDestroy`), which is
 * how it survives React's double-invoked effects. Unmounting one Editor arms two
 * such timers. Nothing in a test advances real time after Testing Library's
 * cleanup, so when a file's last test unmounts an editor those timers are still
 * pending when Vitest tears the jsdom environment down for that file — and the
 * callback then reaches `EditorView.destroy`, which touches `window`, and throws
 * `ReferenceError: window is not defined`.
 *
 * That throw happens inside a timer callback rather than inside any test's own
 * call stack, so Vitest cannot attribute it to a test: it surfaces as an
 * **unhandled error**, which fails the run with every assertion passing — the
 * same worst shape the Range polyfill above exists to prevent, and the reason
 * this is fixed here rather than suppressed. It is also a race: whether the
 * timer beats the teardown depends on machine load, so the same working copy
 * passes one run and fails the next.
 *
 * Only timers of a few milliseconds are tracked, which is the shape of a
 * deferred teardown; a long poll a test left armed is not one of these and is
 * left alone. A test running under fake timers schedules through the fake
 * `setTimeout` rather than this wrapper, so nothing real is pending for it and
 * nothing here has anything to do.
 */
const SHORT_TIMER_MS = 5;
const pendingShortTimers = new Set<unknown>();

function trackShortTimers(): void {
  const realSetTimeout = globalThis.setTimeout;
  const realClearTimeout = globalThis.clearTimeout;
  type SetTimeout = typeof globalThis.setTimeout;

  const tracked = ((handler: TimerHandler, timeout?: number, ...args: unknown[]) => {
    if ((timeout ?? 0) > SHORT_TIMER_MS || typeof handler !== "function") {
      return (realSetTimeout as SetTimeout)(
        handler as TimerHandler,
        timeout,
        ...(args as []),
      );
    }
    // The handle is captured by the wrapper the timer runs, so a timer that
    // fires normally takes itself out of the set and the set holds only what is
    // genuinely still armed.
    let handle: unknown;
    handle = (realSetTimeout as SetTimeout)(
      ((...fired: unknown[]) => {
        pendingShortTimers.delete(handle);
        (handler as (...a: unknown[]) => void)(...fired);
      }) as TimerHandler,
      timeout,
      ...(args as []),
    );
    pendingShortTimers.add(handle);
    return handle as ReturnType<SetTimeout>;
  }) as SetTimeout;

  globalThis.setTimeout = tracked;
  globalThis.clearTimeout = ((handle?: unknown) => {
    pendingShortTimers.delete(handle);
    return (realClearTimeout as (h?: unknown) => void)(handle);
  }) as typeof globalThis.clearTimeout;
}

trackShortTimers();

/**
 * The tests that prove "nothing is written to web storage" need the jsdom
 * `localStorage` and `sessionStorage`. On Node 25 and later, Node puts its own
 * stores on the global, and Vitest does not replace them: `localStorage` is
 * then `undefined` and `sessionStorage` is a Node store that jsdom does not
 * know. A test that skips a missing store then passes and proves nothing.
 * `test.execArgv` in `vitest.config.ts` removes the Node stores. This check
 * stops the run if that flag stops having its effect.
 */
function requireJsdomStorage(): void {
  for (const name of ["localStorage", "sessionStorage"] as const) {
    if (!(globalThis[name] instanceof Storage)) {
      throw new Error(
        `${name} is not the jsdom Storage; check test.execArgv in vitest.config.ts`,
      );
    }
  }
}

requireJsdomStorage();

/**
 * EDT-FR-70 / FLO-FR-27: put down whatever the finished test left scheduled.
 *
 * A document's rest is a real timer held by its session store, and a test that
 * abandons a store rather than clearing it leaves that timer armed. It would
 * then fire in the middle of some later test — writing through whichever backend
 * mock is installed by then, against an id the two tests happen to share — which
 * reads as a mystery save appearing from nowhere. The application never needs
 * this: it clears its stores when the project closes.
 *
 * And then let the short timers of the block above **run**, inside the
 * environment that is still standing. Running them is what the library intends
 * — the editor is disposed rather than abandoned — and it is what keeps the
 * callback out of a torn-down jsdom. Anything still armed after that turn could
 * only fire later than the environment lives, so it is cleared.
 */
afterEach(async () => {
  cancelAllScheduledWrites();
  // The jsdom stores live for one test file. Clear them, so that a write in one
  // test does not show in the next test.
  localStorage.clear();
  sessionStorage.clear();
  // CVP-FR-47: the discussion session store and the focus registry live outside
  // any component, so a test that leaves text, turns, or a surface in them would
  // show it to the next test.
  // Imported here and not at the top, because a static import would load the
  // module graph before a test file's `vi.mock` calls take effect.
  const [{ clearAllDiscussionSessions }, { resetDiscussionFocus }] = await Promise.all([
    import("../state/discussionSession"),
    import("../state/discussionFocus"),
  ]);
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  if (pendingShortTimers.size === 0 || vi.isFakeTimers()) {
    pendingShortTimers.clear();
    return;
  }
  // One real turn of the timer phase, which is past the deadline of every timer
  // this set can hold. Taken only when something is armed, so a suite of tests
  // that mount nothing pays nothing for it.
  await new Promise<void>((resolve) =>
    setTimeout(resolve, SHORT_TIMER_MS + 1),
  );
  for (const handle of pendingShortTimers) {
    clearTimeout(handle as ReturnType<typeof setTimeout>);
  }
  pendingShortTimers.clear();
});
