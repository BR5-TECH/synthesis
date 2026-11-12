/**
 * The typed refusals of a work stream operation, in words
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-NPXC).
 *
 * Every refusal the backend answers with is a code the author never sees. This
 * is the one place that turns one into a sentence, so a row, a window and a
 * log all say the same thing about the same code.
 */

import { shortForm, toOpenSpecFailure } from "../../state/openspec/failureNotice";
import { STREAM_ERRORS } from "../../types";

export function splitRefusal(text: string): [string, string | null] {
  const cleaned = text.replace(/^Error:\s*/, "");
  const at = cleaned.indexOf(": ");
  if (at === -1) return [cleaned.trim(), null];
  return [cleaned.slice(0, at).trim(), cleaned.slice(at + 2).trim()];
}

/**
 * WSS-FR-RWLB: every state is carried in words. A typed refusal is a code, and
 * a code is not a sentence.
 */
export function refusalText(text: string): string {
  const [code, detail] = splitRefusal(text);
  const archive = archiveRefusalText(code, detail);
  if (archive !== null) return archive;
  // GSU-FR-RQDS / WSS-FR-NPXC: the path is the refusal's detail, so the sentence
  // names it rather than appending it.
  if (code === STREAM_ERRORS.dockerSocketUnavailable) {
    const where = detail ? `at ${detail}` : "at the configured path";
    return `The Docker socket is not available ${where}, so a conflicting merge cannot be handed off. Start Docker or restore the socket at that path, or correct the Docker-in-Docker socket path in Project settings, in the Docker section. Nothing was written.`;
  }
  // WKS-FR-HRUZ, WSS-FR-KPER: the branch is the refusal's detail.
  if (code === STREAM_ERRORS.branchExists) {
    return `The branch ${detail ?? "of that name"} already exists. Choose another name, or delete that branch in the Git panel.`;
  }
  const said: Record<string, string> = {
    [STREAM_ERRORS.notAGitRepository]: "This project is not inside a Git repository.",
    [STREAM_ERRORS.baseBranchRequired]:
      "Name the branch this stream is created from.",
    [STREAM_ERRORS.nameTaken]: "Another stream already uses that name.",
    [STREAM_ERRORS.nameInvalid]: "That name holds no character a branch can use.",
    [STREAM_ERRORS.creationFailed]: "The stream could not be created.",
    [STREAM_ERRORS.cleanupFailed]: "Some of the stream could not be removed.",
    [STREAM_ERRORS.unknownStream]: "That stream is not there any more.",
    [STREAM_ERRORS.busy]: "A run holds this stream or waits in its queue.",
    [STREAM_ERRORS.dirty]: "The stream holds work that is not committed.",
    [STREAM_ERRORS.baseDirty]:
      "The base branch's worktree holds work that is not committed.",
    [STREAM_ERRORS.missing]: "This stream's working copy is gone.",
    [STREAM_ERRORS.baseNotCheckedOut]: "No worktree holds the base branch.",
    [STREAM_ERRORS.unmerged]: "This stream holds commits its base branch does not.",
    [STREAM_ERRORS.hasRuns]:
      "A run of this stream is not finished, so the stream stays.",
    [STREAM_ERRORS.active]:
      "This stream is the active worktree. Open another worktree first.",
    [STREAM_ERRORS.mergeInProgress]:
      "This repository is already merging or updating a stream.",
    [STREAM_ERRORS.mergeBranchMoved]:
      "The stream branch or the base branch moved after the merge was handed off. Nothing was written to either branch. Start the merge again.",
    [STREAM_ERRORS.mergeDirtySide]:
      "A working copy of the stream or of the base branch holds uncommitted changes, so the merge could not be applied. Nothing was written.",
    [STREAM_ERRORS.mergeGuardHeld]:
      "Another merge or update of the repository holds the repository. Nothing was written.",
    [STREAM_ERRORS.mergeApplyFailed]:
      "The application could not write the merge result. Nothing was written.",
    [STREAM_ERRORS.vendorImageUnconfigured]:
      "This project has no agent image configured for the active agent, so a conflicting merge cannot be handed off. Nothing was written.",
    [STREAM_ERRORS.vendorImageInvalid]:
      "This project's agent image cannot carry a run, so a conflicting merge cannot be handed off. Nothing was written.",
    [STREAM_ERRORS.vendorExecutionUnsupported]:
      "The active agent cannot be executed in a container, so a conflicting merge cannot be handed off. Nothing was written.",
    [STREAM_ERRORS.dockerBackendUnverified]:
      "The machine's Docker backend has not been verified, so a conflicting merge cannot be handed off. Nothing was written.",
    [STREAM_ERRORS.unsupportedConflict]:
      "Git found a conflict that a text edit cannot settle, such as a symbolic link, a submodule, or a path that is a file on one side and a directory on the other. Settle it with Git. Nothing was written.",
    [STREAM_ERRORS.artifactGenerationFailed]:
      "The material the update turn reads could not be written.",
    [STREAM_ERRORS.staleBaseRevision]:
      "The source branch moved since this window read it. Nothing was written — open Update again.",
    [STREAM_ERRORS.updateAttemptsExhausted]:
      "Three agent turns could not settle the conflict. Nothing was written.",
    [STREAM_ERRORS.updateInProgress]: "This repository is already updating a stream.",
    [STREAM_ERRORS.updateCancelled]:
      "The update was cancelled. Neither branch was changed.",
    [STREAM_ERRORS.updateInterrupted]:
      "The application stopped while this update was running. Neither branch was changed.",
    [STREAM_ERRORS.updateStateNotPermitted]:
      "This update does not rest where that action needs it to.",
  };
  const sentence = said[code];
  // A reason this surface has no wording for is rendered whole. Cutting it at
  // its first colon would leave the author the front half of a sentence.
  if (!sentence) return detail ? `${code}: ${detail}` : code || text;
  return detail ? `${sentence} ${detail}` : sentence;
}

