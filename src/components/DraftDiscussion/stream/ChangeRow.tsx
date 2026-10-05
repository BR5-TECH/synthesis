/**
 * A change reference, as one row of the column
 * (`../../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-XHRB,
 * DDS-FR-MCUP, DDS-FR-YSNB).
 *
 * The chip, the file name, how many changes it holds, the state word, and the
 * time — on one line. The row **is** the control that opens the review, in place
 * of the chip a card renders below a body (per
 * `../../../../specifications/ui/CMT-comments.md` CMT-FR-48). It fetches nothing
 * (CMT-FR-49): the state comes from the proposal store the reference is read
 * through, and the reference itself carries only identity (CMT-FR-67).
 *
 * A reading that has not answered yet says **nothing** about the proposal
 * (CTA-FR-UKIG). The row is inert and carries no state word until a reading
 * either returns the proposal or reports it gone, so nothing on it can be read
 * as a claim about what the proposal has become.
 */
import {
  openReview,
  undecidedHunks,
  useProposalReference,
} from "../../../state/draftProposals";
import {
  openPromptReviewFor,
  usePromptProposalReference,
} from "../../../state/promptProposals";
import { setFocusedHunk } from "../../../state/draftDiscussion";
import { formatRelative } from "../../ProjectPicker";
import { TypeChip } from "./TypeChip";
import type { ProposalState } from "../../../types";
import type { ChangeReference } from "./streamItems";

/**
 * DDS-FR-YSNB: one lowercase word, in that state's own tone.
 *
 * `null` while the reading has not answered — the row then states nothing
 * (CTA-FR-UKIG).
 */
export function stateWord(
  state: ProposalState | null,
  gone: boolean,
): { word: string; tone: "ok" | "danger" | "warn" | "quiet" } | null {
  if (state === "accepted") return { word: "accepted", tone: "ok" };
  if (state === "rejected") return { word: "rejected", tone: "danger" };
  if (state === "pending") return { word: "pending", tone: "warn" };
  return gone ? { word: "unavailable", tone: "quiet" } : null;
}

export interface ChangeRowProps {
  reference: ChangeReference;
  /** The time the comment carrying the reference was written. */
  createdAt: string;
}

export function ChangeRow(props: ChangeRowProps) {
  return props.reference.kind === "proposal" ? (
    <DraftChangeRow {...props} reference={props.reference} />
  ) : (
    <PromptChangeRow {...props} reference={props.reference} />
  );
}

/** A change to one file of the draft this column belongs to. */
function DraftChangeRow({
  reference,
  createdAt,
}: {
  reference: Extract<ChangeReference, { kind: "proposal" }>;
  createdAt: string;
}) {
  const read = useProposalReference(reference.draftId, reference.proposalId);
  const proposal = read.kind === "known" ? read.proposal : null;

  return (
    <Row
      path={reference.path}
      changes={proposal?.hunkCount ?? 0}
      state={proposal?.state ?? null}
      gone={read.kind === "gone"}
      createdAt={createdAt}
      onOpen={() => {
        // DCR-FR-HVXK / DCR-FR-03: the route to the review is the document
        // column scrolling to the first change still to decide.
        if (proposal) {
          const first = undecidedHunks(proposal)[0];
          if (first) setFocusedHunk(proposal.draftId, first.id);
        }
        openReview(reference.proposalId);
      }}
    />
  );
}

/** A change to a prompt artifact the project already holds. */
function PromptChangeRow({
  reference,
  createdAt,
}: {
  reference: Extract<ChangeReference, { kind: "promptProposal" }>;
  createdAt: string;
}) {
  const read = usePromptProposalReference(
    reference.artifactId,
    reference.proposalId,
  );
  const proposal = read.kind === "known" ? read.proposal : null;

  return (
    <Row
      path={reference.path}
      // PCP records no change count on a prompt proposal, so the row states
      // none rather than a zero that would read as "changes nothing".
      changes={0}
      state={proposal?.state ?? null}
      gone={read.kind === "gone"}
      createdAt={createdAt}
      onOpen={() => openPromptReviewFor(reference.artifactId, reference.proposalId)}
    />
  );
}

function Row({
  path,
  changes,
  state,
  gone,
  createdAt,
  onOpen,
}: {
  path: string;
  changes: number;
  state: ProposalState | null;
  gone: boolean;
  createdAt: string;
  onOpen: () => void;
}) {
  const status = stateWord(state, gone);
  return (
    <button
      type="button"
      className="dds-change"
      data-testid="dds-change-row"
      data-state={state ?? (gone ? "missing" : "unresolved")}
      // A row whose reading has not answered opens nothing, so it does not
      // offer to (CTA-FR-UKIG).
      disabled={state === null}
      // The row is the whole control, so its accessible name has to carry
      // everything the line says (DDS-FR-FKZL, CMT-FR-35).
      aria-label={
        status === null
          ? path
          : `${path}, ${changes} ${changes === 1 ? "change" : "changes"}, ${status.word}`
      }
      // The row truncates a long file name, so the whole of it is readable on
      // hover — the path is what says which artifact the row is about.
      title={path}
      onClick={onOpen}
    >
      <TypeChip kind="change" />
      <span className="dds-change__path">{path}</span>
      {changes > 0 && (
        <span className="dds-change__count">
          {changes} {changes === 1 ? "change" : "changes"}
        </span>
      )}
      <span className="dds-stream__spacer" />
      {status !== null && (
        <>
          {/* DDS-FR-YSNB: a dot and one lowercase word, so the state is legible
              without colour discrimination. */}
          <span
            className="dds-change__dot"
            data-tone={status.tone}
            aria-hidden="true"
          />
          <span className="dds-change__state" data-tone={status.tone}>
            {status.word}
          </span>
        </>
      )}
      <span className="dds-stream__time" title={createdAt}>
        {formatRelative(createdAt)}
      </span>
    </button>
  );
}
