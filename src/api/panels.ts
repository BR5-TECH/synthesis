/**
 * The panels' own state, the dashboard's readings, and what reports progress.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ChangesPanelState,
  DraftsPanelStateRecord,
  NotificationPermissionState,
  NotificationRequest,
  PostedNotification,
  LibraryPanelState,
  NotesPanelState,
  Operation,
  ProjectConfig,
  ProjectConfigPatch,
  ActiveDraftItem,
  AgentRunItem,
  PendingGitActivity,
  RecentlyEditedArtifact,
} from "../types";

// --- Dashboard widget loaders (dashboard.rs) ------------------------------
//
// Every loader applies its own five-item limit (PST-FR-31..34), so this surface
// renders what it is given and two surfaces cannot disagree about what "the five
// most recent" means. None of them throws for an absent or empty project: an
// empty return is what hides a widget (DSH-FR-04).

/**
 * DSH-FR-09 / PST-FR-31: the five artifacts whose primary source files were
 * modified most recently, most-recent first. There is no recently-edited MRU
 * behind it — the order is the filesystem's (PSS-FR-12).
 */
export const listRecentlyEditedArtifacts = () =>
  invoke<RecentlyEditedArtifact[]>("list_recently_edited_artifacts");

/**
 * DSH-FR-11 / PST-FR-32: the five `active` drafts whose prompt files were
 * written most recently. An archived or graduated draft never appears.
 */
export const listActiveDrafts = () =>
  invoke<ActiveDraftItem[]>("list_active_drafts");

/**
 * DSH-FR-13 / PST-FR-33: the five most recently updated graduation runs, runs
 * still `queued` among them.
 */
export const listRecentAgentRuns = () =>
  invoke<AgentRunItem[]>("list_recent_agent_runs");

/**
 * DSH-FR-14 / PST-FR-34: the four counts describing what the project has
 * pending in Git. Reaches no network — both commit counts come from local refs.
 */
export const listPendingGitActivity = () =>
  invoke<PendingGitActivity>("list_pending_git_activity");

// --- Progress reporting (progress.rs) -------------------------------------

/**
 * PRG-FR-02: the operations in flight right now, most-recently-started first.
 * Returns an empty list when none is, and never errors — so the status bar's
 * mount read (STB-FR-15) needs no failure branch of its own.
 */
export const listInFlightOperations = () =>
  invoke<Operation[]>("list_in_flight_operations");

// --- OS notifications (notifications.rs) ----------------------------------
//
// `NTD-notification-delivery.md`. Reached from `src/state/notifications.ts` and
// nowhere else: the post policy of NTF-FR-08 governs every notification the
// application posts, which only holds while there is one route to these.

/** NTD-FR-02: the platform's disposition. Prompts for nothing, never errors. */
export const getNotificationPermission = () =>
  invoke<NotificationPermissionState>("get_notification_permission");

/** NTD-FR-03: the only call in the application that can raise an OS prompt. */
export const requestNotificationPermission = () =>
  invoke<NotificationPermissionState>("request_notification_permission");

/**
 * NTD-FR-06 / NTD-FR-14: show a notification, replacing one already showing
 * under the same `key`. Rejects with the bare code `permission_denied`,
 * `unsupported`, or `delivery_failed` — a bare code rather than prose so a
 * caller branches on equality instead of matching a substring.
 */
export const postNotification = (request: NotificationRequest) =>
  invoke<PostedNotification>("post_notification", { request });

/** Idempotent: an id naming nothing showing is not an error. */
export const withdrawNotification = (id: string) =>
  invoke<void>("withdraw_notification", { id });

/** NTD-FR-12: withdraw everything this run posted. */
export const withdrawAllNotifications = () =>
  invoke<void>("withdraw_all_notifications");

// --- Project-public config (project_settings.rs) --------------------------

/** PSS-FR-17: the project-public config, incl. the line-ending convention. */
export const loadProjectConfig = () =>
  invoke<ProjectConfig>("load_project_config");

/**
 * PSS-FR-17: persist the project-public sections this payload names, carrying
 * every section it does not name through unchanged. A `draftTemplate` of `""`
 * clears the stored template back to unset (PSS-FR-21 / SET-FR-19).
 */
export const saveProjectConfig = (config: ProjectConfigPatch) =>
  invoke<void>("save_project_config", { config });

// --- Changes panel state (project_settings.rs) ----------------------------

/** PSS-FR-15: the persisted mode + target branch (project-local scope). */
export const loadChangesPanelState = () =>
  invoke<ChangesPanelState>("load_changes_panel_state");

export const saveChangesPanelState = (state: ChangesPanelState) =>
  invoke<void>("save_changes_panel_state", { state });

// --- Library panel state (project_settings.rs) ----------------------------

/**
 * PSS-FR-18 / LIB-FR-14: the persisted expanded folder set and the panel's two
 * local filters (project-local scope, so per machine and per worktree).
 */
export const loadLibraryPanelState = () =>
  invoke<LibraryPanelState>("load_library_panel_state");

export const saveLibraryPanelState = (state: LibraryPanelState) =>
  invoke<void>("save_library_panel_state", { state });

// --- Notes panel state (project_settings.rs) ------------------------------

/**
 * PSS-FR-19 / NTS-FR-09 / NTS-FR-13: the persisted scope position and filter
 * text (project-local scope, so per machine and per worktree).
 */
export const loadNotesPanelState = () =>
  invoke<NotesPanelState>("load_notes_panel_state");

export const saveNotesPanelState = (state: NotesPanelState) =>
  invoke<void>("save_notes_panel_state", { state });

/**
 * PSS-FR-20 / DRP-FR-14: the persisted status position, filter text and set of
 * expanded drafts folders (project-local scope, so per machine and per
 * worktree). Expansion is recorded rather than collapse, and a path naming a
 * folder that is not currently present is retained rather than pruned.
 */
export const loadDraftsPanelState = () =>
  invoke<DraftsPanelStateRecord>("load_drafts_panel_state");

export const saveDraftsPanelState = (state: DraftsPanelStateRecord) =>
  invoke<void>("save_drafts_panel_state", { state });
