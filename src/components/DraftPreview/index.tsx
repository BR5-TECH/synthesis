/**
 * The draft preview tab (`DCR-draft-change-review.md` DCR-FR-HWLA,
 * DCR-FR-RJZP; `TAB-tabs.md` TAB-FR-CIGU).
 *
 * It shows the body of one new-draft proposal, read-only, rendered as a New
 * Artifact tab renders its prompt. It creates no draft and no file, and it
 * re-reads the body when the proposal is updated.
 */
import { useEffect, useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import { newDraftProposalById, useNewDraftStore } from "../../state/newDraftProposals";
import { DraftVersionReading } from "../DraftVersionReading";

export function DraftPreviewView({ proposalId, label }: { proposalId: string; label: string }) {
  useNewDraftStore();
  const proposal = newDraftProposalById(proposalId);
  const revision = proposal?.revision ?? 0;
  const [body, setBody] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  // DCR-FR-RJZP: a `change` update bumps the revision, and the body is read again.
  useEffect(() => {
    let live = true;
    api.loadNewDraftProposalBody(proposalId).then(
      (answer) => {
        if (!live) return;
        setBody(answer.body);
        setFailed(false);
      },
      (error: unknown) => {
        if (!live) return;
        setFailed(true);
        logWarn(["frontend"], "draft preview body could not be read", {
          proposalId,
          error: String(error),
        });
      },
    );
    return () => {
      live = false;
    };
  }, [proposalId, revision]);

  const place = proposal ? `Part ${proposal.position} of ${proposal.total}` : null;
  return (
    <div className="draft-preview" data-testid="draft-preview">
      <div className="draft-reading draft-preview__strip">
        <span className="draft-reading__state">
          Proposed draft{place ? ` · ${place}` : ""} · Read-only. Nothing is created until you accept it.
        </span>
      </div>
      {failed ? (
        <p className="draft-preview__error" role="alert">
          The proposed draft could not be read.
        </p>
      ) : body === null ? (
        <p className="draft-preview__loading">Reading the proposed draft…</p>
      ) : (
        <DraftVersionReading
          text={body}
          mode="wysiwyg"
          label={`Read-only preview of the proposed draft ${label}`}
        />
      )}
    </div>
  );
}
