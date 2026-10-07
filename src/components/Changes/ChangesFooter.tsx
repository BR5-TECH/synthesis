/**
 * The footer of the Changes panel: the rollback button on the leading edge, the
 * Create a PR button after it, and the commit split control on the trailing edge
 * (CHG-FR-33, CHG-FR-47, CHG-FR-56, CHG-FR-UPFP).
 */
import { pushUnavailableReason } from "../../gitSync";
import type { ChangesCommitAction, UpstreamSyncState } from "../../types";
import { CurrentBranchButton } from "../CreatePullRequest/CurrentBranchButton";
import type { PullRequestSource } from "../CreatePullRequest/types";
import { Icon } from "../icons";
import { SplitAction } from "../SplitAction";

/** The label of the primary button for each action (CHG-FR-33). */
const ACTION_LABELS: Record<ChangesCommitAction, string> = {
  commit: "Commit",
  commit_and_push: "Commit & Push",
  push: "Push",
};

const ACTIONS: ChangesCommitAction[] = ["commit", "commit_and_push", "push"];

interface ChangesFooterProps {
  /** CHG-FR-56: Uncommitted mode of a Git repository, where the rollback button renders. */
  showRollback: boolean;
  /** CHG-FR-58: why a rollback cannot start now, or null. */
  rollbackReason: string | null;
  rollingBack: boolean;
  preparing: boolean;
  onRollback: () => void;
  action: ChangesCommitAction;
  sync: UpstreamSyncState | null;
  pushRunning: boolean;
  busy: boolean;
  primaryEnabled: boolean;
  primaryReason: string | null;
  onSelectAction: (next: ChangesCommitAction) => void;
  onRunPrimary: () => void;
  /** CHG-FR-UPFP: open the Create a PR window for the current branch. */
  onCreatePullRequest?: (source: PullRequestSource) => void;
}

export function ChangesFooter({
  showRollback,
  rollbackReason,
  rollingBack,
  preparing,
  onRollback,
  action,
  sync,
  pushRunning,
  busy,
  primaryEnabled,
  primaryReason,
  onSelectAction,
  onRunPrimary,
  onCreatePullRequest,
}: ChangesFooterProps) {
  return (
    <div className="changes-actions">
      {/* CHG-FR-56: the rollback button holds the leading edge, in
          Uncommitted mode of a Git repository and nowhere else. Because the
          split control holds the trailing edge regardless, a mode that
          renders no rollback button moves nothing in the footer. */}
      {showRollback && (
        <button
          className="btn btn--sm btn--icon changes-actions__rollback"
          // CHG-FR-58: the icon is never the only indication of the action —
          // the accessible name and the tooltip both say it in words.
          aria-label="Discard selected changes"
          title={
            rollbackReason ??
            "Discard selected changes, returning these files to HEAD"
          }
          disabled={rollingBack || rollbackReason !== null}
          data-busy={preparing || undefined}
          onClick={onRollback}
        >
          <Icon.Rollback size={14} />
          {/* CHG-FR-60: the preparation waits on somebody else's in-flight
              save, which is otherwise indistinguishable from nothing
              happening. */}
          {preparing && (
            <span className="changes-actions__rollback-progress">
              Discarding…
            </span>
          )}
        </button>
      )}
      {/* CHG-FR-UPFP / CHG-FR-UCRL: Create a PR, after the rollback button on
          the leading edge. It reads the current branch's head state and
          opens the shared Create a PR window; the panel creates nothing. */}
      <CurrentBranchButton
        label="Create a PR"
        className="btn btn--sm changes-actions__create-pr"
        testId="changes-create-pr"
        // CHG-FR-UCRL: inert while a rollback is confirmed, prepared or run.
        extraReason={rollingBack ? "A rollback is in progress." : null}
        onCreatePullRequest={onCreatePullRequest}
      />
      {/* CHG-FR-33 / CHG-FR-35 / CHG-FR-49: the split control, which is
          the shared one the graduation publication choice also uses — one
          object whose two halves dim together when the selected action
          cannot be performed, whose dropdown stays operable while they do,
          and whose inactive entry carries its reason on hover.

          Commit and Commit & Push stay selectable while nothing is ticked,
          because ticking is how the author makes them available. Push does
          not: nothing done in this panel makes a branch that is level with
          its remote pushable, so it renders inactive with the reason rather
          than disappearing. */}
      <SplitAction<ChangesCommitAction>
        className="changes-actions__split"
        value={action}
        options={ACTIONS.map((value) => ({
          value,
          label: ACTION_LABELS[value],
          unavailable:
            value === "push" ? pushUnavailableReason(sync, pushRunning) : null,
        }))}
        onChange={onSelectAction}
        onActivate={onRunPrimary}
        available={primaryEnabled}
        // CHG-FR-60: inert while a rollback is being confirmed, prepared, or
        // executed. The dropdown is otherwise deliberately operable even
        // when the selected action cannot be performed (CHG-FR-49), so this
        // is the one condition that closes it — changing the commit action
        // mid-rollback would act on a set that is moving.
        primaryDisabled={!primaryEnabled || rollingBack}
        menuDisabled={rollingBack}
        primaryLabel={busy ? "Working…" : undefined}
        primaryTitle={primaryReason ?? undefined}
        menuLabel="Commit action"
      />
    </div>
  );
}
