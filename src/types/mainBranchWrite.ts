/**
 * The main branch write guard (`specifications/core/GTC-git.md` GTC-FR-TTSV,
 * `specifications/ui/MBW-main-branch-write.md`).
 */

/** GTC-FR-VZJR: how GitHub protects a branch. */
export type BranchProtection =
  | "protected"
  | "unprotected"
  | "unknown"
  | "not_applicable";

/** GTC `BranchWriteGuard`. */
export interface BranchWriteGuard {
  /** Empty when the write names no branch (a detached `HEAD`). */
  branch: string;
  /** GTC-FR-UOUS. */
  isDefault: boolean;
  /** GTC-FR-VZJR. */
  protection: BranchProtection;
  /** GTC-FR-VZJR: the typed cause of `unknown`. */
  protectionError?: string;
  /** GTC-FR-TTSV. */
  isMain: boolean;
  /** GTC-FR-YWRZ. */
  confirmed: boolean;
}

/** MBW-FR-DBGA: the writes the confirmation names. */
export type MainBranchWriteKind =
  | "create_draft"
  | "delete_draft"
  | "change_drafts"
  | "accept_change"
  | "create_artifact"
  | "rename_item"
  | "delete_artifact"
  | "paste_artifact"
  | "edit_file"
  | "commit"
  | "push"
  | "merge_stream"
  | "start_graduation"
  | "start_direct_run";
