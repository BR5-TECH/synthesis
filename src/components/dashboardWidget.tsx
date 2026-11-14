/**
 * The parts that every Dashboard widget shares (`DSH-dashboard.md`): the loader
 * hook with its result guard and its error state, the widget frame, and the
 * notice a widget shows while it has no rows.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { logError } from "../logging";
import type { DashboardWidgetName } from "../types";

/** The name of a widget in the log records. The Delivery widget has no backend failure channel. */
export type DashboardWidgetKey = DashboardWidgetName | "delivery";

/**
 * DSH-FR-17 / DSH-FR-18 / DSH-FR-19: what one widget knows about its own data.
 *
 * The four states are deliberately not one enum: a widget that has rendered
 * once and is refreshing holds `data` **and** `stale`, because DSH-FR-18 forbids
 * blanking a widget the author is reading. `error` likewise sits beside `data`
 * rather than replacing it, so the widget can leave the error state on its next
 * successful result without having lost what it was showing.
 */
export interface WidgetState<T> {
  data: T | null;
  error: string | null;
  /** No result has arrived yet — the widget has nothing to show but itself. */
  loading: boolean;
  /** A refresh is outstanding over data already rendered. */
  stale: boolean;
}

const INITIAL: WidgetState<never> = {
  data: null,
  error: null,
  loading: true,
  stale: false,
};

/** How a widget turns a refusal into text, and into a code for the log. */
export interface WidgetErrorText {
  /** The text the widget keeps as its error. */
  describe: (reason: unknown) => string;
  /** The value the log record carries. It must hold no name and no description. */
  code: (reason: unknown) => string;
}

const PLAIN_ERROR: WidgetErrorText = {
  describe: (reason) => String(reason),
  code: (reason) => String(reason),
};

/**
 * One widget's loader, its result guard, and its error state.
 *
 * **DSH-FR-19 lives here.** Every invocation takes the next sequence number and
 * only the newest may settle, so two outstanding calls can never let a slow one
 * replace fresher data with older — and `reset` bumps the same counter, which is
 * what discards every outstanding result when the project or the active worktree
 * changes rather than rendering it against the new root.
 *
 * Nothing is awaited by the caller (DSH-FR-17): `refresh` dispatches and
 * returns, so a slow loader leaves its own widget in a loading state while every
 * other widget renders. `refresh` takes an optional other loader for one call,
 * for a widget whose events ask for two kinds of read (DSH-FR-LGJT).
 */
export function useWidget<T>(
  widget: DashboardWidgetKey,
  load: () => Promise<T>,
  errorText: WidgetErrorText = PLAIN_ERROR,
): {
  state: WidgetState<T>;
  refresh: (other?: () => Promise<T>) => void;
  /** DSH-FR-18: enter the error state from `"dashboard refresh failed"`. */
  fail: (reason: string) => void;
  reset: () => void;
} {
  const [state, setState] = useState<WidgetState<T>>(INITIAL);
  // Not React state: the guard has to be readable by a promise that settles
  // after a re-render, and a stale closure over a `useState` value would let an
  // overtaken result through.
  const seq = useRef(0);
  const live = useRef(true);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);
  const { describe, code } = errorText;

  const refresh = useCallback(
    (other?: () => Promise<T>) => {
      const mine = ++seq.current;
      setState((previous) =>
        previous.data === null && previous.error === null
          ? { ...previous, loading: true }
          : { ...previous, stale: true },
      );
      (other ?? load)().then(
        (data) => {
          if (!live.current || mine !== seq.current) return;
          setState({ data, error: null, loading: false, stale: false });
        },
        (reason: unknown) => {
          if (!live.current || mine !== seq.current) return;
          const message = describe(reason);
          // A loader that fails is the one thing about this surface a reader
          // will come to the Logs panel for, and it is otherwise invisible: it
          // is handled, so only one widget's content area changes. The record
          // carries the widget's name and the failure's code, and nothing that
          // the loaders read.
          logError(["frontend"], "dashboard widget loader failed", {
            widget,
            reason: code(reason),
          });
          setState((previous) => ({
            data: previous.data,
            error: message,
            loading: false,
            stale: false,
          }));
        },
      );
    },
    [load, widget, describe, code],
  );

  const fail = useCallback((reason: string) => {
    setState((previous) => ({
      data: previous.data,
      error: reason,
      loading: false,
      stale: false,
    }));
  }, []);

  const reset = useCallback(() => {
    // Bumping the sequence is what discards every outstanding result; the state
    // goes back to loading because what it holds describes the outgoing root.
    seq.current += 1;
    setState(INITIAL);
  }, []);

  return { state, refresh, fail, reset };
}

interface DashWidgetProps {
  title: string;
  count?: number;
  /** DSH-FR-18: a refresh is outstanding over data already rendered. */
  stale?: boolean;
  children: ReactNode;
}

export function DashWidget({ title, count, stale, children }: DashWidgetProps) {
  return (
    <div className="dash-widget" data-stale={stale ? "true" : undefined}>
      <div className="dash-widget__head">
        <span className="dash-widget__title">{title}</span>
        {stale && (
          <span className="dash-widget__stale" title="Refreshing…">
            Refreshing…
          </span>
        )}
        {count != null && <span className="dash-widget__count">{count}</span>}
      </div>
      <div className="dash-widget__body">{children}</div>
    </div>
  );
}

/**
 * DSH-FR-17 / DSH-FR-18: a widget's content area while it has no rows to show —
 * still loading, failed to read, or genuinely empty.
 *
 * All three read as one centred line rather than three layouts, because they are
 * the same thing to the eye: the widget is present and has nothing in it yet.
 */
export function WidgetNotice({
  kind,
  children,
}: {
  kind: "empty" | "loading" | "error";
  children: ReactNode;
}) {
  return (
    <div
      className={`dash-row__empty dash-row__empty--${kind}`}
      role={kind === "error" ? "alert" : undefined}
    >
      {children}
    </div>
  );
}
