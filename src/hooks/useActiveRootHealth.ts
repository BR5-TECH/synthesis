/**
 * WTS-FR-BIOJ: read whether the content root is gone, at the moments it can
 * have gone. A linked worktree is removed outside the application, so the
 * author comes back to the window, or the tree reports a removal.
 *
 * No timer drives the read, and the primary worktree reads nothing at all.
 */
import { useEffect } from "react";

import * as api from "../api";
import { onProjectTreeChanged } from "../events";
import { logDebug, logWarn } from "../logging";
import {
  applyActiveRootState,
  clearActiveRootState,
  isActiveRootGone,
} from "../state/activeRootHealth";
import type { WorktreeEntry } from "../types";

export function useActiveRootHealth(
  activeWorktree: WorktreeEntry | null,
  contentRootEpoch: number,
): void {
  const linked = activeWorktree !== null && !activeWorktree.isPrimary;
  const path = activeWorktree?.path;

  useEffect(() => {
    if (!linked) {
      clearActiveRootState();
      return;
    }
    let cancelled = false;
    let seq = 0;
    // Async, so a mock that throws is caught here like one that rejects.
    const read = async () => {
      const mine = ++seq;
      try {
        const answer = await api.activeRootState();
        if (cancelled || mine !== seq || !answer || typeof answer !== "object") return;
        if (answer.state === "gone" && !isActiveRootGone()) {
          logWarn(["frontend"], "the active worktree is gone", {
            primaryBranch: answer.primaryBranch ?? null,
          });
        }
        if (answer.state === "unknown") {
          logDebug(["frontend"], "active root state is unknown");
        }
        applyActiveRootState(answer);
      } catch (reason) {
        logWarn(["frontend"], "active root state could not be read", {
          reason: String(reason),
        });
      }
    };

    void read();
    const onFocus = () => void read();
    window.addEventListener("focus", onFocus);
    const unlisten = onProjectTreeChanged((payload) => {
      if (payload.removedPaths?.length) void read();
    });
    return () => {
      cancelled = true;
      window.removeEventListener("focus", onFocus);
      void unlisten.then((un) => un()).catch(() => {});
    };
  }, [linked, path, contentRootEpoch]);
}
