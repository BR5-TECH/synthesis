/**
 * The persisted state of the Changes panel: the mode and the target branch,
 * per machine and project-local (CHG-FR-06, CHG-FR-07), and the selected
 * footer action, user-global (CHG-FR-34, GSS-FR-25).
 */
import { useEffect, useRef, useState } from "react";
import * as api from "../../api";
import {
  loadAppPreferences,
  patchAppPreferences,
} from "../../state/appPreferences";
import type { ChangesCommitAction, ChangesMode } from "../../types";

export function useChangesPanelState() {
  const [mode, setMode] = useState<ChangesMode>("uncommitted");
  const [targetBranch, setTargetBranch] = useState<string | null>(null);
  // The persisted state has to land before the first list call, or the panel
  // would fetch for the wrong mode and then immediately refetch.
  const [restored, setRestored] = useState(false);
  /** CHG-FR-34: the selected action, read on mount from user-global preferences. */
  const [action, setAction] = useState<ChangesCommitAction>("commit");

  // The mode + target the panel last wrote, so a value that merely arrived from
  // the backend is never written straight back. Declared here, beside the state
  // it guards, because the restore below seeds it.
  const persisted = useRef<string | null>(null);

  // CHG-FR-07: restore the persisted mode and target branch; on a project with
  // none, CHG-FR-06 seeds the target from the repository's default branch.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let restoredMode: ChangesMode = "uncommitted";
      let restoredTarget: string | undefined;
      try {
        const state = await api.loadChangesPanelState();
        if (cancelled) return;
        restoredMode = state.mode;
        restoredTarget = state.targetBranch;
        setMode(state.mode);
      } catch {
        // A state that cannot be read leaves the panel on its defaults rather
        // than blocking it; nothing here is worth an error banner.
      }
      if (!restoredTarget) {
        try {
          restoredTarget = await api.getDefaultBranch();
        } catch {
          // Not a repository, or no branches yet. The list call below surfaces
          // the real reason inline.
        }
      }
      if (cancelled) return;
      // Seed the persistence guard with what the restore produced, so the
      // restore itself writes nothing. A derived default is re-derivable from
      // `get_default_branch` on the next open (CHG-FR-06), so only a choice the
      // user actually makes is worth a write.
      persisted.current = `${restoredMode} ${restoredTarget ?? ""}`;
      if (restoredTarget) setTargetBranch(restoredTarget);
      setRestored(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // CHG-FR-07: persist mode + target per machine. Skipped until the restore has
  // landed so the initial defaults never overwrite what was stored. Best-effort:
  // a failed write must not break the panel.
  useEffect(() => {
    if (!restored) return;
    const signature = `${mode} ${targetBranch ?? ""}`;
    if (persisted.current === signature) return;
    persisted.current = signature;
    void api
      .saveChangesPanelState({
        mode,
        targetBranch: targetBranch ?? undefined,
      })
      .catch(() => {});
  }, [mode, targetBranch, restored]);

  /**
   * CHG-FR-34: the selected action is user-global, so it is one choice across
   * every project and every worktree. Read on mount; a record that cannot be
   * read leaves the panel on the **Commit** default rather than blocking it.
   */
  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (!cancelled) setAction(prefs.changesCommitAction ?? "commit");
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const selectAction = (next: ChangesCommitAction) => {
    // CHG-FR-35: an unavailable action is still selectable — the selection
    // describes the author's habit rather than what the repository can do at
    // this moment.
    setAction(next);
    // GSS-FR-20: a whole-record write that carries every other field through.
    void patchAppPreferences({ changesCommitAction: next }).catch(() => {});
  };

  return {
    mode,
    setMode,
    targetBranch,
    setTargetBranch,
    restored,
    action,
    selectAction,
  };
}
