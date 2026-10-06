// Comments (CMS-comments-storage.md / CMT-comments.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { GITHUB_TOKEN_ERRORS } from "./github";

// --- Comments (CMS-comments-storage.md / CMT-comments.md) -----------------

/**
 * CMS-FR-09: who produced a comment or a discussion event.
 *
 * A tagged union rather than one shape with an `is_agent` flag, so nothing about
 * a human's record has to bend to fit an agent — a human is a GitHub account, an
 * agent is an integration — and a third kind slots in without touching either.
 */
export type Participant =
  | {
      kind: "human";
      /** GitHub account login. */
      login: string;
      displayName?: string;
      email?: string;
    }
  | {
      kind: "agent";
      agentId: string;
      /** Short display handle, e.g. `claude`. */
      handle: string;
      model?: string;
      /**
       * CMS-FR-65: the agent's title as it stood when this comment was written
       * — a snapshot, never the agent's current title.
       *
       * Optional because absence is a real state: a comment written before the
       * field existed carries none, and nothing may fill one in for it. Every
       * comment this build writes carries the field, the empty string included
       * for an agent that has no title, so only an older log yields
       * `undefined`. Both render alike (CTA-FR-KYPK).
       */
      title?: string;
    };

/** CMS-FR-HTOA: the display name of the local participant. */
export const LOCAL_PARTICIPANT_NAME = "Me";

/**
 * CMS-FR-HTOA: whether `p` is the fixed local participant a project without a
 * GitHub token writes as — a human with no GitHub login.
 */
export function isLocalParticipant(p: Participant): boolean {
  return p.kind === "human" && p.login === "";
}

/**
 * The label the rail renders for a participant as it was stamped (CMT-FR-10).
 *
 * The local participant reads **Me** here. A surface that renders one shows the
 * current project identity instead (CMT-FR-ZCAE) through `useParticipantLabel`.
 */
export function participantName(p: Participant): string {
  if (p.kind === "agent") return p.handle;
  return isLocalParticipant(p) ? LOCAL_PARTICIPANT_NAME : p.login;
}

/**
 * CTA-FR-KFUF: the title line a comment renders beneath its author's name, or
 * `null` where it renders none.
 *
 * The single place the rule lives, so the rail, the Comments panel, every
 * conversation presentation, and the draft-change review cannot disagree about
 * it. Three states collapse to one outcome: a human participant, an agent whose
 * snapshot is absent, and an agent whose snapshot is empty all render no line
 * and no placeholder — never `Not defined`, which belongs to a prompt's
 * substitution alone (CVL-FR-04) and appears in no comment anywhere.
 *
 * It reads the participant it was handed and resolves nothing: a historical
 * comment shows what it recorded, not what the agent is called today.
 */
export function participantTitle(p: Participant): string | null {
  if (p.kind !== "agent") return null;
  const title = p.title ?? "";
  return title === "" ? null : title;
}

/**
 * The range part of a fragment target: a range in the source text together with
 * the exact substring it covered.
 *
 * Offsets are counted in UTF-16 code units on this side — the unit
 * `String.prototype.slice` and a DOM selection both speak. The quote is what
 * re-anchoring searches for when the offsets have gone stale.
 */
export interface FragmentRange {
  start: number;
  end: number;
  quote: string;
}

/** CMS-FR-16: a comment quoting an earlier one in the same discussion. */
export interface CommentQuote {
  commentId: string;
  excerpt: string;
}

/**
 * CMS-FR-42: an attachment as the backend stores and serves it.
 *
 * Three kinds rather than one shape with optional fields, because what an
 * attachment *is* differs at every use site: a `url` is an address the backend
 * records and never resolves, a `blob` is content it holds, named by the SHA-256
 * of its bytes so the same picture attached twice is one file, and a `proposal`
 * holds no content at all.
 */
