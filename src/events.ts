/**
 * Typed subscriptions for the backend-emitted Tauri event channels.
 *
 * The event-name string literals live ONLY here (and, byte-for-byte, in the
 * Rust emitters) so a rename is a one-line change on each side rather than a
 * silent listener mismatch. Each helper returns the `listen()` promise whose
 * resolved value is the unlisten function — callers keep the same
 * subscribe-once / cancel-flag pattern they used with raw `listen`.
 */
import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { SettingsWindowKind } from "./settingsWindow";
import type {
  ImageBuildFinished,
  ImageBuildProgress,
  ArtifactChangedPayload,
  BranchesChangedPayload,
  GithubPollingChangedPayload,
  AgentActivityState,
  ChangesUpdatedPayload,
  Discussion,
  PendingQuestionSet,
  DashboardRefreshFailedPayload,
  DocumentsSnapshot,
  DraftPromptChangedPayload,
  DraftStatisticsChangedPayload,
  GitOperationFinishedPayload,
  GitOutputLinePayload,
  AgentTurn,
  LogBufferState,
  NotificationActivation,
  Operation,
  GraduationQueueChangedPayload,
  WorkStreamUpdateProgress,
  GraduationLogAppendedPayload,
  WorkStreamsChangedPayload,
  GraduationRunChangedPayload,
  ProposalsChangedPayload,
  PromptProposalsChangedPayload,
  SearchEndedPayload,
  SearchResultsPayload,
  TreeChangedPayload,
  WorktreeContextChangedPayload,
} from "./types";

// Native File-menu activations (SNV-FR-24 / SNV-FR-25). The id doubles as the
// event name; these match `menu.rs`'s MENU_* constants byte-for-byte.
export const MENU_NEW_FILE = "menu:new-file";
export const MENU_NEW_FOLDER = "menu:new-folder";
export const MENU_NEW_ARTIFACT = "menu:new-artifact";
/** SNV-FR-28 / SNV-FR-29: write the active tab's content. */
export const MENU_SAVE = "menu:save";
/** SNV-FR-30 / SNV-FR-31: write everything holding unsaved changes. */
export const MENU_SAVE_ALL = "menu:save-all";
export const MENU_CLOSE_PROJECT = "menu:close-project";
/**
 * SNV-FR-26: the application is quitting. The backend holds the exit while the
 * frontend writes pending Editor changes (EDT-FR-33) and then either releases it
 * and then answers with `finish_exit` — proceeding, or cancelling the quit when
 * a write was blocked (EDT-FR-32).
 */
export const MENU_EXIT_REQUESTED = "menu:exit-requested";
/** SNV-FR-43 / EFR-FR-AYNZ: open, switch to, or toggle the Editor's Find panel. */
export const MENU_FIND = "menu:find";
/** SNV-FR-43 / EFR-FR-BJUY: the same for the Find & Replace panel. */
export const MENU_FIND_REPLACE = "menu:find-replace";
/** SNV-FR-23 / ABT-FR-KMVD: open the About panel in the window that is showing. */
export const MENU_ABOUT = "menu:about";

// Filesystem-watcher and progress channels.
//
// Every name below is kebab-case rather than the spec's abstract wording, for
// one reason that applies to all of them: Tauri validates event names and
// accepts only alphanumerics, `-`, `/`, `:` and `_`. A name with spaces is
// rejected on BOTH sides — `emit` returns an error the Rust emitters discard
// (an event must never take down the operation it reports on) and `listen`
// rejects symmetrically — so the channel is silently dead while every test that
// mocks `listen` stays green. Each of these matches its Rust constant
// byte-for-byte, and `lib.rs`'s
// `every_event_name_is_one_tauri_will_actually_deliver` pins them.

/** PST-FR-16 / EXC-FR-LKHZ: an artifact's on-disk content diverged externally. */
export const ARTIFACT_CHANGED_EXTERNALLY = "artifact-changed-externally";
/** ASC-FR-10 / LIB-FR-10: the project tree changed structurally (debounced). */
export const PROJECT_TREE_CHANGED = "project-tree-changed";
/** CHC-FR-16: the repository or working tree changed the current change set. */
export const CHANGES_UPDATED = "changes-updated";
/** DRS-FR-22: the set of drafts, or one draft's record or file set, changed. */
export const DRAFTS_CHANGED = "drafts-changed";
/** DCP-FR-16: a change was proposed to a draft file, or one was decided. */
export const DRAFT_CHANGE_PROPOSALS_CHANGED = "draft-change-proposals-changed";
/** PCP-FR-17: a change was proposed to a prompt artifact, or one was decided. */
export const PROMPT_CHANGE_PROPOSALS_CHANGED = "prompt-change-proposals-changed";