/** OAR-FR-BEIK: each kind of pending item, in words. */
const PENDING_KIND: Record<string, string> = {
  edit_proposal: "an edit proposal that is not decided or not committed",
  new_change_proposal: "a new-change proposal that is not decided or not committed",
  action_request: "an action request that is not decided",
};

/**
 * WSS-FR-NTGC: the frame sentence of a merge refused by its archive step.
 * Archive commits of changes before the refused one stay on the stream
 * (OAR-FR-OQVF).
 */
export const ARCHIVE_REFUSAL_FRAME =
  "The base branch did not move, and the archive commits stay on the stream.";

/** Join the parts of a list as a sentence does. */
function listed(parts: string[]): string {
  if (parts.length <= 1) return parts.join("");
  return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

/**
 * WSS-FR-NTGC: a merge refused by its archive step. `change_has_pending`
 * names the change and each kind of pending item; its detail is
 * `<change>: <kinds>` (OAR-FR-BEIK). `change_not_archivable` and
 * `archive_failed` render the failure notice of `OFN-openspec-failure-notices.md`
 * where they arrive as an `OpenSpecFailure`; a typed string of them reads as
 * the short form of the notice. Null for every other code.
 */
function archiveRefusalText(code: string, detail: string | null): string | null {
  if (code === STREAM_ERRORS.changeNotArchivable || code === STREAM_ERRORS.archiveFailed) {
    const failure = toOpenSpecFailure(detail ? `${code}: ${detail}` : code);
    const sentence = shortForm(failure);
    if (sentence) return `${sentence} ${ARCHIVE_REFUSAL_FRAME}`;
    // A cause that is not an OpenSpec failure, such as `commit_failed`, is
    // named as it came (OFN-FR-PVYE).
    const name = failure.change ? `“${failure.change}”` : "of this stream";
    const what = code === STREAM_ERRORS.archiveFailed ? "archive" : "check";
    return `The ${what} of the OpenSpec change ${name} failed with ${failure.cause ?? "an unknown code"}. ${ARCHIVE_REFUSAL_FRAME}`;
  }
  if (code !== STREAM_ERRORS.changeHasPending) return null;
  const [change, rest] = detail ? splitRefusal(detail) : ["", null];
  const name = change ? `“${change}”` : "of this stream";
  const kinds = (rest ?? "")
    .split(",")
    .map((k) => k.trim())
    .filter((k) => k !== "")
    .map((k) => PENDING_KIND[k] ?? k);
  const what = kinds.length > 0 ? listed(kinds) : "an item that is not decided";
  return `The OpenSpec change ${name} holds ${what}. Decide ${kinds.length > 1 ? "them" : "it"} first, then merge again. ${ARCHIVE_REFUSAL_FRAME}`;
}