export type Attachment =
  | {
      kind: "url";
      url: string;
      /** IANA media type, e.g. `image/png`. */
      mediaType: string;
      label?: string;
    }
  | {
      kind: "blob";
      /** Lowercase hex SHA-256 of the stored bytes; what reads it back. */
      digest: string;
      mediaType: string;
      filename: string;
      /** Byte length of the stored content. */
      bytes: number;
    }
  /**
   * CMS-FR-60: a reference to a change an agent proposed to a draft file.
   *
   * Carries the proposal's **identity** and nothing else, which is what makes it
   * immutable on the same terms as the other two even though the proposal it
   * names gets accepted or declined afterwards: the state lives in
   * `DCP-draft-change-proposals.md` and is read from there (CMT-FR-67).
   *
   * The rail renders it as a **control** rather than a preview (CMT-FR-48) and
   * fetches nothing for it (CMT-FR-49). No composer can produce one — there is
   * no matching {@link AttachmentInput} variant (CMT-FR-45).
   */
  | {
      kind: "proposal";
      proposalId: string;
      draftId: string;
      /** Draft-relative path the proposal would replace. */
      path: string;
    }
  /**
   * CMS-FR-66: a reference to a change an agent proposed to a **prompt artifact
   * the project already holds**.
   *
   * Separate from `proposal` rather than one kind carrying a target, because the
   * two name two stores with two lifetimes: a `proposal` dies with the draft
   * folder it points into, while this one names a file of the project that
   * outlives every conversation about it.
   *
   * Like its sibling it carries the proposal's **identity** and nothing else,
   * the state living in `PCP-prompt-change-proposals.md` and being read from
   * there (CMT-FR-67). The rail renders it as a **control** rather than a
   * preview (CMT-FR-48) and fetches nothing for it (CMT-FR-49). No composer can
   * produce one — there is no matching {@link AttachmentInput} variant.
   */
  | {
      kind: "promptProposal";
      proposalId: string;
      artifactId: string;
      /** Project-relative path the proposal would replace. */
      path: string;
    };

/**
 * CMS-FR-43: what the rail supplies on an append.
 *
 * Distinct from {@link Attachment} on purpose — a caller may not name a digest,
 * or it could claim content it never supplied. An `inline` input carries the
 * bytes and the backend derives the digest itself.
 */
export type AttachmentInput =
  | { kind: "url"; url: string; mediaType: string; label?: string }
  | {
      kind: "inline";
      mediaType: string;
      filename: string;
      /** Base64 of the file's bytes, with no `data:` prefix. */
      data: string;
    };

/** CMS-FR-48: what `read_comment_attachment` serves back. */
export interface AttachmentContent {
  mediaType: string;
  filename: string;
  /** Base64 of the stored bytes. */
  data: string;
}

/**
 * The label a card and a panel row show for an attachment (CMT-FR-48).
 *
 * A blob is its filename; a link is its label, or the address itself when it
 * carries none — which is what makes a bare pasted link still name something.
 */
export function attachmentName(a: Attachment): string {
  if (a.kind === "blob") return a.filename;
  if (a.kind === "proposal" || a.kind === "promptProposal") return a.path;
  return a.label ?? a.url;
}

/** Whether a card renders this attachment as a thumbnail (CMT-FR-48). */
export function isImageAttachment(a: Attachment): boolean {
  // CMS-FR-42: the two reference kinds hold no content at all, so neither has a
  // media type to read and neither is ever a thumbnail (CMT-FR-48).
  if (a.kind === "proposal" || a.kind === "promptProposal") return false;
  return a.mediaType.split(";")[0].trim().toLowerCase().startsWith("image/");
}

/** CMT-FR-48: whether this attachment renders as a control rather than a preview. */
export function isProposalAttachment(
  a: Attachment,
): a is Extract<Attachment, { kind: "proposal" | "promptProposal" }> {
  return a.kind === "proposal" || a.kind === "promptProposal";
}

/**
 * DCP-FR-04: where a proposed change stands. `pending` is the only state that
 * occupies the draft's one undecided slot.
 */
export type ProposalState = "pending" | "accepted" | "rejected";

/**
 * DCP-FR-HRQN: which way one change alters the text it names.
 *
 * `add` inserts between its lead and trail and names no text of its own; `del`
 * removes the text it names; `replace` puts new text in its place.
 */
export type HunkKind = "add" | "del" | "replace";

/**
 * DCP-FR-PWSF: where one proposed change stands.
 *
 * `pending` and `discussing` are both undecided, so either holds the draft's
 * one pending slot. They differ only in what the surface says about them: a
 * change held for discussion is one the author has answered without settling.
 */
export type HunkState = "pending" | "accepted" | "rejected" | "discussing";

/** Whether the author has settled this change either way (DCP-FR-PWSF). */
export function isUndecided(state: HunkState): boolean {
  return state === "pending" || state === "discussing";
}

