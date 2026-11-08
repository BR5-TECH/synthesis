/**
 * The words of the confirmation (MBW-FR-RXAR, MBW-FR-JZME, MBW-FR-UNAN).
 * No text here holds a token or file content.
 */
import type { BranchWriteGuard, MainBranchWriteKind } from "../../types";

/** MBW-FR-RXAR: what the author asked for, as the second sentence says it. */
export function writeSentence(write: MainBranchWriteKind, branch: string): string {
  switch (write) {
    case "create_draft":
      return `You asked to create a draft. The draft is committed to ${branch}.`;
    case "delete_draft":
      return `You asked to delete a draft. The deletion is committed to ${branch}.`;
    case "change_drafts":
      return `You asked to change the drafts. The change is written to ${branch}.`;
    case "edit_file":
      return `You asked to change a file. The change is saved to ${branch}.`;
    case "commit":
      return `You asked to commit. The commit goes to ${branch}.`;
    case "push":
      return `You asked to push ${branch}.`;
    case "merge_stream":
      return `You asked to merge a stream into ${branch}.`;
    case "accept_change":
      return `You asked to accept a change. The change is written to ${branch}.`;
    case "create_artifact":
      return `You asked to create an artifact. The artifact is written to ${branch}.`;
    case "rename_item":
      return `You asked to rename an item. The new name is written to ${branch}.`;
    case "delete_artifact":
      return `You asked to delete an artifact. The deletion is written to ${branch}.`;
    case "paste_artifact":
      return `You asked to paste an artifact. The copy is written to ${branch}.`;
    case "start_graduation":
      return `You asked to start a graduation. Its draft storage is committed to ${branch}.`;
    case "start_direct_run":
      return `You asked to start a direct run. The run commits to ${branch}.`;
  }
}

/** MBW-FR-RXAR: why the branch is a main branch. */
export function reasonSentence(guard: BranchWriteGuard): string {
  const protectedByGithub = guard.protection === "protected";
  if (guard.isDefault && protectedByGithub)
    return `${guard.branch} is the default branch of this repository, and GitHub protects it.`;
  if (protectedByGithub) return `GitHub protects ${guard.branch}.`;
  return `${guard.branch} is the default branch of this repository.`;
}

/** MBW-FR-JZME: the line for a branch that GitHub protects. */
export function protectedSentence(branch: string): string {
  return `GitHub protects ${branch} and may refuse a push.`;
}

/** MBW-FR-UNAN: the cause of a protection that could not be read. */
export function protectionCause(error: string | undefined): string {
  switch (error) {
    case "github_unreachable":
      return "GitHub is unreachable.";
    case "github_token_rejected":
      return "GitHub refused the project's token.";
    case "github_token_missing":
      return "No GitHub token is stored.";
    case "github_token_selection_required":
      return "No GitHub token is selected for this project.";
    case undefined:
    case "":
      return "The cause is not known.";
    default:
      return error;
  }
}

/** MBW-FR-RXAR: a long branch name, cut in the middle. */
export function middleTruncate(name: string, max = 40): string {
  if (name.length <= max) return name;
  const keep = max - 1;
  const head = Math.ceil(keep / 2);
  return `${name.slice(0, head)}…${name.slice(name.length - (keep - head))}`;
}
