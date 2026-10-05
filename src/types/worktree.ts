// Worktree context (WTC-worktree-context.md / WTS-worktree-selector.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { BottomSurface } from "./logging";

// ---------------------------------------------------------------------------
// Worktree context (WTC-worktree-context.md / WTS-worktree-selector.md)
// ---------------------------------------------------------------------------

/** One of the repository's worktrees (WTC-FR-04 / WTC-FR-05). */
export interface WorktreeEntry {
  /** Absolute path of the worktree directory. */
  path: string;
  /** Display label — the directory's basename. */
  name: string;
  /** Checked-out branch; absent when `isDetached`. */
  branch?: string;
  /** Abbreviated commit id at HEAD. */
  headShortHash: string;
  isDetached: boolean;
  /** Rooting the project right now. */
  isActive: boolean;
  /** The repository's non-linked worktree. */
  isPrimary: boolean;
  /** Git lists it but its directory is gone. */
  isMissing: boolean;
  /** WTC-FR-QKZD: the work stream this worktree is the working copy of. */
  stream?: WorktreeStream;
}

/** WTC-FR-QKZD: what a worktree that is a work stream's carries. */
export interface WorktreeStream {
  streamId: string;
  streamName: string;
  /** The run holding the stream, if one does. */
  busyRunId?: string;
}

/** A branch with no worktree (WTC-FR-06). */
export interface BranchEntry {
  /** Local branch name, or the remote-tracking ref's short name. */
  name: string;
  kind: "local" | "remote";
  upstream?: string;
  headShortHash: string;
}

/** Everything the worktree selector's dropdown renders from (WTC-FR-04). */
export interface WorktreeContext {
  repositoryRoot: string;
  activeWorktreePath: string;
  worktrees: WorktreeEntry[];
  /** Only branches with no worktree. */
  branches: BranchEntry[];
}

/** Payload of the backend `"worktree context changed"` event (WTC-FR-16). */
export interface WorktreeContextChangedPayload {
  activeWorktreePath: string;
  branch?: string;
  isDetached: boolean;
}

/**
 * Payload of the backend `"branches changed"` event (WTC-FR-25).
 *
 * A signal, not a data carrier: it names the repository and nothing else, so
 * each consumer reloads its own listing. The selector reads a `WorktreeContext`
 * and the Git panel reads `listBranches` — two different shapes — and a branch
 * set on the event would have to be one of them.
 */
export interface BranchesChangedPayload {
  repositoryRoot: string;
}

/**
 * How the remote half of a refresh ended (WTC-FR-23).
 *
 * `skipped` is "there was no remote half to run", not a failure: the project has
 * no primary remote, and there is no cause for the author to act on.
 */
export type RemoteRefreshState = "refreshed" | "skipped" | "failed";

/**
 * What `refreshWorktreesAndBranches` answers with (WTC-FR-23).
 *
 * The two halves are reported separately on purpose: `context` is always a fresh
 * enumeration of what is on disk, so a refresh that could not reach the remote
 * still reports a branch created or a worktree added since the last look.
 */
export interface RefreshOutcome {
  context: WorktreeContext;
  remoteState: RemoteRefreshState;
  /** The typed error, when `remoteState` is `"failed"`. */
  remoteError?: string;
}

/**
 * One entry of the Git panel's branches section (GTC-FR-07). Deliberately
 * without a worktree association — that is `WorktreeContext`'s job.
 */
export interface GitBranch {
  name: string;
  kind: "local" | "remote";
  isCurrent: boolean;
}

export type CreateMode = "standalone" | "colocated";

/**
 * Layout preferences payload persisted via the `load_layout_preferences` /
 * `save_layout_preferences` channels (SNV-shell-navigation.md SNV-FR-08). A
 * loose, partial shape: the walking-skeleton frontend round-trips only the
 * main-window geometry fields, so every field is optional.
 *
 * IMPORTANT: `mainWindowOuterWidth` / `mainWindowOuterHeight` are persisted in
 * *logical* pixels. Tauri 2's `outerSize()` returns a `PhysicalSize` (raw device
 * pixels), so on a HiDPI display the value MUST be divided by the window's
 * scaleFactor before persisting; otherwise the window grows by scaleFactor on
 * every launch.
 */
/**
 * SNV-FR-08: the per-project layout payload of `"load layout preferences"` /
 * `"save layout preferences"`.
 *
 * Every field name here must match `LayoutPreferences` in
 * `src-tauri/src/layout.rs` byte-for-byte. The backend struct carries
 * `#[serde(default)]` and does not deny unknown fields, so a field named
 * differently on this side is not a deserialization error — it is silently
 * dropped on save and silently absent on load. Keeping the two in step is what
 * makes the round-trip real.
 *
 * All fields are optional: the backend returns `null` for a slot that was never
 * written, and a partial payload defaults the rest.
 */
export interface LayoutPreferences {
  /**
   * SNV-FR-34: the vertical panel's width as a fraction of the shell's inner
   * width — not a pixel count, so the panel keeps its proportion when the
   * window changes size (SNV-FR-35). Clamping lives in `state/panelLayout.ts`.
   */
  verticalPanelFraction?: number;
  verticalPanelSide?: "left" | "right";
  verticalPanelCollapsed?: boolean;
  verticalPanelHidden?: boolean;
  bottomPanelHeight?: number;
  bottomPanelCollapsed?: boolean;
  bottomPanelHidden?: boolean;
  /**
   * SNV-FR-08 / SNV-FR-46: which of Runs / Logs / Git / History the bottom
   * panel renders. Persisted per project so reopening a project returns to the
   * surface the author was last working in; absent means the Runs default
   * (RUN-FR-01).
   */
  bottomPanelSurface?: BottomSurface;

  // SNV-FR-08 / SNV-FR-12: main window outer dimensions (LOGICAL pixels) +
  // maximized state. The window's OS full-screen state is NOT here — it is
  // user-global, on the app-preferences record (SNV-FR-38 / GSS-FR-19).
  mainWindowOuterWidth?: number;
  mainWindowOuterHeight?: number;
  mainWindowMaximized?: boolean;

  /**
   * SNV-FR-08 / DDS-FR-PNXR: how each draft's New Artifact tab is split between
   * its document column and its discussion column, keyed by draft id.
   *
   * Here rather than on the draft's own record because `draft.toml` is
   * committed content that travels with the draft, and how one author has
   * arranged their columns is a view preference rather than part of what the
   * draft says.
   */
  draftDiscussionRatios?: Record<string, string>;

  /**
   * SNV-FR-08 / DDS-FR-XQMF: whether each draft's discussion column is hidden,
   * keyed by draft id.
   *
   * Beside the ratio rather than inside it: hiding a column is not narrowing
   * it, and a draft returned to must open on the arrangement it was left in.
   */
  draftDiscussionHidden?: Record<string, boolean>;
}

/**
 * Result of the `browse_for_folder` command (FSA-FR-01): either the user
 * cancelled (the bare string) or selected a path.
 */
export type BrowseResult = "cancelled" | { selected: { path: string } };
