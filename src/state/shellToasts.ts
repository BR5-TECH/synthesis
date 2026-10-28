/**
 * The toasts the shell shows about drafts and about the worktree a project
 * opens on (`specifications/ui/NTF-notifications.md` NTF-FR-DRFT,
 * NTF-FR-WTFB, NTF-FR-TSRL).
 */
import { showToast, showToastWhenRootOpens } from "./toasts";

/** NTF-FR-DRFT: the key of the toast a failed draft creation shows. */
export const DRAFT_CREATE_FAILED_KEY = "draft-create-failed";
/** NTF-FR-DRFT: the key of the toast a failed open of a draft shows. */
export const DRAFT_OPEN_FAILED_KEY = "draft-open-failed";
/** NTF-FR-WTFB: the key of the toast a worktree fallback shows. */
export const WORKTREE_FALLBACK_KEY = "worktree-fallback";

/** NTF-FR-DRFT: an Error toast for a draft that was not created. */
export function showDraftCreateFailed(body: string): void {
  // The author just acted, so the toast takes the next free slot.
  showToast(
    { key: DRAFT_CREATE_FAILED_KEY, level: "Error", title: "Draft not created", body, address: null },
    { priority: true },
  );
}

/** NTF-FR-DRFT: an Error toast for a draft that did not open. */
export function showDraftOpenFailed(body: string): void {
  showToast(
    { key: DRAFT_OPEN_FAILED_KEY, level: "Error", title: "Draft not opened", body, address: null },
    { priority: true },
  );
}

/**
 * NTF-FR-WTFB: a Warn toast that the remembered worktree of `projectKey` is
 * gone. It shows when the content root of that project is open.
 */
export function showWorktreeFallback(projectKey: string): void {
  showToastWhenRootOpens(projectKey, {
    key: WORKTREE_FALLBACK_KEY,
    level: "Warn",
    title: "Worktree not found",
    body: "The remembered worktree is no longer on disk. The primary worktree is open.",
    address: null,
  });
}
