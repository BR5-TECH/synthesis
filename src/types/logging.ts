// Session logging (LGC-logging.md / LOG-logs.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { ChangesCommitAction } from "./changes";
import { DiffRenderingMode, DiffVisualizationMode } from "./diff";
import { SearchMode } from "./search";

// --- Session logging (LGC-logging.md / LOG-logs.md) ------------------------

/**
 * LGC-FR-09: the four levels, ordered `DEBUG < INFO < WARN < ERROR`. The panel's
 * selector picks a *floor* rather than a single level, so choosing `WARN` shows
 * `WARN` and `ERROR` (LOG-FR-07).
 */
export type LogLevel = "DEBUG" | "INFO" | "WARN" | "ERROR";

/** The ascending order of `LogLevel`, and the order the panel's selector lists. */
export const LOG_LEVELS: readonly LogLevel[] = ["DEBUG", "INFO", "WARN", "ERROR"];

/**
 * LGC-FR-03: what a record is *about*, rather than which process emitted it — a
 * record emitted by the backend on the author's behalf can be `ai` and `remote`
 * at once, and a record carries a non-empty set of these.
 */
export type LogDomain = "frontend" | "ai" | "backend" | "remote";

/** The domains the panel offers, in the order it renders their checkboxes. */
export const LOG_DOMAINS: readonly LogDomain[] = [
  "frontend",
  "ai",
  "backend",
  "remote",
];

/** A flat map of JSON values (LGC-FR-04) — no nested object, no array. */
export type LogFields = Record<string, unknown>;

/** One diagnostic record as it sits in the session buffer (LGC-FR-03). */
export interface LogRecord {
  /**
   * LGC-FR-05: monotonic, unique for the life of the running application, and
   * never reset by a clear — which is what lets a cursor held from before one
   * never match a record appended after it.
   */
  sequence: number;
  /** ISO-8601 UTC, millisecond precision. The panel renders it in local time. */
  ts: string;
  level: LogLevel;
  domains: LogDomain[];
  message: string;
  fields: LogFields;
}

/** What a caller supplies; `sequence` is stamped by the backend (LGC-FR-05). */
export interface LogInput {
  ts: string;
  level: LogLevel;
  domains: LogDomain[];
  message: string;
  fields: LogFields;
}

/**
 * LGC-FR-09 / LGC-FR-10. Sent with every query and retained by neither side
 * (LGC-FR-13): the panel holds these values in memory for the session
 * (LOG-FR-08) and the backend uses them for the one call.
 */
export interface LogFilter {
  /** Records below this level do not match. */
  minLevel: LogLevel;
  /** A record matches while it carries at least one; empty matches every one. */
  domains: LogDomain[];
  /** Absent or empty matches every record. */
  query?: string;
  /** `false`: case-insensitive substring. `true`: regular expression. */
  queryIsRegex: boolean;
}

/**
 * LGC-FR-22: where a page sits within the match set. Absent means the newest
 * `limit` matching records, which is what the panel asks for on becoming
 * visible (LOG-FR-12).
 */
export type LogCursor = { after: number } | { before: number };

/** One page of the match set, plus the shape of the buffer it came from. */
export interface LogPage {
  /** Always ascending by `sequence`, whichever cursor produced it. */
  records: LogRecord[];
  /** LGC-FR-14: which buffer this page describes. */
  generation: number;
  /** Records in the buffer matching the filter. */
  matchedTotal: number;
  /** Records in the buffer, filter disregarded. */
  bufferTotal: number;
  /** LGC-FR-07: records evicted since the buffer was last cleared. */
  droppedTotal: number;
  highestSequence: number | null;
}

/**
 * What `"log records appended"` carries. No record content, so a consumer
 * cannot evaluate a filter from it (LGC-FR-12) — the panel re-queries instead.
 */
export interface LogBufferState {
  generation: number;
  bufferTotal: number;
  droppedTotal: number;
  highestSequence: number | null;
}

