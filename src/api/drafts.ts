/**
 * Drafts, their statistics and assets, and the change proposals made against
 * them.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  DecisionOutcome,
  DraftChangeProposal,
  DraftStatistics,
  PromptChangeProposal,
  PromptDecisionOutcome,
  PromptProposalContent,
  PromptCandidateSaved,
  CandidateSaved,
  ProposalHunks,
  DraftContents,
  DraftAsset,
  DraftAssetDiscarded,
  DraftAssetSweepScheduled,
  DraftCreated,
  DraftImageContent,
  DraftFolder,
  DraftHierarchy,
  DraftMatch,
  DraftHistoryContent,
  DraftHistoryList,
  DraftRecord,
  DraftStatus,
  DraftSummary,
  DraftPublicationView,
  GithubPublicationSettings,
  MetadataList,
  PublicationChoiceInput,
  PublicationIssueType,
  PublicationMetadata,
  PublicationOutcome,
  PublicationRecoveryChoice,
  PublicationRemoteResolution,
  SaveResult,
} from "../types";

// --- Drafts (drafts.rs / NAW-new-artifact.md, DRP-drafts-panel.md) --------

/**
 * DRP-FR-02 / DRS-FR-08: the active worktree's whole drafts hierarchy from one
 * call — every folder at every depth, empty ones included, and every draft with
 * the folder it is filed in, most-recent-activity first.
 *
 * Each draft carries its file count rather than its files, so the panel renders
 * from this one call however deep the tree runs and never reads a draft's
 * contents.
 */
export const listDrafts = () => invoke<DraftHierarchy>("list_drafts");

/**
 * NAW-FR-03 / DRS-FR-06: create a draft and return its record together with the
 * one Markdown file it was created holding, which the tab opens on. `name` unset
 * is named provisionally (NAW-FR-04).
 *
 * DRS-FR-07: `folder` is the **only** thing a creation says about where anything
 * goes — the drafts folder the draft is filed in (DRP-FR-26). A draft carries no
 * project destination: where its specification lands is the graduation agent's
 * choice from the captured prompt (NAW-FR-19).
 */
export const createDraft = (args: { name?: string | null; folder?: string | null }) =>
  invoke<DraftCreated>("create_draft", {
    name: args.name ?? null,
    folder: args.folder ?? null,
  });

// --- The organisation of the drafts root (DRP-FR-22 … DRP-FR-28) ----------
//
// None of these five reaches inside a draft. `createDraftFolder` below, which
// reads almost the same, creates a folder in ONE draft's own file tree and is
// invoked by the New Artifact tab; these address the author's organising
// folders under the drafts root and are invoked by the Drafts panel alone.

/** DRP-FR-23 / DRS-FR-30: one empty folder directly inside `parent`. */
export const createDraftsFolder = (parent: string, name: string) =>
  invoke<DraftFolder>("create_drafts_folder", { parent, name });

/**
 * DRP-FR-24 / DRS-FR-30: rename in place. The folder keeps its parent, its
 * subtree and every draft beneath it, ids and files intact.
 */
export const renameDraftsFolder = (path: string, name: string) =>
  invoke<DraftFolder>("rename_drafts_folder", { path, name });

/**
 * DRP-FR-25 / DRS-FR-31: remove the folder, reparenting every direct child into
 * its parent first. Deletes no draft.
 */
export const deleteDraftsFolder = (path: string) =>
  invoke<void>("delete_drafts_folder", { path });

/**
 * DRP-FR-27 / DRS-FR-32: file a draft under another folder. Changes exactly one
 * thing about it — where its directory sits.
 */
export const moveDraftToFolder = (draftId: string, folder: string) =>
  invoke<DraftSummary>("move_draft_to_folder", { draftId, folder });

/** DRP-FR-27 / DRS-FR-33: move a folder and its whole subtree. */
export const moveDraftsFolder = (path: string, destination: string) =>
  invoke<DraftFolder>("move_drafts_folder", { path, destination });

/**
 * DRP-FR-13 / DRS-FR-17: the drafts the entered text is found in — matched
 * against each draft's name and against the text of every file it holds, each
 * result saying which of the two matched (DRP-FR-17).
 *
 * The one call this panel makes that reads a draft's files, which is why it is
 * dispatched on a settled query rather than per keystroke.
 */
