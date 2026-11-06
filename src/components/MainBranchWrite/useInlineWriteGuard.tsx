/**
 * The inline section of an overlay that is already open (MBW-FR-URIL,
 * MBW-FR-IOGF, MBW-FR-WCBQ): the commit message window, the merge
 * confirmation of the work stream selector, the graduation start dialog, and
 * the pull reconcile dialog.
 *
 * The overlay reads the guard when it opens. On a main branch with no
 * confirmation, the section shows above the overlay's action row, and the
 * overlay's primary action stays unavailable until the author confirms. A hard
 * block keeps it unavailable. The section is the primary action's accessible
 * description.
 */
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import {
  logConfirmation,
  needsConfirmation,
  parseMainBranchRefusal,
} from "../../state/mainBranchWrite";
import { requestNewWorktree } from "../../state/newWorktreeRequests";
import type { MainBranchWriteKind } from "../../types";
import { BranchName, MainBranchWriteText, isHardBlock } from "./Content";
import { usePanelState } from "./usePanelState";

export interface InlineWriteGuard {
  /** Whether the overlay's primary action is unavailable. */
  blocked: boolean;
  /** Whether the read of the guard has not landed yet. */
  pending: boolean;
  /** The id the primary action names as its description, while it shows. */
  describedBy: string | undefined;
  /** The section, or `null` when no confirmation is needed. */
  section: ReactNode;
  /**
   * MBW-FR-YSMC: take a refusal of the write. Returns `true` when it was a
   * refusal of the check; the section then shows the guard it carries, and a
   * confirm runs `retry` once.
   */
  adoptRefusal: (error: unknown, retry: () => void) => boolean;
}

export function useInlineWriteGuard(options: {
  write: MainBranchWriteKind;
  /** The branch the write lands on; `null` is the active branch. */
  branch?: string | null;
  /** `false` skips the read, for an overlay route that writes nothing. */
  enabled?: boolean;
  /** **Cancel** and **Close** of the section: the overlay's own dismissal. */
  onCancel: () => void;
  /** An action the hard block offers beside **New worktree**. */
  blockAction?: { label: string; run: () => void };
  /**
   * MBW-FR-WCBQ: after a confirm the section goes, and the button that had
   * focus goes with it. Called after that render, so the overlay puts focus
   * on a control of its own and focus never drops to the document body.
   */
  onConfirmed?: () => void;
}): InlineWriteGuard {
  const { write, enabled = true, onCancel, blockAction } = options;
  const onConfirmed = useRef(options.onConfirmed);
  onConfirmed.current = options.onConfirmed;
  const [confirmedAt, setConfirmedAt] = useState(0);
  const branch = options.branch ?? null;
  const state = usePanelState({
    write,
    branch,
    surface: "inline",
    initialGuard: null,
    initialError: null,
  });
  const id = useId();
  const sectionId = `main-branch-write-${id.replace(/:/g, "")}`;
  const retry = useRef<(() => void) | null>(null);

  useEffect(() => {
    if (enabled) void state.tryAgain();
    // The read belongs to the opening of the overlay and to the branch.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, branch, write]);

  const { guard, readError } = state;
  const needed = enabled && (readError !== null || (guard !== null && needsConfirmation(guard)));
  // MBW-FR-IOGF: until the read lands nothing is known, so the primary action
  // waits for it too. A read of a confirmed branch lands at once.
  const pending = enabled && guard === null && readError === null;
  const blocked = isHardBlock(state);

  // MBW-FR-PJTG: shown or blocked, once per guard it shows.
  const logged = useRef<string | null>(null);
  useEffect(() => {
    if (!needed || !guard) return;
    const key = `${guard.branch}:${guard.protection}`;
    if (logged.current === key) return;
    logged.current = key;
    logConfirmation(blocked ? "blocked" : "shown", write, guard.branch, "inline");
  }, [needed, guard, blocked, write]);

  const cancel = () => {
    logConfirmation("cancelled", write, guard?.branch ?? branch, "inline");
    onCancel();
  };

  // MBW-FR-WCBQ: the render after a confirm no longer holds the section.
  useEffect(() => {
    if (confirmedAt === 0) return;
    const at = document.activeElement;
    if (at === null || at === document.body) onConfirmed.current?.();
  }, [confirmedAt]);

  const confirm = async () => {
    if (!(await state.confirm())) return;
    setConfirmedAt((n) => n + 1);
    const again = retry.current;
    retry.current = null;
    again?.();
  };

  const adoptRefusal = (error: unknown, next: () => void) => {
    const refusal = parseMainBranchRefusal(error);
    if (!refusal) return false;
    retry.current = refusal.code === "main_branch_protected" ? null : next;
    if (refusal.guard) state.adopt(refusal.guard, null);
    else void state.tryAgain();
    return true;
  };

  const section = needed ? (
    <div
      className="main-branch-write-section"
      id={sectionId}
      role="alert"
      data-testid="main-branch-write-section"
    >
      <MainBranchWriteText state={state} write={write} idPrefix={`${sectionId}-body`} />
      {guard && (
        <div className="main-branch-write-section__actions">
          <button type="button" className="btn btn--ghost btn--sm" onClick={cancel}>
            {blocked ? "Close" : "Cancel"}
          </button>
          {blocked ? (
            <>
              {blockAction && (
                <button
                  type="button"
                  className="btn btn--default btn--sm"
                  onClick={blockAction.run}
                >
                  {blockAction.label}
                </button>
              )}
              <button
                type="button"
                className="btn btn--default btn--sm"
                onClick={() => {
                  onCancel();
                  requestNewWorktree();
                }}
              >
                New worktree
              </button>
            </>
          ) : (
            <button
              type="button"
              className="btn btn--primary btn--sm"
              disabled={state.busy}
              onClick={() => void confirm()}
              data-testid="main-branch-write-section-confirm"
            >
              Confirm write to <BranchName name={guard.branch} />
            </button>
          )}
        </div>
      )}
    </div>
  ) : null;

  return {
    blocked: needed || pending,
    pending,
    describedBy: needed ? sectionId : undefined,
    section,
    adoptRefusal,
  };
}
