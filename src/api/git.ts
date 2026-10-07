/**
 * The repository: what changed, what a commit does with it, and which
 * worktree it is read through.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  CommitOutcome,
  RollbackOutcome,
  BranchOption,
  ChangeSet,
  DiffPayload,
  DiffScope,
  DiffTotals,
  FileRevisions,
  GitBranch,
  BranchComparison,
  BranchDeletionOutcome,
  BranchDeletionPlan,
  BranchInformation,
  CommitFile,
  CommitHistory,
  CreatedPullRequest,
  PullRequestDetail,
  PullRequestHeadState,
  PullRequestListState,
  PullRequestSummary,
  PullRequestTimeline,
  RefreshOutcome,
  UpstreamSyncState,
  WorkingTreeStatus,
  WorktreeContext,
  WorktreeEntry,
} from "../types";

// --- Changes panel (changes.rs) -------------------------------------------

/** CHC-FR-03: the working tree against HEAD, staged and unstaged alike. */
export const listUncommittedChanges = () =>
  invoke<ChangeSet>("list_uncommitted_changes");

/** CHC-FR-04: the working tree against the merge-base with `targetBranch`. */
export const listBranchChanges = (targetBranch: string) =>
  invoke<ChangeSet>("list_branch_changes", { targetBranch });

/** CHC-FR-13: the repository's default branch, the panel's initial target. */
export const getDefaultBranch = () => invoke<string>("get_default_branch");

/** CHC-FR-14: the options for the target-branch picker. */
export const listComparisonBranches = () =>
  invoke<BranchOption[]>("list_comparison_branches");

/**
 * CHC-FR-21 / CHC-FR-22: the added/removed line totals of the uncommitted change
 * set, which the status bar renders as its diff summary (STB-FR-25).
 *
 * A standalone read — it neither requires nor is affected by any prior
 * `listUncommittedChanges` call, so the summary works whether or not the Changes
 * panel has ever been opened (STB-FR-27). Rejects with the typed
 * `"not a git repository"` error outside a repository (STB-FR-29).
 */
export const getUncommittedDiffTotals = () =>
  invoke<DiffTotals>("get_uncommitted_diff_totals");

// --- Diff payload (git.rs) ------------------------------------------------

/** GTC `get_diff(scope)`: the hunks a Diff tab's unified mode renders (DFV-FR-09). */
export const getDiff = (scope: DiffScope) =>
  invoke<DiffPayload>("get_diff", { scope });

/**
 * GTC-FR-16 `get_file_revisions(scope)`: both sides of the comparison for one
 * file, whole. What the side-by-side, final, and rich modes render (DFV-FR-25).
 *
 * The scope already names the path, so it is not passed twice — two sources for
 * one path could disagree, and the tab would diff one file against another.
 */
export const getFileRevisions = (scope: DiffScope) =>
  invoke<FileRevisions>("get_file_revisions", { scope });

// --- Git branches (git.rs) ------------------------------------------------

/** GTC-FR-07: the Git panel's branches section, active branch marked current. */
export const listBranches = () => invoke<GitBranch[]>("list_branches");

/** GTC-FR-BQNM: at most the latest 100 commits reachable from the active worktree's `HEAD`. */
export const listCommitHistory = () =>
  invoke<CommitHistory>("list_commit_history");

/** GTC-FR-YCEV: the paths one commit changed. Rejects `unknown_commit`. */
export const listCommitFiles = (commitId: string) =>
  invoke<CommitFile[]>("list_commit_files", { commitId });

/** GTC-FR-PDSK: one commit's change to one path. Rejects `unknown_commit` or `path_not_in_commit`. */
export const getCommitFileDiff = (commitId: string, path: string) =>
  invoke<DiffPayload>("get_commit_file_diff", { commitId, path });

/**
 * GTC-FR-YQVD / GTC-FR-MBBH: the paths one branch changed against its base.
 * Rejects `"unknown branch"`, `no_comparison_base`, or `no_merge_base`.
 */
export const listBranchCompareFiles = (name: string, kind: "local" | "remote") =>
  invoke<BranchComparison>("list_branch_compare_files", { name, kind });

