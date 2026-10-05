/**
 * WTS-FR-25 / WTS-FR-32 / WTC-FR-16 / WTC-FR-25: keep the chrome control's
 * worktree label in agreement with the checkout, by whichever route it changed.
 */
import { useEffect } from "react";
import type { Dispatch, SetStateAction } from "react";
import * as api from "../../api";
import { onBranchesChanged, onWorktreeContextChanged } from "../../events";
import type { WorktreeEntry } from "../../types";

export function useActiveWorktreeTracking(
  setActiveWorktree: Dispatch<SetStateAction<WorktreeEntry | null>>,
): void {
  /**
   * WTS-FR-25 / WTC-FR-16: the chrome control follows the active worktree by
   * whichever route it changed. Subscribing here rather than in the selector
   * means the label is right even while the dropdown has never been opened, and
   * a checkout made anywhere relabels it without a reopen.
   */
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void onWorktreeContextChanged((payload) => {
      // The payload names the new worktree but not everything about it — in
      // particular not `isPrimary`, which decides whether a branch may be
      // checked out at all (WTS-FR-12). Merging the payload onto the *previous*
      // entry would carry that flag over from the worktree just left, so the
      // authoritative entry is re-read instead. The payload is still applied
      // first, so the label is right immediately rather than after a round-trip.
      setActiveWorktree((current) =>
        current
          ? {
              ...current,
              path: payload.activeWorktreePath,
              branch: payload.branch,
              isDetached: payload.isDetached,
            }
          : current,
      );
      void api
        .getActiveWorktree()
        .then((entry) => {
          if (!cancelled && entry) setActiveWorktree(entry);
        })
        .catch(() => {
          // The project may have closed under us; the label the payload
          // already applied stands until the next open.
        });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setActiveWorktree]);

  /**
   * WTS-FR-32 / WTC-FR-25: the branch set was re-read, so the chrome control's
   * label is re-read with it — a branch checked out in a terminal since the last
   * look relabels here without the dropdown being opened.
   *
   * Emphatically NOT a worktree switch (WTS-FR-33 / WTC-FR-24): the content-root
   * epoch is left alone, so nothing remounts, no tab closes, and the viewport
   * stays exactly as it was. That is the whole difference between this channel
   * and `"worktree context changed"` above.
   */
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void onBranchesChanged(() => {
      void api
        .getActiveWorktree()
        .then((entry) => {
          if (!cancelled && entry) setActiveWorktree(entry);
        })
        .catch(() => {
          // The project may have closed under us; the current label stands.
        });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [setActiveWorktree]);
}