export const searchDrafts = (text: string) =>
  invoke<DraftMatch[]>("search_drafts", { text });

export const openDraft = (id: string) => invoke<DraftRecord>("open_draft", { id });

/** DRP-FR-11: relabel. An empty name is refused rather than stored. */
export const renameDraft = (id: string, name: string) =>
  invoke<DraftRecord>("rename_draft", { id, name });

/** NAW-FR-16: move between the two status positions; reversible at any time. */
export const setDraftStatus = (id: string, status: DraftStatus) =>
  invoke<DraftRecord>("set_draft_status", { id, status });

/**
 * DRP-FR-12: remove the draft and everything it holds. Writes nothing into the
 * project, because a draft has never been in it.
 */
export const deleteDraft = (id: string) =>
  invoke<void>("delete_draft", { id });

// --- Draft statistics (statistics.rs / DSS-draft-statistics-storage.md) ----
//
// The two the Information modal and its editing-activity observer reach
// (DFI-FR-ZGBU, DFI-FR-XKRM). Neither invokes a draft, conversation, proposal,
// history, or graduation operation of any kind.

/**
 * DSS-FR-YOVS: the draft's captured lifetime aggregate totals, each carrying its
 * own availability.
 *
 * The modal's one read. It creates no telemetry activation boundary
 * (DSS-FR-JRSY), so opening Information for a draft that has captured nothing
 * leaves it with nothing captured.
 */
export const readDraftStatistics = (draftId: string) =>
  invoke<DraftStatistics>("read_draft_statistics", { draftId });

/**
 * DSS-FR-RIDW: record one settled foreground editing interval for a draft.
 *
 * Invoked by the observer of DFI-FR-XKRM rather than by the modal, and
 * fire-and-forget: a failed or slow call never delays the author, blocks a
 * surface, or produces a visible error.
 */
export const recordDraftEditingInterval = (
  draftId: string,
  startedAt: string,
  endedAt: string,
) =>
  invoke<void>("record_draft_editing_interval", {
    draftId,
    startedAt,
    endedAt,
  });

// --- Draft assets (draft_assets.rs / DAS-draft-assets.md) ------------------
//
// The four the New Artifact tab invokes (NAW-FR-50 … NAW-FR-55). None of them
// adds a file to the draft's `files/`, which still holds the one prompt, and
// none reaches outside the draft. `read_prompt_images` is not among them: the
// bytes a conversation is handed reach it inside the backend and no frontend
// call can ask for them (DAS-FR-22).

/**
 * DAS-FR-02 / DAS-FR-05: store a pasted or dropped image under the draft's own
 * `assets/` folder and answer with the destination to insert.
 *
 * The returned `reference` is the Markdown destination the surface writes —
 * `../assets/<opaque-id>.png` — so no caller derives a path of its own. The
 * supplied `filename` is metadata alone and never reaches the path (DAS-FR-02).
 *
 * Refused with `unsupported_media_type`, `image_too_large`, or
 * `malformed_image` where the image is not acceptable, with
 * `draft_locked_by_graduation` where a run holds the draft, and with
 * `asset_store_failed` where the store did not happen — three different things
 * to tell the author (NAW-FR-51).
 */
export const storeDraftImage = (
  draftId: string,
  mediaType: string,
  filename: string | null,
  data: string,
) =>
  invoke<DraftAsset>("store_draft_image", {
    draftId,
    mediaType,
    filename,
    data,
  });

/**
 * DAS-FR-08: the stored bytes of an image the prompt references, for the
 * surface to draw (NAW-FR-52).
 *
 * `path` is the destination the Markdown carries or the draft-relative path the
 * store returned; both name the same file. A path naming nothing under the
 * draft's `assets/` is `asset_not_found` and one leaving the draft is
 * `path_escape`, and nothing outside the draft is opened, followed, or
 * requested to reach either answer (DAS-FR-07).
 */
export const readDraftImage = (draftId: string, path: string) =>
  invoke<DraftImageContent>("read_draft_image", { draftId, path });

/**
 * DAS-FR-09: take back a store whose reference never reached the saved prompt.
 *
 * Idempotent — a path naming nothing is reported discarded having had nothing to
 * discard — and it removes nothing an asset a saved reference still names, which
 * it reports as retained instead.
 */