export type Theme = "light" | "dark";
export type PanelSurface =
  | "library"
  | "documents"
  | "notes"
  | "comments"
  | "changes"
  | "drafts";

/**
 * A request that a vertical panel reveal and select one item — expanding its
 * ancestors, scrolling it into view, and relaxing whichever of that panel's
 * local filters would hide it (LIB-FR-18, DRP-FR-34, CHG-FR-54).
 *
 * `id` is whatever the named panel reveals by: the Project panel's path-derived
 * stable key, the Drafts panel's draft id, the Changes panel's project-relative
 * path. `nonce` is what makes two consecutive requests for the *same* item
 * distinct, so a panel keyed on it re-reveals rather than treating the second as
 * a no-op — while still ignoring the panel's own reloads, which do not move it.
 */
export interface PanelRevealRequest {
  panel: PanelSurface;
  id: string;
  nonce: number;
  /**
   * Whether the item may legitimately not be in the panel **yet**.
   *
   * A creation reveal (NFI-FR-12 / NFW-FR-11 / NAW-FR-10) names something that
   * was just written and is arriving on the next watcher-driven reload, so the
   * panel selects it optimistically and the row highlights when it renders. A
   * **tab-follow** (SNV-FR-64) names something that either exists now or does
   * not exist at all, and SNV-FR-67 requires it to change nothing in the second
   * case — so the panel must resolve it before touching anything.
   *
   * Without this distinction one code path has to serve two contradictory
   * requirements, and the optimistic one wins by default: a follow to a deleted
   * file would clear the author's selection and expand ancestors that are not
   * there.
   */
  optimistic: boolean;
  /**
   * Whether the reveal should also move **keyboard focus** to the row.
   *
   * True for a reveal the author asked for directly — creating a drafts folder,
   * confirming **Move to Folder…** — where landing on the row is the point
   * (DRP-FR-30). False for a tab-follow: the author's focus intent there is the
   * *tab* they just activated, and pulling it into the panel means their next
   * keystroke moves a panel selection instead of doing anything in the tab.
   */
  focus: boolean;
}
export type BottomSurface = "runs" | "logs" | "git" | "history";

// The user-global theme preference (GSS-FR-04). Distinct from the applied
// `Theme` ("light" | "dark") because "system" follows the OS colour scheme.
export type ThemePreference = "light" | "dark" | "system";

/**
 * The user-global app-preferences record (GSS-FR-04). One record, two facts
 * with two unrelated editors — see `state/appPreferences.ts`, which is what
 * keeps a write of either from clearing the other (GSS-FR-20).
 */
