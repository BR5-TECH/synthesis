/**
 * Note publication — the texts and the toasts of the note publish modal
 * (`specifications/ui/NIP-note-issue-publication.md`, `NTF-notifications.md`
 * NTF-FR-AGRZ, NTF-FR-KBBO, NTF-FR-IWAR, NTF-FR-XNCG).
 *
 * The toasts have no address: each is the author's own action answered at
 * once. Two of them carry a link that opens the issue in the system browser
 * through `"open publication repository issue (url)"` (GHP-FR-MAMB).
 */
import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import { githubPollingErrorMessage, refusalCode } from "./githubPolling";
import { publicationErrorText } from "./publicationChoice";
import { showToast } from "./toasts";

/** NIP-FR-FDAN / GHP-FR-HKMC: the most characters a title holds. */
export const MAX_TITLE_CHARS = 256;
/** NIP-FR-DIXC / GHP-FR-HKMC: the most characters a description holds. */
export const MAX_BODY_CHARS = 65_536;

/** NTF-FR-AGRZ: the key and the title of the success toast. */
export const NOTE_PUBLISHED_KEY = "note-published";
export const NOTE_PUBLISHED_TITLE = "Issue created";
/** NTF-FR-KBBO: the key and the title of the failure toast. */
export const NOTE_PUBLISH_FAILED_KEY = "note-publish-failed";
export const NOTE_PUBLISH_FAILED_TITLE = "Note not published";
/** NTF-FR-IWAR: the key and the title of the toast of a note that stays. */
export const NOTE_NOT_REMOVED_KEY = "note-not-removed";
export const NOTE_NOT_REMOVED_TITLE = "Note not removed";

/**
 * GHP-FR-HKMC: the length of a text as the backend counts it, in Unicode code
 * points rather than in UTF-16 units.
 */
export function charCount(text: string): number {
  return Array.from(text).length;
}

/** The note-publication codes, in words. */
const NOTE_ERROR_TEXT: Record<string, string> = {
  github_project_not_selected: "No GitHub Project is selected in Project settings.",
  note_not_found: "The note does not exist any more.",
  invalid_note_publication:
    "The title must not be empty and must hold at most 256 characters, and the description at most 65,536 characters.",
  label_unavailable: "GitHub does not offer one of the selected labels.",
  issue_create_failed: "GitHub did not create the issue.",
  issue_url_refused: "That address is not an issue of the publication repository.",
  discussion_cleanup_failed: "The discussion of the note could not be removed.",
  note_delete_failed: "The note file could not be removed.",
};

/** NIP-FR-ISQT: the displayable text of a failed note publication. */
export function notePublicationErrorText(raw: unknown): string {
  const code = refusalCode(raw);
  const known = NOTE_ERROR_TEXT[code];
  if (known) return `${known} (${code})`;
  const publication = publicationErrorText(String(raw));
  return publication !== String(raw) ? publication : githubPollingErrorMessage(raw);
}

/** NIP-FR-SNLZ: the inline text of an issue that was not linked. */
export function linkFailedText(issueNumber: number): string {
  return `Issue #${issueNumber} was created, but GitHub did not link it to its parent. The note is kept. A second Publish creates a second issue.`;
}

/** NTF-FR-XNCG: the action of a toast link, which opens the issue. */
function openIssue(url: string): () => void {
  return () => {
    api.openPublicationRepositoryIssue(url).catch((e) => {
      logWarn(["frontend", "remote"], "published issue could not be opened", {
        code: refusalCode(e),
      });
    });
  };
}

/** NTF-FR-AGRZ: the Info toast of a published note. */
export function showNotePublishedToast(
  issueNumber: number,
  issueUrl: string,
  repository: string,
): void {
  logInfo(["frontend", "remote"], "note published toast shown", { issueNumber });
  showToast({
    key: NOTE_PUBLISHED_KEY,
    level: "Info",
    title: NOTE_PUBLISHED_TITLE,
    body: `Issue #${issueNumber} was created in ${repository}.`,
    address: null,
    link: { label: `Open issue #${issueNumber}`, onActivate: openIssue(issueUrl) },
  });
}

/** NTF-FR-KBBO: the Error toast of a note publication that failed. */
export function showNotePublishFailedToast(body: string): void {
  showToast(
    {
      key: NOTE_PUBLISH_FAILED_KEY,
      level: "Error",
      title: NOTE_PUBLISH_FAILED_TITLE,
      body,
      address: null,
    },
    { priority: true },
  );
}

/** NTF-FR-IWAR: the Error toast of an issue whose note stays. */
export function showNoteNotRemovedToast(
  issueNumber: number,
  issueUrl: string,
  errorCode: string,
): void {
  showToast(
    {
      key: NOTE_NOT_REMOVED_KEY,
      level: "Error",
      title: NOTE_NOT_REMOVED_TITLE,
      body: `Issue #${issueNumber} was created, but the note could not be removed. ${notePublicationErrorText(errorCode)}`,
      address: null,
      link: { label: `Open issue #${issueNumber}`, onActivate: openIssue(issueUrl) },
    },
    { priority: true },
  );
}