export const discardDraftImage = (draftId: string, path: string) =>
  invoke<DraftAssetDiscarded>("discard_draft_image", { draftId, path });

/**
 * DAS-FR-15: schedule one housekeeping pass over this draft.
 *
 * A **trigger and not a query**: it answers that a pass was scheduled and
 * whether this request joined one already pending, and it carries no account of
 * what any pass found. Nothing waits on it and nothing renders from it —
 * housekeeping is something the application does about a draft rather than
 * something the author is shown (NAW-FR-55).
 */
export const sweepDraftAssets = (draftId: string) =>
  invoke<DraftAssetSweepScheduled>("sweep_draft_assets", { draftId });

/**
 * DCP-FR-09: every change proposed against this draft — pending, accepted, and
 * rejected alike — most recently created first.
 *
 * Reads records and never a candidate's text, so a draft carrying a long history
 * of proposals costs a tab no more on open than one carrying none.
 */
export const listDraftChangeProposals = (draftId: string) =>
  invoke<DraftChangeProposal[]>("list_draft_change_proposals", { draftId });

/**
 * DCP-FR-10 / DCR-FR-05: the proposal's changes in proposal order, each with
 * where it lands in the prompt as it stands now.
 *
 * A change whose placement is `lost` names text the author has since rewritten
 * (DCP-FR-BMLX). The placement is derived on every read and never stored, so
 * undoing that edit brings the change back by itself.
 */
export const loadDraftChangeProposalHunks = (proposalId: string) =>
  invoke<ProposalHunks>("load_draft_change_proposal_hunks", { proposalId });

/**
 * DCR-FR-13 / DCP-FR-11: accept one change. Writes only the text this change
 * names, through the draft's own save path, and leaves every other change of
 * the proposal undecided.
 *
 * Refused `anchor_lost` when the prompt no longer holds the text the change
 * names, and `hunk_already_decided` when it has been decided already. The
 * comment that answers the agent is appended by the decision that leaves
 * nothing undecided, not by this one (DCP-FR-15).
 */
export const acceptDraftChangeHunk = (
  proposalId: string,
  hunkId: string,
  feedback?: string | null,
) =>
  invoke<DecisionOutcome>("accept_draft_change_hunk", {
    proposalId,
    hunkId,
    feedback: feedback ?? null,
  });

/**
 * DCR-FR-13 / DCP-FR-14: reject one change. Writes nothing into the draft.
 *
 * Never refused for a lost anchor, which is what makes rejection the way an
 * author clears a change whose text they have already rewritten (DCP-FR-BMLX).
 */
export const rejectDraftChangeHunk = (
  proposalId: string,
  hunkId: string,
  feedback?: string | null,
) =>
  invoke<DecisionOutcome>("reject_draft_change_hunk", {
    proposalId,
    hunkId,
    feedback: feedback ?? null,
  });

/**
 * DCR-FR-MTFD / DCP-FR-PWSF: hold one change for discussion, or release it.
 *
 * Undecided either way, so the change goes on holding the draft's pending slot.
 * It records that the author has answered without settling.
 */
export const setDraftChangeHunkDiscussing = (
  proposalId: string,
  hunkId: string,
  discussing: boolean,
) =>
  invoke<DraftChangeProposal>("set_draft_change_hunk_discussing", {
    proposalId,
    hunkId,
    discussing,
  });

/**
 * DCP-FR-25 / DCR-FR-26: write the author's rewrite of one change into proposal
 * storage.
 *
 * **Not a change to the draft.** It touches only the proposal's own files: no
 * file under the draft's `files/` is written, `updated_at` is not refreshed, no
 * `"drafts changed"` is emitted, and no decision operation is called. Refused
 * `already_decided` for a decided change and `candidate_stale` when storage has
 * moved on since `baselineChecksum` was read (DCP-FR-26, DCP-FR-27).
 */
export const editDraftChangeHunk = (
  proposalId: string,
  hunkId: string,
  after: string,
  baselineChecksum: string,
) =>
  invoke<CandidateSaved>("edit_draft_change_hunk", {
    proposalId,
    hunkId,
    after,
    baselineChecksum,
  });

/**
 * DCR-FR-13 / DCP-FR-14: decline the whole proposal — reject every change of it
 * that is still undecided. Writes nothing into the draft.
 *
 * The escape hatch the draft needs: a change held for discussion still occupies
 * the draft's one pending slot, and this is what releases it (DCP-FR-04).
 */
