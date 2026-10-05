/**
 * The top-chrome **Push** control
 * (`../../specifications/ui/GIT-git.md` GIT-FR-XXLE, GIT-FR-QMYB, GIT-FR-CNQO,
 * GIT-FR-IMSH).
 *
 * It publishes the active branch's commits through `"push current branch"` and
 * does nothing else: it commits nothing, fetches nothing, and leaves the
 * active worktree and its branch as they are. The push is the one the Git panel
 * and the Changes panel start (GIT-FR-QMYB), so the three controls share one
 * running state and one transcript, which the Git panel renders. This control
 * adds only its own note for the two token causes that need an action from the
 * author.
 */
import { useId, useRef, useState } from "react";

import {
  canStartPush,
  PUSH_RUNNING_REASON,
  pushUnavailableReason,
} from "../gitSync";
import { useGitTransfer } from "../hooks/useGitTransfer";
import { Icon } from "./icons";

export interface PushControlProps {
  /**
   * GIT-FR-XXLE: whether the open project's content root sits inside a Git
   * repository. Nothing renders when it does not.
   */
  inRepository: boolean;
  /**
   * GIT-FR-IMSH / GHA-FR-16: open the token picker. Resolves with whether a
   * token was chosen, so the push can run once more or be abandoned.
   */
  onRequestGithubToken: () => Promise<boolean>;
  /** GIT-FR-IMSH / GHA-FR-19: the route for a project with no stored token. */
  onOpenGlobalSettings: () => void;
}

interface PushNote {
  message: string;
  missingToken: boolean;
}

export function PushControl({
  inRepository,
  onRequestGithubToken,
  onOpenGlobalSettings,
}: PushControlProps) {
  const transfer = useGitTransfer();
  const [note, setNote] = useState<PushNote | null>(null);
  const reasonId = useId();
  // Held in a ref so a second activation in the same tick sees the first.
  const startingRef = useRef(false);

  if (!inRepository) return null;

  const available = canStartPush(transfer.sync, transfer.running);
  const reason = pushUnavailableReason(transfer.sync, transfer.running);
  const state = transfer.running
    ? "busy"
    : available
      ? "available"
      : "unavailable";

  /**
   * GIT-FR-IMSH: `retried` bounds the token-selection loop to one retry, as
   * CHG-FR-45 does for the Changes panel.
   */
  const start = async (retried = false): Promise<void> => {
    const outcome = await transfer.pushBranch();
    if (outcome.ok) return;
    if (outcome.cause === "busy") {
      // Another control began a push while the picker was open.
      if (retried) setNote({ message: PUSH_RUNNING_REASON, missingToken: false });
      return;
    }
    if (outcome.cause === "selection_required" && !retried) {
      const chosen = await onRequestGithubToken();
      if (chosen) return start(true);
      setNote({
        message: "Push cancelled — no GitHub token was selected.",
        missingToken: false,
      });
      return;
    }
    if (outcome.cause === "token_missing") {
      setNote({ message: "Push needs a GitHub token.", missingToken: true });
    }
    // Any other failure is already in the Git panel's output area, written by
    // the terminal event, and is not repeated here.
  };

  const activate = () => {
    // GIT-FR-CNQO: an unavailable control stays focusable and starts nothing.
    if (!available || startingRef.current) return;
    startingRef.current = true;
    setNote(null);
    void start().finally(() => {
      startingRef.current = false;
    });
  };

  return (
    <div className="push-control">
      <button
        type="button"
        className="btn btn--ghost btn--icon btn--sm push-control__button"
        data-testid="top-push"
        data-state={state}
        aria-label="Push"
        aria-disabled={!available}
        aria-busy={transfer.running}
        aria-describedby={reason ? reasonId : undefined}
        // GIT-FR-CNQO: one tooltip only. While a reason shows in its own
        // bubble, a native `title` would add a second one over it.
        title={reason ? undefined : "Push the branch's commits to its remote"}
        onClick={activate}
      >
        {/* The busy state keeps this icon, which the stylesheet pulses. The
            refresh control's icon is not reused for it (GIT-FR-CNQO). */}
        <Icon.PushUp size={13} />
      </button>
      {reason && (
        <>
          <span id={reasonId} className="sr-only" data-testid="top-push-reason">
            {reason}
          </span>
          {/* GIT-FR-CNQO: the reason in words, visible on hover and on keyboard
              focus. The `title` alone appears for a pointer only. A note uses
              the same place below the control, so the bubble is not shown
              while a note is open. */}
          {!note && (
            <span className="push-control__reason" aria-hidden="true">
              {reason}
            </span>
          )}
        </>
      )}
      {note && (
        <div
          className="push-control__note"
          role="status"
          data-testid="top-push-note"
        >
          <span className="push-control__note-text">⚠ {note.message}</span>
          {note.missingToken && (
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              data-testid="top-push-note-settings"
              onClick={() => {
                setNote(null);
                onOpenGlobalSettings();
              }}
            >
              Global settings
            </button>
          )}
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Dismiss"
            data-testid="top-push-note-dismiss"
            onClick={() => setNote(null)}
          >
            <Icon.X size={11} />
          </button>
        </div>
      )}
    </div>
  );
}
