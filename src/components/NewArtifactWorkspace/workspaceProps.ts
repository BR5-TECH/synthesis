/**
 * The props of the New Artifact tab (`NAW-new-artifact.md`). Split out of
 * `index.tsx` to keep that file within the size rule.
 */
import type { DraftSessionStore } from "../../state/draftSessions";

export interface WorkspaceProps {
  draftId: string;
  /**
   * NAW-FR-08 / NAW-FR-12: the shell-owned store this draft's rail state and
   * edit session outlive the component in. Held there rather than here because
   * the workspace unmounts on every tab switch, and because the shell must be
   * able to flush a dirty prompt before a teardown (NAW-FR-23 / NAW-FR-24).
   */
  drafts: DraftSessionStore;
  /**
   * NAW-FR-04: the draft's name, as the shell holds it. Rendered rather than
   * the copy inside the record this component loaded on mount, because a rename
   * can happen from the Drafts panel while this tab is open — the shell is the
   * one place that hears about every rename, so it is the one place the name
   * can be read from without going stale.
   */
  name: string;
  /** Bumped so the Drafts panel reflects a rename, a status change, or a graduation. */
  onDraftChanged: () => void;
  /**
   * NAW-FR-25: the shell's count of everything that has moved a draft, this tab
   * included. A rename made in the Drafts panel renames this draft's prompt, so
   * a tab told only about the *name* would keep showing the old path in the
   * action row and keep writing the author's typing to it.
   */
  draftsRevision: number;
  /**
   * NAW-FR-20: graduating does **not** end the draft. The run is enqueued, the
   * prompt is locked while it is in flight, and the tab stays open — read-only
   * — so the author can read the prompt the run is answering. This reports the
   * run so the shell can route to it (GRU-FR-MYFA).
   */
  onGraduationStarted: (runId: string) => void;
  /**
   * / CHG-FR-45: the graduation start preflight's optional push
   * reached a project with no GitHub token bound. Opens the shell's token
   * picker and resolves with whether one was chosen — the picker being an
   * overlay of the window rather than of this tab.
   */
  /** NAW-FR-BJQX: the route a locked draft's tab offers to the run holding it. */
  onOpenRun: (runId: string) => void;
  /**
   * NAW-FR-28: **Archive** closes the tab, since a draft being retired is one
   * the author is finished looking at. Through the shell's ordinary tab close
   * rather than the graduation path: the draft still exists, so its retained
   * edit state stays where it is (NAW-FR-16, NAW-FR-23).
   */
  onArchived: () => void;
  /** NAW-FR-04: the strip's label follows the draft's name. */
  onNameChanged: (name: string) => void;
}
