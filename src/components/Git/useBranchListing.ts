import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../api";
import { onBranchesChanged } from "../../events";
import { logDebug, logWarn } from "../../logging";
import type { GitBranch } from "../../types";
import { parseRejection, rejectionMessage } from "./errors";

/**
 * GIT-FR-06, GIT-FR-11, GIT-FR-HLGO: the branch listing.
 *
 * It loads once the section is first shown, and reloads when `reload` is called
 * and on `"branches changed"`, which a fetch from the top chrome raises. It
 * reloads on remount too, which a worktree switch forces, so `isCurrent` always
 * names the active worktree's branch.
 */
export function useBranchListing(active: boolean) {
  const [branches, setBranches] = useState<GitBranch[] | null>(null);
  const [error, setError] = useState("");
  /**
   * Bumped when the repository's branch set has been re-read, so the section
   * reloads its listing. An epoch rather than clearing `branches` back to null:
   * the rows stay on screen while the reload runs, instead of the section
   * blinking through an empty state on every refresh.
   */
  const [epoch, setEpoch] = useState(0);
  const loadedEpoch = useRef<number | null>(null);
  /** The epoch whose read has answered, so a caller knows the rows are current. */
  const [settledEpoch, setSettledEpoch] = useState<number | null>(null);

  useEffect(() => {
    // Loaded lazily: the epoch is only recorded once the section is actually
    // shown, so an event that arrives while another section is active still
    // produces a fresh listing the next time this one is opened.
    if (!active || loadedEpoch.current === epoch) return;
    // Claimed up front so a re-render cannot start a second listing for the same
    // epoch, but released again below if this one never settled. Without that,
    // leaving the section mid-flight would leave the epoch marked as loaded with
    // nothing loaded, and the section would sit on "Loading" for good.
    loadedEpoch.current = epoch;
    let settled = false;
    let cancelled = false;
    logDebug(["frontend", "backend"], "branch listing read started");
    api
      .listBranches()
      .then((b) => {
        if (cancelled) return;
        settled = true;
        setBranches(b ?? []);
        setError("");
        setSettledEpoch(epoch);
        logDebug(["frontend"], "branch listing read finished", {
          count: b?.length ?? 0,
        });
      })
      .catch((e) => {
        if (cancelled) return;
        settled = true;
        setBranches((prev) => prev ?? []);
        setError(rejectionMessage(e));
        setSettledEpoch(epoch);
        logWarn(["frontend", "backend"], "branch listing read failed", {
          code: parseRejection(e).code,
        });
      });
    return () => {
      cancelled = true;
      if (!settled) loadedEpoch.current = null;
    };
  }, [active, epoch]);

  /**
   * GIT-FR-11 / WTC-FR-25: a branch fetched by the top-chrome refresh control
   * appears here, and a remote-tracking entry that refresh pruned disappears,
   * without the panel being closed and reopened.
   *
   * Reloading the branch listing is NOT a worktree change: no other section
   * reloads and the inline diff is left exactly as it is.
   */
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void onBranchesChanged(() => setEpoch((n) => n + 1)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const reload = useCallback(() => setEpoch((n) => n + 1), []);
  return {
    branches,
    listError: error,
    reload,
    /** The rows answer the latest epoch: no newer read is due or running. */
    current: settledEpoch === epoch,
  };
}
