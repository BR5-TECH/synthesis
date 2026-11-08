/**
 * The open pull requests of the repository, as the openers of the Create a PR
 * window read them (`../../specifications/ui/CPR-create-pull-request.md`
 * CPR-FR-RWOE, CPR-FR-TJYF; `CHG-changes.md` CHG-FR-NKGY; `GIT-git.md`
 * GIT-FR-HMRW; `WSS-work-stream-selector.md` WSS-FR-OBZK).
 *
 * One read serves every opener, so the Changes panel, the Git panel and the
 * stream rows offer **Create a PR** on the same terms. A read that runs is
 * shared by every caller that asks while it runs. A read that fails, or that
 * still runs, names no pull request, so it blocks nothing.
 */
import { useEffect, useSyncExternalStore } from "react";

import * as api from "../api";
import { parseRejection } from "../components/Git/errors";
import { logWarn } from "../logging";
import type { PullRequestSummary } from "../types";

let pulls: PullRequestSummary[] | null = null;
let inflight: Promise<void> | null = null;
/** A counter, so a read that a clear made stale changes nothing. */
let generation = 0;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

/**
 * CPR-FR-RWOE / CPR-FR-TJYF: read the open pull requests again. A caller that
 * asks while a read runs shares it, unless it needs a read that starts after
 * its own change (`fresh`): that one waits for the running read and then reads.
 */
export function refreshOpenPullRequests(fresh = false): Promise<void> {
  if (inflight && fresh) {
    const mine = generation;
    return inflight.then(() =>
      mine === generation ? refreshOpenPullRequests(true) : undefined,
    );
  }
  if (inflight) return inflight;
  const mine = generation;
  inflight = api
    .listPullRequests("open")
    .then((rows) => {
      if (mine !== generation) return;
      pulls = Array.isArray(rows) ? rows : [];
      emit();
    })
    .catch((error: unknown) => {
      // CPR-FR-RWOE: a failed read adds no block.
      logWarn(["frontend", "remote"], "open pull requests read failed", {
        error: parseRejection(error).code,
      });
      if (mine !== generation) return;
      pulls = null;
      emit();
    })
    .finally(() => {
      if (mine === generation) inflight = null;
    });
  return inflight;
}

/** A change of the project: the list of the other repository is gone. */
export function clearOpenPullRequests(): void {
  generation += 1;
  inflight = null;
  if (pulls !== null) {
    pulls = null;
    emit();
  }
}

/** Test seam: forget every list and every read. */
export function resetOpenPullRequestsForTest(): void {
  generation += 1;
  inflight = null;
  pulls = null;
  listeners.clear();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * The open pull request whose head is `head` in the repository itself, or
 * null. A pull request from a fork does not count (CPR-FR-RWOE).
 */
export function openPullRequestFor(
  rows: PullRequestSummary[] | null,
  head: string | null,
): PullRequestSummary | null {
  if (!rows || !head) return null;
  return (
    rows.find((pr) => pr.headBranch === head && pr.state === "open" && !pr.headIsFork) ?? null
  );
}

/** CPR-FR-RWOE: the reason a branch with an open pull request states. */
export function openPullRequestReason(pr: PullRequestSummary): string {
  return `Pull request #${pr.number} is already open for this branch.`;
}

/**
 * The open pull requests, read again whenever `readKey` changes: on mount,
 * and when the head branch of the caller changes.
 */
export function useOpenPullRequestList(readKey: string | null, enabled = true) {
  const rows = useSyncExternalStore(subscribe, () => pulls);
  useEffect(() => {
    if (!enabled) return;
    void refreshOpenPullRequests();
  }, [readKey, enabled]);
  return rows;
}
