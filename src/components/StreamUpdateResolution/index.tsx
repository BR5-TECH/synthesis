/**
 * Where the author drives an update that stopped
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-KDVU,
 * WSS-FR-ZMPC).
 *
 * An update runs on its own and may stop on a question only the author can
 * answer. This window is where they answer it, run the update again, or put it
 * down. It renders the stream's update record and nothing it inferred, so what
 * it shows survives the dropdown closing, a window reload, and a relaunch
 * (WSS-FR-FVKO).
 *
 * It is dismissible on purpose (WSS-FR-KDVU): answering the question means
 * reading the files the update could not settle, and a window the author cannot
 * put down would block the decision it asks for. The row that opened it opens
 * it again. It never renders a merge: a merge is a merge run, handled in Runs
 * (WSS-FR-NRCQ).
 */

import { useEffect, useRef } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { EscalationForm } from "../GraduationRuns/escalation";
import { fateSentence, type ResolutionSubject } from "./subject";
import { refusalText } from "../WorkStreamSelector/refusals";

export { fateSentence, updateStateSentence, updateSubject } from "./subject";
export type { ResolutionSubject } from "./subject";

export interface StreamUpdateResolutionProps {
  streamName: string;
  streamBranch: string;
  /**
   * WSS-FR-PMYA: the update record this window was opened against, normalized.
   * Every act below reaches the update operations alone.
   */
  subject: ResolutionSubject;
  /** True while a call this window made is still out. */
  busy: boolean;
  /** WSS-FR-NPXC: a refusal of an action made here, in the words the author reads. */
  error: string | null;
  onClose: () => void;
  /**
   * GEA-FR-TEMG: answers the call's own promise. A refusal that answered as an
   * acceptance would close this window and drop everything the author typed.
   */
  onSend: (
    answers: Parameters<typeof api.answerWorkStreamUpdateEscalation>[1],
  ) => Promise<unknown>;
  onRetry: () => void;
  onCancelUpdate: () => void;
  onDismissUpdate: () => void;
  onError: (message: string) => void;
}

export function StreamUpdateResolution({
  streamName,
  streamBranch,
  subject,
  busy,
  error,
  onClose,
  onSend,
  onRetry,
  onCancelUpdate,
  onDismissUpdate,
  onError,
}: StreamUpdateResolutionProps) {
  // WSS-FR-KDVU: Escape puts the window down, like every other overlay of this
  // window.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const escalation = subject.escalation;
  const running = subject.running;

  // The window declares itself modal, so it takes the keyboard rather than
  // leaving focus on the row behind its own scrim, and gives it back to
  // whatever held it when the window closes.
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const returnFocusTo = useRef<Element | null>(null);
  useEffect(() => {
    returnFocusTo.current = document.activeElement;
    dialogRef.current
      ?.querySelector<HTMLElement>(
        'button, [href], input, textarea, select, [tabindex]:not([tabindex="-1"])',
      )
      ?.focus();
    return () => {
      const back = returnFocusTo.current;
      if (back instanceof HTMLElement && back.isConnected) back.focus();
    };
  }, []);

  /**
   * The keyboard stays inside the window while it stands (`aria-modal`).
   *
   * Tab from the last control returns to the first rather than walking into the
   * shell behind the scrim, which the author cannot see and cannot act on.
   */
  const trapTab = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? [],
    ).filter((el) => el.offsetParent !== null || el === document.activeElement);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <div
      className="scrim"
      data-testid="update-resolution-scrim"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        className="modal modal--wide update-resolution"
        role="dialog"
        aria-modal="true"
        aria-labelledby="update-resolution-title"
        data-testid="update-resolution"
        ref={dialogRef}
        onKeyDown={trapTab}
      >
        <div className="modal__head">
          <h2 className="modal__title" id="update-resolution-title">
            {streamName}
          </h2>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            aria-label="Close"
            data-testid="update-resolution-close"
            onClick={onClose}
          >
            Close
          </button>
        </div>

        <div className="modal__body">
          {/* WSS-FR-ZMPC: the stream, and the base branch, in the direction an
              update runs: the base branch into the stream. */}
          <p className="update-resolution__branches t-muted">
            {`${subject.baseBranch} into ${streamBranch}`}
          </p>

          <p className="update-resolution__state" role="status">
            {subject.sentence}
          </p>

          {/* WSS-FR-ZMPC / GEA-FR-ZRGP: the paths the update could not settle,
              named rather than counted. A decision between two versions of a
              requirement is a decision about named files. */}
          {subject.conflicts.length > 0 && (
            <section className="update-resolution__paths">
              <h3 className="update-resolution__paths-title">
                What the update could not settle
              </h3>
              <ul data-testid="update-resolution-paths">
                {subject.conflicts.map((conflict) => (
                  <li key={conflict.path}>
                    <span className="update-resolution__path">{conflict.path}</span>
                    <span className="update-resolution__fate t-muted">
                      {fateSentence(conflict)}
                    </span>
                  </li>
                ))}
              </ul>
            </section>
          )}

          {/* GEA-FR-BZQN / GEA-FR-LWQD: the escalation is answered here, on the
              terms GEA sets for every escalation, keyed by the stream. */}
          {escalation && (
            <EscalationForm
              ownerId={subject.streamId}
              escalation={escalation}
              busy={busy}
              onSend={(answers) => {
                logInfo(["frontend"], "work stream update answers sent", {
                  streamId: subject.streamId,
                  answers: answers.length,
                });
                // GEA-FR-TEMG / GEA-FR-VIPR: the real call's promise. A refusal
                // must reach the form that composed the set, which keeps every
                // answer and every question reachable rather than closing over
                // work the backend never took.
                return onSend(answers);
              }}
              onAnswered={onClose}
              onError={onError}
              describeError={refusalText}
            />
          )}
        </div>

        {/* WSS-FR-ZMPC: what the record's state permits, and nothing else. */}
        <div className="modal__actions">
          {error && (
            <p className="update-resolution__error" role="alert">
              {error}
            </p>
          )}
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            data-testid="update-resolution-dismiss"
            disabled={busy || running}
            onClick={() => {
              logInfo(["frontend"], "work stream update dismissed", {
                streamId: subject.streamId,
              });
              onDismissUpdate();
            }}
          >
            Dismiss
          </button>
          {running && (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              data-testid="update-resolution-cancel"
              onClick={() => {
                logWarn(
                  ["frontend"],
                  "work stream update cancellation requested",
                  { streamId: subject.streamId },
                );
                onCancelUpdate();
              }}
            >
              Cancel update
            </button>
          )}
          {subject.retryable && (
            <button
              type="button"
              className="btn btn--primary btn--sm"
              data-testid="update-resolution-retry"
              disabled={busy}
              onClick={() => {
                logInfo(["frontend"], "work stream update retry requested", {
                  streamId: subject.streamId,
                });
                onRetry();
              }}
            >
              Retry update
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
