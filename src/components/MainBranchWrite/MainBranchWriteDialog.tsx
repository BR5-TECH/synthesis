/**
 * The standalone confirmation of a write to a main branch
 * (`../../../specifications/ui/MBW-main-branch-write.md` MBW-FR-RXAR,
 * MBW-FR-XBJS, MBW-FR-JZME, MBW-FR-UNAN, MBW-FR-GYVD, MBW-FR-WCBQ).
 *
 * The shell mounts it for the request of `state/mainBranchWrite`. It is
 * modal, takes focus on **Cancel** when it opens, keeps focus inside itself,
 * and returns focus to the control that opened it when it closes.
 */
import { useEffect, useRef, useState } from "react";

import {
  logConfirmation,
  needsConfirmation,
  settleMainBranchWrite,
  type MainBranchWriteRequest,
} from "../../state/mainBranchWrite";
import { requestNewWorktree } from "../../state/newWorktreeRequests";
import { BranchName, MainBranchWriteText, isProtectedBranch } from "./Content";
import { usePanelState } from "./usePanelState";

export function MainBranchWriteDialog({ request }: { request: MainBranchWriteRequest }) {
  const { write } = request;
  const state = usePanelState({
    write,
    branch: request.branch,
    surface: "dialog",
    initialGuard: request.guard,
    initialError: request.readError,
  });
  const { guard } = state;
  const surfaceRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const [opener] = useState(() => document.activeElement as HTMLElement | null);
  /**
   * MBW-FR-JZME: **New worktree** closes the confirmation to open the New
   * worktree dialog, which takes focus itself. Focus then does not go back to
   * the opener behind that dialog.
   */
  const handedOff = useRef(false);
  const protectedBranch = isProtectedBranch(state);
  const branch = guard?.branch ?? request.branch;

  const close = (confirmed: boolean) => settleMainBranchWrite(confirmed);
  const cancel = () => {
    logConfirmation("cancelled", write, branch, "dialog");
    close(false);
  };
  const cancelRef2 = useRef(cancel);
  cancelRef2.current = cancel;

  // A request with neither a guard nor an error reads the guard itself.
  useEffect(() => {
    if (!request.guard && !request.readError) void state.tryAgain();
    // Mount-only: the request does not change under a mounted dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // MBW-FR-RMCO: a read that finds no confirmation needed lets the write go.
  useEffect(() => {
    if (guard && !needsConfirmation(guard)) close(true);
  }, [guard]);

  // MBW-FR-PJTG: shown, once per guard it shows.
  const logged = useRef<string | null>(null);
  useEffect(() => {
    if (!guard || !needsConfirmation(guard)) return;
    const key = `${guard.branch}:${guard.protection}`;
    if (logged.current === key) return;
    logged.current = key;
    logConfirmation("shown", write, guard.branch, "dialog");
  }, [guard, write]);

  // MBW-FR-GYVD: focus on Cancel, kept inside, and given back to the opener.
  useEffect(() => {
    cancelRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        cancelRef2.current();
        return;
      }
      if (e.key !== "Tab") return;
      const items = Array.from(
        surfaceRef.current?.querySelectorAll<HTMLElement>("button:not([disabled])") ?? [],
      );
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey && (active === first || !surfaceRef.current?.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (active === last || !surfaceRef.current?.contains(active))) {
        e.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      if (!handedOff.current && opener?.isConnected) opener.focus();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const confirm = async () => {
    if (await state.confirm()) close(true);
  };

  const title = (
    <>Write to {branch ? <BranchName name={branch} /> : "the main branch"}?</>
  );

  return (
    <div
      className="scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) cancel();
      }}
    >
      <div
        ref={surfaceRef}
        className="modal main-branch-write"
        role="dialog"
        aria-modal="true"
        aria-labelledby="main-branch-write-title"
        aria-describedby="main-branch-write-text"
        data-testid="main-branch-write-dialog"
      >
        <div className="modal__head">
          <h2 className="modal__title" id="main-branch-write-title">
            {title}
          </h2>
        </div>
        <div className="modal__body">
          <MainBranchWriteText state={state} write={write} idPrefix="main-branch-write" />
        </div>
        <div className="modal__actions">
          {protectedBranch && (
            <button
              type="button"
              className="btn btn--ghost"
              onClick={() => {
                logConfirmation("cancelled", write, branch, "dialog");
                handedOff.current = true;
                close(false);
                requestNewWorktree();
              }}
            >
              New worktree
            </button>
          )}
          <button
            ref={cancelRef}
            type="button"
            className="btn btn--ghost"
            onClick={cancel}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--primary"
            disabled={!guard || state.busy}
            onClick={() => void confirm()}
            data-testid="main-branch-write-confirm"
          >
            Confirm write to {guard ? <BranchName name={guard.branch} /> : "the branch"}
          </button>
        </div>
      </div>
    </div>
  );
}