/**
 * GTC-FR-PYVV: one branch's change to one path against its base. Rejects as
 * `listBranchCompareFiles` does, and `path_not_in_comparison`.
 */
export const getBranchCompareFileDiff = (
  name: string,
  kind: "local" | "remote",
  path: string,
) => invoke<DiffPayload>("get_branch_compare_file_diff", { name, kind, path });

/** GTC-FR-TGOI: what one branch is and holds. */
export const getBranchInformation = (name: string, kind: "local" | "remote") =>
  invoke<BranchInformation>("get_branch_information", { name, kind });

/**
 * GTC-FR-UMXA: read-only. Resolves the plan of a deletion the backend would
 * accept right now, or rejects with the typed refusal `deleteBranch` would
 * return (`branch_in_primary_worktree`, `branch_in_active_worktree`, ...).
 */
export const inspectBranchDeletion = (name: string) =>
  invoke<BranchDeletionPlan>("inspect_branch_deletion", { name });

/**
 * GTC-FR-WNZH: delete a local branch and its linked worktree. The backend
 * re-checks everything; rejects with a typed refusal, `worktree_dirty: a, b`
 * when uncommitted paths exist and `discardUncommitted` is false.
 */
export const deleteBranch = (
  name: string,
  deleteRemote: boolean,
  discardUncommitted: boolean,
) =>
  invoke<BranchDeletionOutcome>("delete_branch", {
    name,
    deleteRemote,
    discardUncommitted,
  });

// --- Pull requests (git/pull_requests.rs) ---------------------------------

/** GTC-FR-GXUB: the primary GitHub remote's pull requests of one state. */
export const listPullRequests = (state: PullRequestListState) =>
  invoke<PullRequestSummary[]>("list_pull_requests", { state });

/** GTC-FR-ZIHE: one pull request's header fields and description. */
export const getPullRequestDetail = (id: number) =>
  invoke<PullRequestDetail>("get_pull_request_detail", { id });

/** GTC-FR-CKTM: one pull request's full conversation and activity, oldest first. */
export const listPullRequestTimeline = (id: number) =>
  invoke<PullRequestTimeline>("list_pull_request_timeline", { id });

/**
 * GTC-FR-MMFM: open a pull request from `head` into `base`. Pushes and commits
 * nothing.
 */
export const createPullRequest = (
  title: string,
  body: string,
  base: string,
  head: string,
  draft: boolean,
) =>
  invoke<CreatedPullRequest>("create_pull_request", {
    title,
    body,
    base,
    head,
    draft,
  });

/**
 * GTC-FR-NEIW: what stands between a local branch and a pull request for it.
 * A null `base` is the repository's default branch.
 */
export const getPullRequestHeadState = (head: string, base: string | null) =>
  invoke<PullRequestHeadState>("get_pull_request_head_state", { head, base });

// --- Commit and push (git.rs) ---------------------------------------------

/**
 * GTC-FR-19 / CMW-FR-07: create one commit holding exactly `paths`, resolving
 * to its hash.
 *
 * Path-scoped rather than index-based: a path that is not named is left exactly
 * as it was and is absent from the commit, including one that happened to be
 * staged. An untracked named path is added to version control by this call
 * (CHG-FR-40), so the caller draws no distinction between tracked and not.
 *
 * Rejects with the typed `empty_commit_message`, `no_paths_selected`, or
 * `nothing_to_commit` (GTC-FR-20); on any of them nothing was written.
 */
export const commitPaths = (
  message: string,
  paths: string[],
  /**
   * GTC-FR-31: bind the commit to one checkout. Where it is given and is not the
   * project's active worktree, the call is refused with
   * `worktree_identity_changed` ahead of every other check, having created no
   * commit and touched no index — so a commit started against the checkout the
   * author was shown lands in that checkout or in none.
   */
  expectedWorktree?: string,
) => invoke<CommitOutcome>("commit_paths", { message, paths, expectedWorktree });

/**
 * GTC-FR-29 … GTC-FR-32: the active worktree's **complete and unclassified**
 * uncommitted state, each path carrying its staged status and its unstaged
 * status separately (GTC-FR-30).
 *
 * The primitive a precondition is read from rather than a surface's change set:
 * it resolves no artifact type, applies no lens, and drops only what the
 * repository's ignore rules exclude. `expectedWorktree` is the identity guard of
 * GTC-FR-31.
 */
