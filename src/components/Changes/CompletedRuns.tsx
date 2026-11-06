/**
 * The read-only **Completed runs** list below the Changes panel's footer
 * (`../../../specifications/ui/CHG-changes.md` CHG-FR-QLZO, CHG-FR-SUTJ,
 * CHG-FR-GXNP, CHG-FR-KIPM).
 *
 * It names the completed runs whose work is on the current branch, read from
 * `list_branch_completed_runs` (`GRD-graduation.md` GRD-FR-BHFP), oldest
 * landing first. It offers no control, and no row acts on a run.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import {
  onChangesUpdated,
  onGraduationQueueChanged,
  onGraduationRunChanged,
  onWorktreeContextChanged,
} from "../../events";
import { logWarn } from "../../logging";
import type { BranchCompletedRun } from "../../types";
import { parseRejection } from "../Git/errors";
import { formatRelative } from "../ProjectPicker";

type View =
  /** CHG-FR-SUTJ: nothing to show: a read in flight, a detached HEAD, or no run. */
  | { status: "absent" }
  /** CHG-FR-SUTJ: the read failed, so the list states it and shows no rows. */
  | { status: "failed" }
  | { status: "ready"; runs: BranchCompletedRun[] };

/** CHG-FR-GXNP: the events after which the list reads again. */
const REFRESH_EVENTS = [onGraduationRunChanged, onGraduationQueueChanged, onChangesUpdated];

export function CompletedRuns() {
  const [view, setView] = useState<View>({ status: "absent" });
  const seq = useRef(0);
  /**
   * CHG-FR-GXNP: the branch the newest read found current. A response for any
   * other branch is discarded. `undefined` until the first read lands.
   */
  const currentBranch = useRef<string | null | undefined>(undefined);

  const read = useCallback(async () => {
    const mine = ++seq.current;
    let branch: string | null;
    try {
      const context = await api.listWorktreesAndBranches();
      const active =
        context?.worktrees?.find((w) => w.path === context.activeWorktreePath) ??
        context?.worktrees?.find((w) => w.isActive);
      branch = active && !active.isDetached && active.branch ? active.branch : null;
    } catch (error) {
      if (mine !== seq.current) return;
      logWarn(["frontend"], "completed runs branch read failed", {
        error: parseRejection(error).code,
      });
      setView({ status: "failed" });
      return;
    }
    if (mine !== seq.current) return;
    currentBranch.current = branch;
    // CHG-FR-SUTJ: a detached HEAD has no branch, so it has no list.
    if (branch === null) {
      setView({ status: "absent" });
      return;
    }
    try {
      const runs = await api.listBranchCompletedRuns(branch);
      if (mine !== seq.current || currentBranch.current !== branch) return;
      setView({ status: "ready", runs: Array.isArray(runs) ? runs : [] });
    } catch (error) {
      if (mine !== seq.current || currentBranch.current !== branch) return;
      logWarn(["frontend"], "completed runs read failed", {
        error: parseRejection(error).code,
      });
      setView({ status: "failed" });
    }
  }, []);

  useEffect(() => {
    void read();
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    };
    for (const subscribe of REFRESH_EVENTS) {
      void subscribe(() => void read()).then(keep);
    }
    // CHG-FR-GXNP: a change of the current branch makes every read for the old
    // branch stale at once, before the new read lands.
    void onWorktreeContextChanged((payload) => {
      currentBranch.current = payload.isDetached ? null : (payload.branch ?? null);
      void read();
    }).then(keep);
    return () => {
      cancelled = true;
      seq.current += 1;
      for (const fn of unlisteners) fn();
    };
  }, [read]);

  if (view.status === "absent") return null;
  if (view.status === "ready" && view.runs.length === 0) return null;

  return (
    <section className="changes-completed" aria-label="Completed runs">
      <div className="changes-completed__heading">
        Completed runs
        {view.status === "ready" && (
          <span className="changes-completed__count"> · {view.runs.length}</span>
        )}
      </div>
      {view.status === "failed" ? (
        <div className="changes-completed__error" role="status">
          The completed runs could not be read.
        </div>
      ) : (
        // CHG-FR-KIPM: plain text rows in a list that scrolls itself past five.
        <ul className="changes-completed__list" data-testid="changes-completed-list">
          {view.runs.map((run) => (
            <li key={run.runId} className="changes-completed__row">
              <span className="changes-completed__title" title={run.title}>
                {run.title}
              </span>
              <span className="changes-completed__meta">
                {run.kind} · landed{" "}
                <time dateTime={run.landing.at}>{formatRelative(run.landing.at)}</time>
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
