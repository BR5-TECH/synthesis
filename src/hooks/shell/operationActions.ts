/**
 * The shell's half of a status bar row's activation
 * (`STB-status-bar.md` STB-FR-RWPD).
 *
 * Each destination type opens the surface that owns its target by that
 * surface's own route: the Runs panel's graduation section for a run, the one
 * `revealDiscussion` route for a discussion, and the Git panel for a push.
 */
import * as api from "../../api";
import { discussionSubject } from "../../components/Comments";
import { logDebug, logWarn } from "../../logging";
import { ownerLabelOf } from "../../state/ownerAvailability";
import { destinationOf } from "../../state/operationActivation";
import type { DiscussionReveal } from "../../state/revealDiscussion";
import type { Operation } from "../../types";

export interface OperationActionDeps {
  /** GRU-FR-VQJB: open the Runs panel's graduation section on this run. */
  openGraduationRun: (runId: string) => void;
  /** CVP-FR-06: the one route that reveals a discussion. */
  revealDiscussion: (reveal: DiscussionReveal) => unknown;
  /** GIT-FR-FZMS: open the Git panel and select this branch. */
  openGitBranch: (branch: string) => void;
}

/**
 * CVP-FR-06: the route takes a whole reveal, and the operation names only the
 * discussion id, so the discussion is read by its id first.
 */
async function revealDiscussionById(
  discussionId: string,
  reveal: OperationActionDeps["revealDiscussion"],
): Promise<void> {
  try {
    const discussion = await api.readDiscussion(discussionId);
    reveal({
      discussionId: discussion.id,
      target: discussion.target,
      fragmentTarget: discussion.fragmentTarget,
      resolved: discussion.resolved,
      ownerLabel: ownerLabelOf(discussion.target),
      subject: discussionSubject(discussion),
    });
  } catch (error) {
    // The discussion could not be read, so there is nothing to reveal. The
    // reason is the typed value the command returned, never the conversation.
    logWarn(["frontend"], "a status bar row could not reveal its discussion", {
      discussionId,
      reason: String(error),
    });
  }
}

/**
 * Open the surface that owns an operation's target. Answers whether the
 * operation had a destination; an operation without one opens nothing
 * (STB-FR-DNLC).
 */
export function createOperationActions(deps: OperationActionDeps) {
  return {
    activateOperation(operation: Operation): boolean {
      const destination = destinationOf(operation);
      if (!destination) return false;
      logDebug(["frontend"], "a status bar row was activated", {
        operationId: operation.id,
        destination: destination.type,
      });
      switch (destination.type) {
        case "graduation_run":
          deps.openGraduationRun(destination.runId);
          break;
        case "discussion":
          void revealDiscussionById(
            destination.discussionId,
            deps.revealDiscussion,
          );
          break;
        case "git_push":
          deps.openGitBranch(destination.branch);
          break;
      }
      return true;
    },
  };
}
