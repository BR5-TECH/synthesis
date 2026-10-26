/**
 * The copy part of the publication tag: the issue URL of the current record
 * goes to the system clipboard (`NAW-new-artifact.md` NAW-FR-KVFM,
 * NAW-FR-ZBZH).
 *
 * The copy is a write to the clipboard and nothing else. It invokes no backend
 * operation and changes nothing of the draft.
 */
import { logError, logInfo } from "../../logging";
import { showToast } from "../../state/toasts";

/** NTF-FR-EPOA: the key of the toast a copy that succeeds shows. */
export const LINK_COPIED_KEY = "draft-issue-link-copied";
/** NTF-FR-EPOA: the title of that toast. */
export const LINK_COPIED_TITLE = "Link copied";
/** NTF-FR-RQFW: the key of the toast a copy that fails shows. */
export const LINK_COPY_FAILED_KEY = "draft-issue-link-copy-failed";
/** NTF-FR-RQFW: the title of that toast. */
export const LINK_COPY_FAILED_TITLE = "Link not copied";

/** NTF-FR-EPOA: the body of the toast a copy that succeeds shows. */
export function linkCopiedBody(issueNumber: number): string {
  return `The link to issue #${issueNumber} is on the clipboard.`;
}

/** NTF-FR-RQFW: the body of the toast a copy that fails shows. */
export function linkCopyFailedBody(issueNumber: number): string {
  return `The link to issue #${issueNumber} was not copied to the clipboard.`;
}

/** NTF-FR-RQFW: an Error toast with no address, so a click only dismisses it. */
function failed(issueNumber: number, reason: string, message?: string): false {
  logError(["frontend"], "issue link copy failed", {
    issueNumber,
    reason,
    // A platform DOMException message names the refusal and holds no URL.
    ...(message !== undefined ? { message } : {}),
  });
  // The author just acted, so the toast takes the next free slot.
  showToast(
    {
      key: LINK_COPY_FAILED_KEY,
      level: "Error",
      title: LINK_COPY_FAILED_TITLE,
      body: linkCopyFailedBody(issueNumber),
      address: null,
    },
    { priority: true },
  );
  return false;
}

/**
 * NAW-FR-KVFM / NAW-FR-ZBZH: write `issueUrl` to the clipboard as plain text.
 * Resolves `true` and shows the Info toast of NTF-FR-EPOA on success. Resolves
 * `false` and shows the Error toast of NTF-FR-RQFW on failure. It never
 * rejects.
 */
export async function copyIssueLink(
  issueNumber: number,
  issueUrl: string,
): Promise<boolean> {
  const clipboard = navigator.clipboard;
  if (!clipboard?.writeText) return failed(issueNumber, "unavailable");
  try {
    await clipboard.writeText(issueUrl);
  } catch (e) {
    return e instanceof DOMException
      ? failed(issueNumber, e.name, e.message)
      : failed(issueNumber, e instanceof Error ? e.name : typeof e);
  }
  logInfo(["frontend"], "issue link copied to the clipboard", { issueNumber });
  showToast({
    key: LINK_COPIED_KEY,
    level: "Info",
    title: LINK_COPIED_TITLE,
    body: linkCopiedBody(issueNumber),
    address: null,
  });
  return true;
}
