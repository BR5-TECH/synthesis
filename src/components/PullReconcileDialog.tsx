/**
 * The merge-or-rebase dialog a diverged pull opens
 * (`../../specifications/ui/GIT-git.md` GIT-FR-MRBQ, GIT-FR-RCDL,
 * `../../specifications/ui/WTS-worktree-selector.md` WTS-FR-DVGQ).
 *
 * A pull writes nothing to a branch that has commits of its own, because how the
 * two histories join is the author's decision. The dialog names both sides and
 * offers the two ways. Cancel invokes nothing. **Merge** and **Rebase** run as
 * one transfer of the window (GIT-FR-QMYB), at the tips the dialog shows, and
 * the dialog then states what the reconcile did.
 *
 * It is a floating overlay of the main window and is mutually exclusive with the
 * others (SNV-FR-56), so the shell mounts it.
 */
import { useEffect, useRef, useState } from "react";

import { useGitTransfer } from "../hooks/useGitTransfer";
import { logInfo, logWarn } from "../logging";
import { reconcileText } from "../state/pullText";
import type { PullOutcome, PullReconcileOutcome, PullStrategy } from "../types";

export interface PullReconcileDialogProps {
  /** The diverged outcome of the pull that opened the dialog. */
  outcome: PullOutcome;
  onClose: () => void;
  /** GIT-FR-RCDL: the route to the pull run in the Runs panel. */
  onOpenRun: (runId: string) => void;
}

type Phase =
  | { kind: "choose" }
  | { kind: "running"; strategy: PullStrategy }
  | { kind: "done"; result: PullReconcileOutcome }
  | { kind: "failed"; message: string };

const short = (tip?: string) => (tip ?? "").slice(0, 7);

export function PullReconcileDialog({
  outcome,
  onClose,
  onOpenRun,
}: PullReconcileDialogProps) {
  const transfer = useGitTransfer();
  const [phase, setPhase] = useState<Phase>({ kind: "choose" });
  const surfaceRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const running = phase.kind === "running";
  const branch = outcome.branch ?? "";
  const upstream = outcome.upstream ?? "";

  // Focus lands on Cancel, so a reflexive Enter on a dialog the author did not
  // expect changes no history. On close it goes back to where it was, where
  // that control can still take it.
  useEffect(() => {
    const before = document.activeElement as HTMLElement | null;
    cancelRef.current?.focus();
    return () => {
      if (before?.isConnected && !before.hasAttribute("disabled")) before.focus();
    };
  }, []);

  // The result replaces the choice, and the controls that held focus are gone,
  // so focus moves to the first action of the result.
  const settled = phase.kind === "done" || phase.kind === "failed";
  useEffect(() => {
    if (!settled) return;
    const first = surfaceRef.current?.querySelector<HTMLButtonElement>(
      ".modal__actions button",
    );
    first?.focus();
  }, [settled]);

  // Escape dismisses, unless a reconcile runs: the dialog stays to report it.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !running) {
        e.stopPropagation();
        onClose();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose, running]);

  const choose = async (strategy: PullStrategy) => {
    if (running) return;
    setPhase({ kind: "running", strategy });
    logInfo(["frontend"], "pull reconcile chosen", { strategy });
    const call = await transfer.reconcilePull(
      strategy,
      outcome.localTip ?? "",
      outcome.remoteTip ?? "",
    );
    if (call.ok) {
      setPhase({ kind: "done", result: call.outcome });
      return;
    }
    const message =
      call.cause === "busy"
        ? "A push or a pull is already running. Wait for it to end, then try again."
        : `The pull did not run: ${call.error}`;
    logWarn(["frontend"], "pull reconcile did not run", {
      cause: call.cause,
    });
    setPhase({ kind: "failed", message });
  };

  const result = phase.kind === "done" ? phase.result : null;
  const runId = result?.state === "handed_to_run" ? result.runId : undefined;

  return (
    <div
      className="scrim"
      onMouseDown={(e) => {
        if (running) return;
        if (!surfaceRef.current?.contains(e.target as Node)) onClose();
      }}
    >
      <div
        className="modal pull-reconcile"
        role="dialog"
        aria-modal="true"
        aria-labelledby="pull-reconcile-title"
        aria-describedby="pull-reconcile-sides"
        aria-busy={running}
        data-testid="pull-reconcile-dialog"
        ref={surfaceRef}
      >
        <div className="modal__head">
          <h2 id="pull-reconcile-title" className="modal__title">
            {branch} and {upstream} have diverged
          </h2>
        </div>
        <div className="modal__body">
          <p id="pull-reconcile-sides" data-testid="pull-reconcile-sides">
            {branch} at <code title={outcome.localTip}>{short(outcome.localTip)}</code>{" "}
            has {outcome.ahead} commit{outcome.ahead === 1 ? "" : "s"} that{" "}
            {upstream} does not. {upstream} at{" "}
            <code title={outcome.remoteTip}>{short(outcome.remoteTip)}</code> has{" "}
            {outcome.behind} commit{outcome.behind === 1 ? "" : "s"} that {branch}{" "}
            does not.
          </p>
          {phase.kind === "choose" && (
            <p className="t-muted">
              <strong>Merge</strong> joins them with one merge commit.{" "}
              <strong>Rebase</strong> replays the local commits on top of {upstream}.
              If Git cannot settle a conflict, a pull run settles it, and nothing
              changes until its review lets the result through.
            </p>
          )}
          {running && (
            <p role="status" data-testid="pull-reconcile-status">
              {phase.strategy === "merge" ? "Merging…" : "Rebasing…"}
            </p>
          )}
          {result && (
            <p
              role={result.state === "refused" ? "alert" : "status"}
              data-testid="pull-reconcile-result"
            >
              {reconcileText(result)}
            </p>
          )}
          {phase.kind === "failed" && (
            <p role="alert" data-testid="pull-reconcile-result">
              {phase.message}
            </p>
          )}
        </div>
        <div className="modal__actions">
          {phase.kind === "choose" || running ? (
            <>
              <button
                className="btn btn--sm"
                ref={cancelRef}
                disabled={running}
                onClick={onClose}
              >
                Cancel
              </button>
              <button
                className="btn btn--sm btn--default"
                disabled={running || transfer.running}
                data-testid="pull-reconcile-rebase"
                onClick={() => void choose("rebase")}
              >
                Rebase
              </button>
              <button
                className="btn btn--sm btn--primary"
                disabled={running || transfer.running}
                data-testid="pull-reconcile-merge"
                onClick={() => void choose("merge")}
              >
                Merge
              </button>
            </>
          ) : (
            <>
              {runId && (
                <button
                  className="btn btn--sm btn--default"
                  data-testid="pull-reconcile-open-run"
                  onClick={() => {
                    onClose();
                    onOpenRun(runId);
                  }}
                >
                  Open run
                </button>
              )}
              <button
                className="btn btn--sm btn--primary"
                ref={cancelRef}
                data-testid="pull-reconcile-close"
                onClick={onClose}
              >
                Close
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
