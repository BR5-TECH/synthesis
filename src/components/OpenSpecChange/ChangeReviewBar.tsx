/**
 * The review bar of a change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-EJGH).
 *
 * While the change holds a pending edit proposal, the bar above the reading
 * pane states how many hunks are still undecided across all the files of the
 * change, and offers Reject all. Each hunk is decided in the Source of its
 * file, so the bar has no control that moves between hunks: the artifact list
 * already marks where they are (OCW-FR-RRXG).
 */
import { useEffect, useRef, useState } from "react";

import { logInfo } from "../../logging";
import { Modal } from "../DraftsPanelParts";

function changes(count: number): string {
  return count === 1 ? "1 undecided change" : `${count} undecided changes`;
}

export function ChangeReviewBar({
  undecided,
  onRejectAll,
}: {
  /** OCW-FR-EJGH: the count of undecided hunks across the change. */
  undecided: number;
  /** Resolves null when the decline landed, or the refusal in words. */
  onRejectAll: () => Promise<string | null>;
}) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const opener = useRef<HTMLButtonElement | null>(null);

  // A refusal keeps the bar. The confirmation closed while the button was
  // disabled, so the focus is on the document body: give it back to the
  // button.
  useEffect(() => {
    if (error === null || busy) return;
    const active = document.activeElement;
    if (active === null || active === document.body) opener.current?.focus();
  }, [error, busy]);

  const reject = async () => {
    setConfirming(false);
    setBusy(true);
    setError(null);
    logInfo(["frontend"], "Reject all of a change edit proposal was confirmed", { undecided });
    const refused = await onRejectAll();
    setBusy(false);
    setError(refused);
  };

  return (
    <div
      className="openspec-review"
      role="region"
      aria-label="Proposed edits"
      data-testid="openspec-review-bar"
    >
      <span className="openspec-review__count t-ui-sm" role="status" data-testid="openspec-review-count">
        {undecided === 0
          ? "Every proposed change is decided."
          : `${changes(undecided)} in this change. Decide each one in the Source of its file.`}
      </span>
      <span className="spacer" />
      <button
        ref={opener}
        type="button"
        className="btn btn--sm"
        disabled={busy || undecided === 0}
        onClick={() => setConfirming(true)}
      >
        Reject all
      </button>
      {error !== null && (
        <p className="openspec-review__error t-ui-xs" role="alert">
          {error}
        </p>
      )}
      {confirming && (
        <Modal label="Reject all" returnFocus={opener.current} onClose={() => setConfirming(false)}>
          <div className="modal__head">
            <div className="modal__title">Reject all</div>
          </div>
          <div className="modal__body">
            <p className="t-ui-sm" data-testid="openspec-reject-all-statement">
              Reject the {changes(undecided)} of this proposal? No file of the change is written.
            </p>
          </div>
          <div className="modal__actions">
            <button type="button" className="btn btn--ghost" onClick={() => setConfirming(false)}>
              Cancel
            </button>
            <button type="button" className="btn btn--danger" onClick={() => void reject()}>
              Reject all
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