/**
 * DCP-FR-HRQN: the content anchor one change carries.
 *
 * The text is the identity and the offsets are only a hint, exactly as a
 * comment anchor works (CMT-FR-06, CMT-FR-18). This is what makes changes
 * decidable in any order: accepting one moves the others in the prompt without
 * invalidating what they name.
 */
export interface HunkAnchor {
  /** Exact prompt text immediately before the changed text, bounded. */
  lead: string;
  /** Exact prompt text immediately after the changed text, bounded. */
  trail: string;
  /** Byte offsets into the prompt as it stood when the proposal was recorded. */
  hint_start: number;
  hint_end: number;
}

/** DCP-FR-HRQN: one proposed change, as the agent composed it or as edited. */
export interface ProposalHunk {
  id: string;
  kind: HunkKind;
  /** Bumped by an agent revision, never by an author edit (DCP-FR-XDRV). */
  revision: number;
  /** What the agent said about this one change, if anything. */
  note?: string | null;
  /** The exact prompt text this change replaces or deletes. Absent for an add. */
  before?: string | null;
  /** The exact new text. Absent for a deletion. */
  after?: string | null;
  anchor: HunkAnchor;
}

/**
 * DCP-FR-BMLX: where one change applies in the prompt as it stands, or that it
 * no longer does.
 *
 * Derived on every read and never stored, because the author can undo the edit
 * that lost it. A lost change is refused acceptance and cleared by rejection.
 */
export type HunkPlacement =
  | { kind: "resolved"; start: number; end: number }
  | { kind: "lost" };

/** DCP-FR-PWSF: how a proposal's changes stand, in one shape. */
export interface HunkCounts {
  pending: number;
  accepted: number;
  rejected: number;
  discussing: number;
}

/** DCP-FR-01: one row of a proposal's decision ledger. Identity, never text. */
export interface HunkLedgerRow {
  id: string;
  kind: HunkKind;
  state: HunkState;
  /** True while this change's text differs from the agent's own (DCP-FR-25). */
  edited: boolean;
  revision: number;
  /** RFC 3339 UTC; absent while undecided. */
  decidedAt?: string | null;
}

/**
 * DCP-FR-10: a proposal's changes in proposal order, each with its placement
 * against the prompt as it stands, and the baseline the next edit of one of
 * them is checked against (DCP-FR-27).
 *
 * `resolutions` is parallel to `hunks` — index for index.
 */
export interface ProposalHunks {
  hunks: ProposalHunk[];
  resolutions: HunkPlacement[];
  checksum: string;
  /** DCP-FR-QLMH: the text of a legacy proposal is not editable. */
  legacy: boolean;
}

/**
 * How many of a proposal's changes are still undecided (DCP-FR-PWSF).
 *
 * Tolerant of a record written before proposals held changes: such a record
 * carries no counts, and it still arrives, because `proposals/` is committed
 * and travels through Git (DCP-FR-02).
 */
export function undecidedCount(counts: HunkCounts | undefined): number {
  if (!counts) return 0;
  return counts.pending + counts.discussing;
}

/**
 * DCP contract surface: a change an agent proposed to one file of one draft.
 *
 * `rationale` is what the agent wrote, and is also the body of the comment that
 * announced it — the modal shows the same text the rail does rather than a
 * second explanation (DCR-FR-04).
 */
export interface DraftChangeProposal {
  id: string;
  draftId: string;
  /** Draft-relative path of the file this would replace. */
  path: string;
  agent: Participant;
  rationale: string;
  threadId: string;
  /** The comment carrying the reference to this proposal (CMS-FR-60). */
  commentId: string;
  state: ProposalState;
  /**
   * DCP-FR-25: sha256 of the candidate as the recording wrote it. Absent on a
   * record written before candidates could be edited.
   */
  originChecksum?: string | null;
  /**
   * DCP-FR-25: true while the candidate differs from the agent's own text —
   * which is what DCR-FR-27's standing line is said from, and what the decision
   * comment tells the agent.
   */
  candidateEdited: boolean;
  /**
   * DCP-FR-QLMH: a proposal held as one whole document, with no hunk file
   * beside it. Such a proposal arrives through Git long after this build
   * stopped writing them, so it is read rather than migrated, and its text is
   * not editable.
   */
  legacy: boolean;
  /** DCP-FR-HRQN: sha256 of the prompt as the agent read it, at record time. */
  baseSha256?: string | null;
  /** How many changes the proposal carries. */
  hunkCount: number;
  /**
   * DCP-FR-PWSF: how the proposal's changes stand, from which `state` itself is
   * derived. A surface reads the counts rather than recomputing them, so no two
   * readers can disagree about what "3 of 5 decided" means.
   */
  counts: HunkCounts;
  /**
   * DCP-FR-01: the per-change decision ledger, in proposal order. It carries
   * identity and decision and no text, so listing proposals never reads a
   * document (DCP-FR-09).
   */
  ledger: HunkLedgerRow[];
  createdAt: string;
  /** RFC 3339 UTC; absent while pending. */
  decidedAt?: string | null;
}

