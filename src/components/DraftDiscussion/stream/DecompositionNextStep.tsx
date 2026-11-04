/**
 * The What next? row below the newest decided new-draft card
 * (`DCR-draft-change-review.md` DCR-FR-KNDA, DCR-FR-UVMY, DCR-FR-BELO,
 * DCR-FR-WRQT, DCR-FR-PBDD, DCR-FR-TOOT, DCR-FR-FKNQ).
 */
import { useState } from "react";

import * as api from "../../../api";
import { logInfo, logWarn } from "../../../logging";
import type { NewDraftProposal } from "../../../types";
import { askForNextDraft } from "./decompositionDispatch";

export function DecompositionNextStep({
  newest,
  accepted,
  blocked,
  onDone,
}: {
  /** The newest decided proposal of the set. */
  newest: NewDraftProposal;
  /** DCR-FR-WRQT: the accepted proposals; Confirm is offered for one or more. */
  accepted: readonly NewDraftProposal[];
  /** DCR-FR-PBDD: why the original draft takes no confirmation, or null. */
  blocked: string | null;
  /** DCR-FR-FKNQ: the row is gone, so the card takes focus. */
  onDone?: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [asked, setAsked] = useState(false);

  const confirm = () => {
    setBusy(true);
    setError(null);
    api.confirmDraftDecomposition(newest.draftId).then(
      () => {
        setBusy(false);
        logInfo(["frontend"], "decomposition confirmed", { draftId: newest.draftId });
        onDone?.();
      },
      (e: unknown) => {
        setBusy(false);
        setError(String(e));
        logWarn(["frontend"], "decomposition could not be confirmed", {
          draftId: newest.draftId,
          error: String(e).slice(0, 80),
        });
      },
    );
  };

  const ask = () => {
    setBusy(true);
    void askForNextDraft(newest).then((n) => {
      setBusy(false);
      setAsked(n > 0);
      if (n > 0) onDone?.();
      if (n === 0) setError("No agent could be asked. Write to the agent in the discussion.");
    });
  };

  return (
    <div className="dds-next-step" role="group" aria-label="What next?" data-testid="decomposition-next-step">
      <span className="dds-next-step__label">What next?</span>
      <div className="dds-next-step__actions">
        <button type="button" className="btn btn--sm" disabled={busy || asked} onClick={ask}>
          Ask for next draft
        </button>
        {accepted.length > 0 && (
          <button
            type="button"
            className="btn btn--primary btn--sm"
            disabled={busy || blocked !== null}
            onClick={confirm}
          >
            Confirm decomposition done
          </button>
        )}
      </div>
      {accepted.length > 0 && blocked !== null && (
        <p className="dds-next-step__reason">{blocked}</p>
      )}
      {error !== null && (
        <p className="dds-next-step__error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}
