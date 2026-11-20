/**
 * Whether the current branch has anything to publish
 * (`specifications/core/GTC-git.md` GTC-FR-21, `specifications/ui/CHG-changes.md`
 * CHG-FR-37).
 *
 * Lives outside both surfaces that ask because both must answer identically: a
 * push started from the Changes panel's footer and one started from the Git
 * panel's branches section are the same operation, so a branch that is level
 * with its remote cannot be pushable from one of them and not the other.
 */
import type { RepositorySyncState, UpstreamSyncState } from "./types";

/** CHG-FR-37: whether one standing holds something to publish. */
function standingPushable(state: {
  hasRemote: boolean;
  hasUpstream: boolean;
  ahead: number | null;
}): boolean {
  if (!state.hasRemote) return false;
  if (!state.hasUpstream) return true;
  return (state.ahead ?? 0) > 0;
}

/**
 * CHG-FR-KQWD: the repositories of the repository set, when the state lists
 * more than one. `null` for a project with one repository, which keeps the
 * rule of CHG-FR-37 on the top-level fields.
 */
function memberStates(state: UpstreamSyncState): RepositorySyncState[] | null {
  const members = state.repositories ?? [];
  return members.length > 1 ? members : null;
}

/**
 * CHG-FR-37: whether **Push** is offered, from the branch's standing against
 * its upstream.
 *
 * A branch with a configured remote and no upstream yet is pushable, because
 * the push publishes it; a branch level with or behind its upstream is not; and
 * a repository with no remote configured is never pushable. Nothing here
 * consults the checkboxes or the panel's active mode — Push is independent of
 * both. `null` — the state has not been read, or the read failed — is not
 * pushable, so a surface never offers a transfer it cannot describe.
 */
export function canPush(state: UpstreamSyncState | null): boolean {
  if (!state) return false;
  // CHG-FR-KQWD: with more than one repository, one push reaches every
  // affected repository (GSM-FR-REIT), so any pushable repository with a
  // branch makes Push available. A detached `HEAD` never does.
  const members = memberStates(state);
  if (members) return members.some((m) => m.branch != null && standingPushable(m));
  return standingPushable(state);
}

/** GIT-FR-QMYB: the sentence every Push control states while a push runs. */
export const PUSH_RUNNING_REASON = "A push is already running.";

/** GIT-FR-QMYB / GIT-FR-CNQO: the sentence every Push control states while a pull runs. */
export const PULL_RUNNING_REASON = "A pull is running.";

/** GIT-FR-QMYB: the sentence for the transfer that runs. */
export function runningReason(operation: "push" | "pull" | null = "push"): string {
  return operation === "pull" ? PULL_RUNNING_REASON : PUSH_RUNNING_REASON;
}

/**
 * GIT-FR-QMYB: whether a Push control may start a push now — the branch is
 * publishable (CHG-FR-37) and no push is running, whichever control started it.
 */
export function canStartPush(
  state: UpstreamSyncState | null,
  running: boolean,
): boolean {
  return !running && canPush(state);
}

/**
 * Why Push is unavailable, for the control's tooltip. `null` when it is
 * available. A running push takes precedence, because it is the one reason that
 * ends without the author doing anything.
 */
export function pushUnavailableReason(
  state: UpstreamSyncState | null,
  running = false,
  operation: "push" | "pull" | null = "push",
): string | null {
  if (running) return runningReason(operation);
  if (canPush(state)) return null;
  if (!state) return "The branch's standing against its remote is unknown.";
  const members = memberStates(state);
  if (members) {
    if (members.every((m) => !m.hasRemote))
      return "No repository of this project has a remote configured.";
    return "Nothing to push — no repository on a branch holds commits its remote does not.";
  }
  if (!state.hasRemote) return "This repository has no remote configured.";
  return "Nothing to push — the branch is level with its remote.";
}