/**
 * DCR-FR-15: what a decision hands back.
 *
 * The proposal as it now stands, plus what the surface needs to dispatch one
 * fresh turn to the proposing agent: the comment the decision was recorded as,
 * and the origin kind of the conversation it landed in.
 */
export interface DecisionOutcome {
  proposal: DraftChangeProposal;
  /**
   * Absent when the conversation would not take the comment — the decision
   * still stands, and there is then nothing for a turn to answer.
   */
  commentId?: string;
  originKind: "draft_discussion" | "draft_comment";
}

/**
 * DCP-FR-10: the candidate's text, for the surface about to render a diff of
 * it, with the checksum that is the baseline its next save is checked against
 * (DCP-FR-27).
 */
export interface ProposalContent {
  content: string;
  checksum: string;
}

/** DCP-FR-25: what a candidate save reports — the caller's next baseline. */
export interface CandidateSaved {
  checksum: string;
}

/** DCP-FR-16: what `draft-change-proposals-changed` carries. */
export interface ProposalsChangedPayload {
  draftId: string;
  proposal: DraftChangeProposal;
}

/** DCP contract surface: the typed refusals a decision can return. */
export const PROPOSAL_ERRORS = {
  notFound: "proposal_not_found",
  /**
   * DCP-FR-22: `draftId` names no draft of the active worktree.
   *
   * A **completed answer** rather than a failed read, and the difference is
   * load-bearing for CTA-FR-SACG: a draft that has been graduated or deleted took
   * its proposals with it (DCP-FR-02), so a reference to one of them is a
   * reference to something that is gone rather than to something nobody has
   * looked for yet.
   */
  draftNotFound: "draft_not_found",
  alreadyDecided: "already_decided",
  /** DCP-FR-27: the stored candidate moved on since this surface read it. */
  candidateStale: "candidate_stale",
  pathMissing: "path_missing",
  writeFailed: "write_failed",
  /**
   * DHS-FR-17: the acceptance **landed** and the decision comment is still
   * owed. Not a refusal, and never rendered as one (DCR-FR-16): the prompt
   * holds the accepted text and the version is in the rail.
   */
  acceptanceIncomplete: "acceptance_incomplete",
  /**
   * DHS-FR-21: the draft's acceptance journal could not be reconciled, so
   * nothing about it can be decided until it is.
   */
  historyRecoveryFailed: "history_recovery_failed",
  /**
   * DHS-FR-23: another acceptance stands over this draft's prompt, so a second
   * one is refused rather than interleaved with it.
   */
  acceptanceInProgress: "acceptance_in_progress",
  /** DCP-FR-VZTK: `hunkId` names no change of this proposal. */
  hunkNotFound: "hunk_not_found",
  /**
   * DCP-FR-PWSF: the change has already been accepted or rejected. A decision
   * is made once, and the route back is History rather than a second decision.
   */
  hunkAlreadyDecided: "hunk_already_decided",
  /**
   * DCP-FR-BMLX: the text this change names is no longer in the prompt, so
   * there is nowhere to put it. Rejection still clears it, which is how a lost
   * change stops holding the draft's pending slot.
   */
  anchorLost: "anchor_lost",
} as const;

/**
 * PCP-FR-04: where a proposed change to a prompt artifact stands. `pending` is
 * the only state that occupies the artifact's one undecided slot.
 */
export type PromptProposalState = "pending" | "accepted" | "rejected";

/**
 * PCP contract surface: a change an agent proposed to one prompt artifact the
 * project already holds.
 *
 * `rationale` is what the agent wrote, and is also the body of the comment that
 * announced it — the modal shows the same text the rail does rather than a
 * second explanation (PCR-FR-02).
 */
