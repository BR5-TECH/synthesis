/**
 * **Copy as Source**: the live prompt of a draft goes to the system clipboard
 * (`NAW-new-artifact.md` NAW-FR-NVYZ through NAW-FR-QPTN).
 *
 * The copy is a write to the clipboard and nothing else. It invokes no backend
 * operation, saves nothing, and changes nothing of the draft (NAW-FR-FJXX).
 */
import { logError, logInfo } from "../../logging";
import type { DraftSessionStore } from "../../state/draftSessions";
import { showToast } from "../../state/toasts";

/** NTF-FR-RGVU: the key of the toast a failed copy shows. */
export const COPY_FAILED_KEY = "draft-copy-failed";
/** NTF-FR-RGVU: the title of that toast. */
export const COPY_FAILED_TITLE = "Draft not copied";
/** NTF-FR-RGVU: the body of that toast. */
export const COPY_FAILED_BODY =
  "The prompt of the draft was not copied to the clipboard.";

/** Why a copy failed, as the log records it. */
type CopyFailure = "not_loaded" | "unavailable" | string;

/** NTF-FR-RGVU: an Error toast with no address, so a click only dismisses it. */
function failed(draftId: string, reason: CopyFailure, message?: string): false {
  logError(["frontend"], "draft prompt copy failed", {
    draftId,
    reason,
    // A platform DOMException message names the refusal and holds no prompt text.
    ...(message !== undefined ? { message } : {}),
  });
  // The author just acted, so the toast takes the next free slot.
  showToast(
    {
      key: COPY_FAILED_KEY,
      level: "Error",
      title: COPY_FAILED_TITLE,
      body: COPY_FAILED_BODY,
      address: null,
    },
    { priority: true },
  );
  return false;
}

/**
 * NAW-FR-NVYZ / NAW-FR-KEAL: write the live prompt of `draftId` to the
 * clipboard as plain text, with no change. The text is the edit-session buffer
 * at this moment, so unsaved edits are part of it, and a version being read is
 * not. Resolves `true` on success. On failure it shows the Error toast of
 * NTF-FR-RGVU and resolves `false`. It never rejects.
 *
 * A prompt that has not loaded, or whose read failed, has no live text. To copy empty text then
 * would say **Copied** for a prompt the author never had, so it is a failure.
 */
export async function copyLivePrompt(
  drafts: DraftSessionStore,
  draftId: string,
): Promise<boolean> {
  const prompt = drafts.ensure(draftId).selected;
  const session =
    prompt === null ? undefined : drafts.docs.get(drafts.key(draftId, prompt));
  // A read that failed leaves `loaded` set but no baseline, and no text.
  if (!session?.loaded || session.baseline === null) {
    return failed(draftId, "not_loaded");
  }
  const text = session.buffer;
  const clipboard = navigator.clipboard;
  if (!clipboard?.writeText) return failed(draftId, "unavailable");
  try {
    await clipboard.writeText(text);
  } catch (e) {
    return e instanceof DOMException
      ? failed(draftId, e.name, e.message)
      : failed(draftId, e instanceof Error ? e.name : typeof e);
  }
  // The length only: the prompt is user content and never goes to the log.
  logInfo(["frontend"], "draft prompt copied to the clipboard", {
    draftId,
    length: text.length,
  });
  return true;
}