export interface AppPreferences {
  theme: ThemePreference;
  /**
   * GSS-FR-21 / SCH-FR-13: the universal search bar's active query mode. Read
   * when the search input mounts and written whenever the user changes the
   * active toggle. User-global rather than project-scoped because it describes
   * how the user searches, not anything about a project. Optional so a record
   * from a backend that predates the field still types.
   */
  searchQueryMode?: SearchMode;
  /**
   * GSS-FR-19 / SNV-FR-38: whether the main window is in OS full-screen. Read
   * when the main window mounts (SNV-FR-39) and written whenever the window
   * enters or leaves full-screen. Deliberately user-global rather than part of
   * `LayoutPreferences`, which is per project. The Project picker never applies
   * it (PPK-FR-14). Optional so a record from a backend that predates the field
   * still types.
   */
  mainWindowFullscreen?: boolean;
  /**
   * GSS-FR-24 / DFV-FR-23: how a Diff tab lays a comparison out. Read when a
   * Diff tab mounts and written whenever the user activates a different toggle.
   * User-global rather than per-tab because it describes how the author reads a
   * diff. Optional so a record from a backend that predates the field types.
   */
  diffVisualizationMode?: DiffVisualizationMode;
  /** GSS-FR-24 / DFV-FR-23: source or rich, on the same terms. */
  diffRenderingMode?: DiffRenderingMode;
  /**
   * GSS-FR-25 / CHG-FR-34: which of the Changes panel's three footer actions is
   * selected. User-global rather than project-scoped because it describes the
   * author's committing habit rather than anything about a project. Stored
   * whatever the repository can do at this moment — availability is the panel's
   * decision (CHG-FR-35). Optional so a record from a backend that predates the
   * field still types.
   */
  changesCommitAction?: ChangesCommitAction;
  /**
   * GSS-FR-34 / GRH-FR-MCHQ: the width of the Runs panel's graduation history
   * rail, as a fraction of that panel's usable content width.
   *
   * User-global rather than a field of `LayoutPreferences`, which is per
   * project: the layout slot holds the shape of one project's shell, while this
   * describes how wide the author wants a run history beside a run wherever
   * they read one. The graduation section reads it on mount and writes it when
   * a drag completes or a keyboard adjustment is made.
   *
   * Stored unbounded and clamped where it is read (GRH-FR-MCHQ). Optional so a
   * record from a backend that predates the field still types.
   */
  graduationRailWidthFraction?: number;
  /**
   * GSS-FR-QDNV / GRU-FR-KWRB: the width of the graduation run region's paths
   * column, as a fraction of the width of its two columns. Stored unbounded and
   * clamped where it is read. Optional so a record from a backend that
   * predates the field still types.
   */
  graduationPathsWidthFraction?: number;
  /**
   * GSS-FR-MSPQ / GIT-FR-FATV: the width of the Git panel's files column, as a
   * fraction of the width of its large view. Stored unbounded and clamped where
   * it is read. Optional so a record from a backend that predates the field
   * still types.
   */
  gitFilesWidthFraction?: number;
  /**
   * GSS-FR-29 / GLS-FR-17: the three typographic roles' font settings. The
   * Appearance section reads them on mount and writes them whenever the user
   * changes one of the nine controls. User-global rather than project-scoped
   * because they describe how the author wants to read. Optional so a record
   * from a backend that predates the field still types.
   */
  fonts?: FontSettings;
  /**
   * GSS-FR-32 / GLS-FR-25: whether the application posts OS notifications at
   * all. The Notifications section reads it on mount and writes it whenever the
   * switch changes. User-global because it describes how the author wants to be
   * interrupted rather than anything about a project.
   *
   * Optional so a record from a backend that predates the field still types —
   * and every reader must treat `undefined` as **true**, not false, because a
   * missing field means "never chosen" and the default is on (GSS-FR-32).
   */
  notificationsEnabled?: boolean;
  /**
   * GSS-FR-33 / GLS-FR-28: whether a change of the main viewport's active tab
   * moves the vertical panel's selection to the item that tab is a view onto
   * (SNV-FR-64). The Navigation section reads it on mount and writes it whenever
   * the switch changes; the shell reads it at every qualifying activation.
   *
   * Optional so a record from a backend that predates the field still types —
   * and every reader must treat `undefined` as **true**, not false, because a
   * missing field means "never chosen" and the behaviour is opt-out (GSS-FR-33).
   */
  selectionFollowsTab?: boolean;
}

/**
 * The platform's disposition toward this application
 * (`../core/NTD-notification-delivery.md` NTD-FR-02).
 *
 * `not_requested` and `denied` are distinct because they call for different
 * things from the author — a prompt this application can raise, versus a trip
 * to the operating system's own settings (GLS-FR-26).
 */
export type NotificationPermissionState =
  | "granted"
  | "denied"
  | "not_requested"
  | "unsupported";

/** What `post_notification` is handed (NTD contract surface). */
export interface NotificationRequest {
  /**
   * Identity of the thing being notified about. A post whose key matches one
   * still showing replaces it in place rather than stacking (NTD-FR-06).
   */
  key: string;
  title: string;
  body: string;
  /**
   * The `synthesis://` address, carried opaquely: the backend never parses it
   * (NTD-FR-08), and the grammar lives in `src/state/notificationAddress.ts`.
   */
  payload: string;
}