/**
 * DHS-FR-22: a version of a draft's prompt became visible, or a reconciliation
 * withdrew one. What keeps the New Artifact tab's History rail current without
 * it re-listing (NAW-FR-40).
 */
export const DRAFT_HISTORY_CHANGED = "draft-history-changed";

/**
 * DSS-FR-TWNA: events for a draft have been appended to its statistics log.
 *
 * It carries the draft's id and no totals: a consumer re-reads. What keeps an
 * open Information modal current while an agent works, without polling
 * (DFI-FR-ZGBU).
 */
export const DRAFT_STATISTICS_CHANGED = "draft-statistics-changed";

/** DSS-FR-TWNA: subscribe to [`DRAFT_STATISTICS_CHANGED`]. */
export const onDraftStatisticsChanged = (
  handler: (payload: DraftStatisticsChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<DraftStatisticsChangedPayload>(DRAFT_STATISTICS_CHANGED, (event) =>
    handler(event.payload),
  );

/** GHP-FR-CWTG: one draft's publication store changed. */
export const DRAFT_PUBLICATION_CHANGED = "draft-publication-changed";

/** GHP-FR-CWTG: subscribe to [`DRAFT_PUBLICATION_CHANGED`]. */
export const onDraftPublicationChanged = (
  handler: (payload: { draftId: string }) => void,
): Promise<UnlistenFn> =>
  listen<{ draftId: string }>(DRAFT_PUBLICATION_CHANGED, (event) =>
    handler(event.payload),
  );

/** PRG-FR-04: an operation was registered, advanced, or reached a terminal state. */
export const OPERATION_PROGRESS = "operation-progress";
/** WTC-FR-16: the active worktree, or the branch checked out in it, changed. */
export const WORKTREE_CONTEXT_CHANGED = "worktree-context-changed";
/** WTC-FR-25: the repository's set of branches has been re-read and may differ. */
export const BRANCHES_CHANGED = "branches-changed";
/** GPP-FR-TYOV: the GitHub polling view changed, with the new issues of a poll. */
export const GITHUB_POLLING_CHANGED = "github-polling-changed";
/** GTC-FR-04: one line of a transfer's output, push and pull alike. */
export const GIT_OUTPUT_LINE = "git-output-line";
/** GTC-FR-04: the single terminal event a transfer ends with. */
export const GIT_OPERATION_FINISHED = "git-operation-finished";
/** SCC-FR-19: hits found since the previous emission of the same search. */
export const SEARCH_RESULTS = "search-results";
/** SCC-FR-13: emitted exactly once per search, whatever the outcome. */
export const SEARCH_ENDED = "search-ended";
/** CMS-FR-51: a discussion this application wrote into, as it now folds. */
export const DISCUSSION_CHANGED = "discussion-changed";
/**
 * CMS-FR-BQEN: a discussion's pending question set was reserved, or deleted.
 *
 * Carries the set as it now stands or `null` where the discussion now holds
 * none, so a surface redraws from the payload rather than asking again.
 */
export const DISCUSSION_QUESTION_SET_CHANGED = "discussion-question-set-changed";
/** AGC-FR-21: a turn was registered, or reached a terminal state. */
export const AGENT_TURN_STATE_CHANGED = "agent-turn-state-changed";
/** LGC-FR-17: records were appended to the session buffer, or it was cleared. */
export const LOG_RECORDS_APPENDED = "log-records-appended";
/** NTD-FR-09: the author activated a notification this run posted. */
export const NOTIFICATION_ACTIVATED = "notification-activated";
/** AGV-FR-11: one run's agent activity grew, or was forgotten. */
export const AGENT_ACTIVITY_APPENDED = "agent-activity-appended";

/** DCL-FR-PTRP: the Documents collection's snapshot changed. */
export const DOCUMENTS_CHANGED = "documents-changed";

/**
 * DCL-FR-PTRP: the Documents collection changed. The payload is the whole
 * snapshot, so a consumer replaces what it holds from it without asking again
 * (DPN-FR-FAOR, DTV-FR-SAIG, PDV-FR-YOQS).
 */
export const onDocumentsChanged = (
  handler: (snapshot: DocumentsSnapshot) => void,
): Promise<UnlistenFn> =>
  listen<DocumentsSnapshot>(DOCUMENTS_CHANGED, (event) =>
    handler(event.payload),
  );

/** DRS-FR-42: the content of one draft's prompt file changed on disk. */
export const DRAFT_PROMPT_CHANGED = "draft-prompt-changed";

// The Dashboard's four refresh channels and its failure channel (PST-FR-35,
// PST-FR-36, PST-FR-37). None of them carries widget data: a consumer
// re-invokes the loader the event names and renders what that call returns, so
// what the Dashboard shows is what the loaders hold rather than what an emitter
// believed.

/** PST-FR-35: the Recently edited widget's data may have moved. */
export const DASHBOARD_RECENTLY_EDITED_CHANGED = "dashboard-recently-edited-changed";
/** PST-FR-35: the Active workstreams widget's data may have moved. */
export const DASHBOARD_ACTIVE_DRAFTS_CHANGED = "dashboard-active-drafts-changed";
/** PST-FR-35 / PST-FR-36: the agent-activity widget's data may have moved. */
export const DASHBOARD_AGENT_ACTIVITY_CHANGED = "dashboard-agent-activity-changed";
/** PST-FR-35 / PST-FR-36: the Pending Git widget's data may have moved. */
export const DASHBOARD_PENDING_GIT_CHANGED = "dashboard-pending-git-changed";
/** PST-FR-37: a background refresh of one widget could not be completed. */
export const DASHBOARD_REFRESH_FAILED = "dashboard-refresh-failed";

/** GRD-FR-EFAU: the set of graduation runs, or a run's position, changed. */
export const GRADUATION_QUEUE_CHANGED = "graduation-queue-changed";
/** GRD-FR-EFAU: one graduation run's state or record changed. */
export const GRADUATION_RUN_CHANGED = "graduation-run-changed";
/** GRS-FR-UCZL: records of one run and one stream became durable. */
export const GRADUATION_LOG_RECORDS_APPENDED = "graduation-log-records-appended";

/** WKS-FR-WULF: a work stream was created, changed or removed. */
export const WORK_STREAMS_CHANGED = "work-streams-changed";
/** WKS-FR-FQLS: a running update reached a new turn, or settled. */
export const WORK_STREAM_UPDATE_PROGRESS = "work-stream-update-progress";

/** PSS-FR-27: one update while a project image build runs. */
export const PROJECT_IMAGE_BUILD_PROGRESS = "project-image-build-progress";
/** PSS-FR-28: the one terminal result a project image build has. */
export const PROJECT_IMAGE_BUILD_FINISHED = "project-image-build-finished";

/** PSS-FR-27: watch a project image build report what it is doing. */
export const onProjectImageBuildProgress = (
  handler: (payload: ImageBuildProgress) => void,
): Promise<UnlistenFn> =>
  listen<ImageBuildProgress>(PROJECT_IMAGE_BUILD_PROGRESS, (event) =>
    handler(event.payload),
  );

/** PSS-FR-28: watch for the single terminal result of a project image build. */
export const onProjectImageBuildFinished = (
  handler: (payload: ImageBuildFinished) => void,
): Promise<UnlistenFn> =>
  listen<ImageBuildFinished>(PROJECT_IMAGE_BUILD_FINISHED, (event) =>
    handler(event.payload),
  );

/** A payload-less native File-menu channel (SNV-FR-23..31). */
export type MenuEvent =
  | typeof MENU_NEW_FILE
  | typeof MENU_NEW_FOLDER
  | typeof MENU_NEW_ARTIFACT
  | typeof MENU_SAVE
  | typeof MENU_SAVE_ALL
  | typeof MENU_CLOSE_PROJECT
  | typeof MENU_EXIT_REQUESTED
  | typeof MENU_FIND
  | typeof MENU_FIND_REPLACE
  | typeof MENU_ABOUT;

/** Subscribe to a payload-less native File-menu event (SNV-FR-23..31). */
export const onMenuEvent = (
  event: MenuEvent,
  run: () => void,
): Promise<UnlistenFn> => listen(event, () => run());

/** EXC-FR-LKHZ / PST-FR-16: an artifact's on-disk content diverged externally. */
export const onArtifactChangedExternally = (
  handler: (payload: ArtifactChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<ArtifactChangedPayload>(ARTIFACT_CHANGED_EXTERNALLY, (ev) =>
    handler(ev.payload),
  );

/** LIB-FR-10 / ASC-FR-10: the project tree changed structurally (debounced). */
export const onProjectTreeChanged = (
  handler: (payload: TreeChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<TreeChangedPayload>(PROJECT_TREE_CHANGED, (ev) => handler(ev.payload));

/**
 * DRP-FR-05 / DRS-FR-22: a draft was created, renamed, restatused, written to,
 * graduated, or deleted — from this window or another. The Drafts panel reloads
 * on it, so the list is current without the author refreshing anything.
 */
export const onDraftsChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen(DRAFTS_CHANGED, () => handler());

/**
 * DRS-FR-42: the **content** of one draft's prompt file changed on disk,
 * whoever wrote it — this application, or an external editor reaching the file
 * directly. A rename, a status change, a move between folders, and a write
 * under the draft's sibling folders each emit nothing here; those reach
 * `"drafts changed"` instead.
 */
export const onDraftPromptChanged = (
  handler: (payload: DraftPromptChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<DraftPromptChangedPayload>(DRAFT_PROMPT_CHANGED, (ev) =>
    handler(ev.payload),
  );

/**
 * DSH-FR-16 / PST-FR-35: the Recently edited widget's data may have moved — a
 * source file was written, by this application or by anything else, or the
 * project tree changed structurally. Carries no data: the consumer re-invokes
 * `list_recently_edited_artifacts` and renders what it returns.
 */
export const onDashboardRecentlyEditedChanged = (
  handler: () => void,
): Promise<UnlistenFn> =>
  listen(DASHBOARD_RECENTLY_EDITED_CHANGED, () => handler());

/**
 * DSH-FR-16 / PST-FR-35: which drafts qualify for the Active workstreams
 * widget, how they are named, or how they order may have moved.
 */
export const onDashboardActiveDraftsChanged = (
  handler: () => void,
): Promise<UnlistenFn> =>
  listen(DASHBOARD_ACTIVE_DRAFTS_CHANGED, () => handler());

/**
 * DSH-FR-15 / PST-FR-36: the backend's 5-minute timer refreshed the agent
 * activity reading. This surface starts no timer of its own and polls nothing.
 */
export const onDashboardAgentActivityChanged = (
  handler: () => void,
): Promise<UnlistenFn> =>
  listen(DASHBOARD_AGENT_ACTIVITY_CHANGED, () => handler());

/** DSH-FR-15 / PST-FR-36: the same tick's Pending Git reading. */
export const onDashboardPendingGitChanged = (
  handler: () => void,
): Promise<UnlistenFn> =>
  listen(DASHBOARD_PENDING_GIT_CHANGED, () => handler());

/**
 * DSH-FR-18 / PST-FR-37: a background refresh the author never asked for
 * failed. It names one widget and touches no other, which is how such a failure
 * reaches the author instead of ending in a log.
 */
export const onDashboardRefreshFailed = (
  handler: (payload: DashboardRefreshFailedPayload) => void,
): Promise<UnlistenFn> =>
  listen<DashboardRefreshFailedPayload>(DASHBOARD_REFRESH_FAILED, (ev) =>
    handler(ev.payload),
  );

/**
 * DCP-FR-16: a proposal was recorded or decided, carrying the whole proposal so
 * a consumer redraws from the payload without a read of its own.
 *
 * Two consumers, for two different reasons: the New Artifact tab and the Drafts
 * panel keep their pending markers current from it (NAW-FR-35, DRP-FR-19), and
 * the shell raises a notification from it (DCR-FR-18) — which is why the
 * subscription is mounted at the shell rather than in a tab that may be closed.
 */
export const onDraftChangeProposalsChanged = (
  handler: (payload: ProposalsChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<ProposalsChangedPayload>(DRAFT_CHANGE_PROPOSALS_CHANGED, (event) =>
    handler(event.payload),
  );

/**
 * PCP-FR-17: a proposal against a prompt artifact was recorded or decided,
 * carrying the whole proposal so a consumer redraws from the payload without a
 * read of its own.
 *
 * Subscribed at the **shell** rather than in the Editor tab that renders the
 * review (PCR-FR-17): a tab-scoped listener would hear nothing for a file whose
 * tab is closed, which is exactly the case the author most needs telling about.
 */
export const onPromptChangeProposalsChanged = (
  handler: (payload: PromptProposalsChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<PromptProposalsChangedPayload>(PROMPT_CHANGE_PROPOSALS_CHANGED, (event) =>
    handler(event.payload),
  );

/**
 * CHG-FR-21 / CHC-FR-16: the current change set may have changed (debounced
 * upstream, so a burst arrives as one event). The Changes panel reloads its
 * active mode; an open Diff tab re-fetches its payload (CHG-FR-22).
 */
export const onChangesUpdated = (
  handler: (payload: ChangesUpdatedPayload) => void,
): Promise<UnlistenFn> =>
  listen<ChangesUpdatedPayload>(CHANGES_UPDATED, (ev) => handler(ev.payload));

/**
 * PRG-FR-04 / STB-FR-15: an operation's registration, an advance in its
 * progress or label, or its terminal state. A consumer that subscribes before
 * anything starts converges on the same in-flight set the command would return,
 * without polling.
 */
export const onOperationProgress = (
  handler: (operation: Operation) => void,
): Promise<UnlistenFn> =>
  listen<Operation>(OPERATION_PROGRESS, (ev) => handler(ev.payload));

/**
 * WTC-FR-16 / WTS-FR-25: the active worktree — or the branch checked out in it
 * — changed, by any route. The chrome control relabels from this, so a checkout
 * made from the Git panel's branches section is reflected without reopening the
 * dropdown.
 */
export const onWorktreeContextChanged = (
  handler: (payload: WorktreeContextChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<WorktreeContextChangedPayload>(WORKTREE_CONTEXT_CHANGED, (ev) =>
    handler(ev.payload),
  );

/**
 * WTC-FR-25 / WTS-FR-32: the repository's set of branches may differ
 * from what a consumer holds — a refresh re-read it.
 *
 * Distinct from `onWorktreeContextChanged` on purpose: this never implies the
 * active checkout moved, so a consumer that follows it reloads a branch listing
 * without discarding anything the way a worktree switch does. The payload names
 * the repository and no branches, so each consumer reloads its own listing.
 */
export const onBranchesChanged = (
  handler: (payload: BranchesChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<BranchesChangedPayload>(BRANCHES_CHANGED, (ev) => handler(ev.payload));

/**
 * GPP-FR-TYOV: the GitHub polling view changed. Followed by the main window's
 * polling schedule (GIT-FR-SLRD), the Ready tasks section (GIT-FR-OGHO), the
 * Project settings section (SET-FR-GXJU), and the notification raise
 * (NTF-FR-JLXL).
 */
export const onGithubPollingChanged = (
  handler: (payload: GithubPollingChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<GithubPollingChangedPayload>(GITHUB_POLLING_CHANGED, (ev) =>
    handler(ev.payload ?? { newIssues: [] }),
  );

/**
 * GTC-FR-04 / GIT-FR-PZIE: a line of push output. Surface-agnostic on purpose —
 * a push started from the Changes panel's action control emits on this same
 * channel as one started from the Git panel's branches section, because a push
 * is one operation however it was reached (GTC-FR-22). That is what makes the
 * Git panel's output area the record of every transfer, wherever it began.
 *
 * No line carries a token secret, and a remote URL echoed by the transport
 * arrives with its credential redacted (GTC-FR-11).
 */
export const onGitOutputLine = (
  handler: (payload: GitOutputLinePayload) => void,
): Promise<UnlistenFn> =>
  listen<GitOutputLinePayload>(GIT_OUTPUT_LINE, (ev) => handler(ev.payload));

/**
 * GTC-FR-04 / CHG-FR-43: a transfer reached a terminal state. Emitted exactly
 * once per operation whatever the outcome, so a consumer that renders "running"
 * always has an event that clears it.
 */
export const onGitOperationFinished = (
  handler: (payload: GitOperationFinishedPayload) => void,
): Promise<UnlistenFn> =>
  listen<GitOperationFinishedPayload>(GIT_OPERATION_FINISHED, (ev) =>
    handler(ev.payload),
  );

/**
 * SCC-FR-19 / SCH-FR-16: hits found since the previous emission, streamed while
 * the search is still running rather than withheld until it ends. Every batch
 * carries the id of the search that produced it, so a consumer discards batches
 * belonging to a search it has moved on from (SCC-FR-13).
 */
export const onSearchResults = (
  handler: (payload: SearchResultsPayload) => void,
): Promise<UnlistenFn> =>
  listen<SearchResultsPayload>(SEARCH_RESULTS, (ev) => handler(ev.payload));

/**
 * SCC-FR-13: a search reached a terminal state. Emitted exactly once per search
 * whatever the outcome, and never followed by another batch for that id — which
 * is what lets a consumer keying by id drop its accumulator here.
 */
export const onSearchEnded = (
  handler: (payload: SearchEndedPayload) => void,
): Promise<UnlistenFn> =>
  listen<SearchEndedPayload>(SEARCH_ENDED, (ev) => handler(ev.payload));

/**
 * AGC-FR-21: a turn was registered, or reached a terminal state. Exactly two
 * events per turn — the registration and the terminal state — and no event
 * carries a turn's id after it, so a consumer watching only events converges on
 * the same in-flight set `listAgentTurns` would return, without polling.
 */
export const onAgentTurnStateChanged = (
  handler: (turn: AgentTurn) => void,
): Promise<UnlistenFn> =>
  listen<AgentTurn>(AGENT_TURN_STATE_CHANGED, (ev) => handler(ev.payload));

/**
 * CMS-FR-51: a thread this application appended to, carrying the whole folded
 * thread as it stands after that write.
 *
 * Every append emits — a comment posted here or in another tab, a lock, a
 * resolution, a re-anchor, and an agent's answer landing alike — so a surface
 * showing that conversation redraws from the payload rather than re-reading
 * (CMT-FR-04, CMP-FR-18). Nothing is emitted for a write that did not happen: a
 * refusal and a no-op lock are both silent (CMS-FR-52).
 *
 * This is not a subscription to the log files. A different process appending to
 * one emits nothing, which is CMS-FR-31 — its comment surfaces on the next read.
 */
export const onDiscussionChanged = (
  handler: (discussion: Discussion) => void,
): Promise<UnlistenFn> =>
  listen<Discussion>(DISCUSSION_CHANGED, (ev) => handler(ev.payload));

/**
 * CMS-FR-BQEN: a discussion gained a question set, or stopped holding one.
 *
 * DQA-FR-JWEF: this is how a surface learns a set appeared without asking again
 * on a timer, and how one submitted in another surface goes.
 */
export const onDiscussionQuestionSetChanged = (
  handler: (payload: { discussionId: string; set: PendingQuestionSet | null }) => void,
): Promise<UnlistenFn> =>
  listen<{ discussionId: string; set: PendingQuestionSet | null }>(
    DISCUSSION_QUESTION_SET_CHANGED,
    (ev) => handler(ev.payload),
  );

/**
 * LGC-FR-17: records were appended to the session buffer, or it was cleared.
 * Coalesced upstream, so a burst of a thousand appends arrives as one event
 * carrying the latest state.
 *
 * The payload carries the buffer's *shape* and none of its contents (LGC-FR-12)
 * — a generation, two totals, and the newest sequence. That is deliberate: a
 * consumer cannot evaluate a filter from it and so cannot drift from what
 * `queryLogs` would return, which is what makes "all filtering happens on the
 * backend" structural rather than a convention. The Logs panel follows this and
 * re-queries; a `generation` other than the one it holds means the buffer it was
 * rendering has been cleared and everything it holds is stale (LOG-FR-13).
 */
export const onLogRecordsAppended = (
  handler: (state: LogBufferState) => void,
): Promise<UnlistenFn> =>
  listen<LogBufferState>(LOG_RECORDS_APPENDED, (ev) => handler(ev.payload));

/**
 * NTD-FR-09 / NTF-FR-16: the author activated a notification this run posted.
 *
 * The window has already been raised and focused by the time this arrives, so a
 * consumer routes into a window the author can already see and never raises it
 * itself. Emitted exactly once per activation (NTD-FR-10), and never for a
 * notification that was merely dismissed (NTD-FR-11) or that belongs to an
 * earlier run of the application (NTD-FR-13).
 *
 * The payload is the `synthesis://` address the raiser minted, carried opaquely
 * through the operating system; `src/state/notificationAddress.ts` is the only
 * thing that parses it.
 */
export const onNotificationActivated = (
  handler: (activation: NotificationActivation) => void,
): Promise<UnlistenFn> =>
  listen<NotificationActivation>(NOTIFICATION_ACTIVATED, (ev) =>
    handler(ev.payload),
  );

/**
 * AGV-FR-11: a run's agent activity changed.
 *
 * Carries state and no record content, so a consumer re-reads under whatever
 * cursor it holds rather than rendering what the emitter happened to send —
 * the same arrangement the log panel uses (LGC-FR-12).
 */
export const onAgentActivityAppended = (
  handler: (state: AgentActivityState) => void,
): Promise<UnlistenFn> =>
  listen<AgentActivityState>(AGENT_ACTIVITY_APPENDED, (event) =>
    handler(event.payload),
  );

/**
 * GRD-FR-EFAU: the queue's membership or ordering changed.
 *
 * Consumers re-list rather than reading a queue off the event, which is why the
 * payload names the project and nothing else — a surface that rendered from the
 * payload would be rendering what the emitter believed rather than what the
 * store holds (GRU-FR-ZVTC).
 */
export const onGraduationQueueChanged = (
  handler: (payload: GraduationQueueChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<GraduationQueueChangedPayload>(GRADUATION_QUEUE_CHANGED, (event) =>
    handler(event.payload),
  );

/**
 * WKS-FR-WULF / WSS-FR-JBYF: the project's work streams changed. Every reader
 * reloads its own listing rather than being told what changed, so a listing is
 * never a guess about what a create, a merge or a delete did.
 */
export const onWorkStreamsChanged = (
  handler: (payload: WorkStreamsChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<WorkStreamsChangedPayload>(WORK_STREAMS_CHANGED, (event) =>
    handler(event.payload),
  );

/**
 * WKS-FR-FQLS: a running update advanced a turn, or settled.
 *
 * An update carries the strategy the author chose, so a row names the
 * direction of its work from this event (WSS-FR-BDMU).
 */
export const onWorkStreamUpdateProgress = (
  handler: (payload: WorkStreamUpdateProgress) => void,
): Promise<UnlistenFn> =>
  listen<WorkStreamUpdateProgress>(WORK_STREAM_UPDATE_PROGRESS, (event) =>
    handler(event.payload),
  );

/**
 * GRD-FR-EFAU: one run's state or record changed, after the change landed in the
 * store. The Runs panel re-reads the run it names; an open review follows the
 * run it is about rather than arguing with it.
 */
export const onGraduationRunChanged = (
  handler: (payload: GraduationRunChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<GraduationRunChangedPayload>(GRADUATION_RUN_CHANGED, (event) =>
    handler(event.payload),
  );

/**
 * GRS-FR-UCZL: records of one run and one stream became durable.
 *
 * The payload carries no record content, so a consumer re-reads under its own
 * cursor rather than rendering what the emitter sent.
 */
export const onGraduationLogRecordsAppended = (
  handler: (payload: GraduationLogAppendedPayload) => void,
): Promise<UnlistenFn> =>
  listen<GraduationLogAppendedPayload>(
    GRADUATION_LOG_RECORDS_APPENDED,
    (event) => handler(event.payload),
  );

// --- Settings child windows (SWN-settings-windows.md) ----------------------
//
// Four of these are the backend's (`settings_window.rs`, byte-for-byte); the
// last two are the frontend's own, emitted from one window and listened to in
// another. A settings window is a separate webview with its own module
// instances, so a fact one window owns reaches the other as an event rather
// than as shared state — there is none to share.

/** SWN-FR-08: save every pending change in every section and report back. */
export const SETTINGS_SAVE_AND_CLOSE = "settings-window:save-and-close";
/** SWN-FR-06 / SWN-FR-13: present a named section in the window already open. */
export const SETTINGS_ROUTE = "settings-window:route";
/** SWN-FR-11: the sweep failed; present the section that could not be written. */
export const SETTINGS_PRESENT_FAILURE = "settings-window:present-failure";
/**
 * Which settings window is open, or that none is. The main window needs it for
 * the already-looking-at suppression of NTF-FR-08.
 */
export const SETTINGS_CHANGED = "settings-window:changed";
/**
 * A settings window took or lost OS focus. NTF-FR-08 asks whether **any** window
 * of the application holds focus, and the main window cannot see this one's.
 */
export const SETTINGS_FOCUS = "settings-window:focus";
/**
 * GLS-FR-27 / NTF-FR-25: the Notifications section asked for a rehearsal.
 *
 * The delay and the raise belong to the main window rather than to the section:
 * the whole point of the rehearsal is that the author leaves — closing this
 * window among the ways — and a timer that went with the window would take the
 * notification with it (NTF-FR-25, NTF-FR-17, GLS-FR-27).
 */
export const SETTINGS_REHEARSAL_REQUESTED = "settings-window:rehearsal-requested";
/**
 * GLS-FR-05 / GLS-FR-20: the user-global app preferences were written. The
 * theme and the three typographic roles apply to the whole application
 * immediately, and the window that edits them is no longer the window that
 * shows most of it.
 */
export const APP_PREFERENCES_CHANGED = "app-preferences:changed";
/**
 * SET-FR-11 / STB-FR-19: the project's line-ending convention was written from
 * the other window. Each control reflects a change made from the other without
 * a reload, and a change marks every open Editor tab dirty (STB-FR-18).
 */
export const LINE_ENDINGS_CHANGED = "project-config:line-endings-changed";

/** SWN-FR-08: subscribe to the sweep request this settings window must answer. */
export const onSettingsSaveAndClose = (
  handler: () => void,
): Promise<UnlistenFn> => listen(SETTINGS_SAVE_AND_CLOSE, () => handler());

/** SWN-FR-06 / SWN-FR-13: subscribe to section routing for this window. */
export const onSettingsRoute = (
  handler: (section: string | null) => void,
): Promise<UnlistenFn> =>
  listen<{ section: string | null }>(SETTINGS_ROUTE, (event) =>
    handler(event.payload?.section ?? null),
  );

/** SWN-FR-11: subscribe to the failed-section presentation for this window. */
export const onSettingsPresentFailure = (
  handler: (section: string | null) => void,
): Promise<UnlistenFn> =>
  listen<{ section: string | null }>(SETTINGS_PRESENT_FAILURE, (event) =>
    handler(event.payload?.section ?? null),
  );

/** SWN-FR-05: subscribe to which settings window is open. */
export const onSettingsWindowChanged = (
  handler: (open: SettingsWindowKind | null) => void,
): Promise<UnlistenFn> =>
  listen<{ open: SettingsWindowKind | null }>(SETTINGS_CHANGED, (event) =>
    handler(event.payload?.open ?? null),
  );

/** NTF-FR-08: subscribe to a settings window taking or losing OS focus. */
export const onSettingsWindowFocus = (
  handler: (focused: boolean) => void,
): Promise<UnlistenFn> =>
  listen<{ focused: boolean }>(SETTINGS_FOCUS, (event) =>
    handler(event.payload?.focused === true),
  );

/** GLS-FR-27: subscribe to a rehearsal the Notifications section asked for. */
export const onSettingsRehearsalRequested = (
  handler: (address: string) => void,
): Promise<UnlistenFn> =>
  listen<{ address: string }>(SETTINGS_REHEARSAL_REQUESTED, (event) => {
    const address = event.payload?.address;
    if (typeof address === "string" && address.length > 0) handler(address);
  });

/** GLS-FR-05 / GLS-FR-20: subscribe to a user-global preferences write. */
export const onAppPreferencesChanged = (
  handler: () => void,
): Promise<UnlistenFn> => listen(APP_PREFERENCES_CHANGED, () => handler());

/** SET-FR-11: subscribe to a line-ending convention written elsewhere. */
export const onLineEndingsChanged = (
  handler: (value: "lf" | "crlf") => void,
): Promise<UnlistenFn> =>
  listen<{ value: "lf" | "crlf" }>(LINE_ENDINGS_CHANGED, (event) => {
    const value = event.payload?.value;
    if (value === "lf" || value === "crlf") handler(value);
  });

/**
 * Announce something to the application's other windows.
 *
 * Best-effort by design: the window making the announcement has already applied
 * the change locally, so a delivery that fails costs the other window a stale
 * reflection rather than costing anyone the change itself. The operation that
 * prompted the announcement logs its own outcome; this is the wire, not the
 * event worth a record.
 */
async function announce(name: string, payload?: unknown): Promise<void> {
  try {
    await emit(name, payload);
  } catch {
    // No other window, no bridge, or a rejected emit — see above.
  }
}

/** GLS-FR-27 / NTF-FR-25: ask the main window to run a notification rehearsal. */
export const emitSettingsRehearsalRequested = (address: string): Promise<void> =>
  announce(SETTINGS_REHEARSAL_REQUESTED, { address });

/** GLS-FR-05 / GLS-FR-20: the user-global app preferences were written. */
export const emitAppPreferencesChanged = (): Promise<void> =>
  announce(APP_PREFERENCES_CHANGED);

/** SET-FR-11 / STB-FR-19: the project's line-ending convention was written. */
export const emitLineEndingsChanged = (value: "lf" | "crlf"): Promise<void> =>
  announce(LINE_ENDINGS_CHANGED, { value });
