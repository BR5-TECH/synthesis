/**
 * The card of one proposed new change of a split
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-BGDN,
 * OCW-FR-IJAI), on the terms of the new-draft card
 * (`../DraftDiscussion/stream/NewDraftCard.tsx`).
 *
 * It reads its proposal from the one reading per change that
 * `../../state/openspec/newChangeProposals.ts` keeps, and it decides nothing
 * but on Accept and Reject.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import { openOpenSpecChange } from "../../state/openspec/changeRoutes";
import {
  discussNewChange,
  ensureNewChangesLoaded,
  newChangesOf,
  useNewChangeStore,
} from "../../state/openspec/newChangeProposals";
import {
  NEW_CHANGE_ERRORS,
  participantName,
  type Comment,
  type NewChangeProposal,
  type NewChangeProposalReference,
  type NewChangeState,
} from "../../types";
import { formatRelative } from "../ProjectPicker";
import { TypeChip } from "../DraftDiscussion/stream/TypeChip";
import { refusalCode } from "./decisions";
import { newChangeBusyMessage, newChangeMessage, settleNewChangeDecision } from "./newChangeDecisions";

/** OCW-FR-BGDN: each state in words, with its own marker. */
const STATE: Record<NewChangeState, { word: string; mark: string }> = {
  pending: { word: "Waiting for you", mark: "●" },
  accepted: { word: "Accepted", mark: "✓" },
  rejected: { word: "Rejected", mark: "✕" },
  withdrawn: { word: "Withdrawn", mark: "–" },
};

/** The newest comment of the thread that names this proposal. */
function newestReferenceOf(comments: readonly Comment[], proposalId: string): string | null {
  for (let i = comments.length - 1; i >= 0; i -= 1) {
    const c = comments[i];
    if (c.attachments.some((a) => a.kind === "newChangeProposal" && a.proposalId === proposalId)) {
      return c.id;
    }
  }
  return null;
}

/** OCW-FR-BGDN: View files, read-only. */
function ProposedFiles({ proposal }: { proposal: NewChangeProposal }) {
  const [files, setFiles] = useState<{ path: string; text: string }[] | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    let live = true;
    setFailed(false);
    setFiles(null);
    api.loadNewChangeProposalFiles(proposal.id).then(
      (answer) => {
        if (live) setFiles(Array.isArray(answer?.files) ? answer.files : []);
      },
      (e: unknown) => {
        if (!live) return;
        setFailed(true);
        logWarn(["frontend"], "the files of a proposed change could not be read", {
          proposalId: proposal.id,
          error: refusalCode(e).slice(0, 80),
        });
      },
    );
    return () => {
      live = false;
    };
  }, [proposal.id, proposal.revision]);
  if (failed) {
    return (
      <p className="dds-newdraft__error" role="alert">
        The proposed files could not be read.
      </p>
    );
  }
  if (files === null) {
    return (
      <p className="dds-newdraft__reason" role="status">
        Reading the proposed files…
      </p>
    );
  }
  return (
    <div className="openspec-newchange__files" data-testid="new-change-files">
      {files.map((file) => (
        <section key={file.path} className="openspec-newchange__file">
          <span className="openspec-newchange__file-path t-meta">{file.path}</span>
          <pre className="openspec-source__text">{file.text}</pre>
        </section>
      ))}
    </div>
  );
}

