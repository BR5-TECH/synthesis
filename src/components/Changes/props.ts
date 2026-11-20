/**
 * The props of the Changes panel (`specifications/ui/CHG-changes.md`): what
 * the shell hands it, and the overlays the panel asks the shell to open.
 */
import type { CommitFile } from "../CommitMessageModal";
import type { PullRequestSource } from "../CreatePullRequest/types";
import type { RollbackFile } from "../RollbackConfirm";
import type {
  CommitOutcome,
  DiffTarget,
  PanelRevealRequest,
  RollbackResult,
} from "../../types";

export interface ChangesProps {
  /** CHG-FR-18: open a Diff tab for a changed file under the active comparison. */
  onOpenDiff: (target: DiffTarget) => void;
  /**
   * CHG-FR-39 / CHG-FR-41: open the commit message window carrying `files` and
   * the changed files the filters are hiding (CHG-FR-48), and resolve with the
   * **outcome** of the commit made from it — the commit and the paths it
   * recorded — or `null` if it was dismissed or refused.
   *
   * The outcome rather than a bare `true` because the paths a commit recorded
   * are what close the Diff tabs it has finished with (`TAB-tabs.md`
   * TAB-FR-22), and re-deriving them from the set submitted here would miss a
   * rename's previous location.
   *
   * The window is a modal action surface of the main window and is mutually
   * exclusive with the other overlays (CMW-FR-01), so it is opened by the shell
   * rather than mounted here.
   */
  onRequestCommitMessage?: (
    files: CommitFile[],
    hidden: CommitFile[],
  ) => Promise<CommitOutcome | null>;
  /**
   * CHG-FR-59 – CHG-FR-63: confirm and perform a rollback of `files`, resolving
   * with the backend's per-path outcome, or `null` if the confirmation was
   * dismissed and nothing was done.
   *
   * Owned by the shell rather than mounted here for two reasons. The
   * confirmation is a floating overlay and is mutually exclusive with the
   * window's others (SNV-FR-56), like the commit message window; and the
   * preparation and the reset either side of the call reach the artifact editing
   * sessions and the tab strip (CHG-FR-60, CHG-FR-63), neither of which this
   * panel owns. What comes back is the typed outcome, so this panel can clear
   * exactly the checks the backend confirmed and report the rest (CHG-FR-65).
   */
  onRequestRollback?: (
    files: RollbackFile[],
    onConfirmed: () => void,
  ) => Promise<RollbackResult | null>;
  /**
   * CHG-FR-45 / GHA-FR-16: a push blocked on token selection opens the picker,
   * and resolves with whether one was chosen. The same shell-owned picker the
   * Git panel opens.
   */
  onRequestGithubToken?: () => Promise<boolean>;
  /** CHG-FR-UPFP: open the Create a PR window for the current branch. */
  onCreatePullRequest?: (source: PullRequestSource) => void;
  /** CHG-FR-45 / GHA-FR-19: no token is stored at all — route to where one is added. */
  onOpenGlobalSettings?: () => void;
  /**
   * SNV-FR-67: hand each loaded change set's paths up to the shell, the way the
   * Project panel hands up its tree.
   *
   * The shell has to know whether a Diff tab's file is in this comparison
   * *before* it activates this panel to reveal it — a panel that is not mounted
   * cannot answer, and switching to it first is the thing SNV-FR-67 forbids.
   */
  onChangeSetLoaded?: (paths: string[]) => void;
  /**
   * CHG-FR-54: a pending reveal-and-select, naming a changed file by its
   * project-relative path, or null. Reached from the active tab being followed
   * (SNV-FR-64 / SNV-FR-66) — a Diff tab naming the file half of the pair that
   * identifies it (DFV-FR-04).
   *
   * A nonce rather than a bare path, so two consecutive requests for the same
   * file both run.
   */
  reveal?: PanelRevealRequest | null;
}
