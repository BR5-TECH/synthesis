/**
 * What a decision on a change edit proposal says to the author
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DTRX,
 * OCW-FR-MLXJ): the refusal at the hunk, and the toasts of a close.
 */
import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { runTitle } from "../../state/graduation/merge";
import { showToast } from "../../state/toasts";
import { CHANGE_PROPOSAL_ERRORS, type ChangeDecisionOutcome } from "../../types";

/** A refusal as a plain code, from whatever the command rejected with. */
export function refusalCode(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

/** OCW-FR-DTRX: a typed refusal of a decision, in words. */
export function changeDecisionMessage(code: string): string {
  const E = CHANGE_PROPOSAL_ERRORS;
  if (code.includes(E.anchorLost)) {
    return "The text this change alters is no longer in the file. Reject it to clear it.";
  }
  if (code.includes(E.hunkAlreadyDecided) || code.includes(E.alreadyDecided)) {
    return "This change has already been decided.";
  }
  if (code.includes(E.hunkWithdrawn)) {
    return "The agent withdrew this change, so it cannot be decided. Nothing was changed.";
  }
  if (code.includes(E.hunkNotFound)) return "This change is no longer part of the proposal.";
  if (code.includes(E.fileExists)) {
    return "A file is already at this path, so this new file cannot be created. Nothing was changed.";
  }
  if (code.includes(E.changeNotFound)) {
    return "The change is no longer on its stream, so this change cannot be applied.";
  }
  if (code.includes(E.unknownStream)) {
    return "The work stream is gone, so this change cannot be applied.";
  }
  if (code.includes(E.discussionResolved)) {
    return "The discussion this proposal was posted in is resolved. Reopen it to decide this change. Nothing was changed.";
  }
  if (code.includes(E.writeFailed)) return "The file could not be written. Nothing was changed.";
  if (code.includes(E.acceptanceInProgress)) {
    return "Another change to this proposal is still being applied. Try again in a moment.";
  }
  if (code.includes(E.candidateStale)) {
    return "This change has been rewritten elsewhere since you edited it.";
  }
  if (code.includes(E.notFound)) return "This proposed change is no longer available.";
  return "The change could not be decided. Nothing was changed.";
}

/**
 * CEP-FR-ESQR: the title of the run that holds a stream, or "". The run is the
 * one of the stream that is working or reviewing. A list that fails, or that
 * holds no such run, gives "".
 */
export async function runHoldingStream(streamId: string): Promise<string> {
  try {
    const queue = await api.listGraduationQueue();
    const run = (queue?.runs ?? []).find(
      (r) => r.streamId === streamId && (r.state === "working" || r.state === "reviewing"),
    );
    return run ? runTitle(run) : "";
  } catch (e) {
    logWarn(["frontend"], "the runs could not be listed to name the run that holds a stream", {
      streamId,
      error: refusalCode(e),
    });
    return "";
  }
}

/**
 * OCW-FR-DTRX / CEP-FR-ESQR: the statement of `stream_busy`, with the name of
 * the run that holds the stream where it is known.
 */
export async function streamBusyMessage(streamId: string): Promise<string> {
  const title = await runHoldingStream(streamId);
  if (title !== "") {
    return `The run “${title}” holds this stream, so this change was not applied. Try again when the run stops working.`;
  }
  return "A graduation run holds this stream, so this change was not applied. Try again when the run stops working.";
}

/**
 * OCW-FR-MLXJ: the toasts of a close. A close whose commit landed raises an
 * Info toast that names the change. A close whose commit failed raises an
 * Error toast with Retry, which invokes `"complete change edit decision"`.
 */
export function raiseCloseToast(
  change: string,
  outcome: ChangeDecisionOutcome,
  onRetry: (proposalId: string) => void,
): void {
  if (!outcome.resolved) return;
  const key = `openspec-change-commit:${outcome.proposal.id}`;
  if (outcome.committed) {
    logInfo(["frontend"], "a change edit proposal closed with a commit", {
      proposalId: outcome.proposal.id,
    });
    showToast({
      key,
      level: "Info",
      title: `Edits committed to ${change}`,
      body: "The accepted edits are in one commit on the stream branch.",
      address: null,
    });
    return;
  }
  if (!outcome.proposal.commitOwed) return;
  logWarn(["frontend"], "a change edit proposal closed and its commit failed", {
    proposalId: outcome.proposal.id,
  });
  showToast(
    {
      key,
      level: "Error",
      title: `Edits to ${change} are not committed`,
      body: "The accepted edits are written on the stream, but the commit failed.",
      address: null,
      link: { label: "Retry", onActivate: () => onRetry(outcome.proposal.id) },
    },
    { priority: true },
  );
}
