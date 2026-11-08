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
 *
 * It also reads the GitHub stack of the branch when the branch changes, so the
 * source starts from the parent branch in the stack (CHG-FR-JQSD,
 * CPR-FR-MBTA). That read reaches GitHub, so it does not run on every
 * `"changes updated"`.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import type { PullRequestSource } from "../components/CreatePullRequest/types";
import { parseRejection } from "../components/Git/errors";
import { onChangesUpdated } from "../events";
import { logWarn } from "../logging";
import { openPullRequestReason } from "../state/openPullRequests";
import type { BranchStack, PullRequestHeadState, PullRequestSummary } from "../types";

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
  openPullRequest: PullRequestSummary | null = null,
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
      // CHG-FR-NKGY / CPR-FR-RWOE: a branch with an open pull request needs no
      // second one.
      if (openPullRequest) return { available: false, reason: openPullRequestReason(openPullRequest) };
      return current.state.aheadOfBase > 0
        ? { available: true, reason: null }
        : { available: false, reason: "This branch has no commit of its own yet." };
  }
}

/**
 * CPR-FR-MBTA: the parent branch an answer gives, when it is available and
 * names one for `head`. Null for every other answer.
 */
export function stackParentOf(stack: BranchStack | null, head: string): string | null {
  if (!stack || stack.branch !== head || !stack.available || !stack.inStack) return null;
  return stack.parent ?? null;
}

/**
 * CHG-FR-UPFP / CHG-FR-JQSD: the source the window opens with, for the
 * current branch. The parent branch in the stack takes the place of the
 * default branch.
 */
export function currentBranchSource(
  current: CurrentBranchPullRequest,
  stack: BranchStack | null = null,
): PullRequestSource | null {
  if (current.status !== "ready") return null;
  const parent = stackParentOf(stack, current.head);
  return {
    head: current.head,
    base: parent ?? current.state.base,
    title: current.streamName ?? current.head,
    ...(parent ? { stackParent: parent } : {}),
  };
}

/** CHG-FR-JQSD: the state of the stack read of the current branch. */
export type StackReadStatus = "idle" | "reading" | "ready" | "failed";

type StackRead = {
  head: string;
  /** The read has an answer or has failed. */
  settled: boolean;
  promise: Promise<BranchStack | null>;
};

export function useCurrentBranchPullRequest(enabled = true) {
  const [current, setCurrent] = useState<CurrentBranchPullRequest>({
    status: "reading",
  });
  const seq = useRef(0);
  const [stack, setStack] = useState<BranchStack | null>(null);
  const [stackStatus, setStackStatus] = useState<StackReadStatus>("idle");
  /** The last stack read, so a read runs once per branch. */
  const stackRead = useRef<StackRead | null>(null);

  /**
   * CHG-FR-JQSD: the answer of the stack read of `head`. A read of the same
   * branch that still runs is shared. When `fresh` is set, a read that has
   * ended runs again, so a click never builds on an old answer.
   */
  const readStack = useCallback((head: string, fresh: boolean): Promise<BranchStack | null> => {
    const last = stackRead.current;
    if (last && last.head === head && !(fresh && last.settled)) return last.promise;
    setStack(null);
    setStackStatus("reading");
    const entry: StackRead = { head, settled: false, promise: Promise.resolve(null) };
    entry.promise = api
      .getBranchStack(head)
      .then((answer) => {
        entry.settled = true;
        if (stackRead.current === entry) {
          setStack(answer ?? null);
          setStackStatus("ready");
        }
        return answer ?? null;
      })
      .catch((error: unknown) => {
        // CHG-FR-JQSD: a failed read keeps the default branch.
        entry.settled = true;
        logWarn(["frontend", "remote"], "current branch stack read failed", {
          head,
          error: parseRejection(error).code,
        });
        if (stackRead.current === entry) setStackStatus("failed");
        return null;
      });
    stackRead.current = entry;
    return entry.promise;
  }, []);

  /** CHG-FR-JQSD: the stack answer of `head` for a click: a running read, or a new one. */
  const stackFor = useCallback((head: string) => readStack(head, true), [readStack]);

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
      if (mine === seq.current) void readStack(active.branch, false);
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
  }, [readStack]);

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
      stackRead.current = null;
      unlisten?.();
    };
  }, [enabled, read]);

  return { current, stack, stackStatus, stackFor, refresh: read };
}