export const declineDraftChangeProposal = (
  proposalId: string,
  feedback?: string | null,
) =>
  invoke<DecisionOutcome>("decline_draft_change_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

// --- Prompt change proposals (prompt_proposals.rs / PCP) -------------------

/**
 * PCP-FR-10: every change proposed against this prompt artifact — pending,
 * accepted, and rejected alike — most recently created first.
 *
 * Reads records and never a candidate's text, so an artifact carrying a long
 * history of proposals costs a tab no more on open than one carrying none.
 */
export const listPromptChangeProposals = (artifactId: string) =>
  invoke<PromptChangeProposal[]>("list_prompt_change_proposals", { artifactId });

/**
 * PCP-FR-11 / PCR-FR-09: the proposed text, read when a comparison is actually
 * rendered rather than when proposals are listed.
 */
export const loadPromptChangeProposalContent = (proposalId: string) =>
  invoke<PromptProposalContent>("load_prompt_change_proposal_content", {
    proposalId,
  });

/**
 * PCP-FR-22 / PCR-FR-22: write the author's rewrite of a candidate into proposal
 * storage.
 *
 * **Not a change to the artifact.** It touches only `<proposal-id>.content`: the
 * artifact's file is not created, modified, or deleted, no
 * `"artifact changed externally"` follows, nothing is recorded in the
 * recently-edited list, and neither decision operation is called. It is refused
 * `already_decided` for a proposal that has been decided and `candidate_stale`
 * when the stored candidate has moved on since `baselineChecksum` was read
 * (PCP-FR-23, PCP-FR-24).
 */
export const savePromptChangeProposalCandidate = (
  proposalId: string,
  content: string,
  baselineChecksum: string,
) =>
  invoke<PromptCandidateSaved>("save_prompt_change_proposal_candidate", {
    proposalId,
    content,
    baselineChecksum,
  });

/**
 * PCR-FR-10 / PCP-FR-12: accept the change. Writes the candidate into the
 * artifact through the project's own write path, so no external-change dialog
 * appears for it (PST-FR-16).
 *
 * `feedback` is optional for both decisions and reaches the conversation as part
 * of the decision's own comment (PCR-FR-13).
 */
export const applyPromptChangeProposal = (
  proposalId: string,
  feedback?: string | null,
) =>
  invoke<PromptDecisionOutcome>("apply_prompt_change_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

/** PCR-FR-12 / PCP-FR-15: decline it. Writes nothing into the project. */
export const declinePromptChangeProposal = (
  proposalId: string,
  feedback?: string | null,
) =>
  invoke<PromptDecisionOutcome>("decline_prompt_change_proposal", {
    proposalId,
    feedback: feedback ?? null,
  });

/**
 * PCP-FR-14 / PCR-FR-15: finish an acceptance whose decision comment is still
 * owed.
 *
 * It decides nothing itself — against a proposal that owes no comment it is a
 * no-op returning that proposal as it stands — which is why the review's retry
 * invokes this and never a second `apply_prompt_change_proposal`, that being a
 * second decision rather than the completion of one.
 */
export const completePromptChangeDecision = (proposalId: string) =>
  invoke<PromptDecisionOutcome>("complete_prompt_change_decision", {
    proposalId,
  });

/**
 * The draft counterpart of `loadArtifactContentsById`, on the same
 * body-plus-checksum contract, so the editing surface's dirty and baseline
 * handling is the one it already uses for an artifact (NAW-FR-10).
 */
export const loadDraftFileContents = (id: string, path: string) =>
  invoke<DraftContents>("load_draft_file_contents", { id, path });

/** NAW-FR-13: write a draft file; returns the checksum of the bytes written. */
export const saveDraftFileContents = (id: string, path: string, body: string) =>
  invoke<SaveResult>("save_draft_file_contents", { id, path, body });

/**
 * NAW-FR-40 / DHS-FR-10: the draft's version history — every settled version
 * oldest first, together with the live prompt's own digest, which is what tells
 * the rail whether the author has typed since the last one (NAW-FR-37).
 *
 * Reads no snapshot payload, so a draft carrying fifty versions costs the tab
 * exactly what one carrying a single version costs.
 */
export const listDraftHistory = (draftId: string) =>
  invoke<DraftHistoryList>("list_draft_history", { draftId });

/**
 * NAW-FR-40 / DHS-FR-11: one version's text, verified against the digest its
 * manifest recorded. Invoked when a version is actually selected and at no other
 * moment, so an open rail costs nothing until the author asks to read one.
 */
export const loadDraftHistoryEntry = (entryId: string) =>
  invoke<DraftHistoryContent>("load_draft_history_entry", { entryId });

// --- GitHub publication (github_publication.rs / GHP-github-publication.md) -

/**
 * GHP-FR-CWTG: the draft's current publication, its whole history newest-first,
 * its standing attempt, and whether the action is offered right now.
 *
 * The tab's and the Information modal's one read (NAW-FR-TSQE, DFI-FR-BZQN).
 * It writes nothing and reaches GitHub only for the eligibility checks.
 */
export const getDraftPublication = (draftId: string) =>
  invoke<DraftPublicationView>("get_draft_publication", { draftId });

/**
 * GHP-FR-WKDE: every configured Git remote, classified, and which one the next
 * attempt would use. Mutates nothing on GitHub (NAW-FR-DWKA).
 */
export const listPublicationRemotes = (draftId: string) =>
  invoke<PublicationRemoteResolution>("list_publication_remotes", { draftId });

/**
 * GHP-FR-CVYK: start a new attempt against the named remote. `persistRemote`
 * true stores the choice for later publications in this project (GHP-FR-PWXA).
 */
export const publishDraftToGithub = (args: {
  draftId: string;
  remoteName: string;
  persistRemote: boolean;
  publicationChoice: PublicationChoiceInput;
}) => invoke<PublicationOutcome>("publish_draft_to_github", args);

/**
 * GHP-FR-MDLD: the repository, the settings, and the parent, Type, and
 * milestone lists the publication chooser renders. Mutates nothing on GitHub.
 */
export const loadPublicationMetadata = (draftId: string, remoteName: string) =>
  invoke<PublicationMetadata>("load_publication_metadata", {
    draftId,
    remoteName,
  });

/** GHP-FR-KVRH: the project's publication settings, defaults applied. */
export const getGithubPublicationSettings = () =>
  invoke<GithubPublicationSettings>("get_github_publication_settings");

/** GHP-FR-NQWX: persist the three settings, or refuse and write nothing. */
export const setGithubPublicationSettings = (
  settings: GithubPublicationSettings,
) =>
  invoke<GithubPublicationSettings>("set_github_publication_settings", {
    parentIssueTypes: settings.parentIssueTypes,
    subIssueType: settings.subIssueType,
    subIssueMilestonePolicy: settings.subIssueMilestonePolicy,
  });

/** GHP-FR-PTYL: the issue Types of the publication repository's owner. */
export const listGithubIssueTypes = () =>
  invoke<MetadataList<PublicationIssueType>>("list_github_issue_types");

/**
 * GHP-FR-OWLB: continue the standing attempt, reusing its marker and searching
 * for it before anything is created.
 */
export const retryDraftPublication = (draftId: string) =>
  invoke<PublicationOutcome>("retry_draft_publication", { draftId });

/** GHP-FR-YPGL: the author's answer to a recovery choice. */
export const resolveDraftPublicationConflict = (
  draftId: string,
  choice: PublicationRecoveryChoice,
) =>
  invoke<PublicationOutcome>("resolve_draft_publication_conflict", {
    draftId,
    choice,
  });

/** GHP-FR-EBSA: leave the attempt recoverable and add no history entry. */
export const cancelDraftPublicationConflict = (draftId: string) =>
  invoke<void>("cancel_draft_publication_conflict", { draftId });

/** GHP-FR-NAXT: abandon the standing attempt, appending nothing. */
export const cancelDraftPublicationAttempt = (draftId: string) =>
  invoke<void>("cancel_draft_publication_attempt", { draftId });

/**
 * GHP-FR-MJTB: open one recorded issue in the system's external browser. A URL
 * no record of this draft holds is refused, so no in-application page opens and
 * no arbitrary address can be reached through it.
 */
export const openPublicationIssue = (draftId: string, url: string) =>
  invoke<void>("open_publication_issue", { draftId, url });