export interface PromptChangeProposal {
  id: string;
  /** The stable, path-derived node key of the target (ASC-FR-13). */
  artifactId: string;
  /** Project-relative path of the prompt artifact. */
  path: string;
  agent: Participant;
  rationale: string;
  threadId: string;
  /** The comment carrying the reference to this proposal (CMS-FR-66). */
  commentId: string;
  state: PromptProposalState;
  /** PCP-FR-22: sha256 of the candidate as the recording wrote it. */
  originChecksum?: string | null;
  /** PCP-FR-22: true while the candidate differs from the agent's own text. */
  candidateEdited: boolean;
  /**
   * PCP-FR-14: true between an acceptance landing and its decision comment
   * being appended. The decision is settled either way; only the conversation is
   * owed one, which is why every surface treats such a proposal as decided
   * (PCR-FR-26).
   */
  commentOwed: boolean;
  createdAt: string;
  /** RFC 3339 UTC; absent while pending. */
  decidedAt?: string | null;
}

/**
 * PCR-FR-14: what a decision hands back — the proposal as it now stands, plus
 * what the surface needs to dispatch one fresh turn to the proposing agent.
 */
export interface PromptDecisionOutcome {
  proposal: PromptChangeProposal;
  /**
   * Absent when the comment could not be appended — no turn then has anything
   * to answer.
   */
  commentId?: string;
  originKind: "artifact_discussion" | "artifact_comment";
}

/**
 * PCP-FR-11: the candidate's text, for the surface about to render a comparison
 * of it, with the checksum that is the baseline its next save is checked against
 * (PCP-FR-24).
 */
export interface PromptProposalContent {
  content: string;
  checksum: string;
}

/** PCP-FR-22: what a candidate save reports — the caller's next baseline. */
export interface PromptCandidateSaved {
  checksum: string;
}

/** PCP-FR-17: what `prompt-change-proposals-changed` carries. */
export interface PromptProposalsChangedPayload {
  artifactId: string;
  proposal: PromptChangeProposal;
}

/** PCP contract surface: the typed refusals its operations can return. */
export const PROMPT_PROPOSAL_ERRORS = {
  noProjectOpen: "no_project_open",
  notFound: "proposal_not_found",
  /** PCP-FR-19: the target no longer names a file the project holds. */
  artifactNotFound: "artifact_not_found",
  /** PCP-FR-19: the target's resolved artifact type is no longer `prompt`. */
  notAPrompt: "not_a_prompt_artifact",
  alreadyDecided: "already_decided",
  /** PCP-FR-24: the stored candidate moved on since this surface read it. */
  candidateStale: "candidate_stale",
  discussionLocked: "discussion_locked",
  /**
   * PCP-FR-14: a failure at or before the commit point. **The file has not
   * changed** — the backend never wrote it on any failing path — and the
   * proposal is still the author's to decide (PCR-FR-15).
   */
  writeFailed: "write_failed",
  /**
   * PCP-FR-14: the acceptance **landed** and its decision comment is still
   * owed. Not a refusal, and never rendered as one (PCR-FR-15).
   */
  acceptanceIncomplete: "acceptance_incomplete",
  /** PCP-FR-14: another acceptance of this proposal is in flight. */
  acceptanceInProgress: "acceptance_in_progress",
} as const;

export interface Comment {
  id: string;
  author: Participant;
  /** Markdown source; rendered as rich text in the card (CMT-FR-09). */
  body: string;
  quotes: CommentQuote[];
  /** CMS-FR-42: empty when the comment attaches nothing. */
  attachments: Attachment[];
  createdAt: string;
}

/**
 * One discussion of the project-wide listing the Comments panel reads, together
 * with what the filesystem currently says about the owner it belongs to.
 *
 * `ownerUnavailable` is the owner having gone from the project, not the
 * fragment having gone from the owner's text: the panel never reads owner text,
 * so it cannot know about an orphaned fragment.
 */
export interface DiscussionListItem {
  discussion: Discussion;
  /** The owner no longer resolves, so the row opens the conversation tab. */
  ownerUnavailable: boolean;
}

/**
 * CMS-FR-XWDA: the durable question set a discussion holds while it waits on the
 * author (`ADQ-ask-discussion-questions-tool.md`).
 *
 * It is **not** a comment and occupies no position in the conversation's history
 * (DQA-FR-TVMH): it stands beside the log until the author submits, and only
 * then does the discussion gain lines. It carries no selected option and no note
 * — a restart finds the questions and none of the answers (ADQ-FR-PZWD).
 */