/** What `post_notification` answers with. */
export interface PostedNotification {
  id: string;
}

/** The `"notification activated"` payload (NTD-FR-09). */
export interface NotificationActivation {
  id: string;
  key: string;
  payload: string;
}

/**
 * One typographic role's font setting (GSS-FR-29).
 *
 * Each field is independently `undefined` until chosen, and `undefined` means
 * "the application's built-in default for this role" — the value the CSS token
 * already carries, not a number the frontend duplicates. That is what lets a
 * user pick a family without also committing to a size, and it is why
 * `resolveFontRole` writes *nothing* to the root for an unset field rather than
 * writing a default over it.
 */
export interface FontRole {
  /** Family name as chosen from `list_system_fonts` (FNT-FR-02). */
  family?: string;
  /** In CSS pixels. Bounded by the Appearance section before it is committed
   *  (GLS-FR-19); storage bounds nothing (GSS-FR-29). */
  sizePx?: number;
  /** A multiplier of the role's font size, not a length. */
  lineHeight?: number;
}

/**
 * The three typographic roles of `OVW-overview.md` OVW-FR-13, each carrying its
 * own family, size, and line height (GSS-FR-29).
 *
 * The partition is total: every piece of text the application renders belongs to
 * exactly one of them, and `ui` is what text belongs to unless one of the other
 * two names it — so a surface added later needs no typographic decision to
 * render correctly.
 */
export interface FontSettings {
  /** Panels, tabs, chrome, modals, the status bar, comment and note bodies. */
  ui?: FontRole;
  /** The prose of a rendered Markdown document: the Editor's WYSIWYG surface
   *  and a Diff tab's Rich rendering (EDT-FR-62, DFV-FR-37). */
  rich?: FontRole;
  /** Literal source text and code: the Editor's raw-text surface, code and
   *  frontmatter inside a rendered document, and a Diff tab's Source rendering
   *  with its line-number gutter (EDT-FR-62, DFV-FR-37). */
  source?: FontRole;
}

/** The three roles, in the order the Appearance section presents them. */
export type FontRoleKey = "ui" | "rich" | "source";

/**
 * One installed font family (`FNT-font-enumeration.md` FNT-FR-02).
 *
 * `monospace` is a *description* of the family and never a permission: the
 * Source code role's control presents fixed-width families first and marks
 * them, but offers the rest below rather than withholding them (GLS-FR-18), so
 * a family the platform describes imprecisely is still choosable for code.
 */
export interface FontFamily {
  family: string;
  monospace: boolean;
}

export interface RecentProject {
  name: string;
  path: string;
  lastOpenedAt: string;
  // Added by user-global settings storage (GSS-FR-05). Optional so the
  // view-and-open Project picker, which ignores them, keeps its existing shape.
  pinned?: boolean;
  missing?: boolean;
}

export interface InstalledPlugin {
  id: string;
  name: string;
  source: string;
}

export interface InstalledAdapter {
  id: string;
  name: string;
  source: string;
}

/**
 * What an open/create returns (PST-FR-01 / `WTC-worktree-context.md` WTC-FR-18).
 *
 * `path` is the project's identity **anchor** — the repository's primary
 * worktree when the project is inside a Git repository, the opened root when it
 * is not. It is what the recent-projects list records, so a repository with
 * several worktrees is one entry. `activeWorktreePath` is the **content root**:
 * the checkout every project path resolves against (WTC-FR-03).
 */
export interface ProjectHandle {
  name: string;
  path: string;
  activeWorktreePath: string;
  /**
   * WTC-FR-17: the project resumed on its primary worktree because the worktree
   * it was last working in no longer exists.
   */
  rememberedWorktreeUnavailable?: boolean;
}
