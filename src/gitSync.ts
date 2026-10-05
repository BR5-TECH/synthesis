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
import type { UpstreamSyncState } from "./types";

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
  if (!state || !state.hasRemote) return false;
  if (!state.hasUpstream) return true;
  return (state.ahead ?? 0) > 0;
}

/** GIT-FR-QMYB: the sentence every Push control states while a push runs. */
export const PUSH_RUNNING_REASON = "A push is already running.";

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
): string | null {
  if (running) return PUSH_RUNNING_REASON;
  if (canPush(state)) return null;
  if (!state) return "The branch's standing against its remote is unknown.";
  if (!state.hasRemote) return "This repository has no remote configured.";
  return "Nothing to push — the branch is level with its remote.";
}