export interface PendingQuestionSet {
  setId: string;
  discussionId: string;
  /**
   * CMS-FR-41: the asking agent's participant snapshot, taken when the set was
   * recorded. It is what the block names and what the generated question
   * comments are stamped with (DQA-FR-PDLN).
   */
  askedBy: Participant;
  askedAt: string;
  questions: PendingQuestion[];
}

/** One question of a set, at the position the application assigned it. */
export interface PendingQuestion {
  /**
   * ADQ-FR-TWNS: 1-based, stable, ascending. Every later act names this rather
   * than the text, because two questions may read alike.
   */
  position: number;
  text: string;
  options: QuestionOption[];
}

/** One proposed answer, at the position it was recorded in. */
export interface QuestionOption {
  position: number;
  value: string;
}

/**
 * DQA-FR-KDVU: one entry of a submission, each field named separately.
 *
 * The value travels beside the position and is checked against the record
 * (CMS-FR-PJBV), so a block answering a set that has since changed is refused
 * rather than recording whichever option now stands there.
 */
export interface QuestionAnswerInput {
  questionPosition: number;
  /**
   * CMS-FR-GNTB: an entry names a chosen option or the author's own words,
   * never both and never neither.
   */
  optionPosition?: number;
  optionValue?: string;
  /** DQA-FR-FCZL: what the author wrote where no recorded option suited. */
  ownAnswer?: string;
  /** DQA-FR-JJON: the addition to a chosen option, and to nothing else. */
  note?: string;
}

/** CMS-FR-TXRB: what an accepted submission returns. */
export interface QuestionAnswersSubmitted {
  discussion: Discussion;
  /**
   * DQA-FR-CIRK: the comment a fresh turn names as its trigger — the **final**
   * answer of the set, so one submission dispatches one turn per active agent
   * rather than one per question (DQA-FR-QMEA).
   */
  finalAnswerCommentId: string;
}

/**
 * The owner of a discussion: one file of the project, one draft, or one note.
 * Which of the three it is decides where the log lives, whether it is
 * committed, and how long it lives.
 */
export type DiscussionTarget =
  | { kind: "draft"; draftId: string }
  | { kind: "artifact"; artifactId: string }
  /**
   * One note of the project, itself — rather than whatever entity the note
   * happens to be filed against. A note carries at most one discussion, and the
   * association is the log's existence rather than a field on the note record
   * (CMS-FR-62).
   */
  | { kind: "note"; noteId: string };

/**
 * A stable string for one target, for keying session-lived state and for the
 * dependency of a read. Two targets of different kinds can never collide: the
 * prefix is part of the key.
 */
export function discussionTargetKey(target: DiscussionTarget): string {
  switch (target.kind) {
    case "draft":
      return `draft:${target.draftId}`;
    case "artifact":
      return `artifact:${target.artifactId}`;
    case "note":
      return `note:${target.noteId}`;
  }
}

/**
 * The passage of an owner a discussion is about. Only an `artifact` owner (a
 * text artifact) or a `draft` owner (a draft prompt) can hold one.
 *
 * `path` is the project-relative path of the artifact, or the draft-relative
 * path of the draft file. Offsets count UTF-16 code units of the source.
 */
export interface FragmentTarget {
  owner: DiscussionTarget;
  path: string;
  start: number;
  end: number;
  quote: string;
}

/** A discussion as the backend's fold produces it. */
export interface Discussion {
  id: string;
  /** The owner of the discussion. */
  target: DiscussionTarget;
  /** `null` is a discussion about the whole target. */
  fragmentTarget: FragmentTarget | null;
  /** Oldest first. */
  comments: Comment[];
  locked: boolean;
  resolved: boolean;
  createdAt: string;
  updatedAt: string;
}

/**
 * Whether the discussion is about a passage. Reads through `?? null` so a
 * payload that omits the field reads as a whole-target discussion.
 */
export function isFragmentTargeted(d: Discussion): boolean {
  return (d.fragmentTarget ?? null) !== null;
}

/** The fragment target of a discussion, or `null` for a whole-target one. */
export function discussionFragment(d: Discussion): FragmentTarget | null {
  return d.fragmentTarget ?? null;
}

/**
 * The artifact path the discussion belongs to: the artifact id of an artifact
 * target, or the fragment path of a draft fragment. `undefined` otherwise.
 */
export function discussionArtifactPath(d: Discussion): string | undefined {
  if (d.target.kind === "artifact") return d.target.artifactId;
  return d.fragmentTarget?.path;
}

