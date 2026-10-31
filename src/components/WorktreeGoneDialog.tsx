/**
 * The dialog of a gone active worktree
 * (`../../specifications/ui/WTS-worktree-selector.md` WTS-FR-WIFO, WTS-FR-LLBR,
 * WTS-FR-FEEK, WTS-FR-YKSZ, WTS-FR-BTMD).
 *
 * No surface works on a root that is gone, so this stands in front of them and
 * offers the two ways out. Nothing but those two and the worktree coming back
 * closes it, and nothing behind it takes a key or the focus (WTS-FR-ZHLG).
 */
import { useEffect, useRef, useState } from "react";

import { useActiveRootHealth } from "../hooks/useActiveRootHealth";
import { logError, logInfo } from "../logging";
import { useGoneActiveRoot } from "../state/activeRootHealth";
import type { WorktreeEntry } from "../types";

export interface WorktreeGoneDialogProps {
  worktreeName: string;
  /** The primary worktree's branch, or null where it holds none. */
  primaryBranch: string | null;
  /** Answers the typed error of a switch that failed, or null. */
  onSwitch: () => Promise<string | null>;
  onCloseProject: () => void;
}

export function WorktreeGoneDialog({
  worktreeName,
  primaryBranch,
  onSwitch,
  onCloseProject,
}: WorktreeGoneDialogProps) {
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const switchRef = useRef<HTMLButtonElement>(null);
  const label = primaryBranch ? `Switch to ${primaryBranch}` : "Switch to primary worktree";

  useEffect(() => {
    switchRef.current?.focus();
  }, []);

  // A disabled control drops the focus, so a failed switch gives it back.
  useEffect(() => {
    if (error) switchRef.current?.focus();
  }, [error]);

  // WTS-FR-ZHLG, WTS-FR-FEEK: captured ahead of every surface beneath. A key that
  // starts outside goes nowhere and brings the focus back. Inside, Tab stays in
  // the dialog, and only what operates a button keeps its default.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const dialog = dialogRef.current;
      if (!dialog) return;
      const inside = e.target instanceof Node && dialog.contains(e.target);
      if (e.key === "Tab" || !inside) {
        e.preventDefault();
        e.stopImmediatePropagation();
        const items = Array.from(
          dialog.querySelectorAll<HTMLElement>("button:not([disabled])"),
        );
        if (items.length === 0) return;
        const at = items.indexOf(document.activeElement as HTMLElement);
        const step = e.key === "Tab" && at !== -1 ? (e.shiftKey ? -1 : 1) : 0;
        items[(Math.max(at, 0) + step + items.length) % items.length].focus();
        return;
      }
      if (e.key === "Enter" || e.key === " ") return;
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    // WTS-FR-ZHLG: a surface that takes the focus does not keep it.
    const onFocus = (e: FocusEvent) => {
      const dialog = dialogRef.current;
      if (dialog && e.target instanceof Node && !dialog.contains(e.target)) {
        switchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("focusin", onFocus, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("focusin", onFocus, true);
    };
  }, []);

  const leave = async () => {
    if (running) return;
    setRunning(true);
    setError(null);
    logInfo(["frontend"], "leaving the gone worktree");
    const failure = await onSwitch();
    if (failure === null) return;
    logError(["frontend"], "the gone worktree could not be left", {
      reason: failure,
    });
    setError(failure);
    setRunning(false);
  };

  return (
    // A press on the scrim keeps the focus where it is.
    <div className="scrim worktree-gone-scrim" onMouseDown={(e) => e.preventDefault()}>
      <div
        className="modal worktree-gone"
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="worktree-gone-title"
        aria-describedby="worktree-gone-body"
        aria-busy={running}
        data-testid="worktree-gone-dialog"
      >
        <div className="modal__head">
          <h2 id="worktree-gone-title" className="modal__title">
            Worktree is gone
          </h2>
        </div>
        <div className="modal__body" id="worktree-gone-body">
          <p>
            The worktree <code>{worktreeName}</code> is no longer on disk.
          </p>
          <p className="t-muted">Unsaved changes in open tabs are discarded.</p>
          {error && (
            <p role="alert" data-testid="worktree-gone-error">
              {error}
            </p>
          )}
        </div>
        <div className="modal__actions">
          <button
            className="btn btn--sm btn--default"
            disabled={running}
            data-testid="worktree-gone-close"
            onClick={onCloseProject}
          >
            Close project
          </button>
          <button
            className="btn btn--sm btn--primary"
            ref={switchRef}
            disabled={running}
            data-testid="worktree-gone-switch"
            title={label}
            onClick={() => void leave()}
          >
            {label}
          </button>
        </div>
      </div>
    </div>
  );
}

export interface WorktreeGoneGateProps {
  activeWorktree: WorktreeEntry | null;
  contentRootEpoch: number;
  /** Move off the gone root. Answers the typed error, or null. */
  onSwitch: () => Promise<string | null>;
  onCloseProject: () => void;
  /** SNV-FR-56: every other overlay closes when the dialog opens. */
  onOpened: () => void;
}

/** WTS-FR-BIOJ, WTS-FR-WIFO: the read, and the dialog its answer opens. */
export function WorktreeGoneGate({
  activeWorktree,
  contentRootEpoch,
  onSwitch,
  onCloseProject,
  onOpened,
}: WorktreeGoneGateProps) {
  useActiveRootHealth(activeWorktree, contentRootEpoch);
  const gone = useGoneActiveRoot();
  const isGone = gone !== null;

  const opened = useRef(onOpened);
  opened.current = onOpened;
  useEffect(() => {
    if (isGone) opened.current();
  }, [isGone]);

  if (!gone) return null;
  return (
    <WorktreeGoneDialog
      worktreeName={activeWorktree?.name ?? ""}
      primaryBranch={gone.primaryBranch ?? null}
      onSwitch={onSwitch}
      onCloseProject={onCloseProject}
    />
  );
}
