//! The typed refusals this module answers with (GRD contract surface).

/// No project is open, so there is nothing to run against.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
/// GSU-FR-IJEE: the draft is not a single prompt file.
pub const ERR_DRAFT_NOT_SINGLE_FILE: &str = "draft_not_single_file";
/// GSU-FR-IJEE / GRD-FR-DXWL: a non-terminal run already holds this draft.
pub const ERR_DRAFT_LOCKED: &str = "draft_locked_by_graduation";
/// GSU-FR-JBKZ: the request names no live stream of the project.
pub const ERR_UNKNOWN_STREAM: &str = "unknown_stream";
/// GRD-FR-QJHM: the run is not in a state this operation is accepted from.
pub const ERR_RUN_STATE_NOT_PERMITTED: &str = "run_state_not_permitted";
/// GRD-FR-LGDV: no run of that id.
pub const ERR_UNKNOWN_RUN: &str = "unknown_run";
/// GRD-FR-RHNP: the caller's index is not the one the run holds.
pub const ERR_QUEUE_POSITION_STALE: &str = "queue_position_stale";
/// GRD-FR-RHNP: the target index names no position of that queue.
pub const ERR_QUEUE_POSITION_OUT_OF_RANGE: &str = "queue_position_out_of_range";
/// GRD-FR-JOFE: a filed-away run is not one the author is arranging.
pub const ERR_RUN_ARCHIVED: &str = "run_archived";
/// GRD-FR-BLCR: the run made no commit, so there is nothing to revert.
pub const ERR_RUN_NOT_REVERTABLE: &str = "run_not_revertable";
/// GSU-FR-SQRC: the image preflight's four refusals.
pub const ERR_VENDOR_IMAGE_UNCONFIGURED: &str = "vendor_image_unconfigured";
pub const ERR_VENDOR_IMAGE_INVALID: &str = "vendor_image_invalid";
pub const ERR_VENDOR_EXECUTION_UNSUPPORTED: &str = "vendor_execution_unsupported";
pub const ERR_DOCKER_BACKEND_UNVERIFIED: &str = "docker_backend_unverified";
/// GSU-FR-SZTZ: the active worktree holds no branch, so a direct run has none to
/// pin.
pub const ERR_DIRECT_WORKTREE_DETACHED: &str = "direct_worktree_detached";
/// GSU-FR-SZTZ: the active worktree holds a branch other than the one the
/// author confirmed.
pub const ERR_DIRECT_BRANCH_CHANGED: &str = "direct_branch_changed";
/// GSU-FR-SZTZ: the active worktree holds uncommitted paths.
pub const ERR_DIRECT_WORKTREE_DIRTY: &str = "direct_worktree_dirty";
/// GSU-FR-NPFB / GRD-FR-BSNI: the pinned worktree's directory is gone.
pub const ERR_DIRECT_WORKTREE_MISSING: &str = "direct_worktree_missing";
/// GRD-FR-VAUE: the pinned worktree no longer stands on the pinned branch, so
/// the commit was not made.
pub const ERR_DIRECT_TARGET_CHANGED: &str = "direct_target_changed";
/// GSU-FR-SZTZ: the project is in no Git repository, so it has no worktree.
pub const ERR_NOT_A_GIT_REPOSITORY: &str = "not_a_git_repository";
