/**
 * What the Changes panel and the Git panel need to offer **Create a PR** for the
 * project's current branch
 * (`../../specifications/ui/CHG-changes.md` CHG-FR-UPFP, CHG-FR-UCRL;
 * `../../specifications/ui/GIT-git.md` GIT-FR-05).
 *
 * It reads the active worktree's branch and then the branch's head state
 * against the repository's default branch, and reads both again whenever
 * `"changes updated"` fires, so the button follows a commit as soon as it
 * lands. The two surfaces read it the same way, so one cannot offer what the
 * other refuses.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import type { PullRequestSource } from "../components/CreatePullRequest/types";
import { parseRejection } from "../components/Git/errors";
import { onChangesUpdated } from "../events";
import { logWarn } from "../logging";
import type { PullRequestHeadState } from "../types";

export type CurrentBranchPullRequest =
  | { status: "reading" }
  | { status: "detached" }
  | { status: "failed" }
  | {
      status: "ready";
      head: string;
      /** CHG-FR-UPFP: the stream the branch belongs to, when it belongs to one. */
      streamName: string | null;
      state: PullRequestHeadState;
    };

/**
 * CHG-FR-UCRL: whether **Create a PR** is available, and why not in words.
 * `reason` is null when it is available.
 */
export function createPullRequestAvailability(
  current: CurrentBranchPullRequest,
): { available: boolean; reason: string | null } {
  switch (current.status) {
    case "reading":
      return { available: false, reason: "Reading the branch…" };
    case "detached":
      return {
        available: false,
        reason: "HEAD is detached, so there is no branch to propose.",
      };
    case "failed":
      return { available: false, reason: "The branch could not be read." };
    case "ready":
      return current.state.aheadOfBase > 0
        ? { available: true, reason: null }
        : { available: false, reason: "This branch has no commit of its own yet." };
  }
}

/** CHG-FR-UPFP: the source the window opens with, for the current branch. */
export function currentBranchSource(
  current: CurrentBranchPullRequest,
): PullRequestSource | null {
  if (current.status !== "ready") return null;
  return {
    head: current.head,
    base: current.state.base,
    title: current.streamName ?? current.head,
  };
}

export function useCurrentBranchPullRequest(enabled = true) {
  const [current, setCurrent] = useState<CurrentBranchPullRequest>({
    status: "reading",
  });
  const seq = useRef(0);

  const read = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const context = await api.listWorktreesAndBranches();
      const active =
        context?.worktrees?.find((w) => w.path === context.activeWorktreePath) ??
        context?.worktrees?.find((w) => w.isActive);
      if (!active || active.isDetached || !active.branch) {
        if (mine === seq.current) setCurrent({ status: "detached" });
        return;
      }
      const state = await api.getPullRequestHeadState(active.branch, null);
      if (!state) throw new Error("no head state");
      if (mine === seq.current)
        setCurrent({
          status: "ready",
          head: active.branch,
          streamName: active.stream?.streamName ?? null,
          state,
        });
    } catch (error) {
      logWarn(["frontend", "remote"], "current branch pull request read failed", {
        error: parseRejection(error).code,
      });
      if (mine === seq.current) setCurrent({ status: "failed" });
    }
  }, []);

  useEffect(() => {
    if (!enabled) return;
    void read();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onChangesUpdated(() => void read()).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      seq.current += 1;
      unlisten?.();
    };
  }, [enabled, read]);

  return { current, refresh: read };
}