export function NewChangeCard({
  reference,
  comment,
  comments,
}: {
  reference: NewChangeProposalReference;
  /** The comment that carries this reference. */
  comment: Comment;
  /** Every comment of the thread, to find the newest reference. */
  comments: readonly Comment[];
}) {
  useNewChangeStore();
  useEffect(
    () => ensureNewChangesLoaded(reference.streamId, reference.change),
    [reference.streamId, reference.change],
  );
  const { proposals, split } = newChangesOf(reference.streamId, reference.change);
  const proposal = proposals.find((p) => p.id === reference.proposalId);
  const [feedback, setFeedback] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showFiles, setShowFiles] = useState(false);
  const cardRef = useRef<HTMLElement | null>(null);

  if (!proposal) {
    return (
      <div className="dds-newdraft-ref" data-testid="new-change-card-unread">
        <TypeChip kind="newDraft" /> Proposed change
      </div>
    );
  }

  const place = `Part ${proposal.position} of ${proposal.total}`;
  const state = STATE[proposal.state];

  // An earlier comment of the same proposal is one line.
  if (newestReferenceOf(comments, proposal.id) !== comment.id) {
    const verb =
      reference.update === "change"
        ? "Changed the proposed change"
        : reference.update === "withdraw"
          ? "Withdrew the proposed change"
          : "Proposed a change";
    return (
      <div className="dds-newdraft-ref" data-testid="new-change-reference">
        <TypeChip kind="newDraft" /> {verb} · {place} · {proposal.name}
      </div>
    );
  }

  const confirmed = Boolean(split?.confirmedAt);
  const decide = (accept: boolean) => {
    setBusy(true);
    setError(null);
    const sent = feedback.trim();
    const call = accept
      ? api.acceptNewChangeProposal(proposal.id, sent === "" ? null : sent)
      : api.rejectNewChangeProposal(proposal.id, sent === "" ? null : sent);
    call.then(
      (outcome) => {
        setBusy(false);
        setFeedback("");
        settleNewChangeDecision(outcome, sent);
        requestAnimationFrame(() => cardRef.current?.focus());
      },
      async (e: unknown) => {
        const code = refusalCode(e);
        logWarn(["frontend"], "a proposed change could not be decided", {
          streamId: proposal.streamId,
          proposalId: proposal.id,
          code: code.slice(0, 80),
        });
        // OCW-FR-IJAI: `stream_busy` names the run that holds the stream.
        const message = code.includes(NEW_CHANGE_ERRORS.streamBusy)
          ? await newChangeBusyMessage(proposal.streamId)
          : newChangeMessage(code);
        setBusy(false);
        setError(message);
      },
    );
  };

  const discuss = () => {
    // A card outside the change's own tab opens that tab.
    if (!discussNewChange(proposal)) {
      openOpenSpecChange({ kind: "stream", streamId: proposal.streamId }, proposal.change);
    }
  };

  const created = proposal.createdChange ?? null;

  return (
    <article
      ref={cardRef}
      tabIndex={-1}
      className="dds-question dds-newdraft openspec-newchange"
      data-testid="new-change-card"
      data-state={proposal.state}
      aria-label={`Proposed change, part ${proposal.position} of ${proposal.total}, ${proposal.name}, ${state.word}`}
    >
      <div className="dds-question__head">
        <TypeChip kind="newDraft" />
        <span className="dds-question__author">{participantName(comment.author)}</span>
        <span className="dds-stream__time" title={comment.createdAt}>
          {formatRelative(comment.createdAt)}
        </span>
      </div>
      <div className="dds-newdraft__title">
        <span className="dds-newdraft__name">
          {place} · {proposal.name}
        </span>
        <span className="dds-newdraft__state" data-state={proposal.state}>
          <span aria-hidden="true">{state.mark}</span> {state.word}
        </span>
      </div>
      {/* OCW-FR-BGDN: the paths of its files. */}
      <ul className="openspec-newchange__paths" aria-label="Proposed files">
        {proposal.paths.map((path) => (
          <li key={path} className="t-meta">
            {path}
          </li>
        ))}
      </ul>
      {/* OCW-FR-IJAI: the created change opens its tab. */}
      {proposal.state === "accepted" && created !== null && (
        <p className="dds-newdraft__created">
          Created change{" "}
          <button
            type="button"
            className="btn btn--ghost btn--sm openspec-newchange__created"
            onClick={() => openOpenSpecChange({ kind: "stream", streamId: proposal.streamId }, created)}
          >
            {created}
          </button>
          {proposal.commitOwed && " · its commit is owed"}
        </p>
      )}
      <div className="dds-newdraft__decide">
        {proposal.state === "pending" && (
          <input
            className="dds-newdraft__feedback"
            type="text"
            value={feedback}
            placeholder="Feedback (optional)"
            aria-label="Feedback for the agent (optional)"
            onChange={(e) => setFeedback(e.target.value)}
          />
        )}
        <div className="dds-newdraft__actions">
          {proposal.state === "pending" && (
            <>
              <button
                type="button"
                className="btn btn--primary btn--sm"
                disabled={busy || confirmed}
                onClick={() => decide(true)}
              >
                Accept
              </button>
              <button
                type="button"
                className="btn btn--sm"
                disabled={busy || confirmed}
                onClick={() => decide(false)}
              >
                Reject
              </button>
              <button type="button" className="btn btn--ghost btn--sm" onClick={discuss}>
                Discuss →
              </button>
            </>
          )}
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            aria-expanded={showFiles}
            onClick={() => setShowFiles((open) => !open)}
          >
            {showFiles ? "Hide files" : "View files"}
          </button>
        </div>
        {proposal.state === "pending" && confirmed && (
          <p className="dds-newdraft__reason">{newChangeMessage(NEW_CHANGE_ERRORS.splitConfirmed)}</p>
        )}
      </div>
      {error !== null && (
        <p className="dds-newdraft__error" role="alert" data-testid="new-change-refusal">
          {error}
        </p>
      )}
      {showFiles && <ProposedFiles proposal={proposal} />}
    </article>
  );
}
