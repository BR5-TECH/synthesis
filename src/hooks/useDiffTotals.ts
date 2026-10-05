import { useEffect, useState } from "react";
import * as api from "../api";
import { onChangesUpdated } from "../events";
import type { DiffTotals } from "../types";

/**
 * STB-FR-25–STB-FR-30: the status bar's uncommitted diff summary.
 *
 * Always the *uncommitted* comparison of the active worktree, whatever mode the
 * Changes panel is in and whatever target branch it is configured against
 * (STB-FR-26) — the two read different backend operations, so the panel's mode
 * cannot leak in here.
 *
 * `null` means "render nothing in its place": either the totals have not loaded
 * yet, or the project is not inside a Git repository, which the totals operation
 * reports as the typed `"not a git repository"` error (STB-FR-29). Zeros are a
 * real answer and are rendered as such.
 *
 * `epoch` re-runs the load against a new content root (STB-FR-30): the caller
 * passes the shell's content-root epoch, which changes on every worktree switch
 * including an in-place branch checkout onto the same directory.
 */
export function useDiffTotals(epoch: number): DiffTotals | null {
  const [totals, setTotals] = useState<DiffTotals | null>(null);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    // STB-FR-30: drop the outgoing root's counts rather than showing them
    // against the new one while the reload is in flight.
    setTotals(null);

    const load = () => {
      void api
        .getUncommittedDiffTotals()
        .then((next) => {
          // Same boundary guard as the progress read: a malformed answer
          // renders as "no comparison here" rather than as `+undefined`.
          if (cancelled) return;
          setTotals(
            typeof next?.addedLines === "number" &&
              typeof next?.removedLines === "number"
              ? next
              : null,
          );
        })
        .catch(() => {
          // STB-FR-29: outside a repository the summary renders nothing at all
          // — not zeros, which would claim a clean tree, and not an error
          // message, which would be noise in a strip of ambient facts.
          if (!cancelled) setTotals(null);
        });
    };

    load();
    // STB-FR-27: the totals track edits made in the app and by external
    // processes alike, and do not depend on the Changes panel ever having been
    // opened — this subscription is the status bar's own.
    void onChangesUpdated(load).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [epoch]);

  return totals;
}