export const getWorkingTreeStatus = (expectedWorktree?: string) =>
  invoke<WorkingTreeStatus>("get_working_tree_status", { expectedWorktree });

/**
 * GTC-FR-23 / CHG-FR-62: return exactly `paths` to the state `HEAD` holds for
 * them — restoring the ones Git tracks and removing the ones it does not.
 *
 * **Per-path rather than atomic** (GTC-FR-25): a failure at one path neither
 * aborts the run nor undoes a path already restored, so the result is read entry
 * by entry. `restoredPaths` and `removedPaths` hold only what was confirmed on
 * disk, which is what makes it safe to discard the corresponding in-memory
 * buffer (CHG-FR-63) — a path that failed appears in neither.
 *
 * Rejects with the typed `no_paths_selected` (GTC-FR-27) or `"not a git
 * repository"`; on either, nothing was written.
 */
export const rollbackPaths = (paths: string[]) =>
  invoke<RollbackOutcome>("rollback_paths", { paths });

/**
 * GTC-FR-21 / CHG-FR-37: whether the current branch holds commits its remote
 * does not, which is what decides whether **Push** is offered.
 *
 * Resolved from local refs alone — no network — so reading it offline neither
 * stalls nor errors. Rejects with the typed `"not a git repository"` error
 * outside a repository.
 */
export const getUpstreamSyncState = () =>
  invoke<UpstreamSyncState>("get_upstream_sync_state");

/**
 * GTC-FR-22 / CHG-FR-43: push the current branch to the project's primary
 * remote, authenticated with the token the project resolves (GTC-FR-09).
 *
 * Output streams on the `"git output line"` channel and the terminal status
 * arrives on `"git operation finished"` (see `./events`) — identically whether
 * the push was started from the Git panel or the Changes panel, because a push
 * is one operation however it was reached. Rejects with the two typed token
 * errors of GTC-FR-10, which the caller answers differently (CHG-FR-45).
 */
export const pushCurrentBranch = () => invoke<void>("push_current_branch");

// --- Worktree context (worktree.rs) ---------------------------------------

/** WTC-FR-04 / WTC-FR-06: the worktree selector's dropdown contents. */
export const listWorktreesAndBranches = () =>
  invoke<WorktreeContext>("list_worktrees_and_branches");

/**
 * WTC-FR-07: the chrome control's resting label. Rejects with the typed
 * `"not a git repository"` error when the project's content root is not inside
 * a repository, which is what hides the selector (WTS-FR-02).
 */
export const getActiveWorktree = () =>
  invoke<WorktreeEntry>("get_active_worktree");

/** WTC-FR-08: make an existing worktree the project's content root. */
export const activateWorktree = (path: string) =>
  invoke<WorktreeContext>("activate_worktree", { path });

/** WTC-FR-10: check a branch out in the active worktree, then re-root on it. */
export const checkOutBranchInActiveWorktree = (branch: string) =>
  invoke<WorktreeContext>("check_out_branch_in_active_worktree", { branch });

/** WTC-FR-13: the New worktree dialog's pre-filled location. Read-only. */
export const proposeWorktreePath = (branch: string) =>
  invoke<string>("propose_worktree_path", { branch });

/** WTC-FR-14: create a worktree for `branch` at `path` and root the project on it. */
export const createWorktree = (branch: string, path: string) =>
  invoke<WorktreeContext>("create_worktree", { branch, path });

/**
 * WTC-FR-22 / WTC-FR-23: fetch from the project's primary remote, then
 * re-enumerate — the top-chrome refresh control's action (WTS-FR-30).
 *
 * The one operation in this group that reaches a remote. Its local half always
 * completes, so a resolved promise does NOT mean the remote was reached: read
 * `remoteState` for that, and `remoteError` for the typed cause when it failed.
 * Rejects only when the content root is not inside a Git repository (WTC-FR-02).
 *
 * Changes no checkout (WTC-FR-24), so it never drives the worktree-switch
 * transition and must not be routed through `switchWorktree`.
 */
export const refreshWorktreesAndBranches = () =>
  invoke<RefreshOutcome>("refresh_worktrees_and_branches");