/** The draft id of a draft target, or `undefined`. */
export function discussionDraftId(d: Discussion): string | undefined {
  return d.target.kind === "draft" ? d.target.draftId : undefined;
}

/** The note id of a note target, or `undefined`. */
export function discussionNoteId(d: Discussion): string | undefined {
  return d.target.kind === "note" ? d.target.noteId : undefined;
}

/**
 * The typed errors the comment commands reject with, mirroring the constants in
 * `src-tauri/src/comments.rs` and `src-tauri/src/github_tokens.rs`.
 *
 * These are matched on rather than merely displayed: a selection-required
 * identity opens the token picker (CMT-FR-25) while a missing one routes to
 * Global settings (CMT-FR-26), and a locked discussion re-renders the card
 * (CMT-FR-34). A divergence from the Rust spelling is silent, which is why both
 * sides keep the list as named constants.
 */
export const COMMENT_ERRORS = {
  discussionNotFound: "discussion_not_found",
  discussionLocked: "discussion_locked",
  quotedCommentNotInDiscussion: "quoted_comment_not_in_discussion",
  invalidFragment: "invalid_fragment",
  /** CMS-FR-47: the media type is outside the set the backend accepts. */
  unsupportedMediaType: "unsupported_media_type",
  /** CMS-FR-47: the decoded file is past the backend's size bound. */
  attachmentTooLarge: "attachment_too_large",
  /** CMS-FR-47: the input did not parse — bad base64, or a malformed address. */
  malformedAttachment: "malformed_attachment",
  /** CMS-FR-48: the digest names no attachment in this discussion's scope. */
  attachmentNotFound: "attachment_not_found",
  /** CMS-FR-57: the `draftId` names no draft in the open project. */
  draftNotFound: "draft_not_found",
  /** CMS-FR-57: a discussion target naming a path the project does not hold. */
  artifactNotFound: "artifact_not_found",
  /** CMS-FR-59: a fragment move asked of a discussion that has no fragment target. */
  notFragmentTargeted: "not_fragment_targeted",
  /** CMS-FR-62: the `noteId` names no note in the open project. */
  noteNotFound: "note_not_found",
  /**
   * CMS-FR-57: an operation that cannot serve the target it was handed —
   * `open_discussion` on a note, which could open a second discussion on
   * a target that carries at most one.
   */
  notSupported: "not_supported",
  /** CMS-FR-62: an opening message with nothing in it (NTS-FR-27). */
  emptyBody: "empty_body",
  /**
   * CMS-FR-YQND: the repository machine store could not be resolved
   * (`RMS-repository-machine-storage.md` RMS-FR-WGQS). A read refuses on the
   * same terms a write does, so a conversation that could not be read is never
   * presented as one that holds nothing.
   */
  storeUnavailable: "store_unavailable",
  /**
   * CMS-FR-QLDW: the discussion already holds a question set waiting on the
   * author, so a second could not be recorded.
   */
  questionSetAlreadyPending: "question_set_already_pending",
  /**
   * CMS-FR-PJBV: the submission names a set this discussion does not hold and
   * has not already committed.
   */
  questionSetNotFound: "question_set_not_found",
  /**
   * CMS-FR-PJBV: the answers do not cover every recorded question exactly once,
   * or one of them names an option the record does not hold.
   */
  questionAnswersIncomplete: "question_answers_incomplete",
} as const;

/**
 * Why the rail could not resolve an author, and what the user does about it
 * (CMT-FR-24–CMT-FR-26).
 *
 * These are the GitHub token vocabulary rather than a parallel list: identity
 * resolution *is* token resolution (CMS-FR-12), and a second spelling of the
 * same wire strings is exactly the drift `github-error-parity.test.ts` exists to
 * prevent. Each maps to a different thing for the author to do next, which is why
 * the rail matches on them instead of printing whatever came back.
 */
export const COMMENT_IDENTITY_ERRORS = {
  /** More than one token stored and none bound: the picker resolves it. */
  selectionRequired: GITHUB_TOKEN_ERRORS.selectionRequired,
  /** A token resolves but names no verified account. */
  unresolved: GITHUB_TOKEN_ERRORS.identityUnresolved,
  githubUnreachable: GITHUB_TOKEN_ERRORS.githubUnreachable,
  keychainUnavailable: GITHUB_TOKEN_ERRORS.keychainUnavailable,
} as const;
