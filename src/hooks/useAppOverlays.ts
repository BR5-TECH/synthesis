/**
 * The modal action surfaces of the main window, and the rule that binds them:
 * at most one is mounted at a time, and opening any of them closes the rest
 * (SNV-FR-56, CMW-FR-01, GHA-FR-21, NAW-FR-01).
 *
 * Held in one place because the exclusion is enforced at each *opening site*
 * rather than by listeners coordinating after the fact — so every opener has
 * to be able to reach every other overlay's dismissal. An overlay whose state
 * lived elsewhere would be the one an opener forgot.
 *
 * Each window that answers a caller carries a `settle`, mirrored in a ref: the
 * surface that asked awaits it, and a caller left unsettled is a promise
 * nothing will ever resolve.
 */
import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { useSettingsWindows } from "./useSettingsWindows";
import { usePullRequestWindow } from "./usePullRequestWindow";
import type { useShellSession } from "./useShellSession";
import type { CommitFile } from "../components/CommitMessageModal";
import type { RollbackFile } from "../components/RollbackConfirm";
import type { TabMenuState } from "../components/TabStrip";
import type { CommitOutcome, RollbackResult } from "../types";

export function useAppOverlays(s: ReturnType<typeof useShellSession>) {
  /**
   * GHA-FR-15..21: the GitHub token picker modal.
   *
   * Mounted here rather than inside the surface that asks for it, because two
   * of them do — the Git panel when an operation reports `selection_required`
   * (GHA-FR-16) and the Project settings Project section on demand (SET-FR-12)
   * — and only one picker may exist. `settle` is how the asking surface learns
   * the answer: the Git panel awaits it to decide whether to run its operation
   * or abandon it (GHA-FR-17), and Project settings uses it to re-read the
   * binding it just changed.
   */
  interface PendingPicker {
    /** GHA-FR-20: the project's current binding, preselected on open. */
    currentTokenId: string | null;
    settle: (chosen: boolean) => void;
  }
  const [picker, setPicker] = useState<PendingPicker | null>(null);
  /**
   * SWN-FR-05 / NTF-FR-08: which settings child window is open over this one,
   * and whether it holds focus. Neither is visible from here without the
   * backend saying so — a settings window is a webview of its own.
   */
  const { presenceRef: settingsWindowsRef } = useSettingsWindows();
  /** GLS-FR-27: rehearsals waiting out their delay, so none outlives the shell. */
  const rehearsalTimers = useRef<Set<number>>(new Set());
  /**
   * SNV-FR-56: whether the agents roster is mounted. Held here rather than in
   * the control for the same reason `searchOpen` is: mutual exclusion is the
   * window's rule, and a chrome dropdown opened from the keyboard raises no
   * pointer event for the roster's own dismissal handler to catch.
   */
  const [agentsRosterOpen, setAgentsRosterOpen] = useState(false);
  /**
   * TAB-FR-32 / SNV-FR-56: the tab context menu's open state.
   *
   * Held here rather than inside the strip for the same reason the roster's is:
   * the menu is one of the window's mutually-exclusive floating overlays, and a
   * menu that closed only on its own outside-pointer-down would stay mounted
   * beside a chrome dropdown opened from the keyboard.
   */
  const [tabMenu, setTabMenu] = useState<TabMenuState | null>(null);
  // Mirrored in a ref so opening and closing can settle whatever was pending
  // without reading state through a stale closure — a caller left unsettled is
  // a promise that never resolves, and the Git panel awaits this one.
  const pickerRef = useRef<PendingPicker | null>(null);

  const settlePicker = (chosen: boolean) => {
    const pending = pickerRef.current;
    pickerRef.current = null;
    setPicker(null);
    pending?.settle(chosen);
  };

  /**
   * ABT-FR-KMVD / ABT-FR-GCPK: whether the About panel is mounted. Held here,
   * above the Project picker and the main window alike, because the panel opens
   * over whichever of them is showing.
   */
  const [aboutOpen, setAboutOpen] = useState(false);

  // ABT-FR-LBNA: the panel never outlives the surface it overlays, so a project
  // opening or closing, or the project being switched, takes it down.
  useEffect(() => {
    setAboutOpen(false);
  }, [s.screen, s.projectPath]);

  /**
   * TAB-FR-32 / SNV-FR-56: take the tab context menu down.
   *
   * Every overlay opener in this shell closes its siblings at the opening site,
   * and the menu is one of them. It is given a name of its own rather than
   * being an inline `setTabMenu(null)` at each site because the menu is the one
   * overlay here that can be opened *without a pointer event* — the keyboard
   * context-menu gesture of TAB-FR-32 — so it cannot fall back on its own
   * outside-pointer-down handler the way the others can, and an opener that
   * forgets it leaves two action surfaces mounted at once.
   */
  const dismissTabMenu = () => setTabMenu(null);

  /**
   * GSD-FR-QMTF / GIT-FR-NQTZ / DRP-FR-YYZU: the graduation start dialog the
   * shell mounts for a draft whose tab is not the opener — a GitHub-shadow
   * row of the Drafts panel, and the Ready tasks section of the Git panel.
   *
   * `settle` answers the opener once: `true` when the dialog has mounted, which
   * is the moment a claim may be acknowledged (GPP-FR-BSLI), and `false` where
   * another overlay took its place before it ever opened.
   */
  interface PendingGraduationStart {
    draftId: string;
    draftName: string;
    nonce: number;
    settle: (opened: boolean) => void;
  }
  const [graduationStart, setGraduationStart] =
    useState<PendingGraduationStart | null>(null);
  const graduationStartRef = useRef<PendingGraduationStart | null>(null);
  /** A counter rather than a clock, so two openings never share a key. */
  const graduationStartSeq = useRef(0);

  /** Close the shell's start dialog. An unopened one answers `false`. */
  const closeGraduationStart = () => {
    const pending = graduationStartRef.current;
    graduationStartRef.current = null;
    setGraduationStart(null);
    pending?.settle(false);
  };

  /**
   * SNV-FR-56: the other half of the graduation overlays' mutual exclusion.
   *
   * The draft's tab mounts its own start dialog; the one the shell mounts for
   * the Drafts panel and the Git panel is taken down here.
   */
  const dismissGraduationOverlays = () => {
    if (graduationStartRef.current) closeGraduationStart();
    // ABT-FR-GCPK: every opener in this shell already ends by calling this, so
    // it is also where the About panel is taken down when another overlay opens.
    setAboutOpen(false);
  };


  // GHA-FR-21: opening the picker closes any other overlay, so the two never
  // coexist. Enforced at the opening site — the same rule the New Artifact
  // modal and the search overlay follow — rather than by listener coordination.
  const openTokenPicker = (
    currentTokenId: string | null,
    settle: (chosen: boolean) => void,
  ) => {
    s.setNewFileSeed(null);
    s.setNewFolderSeed(null);
    s.setNewTypedArtifactSeed(null);
    s.setSearchOpen(false);
    s.closeProgressOverlay();
    dismissCommitWindow();
    dismissRollbackWindow();
    dismissTabMenu();
    dismissGraduationOverlays();
    // CPR-FR-IWDK: the Create a PR window is an overlay too. The window that
    // asked for this picker is held by the hook rather than closed (CPR-FR-VZUZ).
    pullRequest.dismissPullRequestWindow();
    // Re-opening while one is outstanding settles the previous caller rather
    // than dropping it: an abandoned `settle` leaves the Git panel awaiting a
    // promise nothing will ever resolve, with no note and no operation.
    pickerRef.current?.settle(false);
    const pending = { currentTokenId, settle };
    pickerRef.current = pending;
    setPicker(pending);
  };

  // The other half of GHA-FR-21. Every overlay in this shell closes its
  // siblings when it opens, in both directions; this is what the openers that
  // predate the picker call so it is not left mounted underneath them.
  const dismissTokenPicker = () => {
    if (pickerRef.current) settlePicker(false);
  };

  /**
   * CMW-commit-message.md: the commit message window.
   *
   * Mounted here rather than inside the Changes panel because it is a modal
   * action surface of the main window (OVW-FR-06) and is mutually exclusive
   * with the other overlays (CMW-FR-01) — a rule enforced at the opening site,
   * as every other overlay in this shell enforces it. `settle` is how the panel
   * learns the answer: it awaits `true` to clear its checks and, for
   * **Commit & Push**, to run the push half (CHG-FR-41 / CHG-FR-42).
   */
  interface PendingCommit {
    files: CommitFile[];
    /** CHG-FR-48: what the panel's filters were hiding when it asked. */
    hidden: CommitFile[];
    settle: (outcome: CommitOutcome | null) => void;
  }
  const [commitWindow, setCommitWindow] = useState<PendingCommit | null>(null);
  /**
   * CMW-FR-KRVP / WSS-FR-TQBN: the same window, opened by a work stream's merge
   * confirmation to take the message the merge commit is made under.
   *
   * A second mount rather than a third field on `PendingCommit`, because the
   * two routes settle differently: this one answers with **the message alone**
   * and closes no Diff tab. It starts no merge and waits for none — the merge
   * is the selector's, which is where it is shown and stopped (WSS-FR-HGWL).
   */
  interface PendingStreamMerge {
    streamId: string;
    streamName: string;
    settle: (message: string | null) => void;
  }
  const [streamMergeWindow, setStreamMergeWindow] =
    useState<PendingStreamMerge | null>(null);
  // Mirrored in a ref for the reason the commit window's is: a caller left
  // unsettled is a promise the selector awaits forever.
  const streamMergeRef = useRef<PendingStreamMerge | null>(null);
  const requestMergeCommit = (streamId: string, streamName: string) =>
    new Promise<string | null>((settle) => {
      // CMW-FR-01 / SNV-FR-56: opening this window closes every overlay the
      // shell owns, exactly as `openCommitWindow` does. The work stream
      // dropdown is not one of them — its open state is its own — so the
      // selector closes it before it asks for this window (per
      // `WorkStreamSelector.tsx`), which is what keeps it from painting over
      // this modal's scrim.
      closeSiblingOverlays();
      // Re-opening while one is outstanding settles the previous caller rather
      // than abandoning it.
      streamMergeRef.current?.settle(null);
      const pending = { streamId, streamName, settle };
      streamMergeRef.current = pending;
      setStreamMergeWindow(pending);
    });
  const settleStreamMergeWindow = (message: string | null) => {
    const pending = streamMergeRef.current;
    streamMergeRef.current = null;
    setStreamMergeWindow(null);
    pending?.settle(message);
  };
  /**
   * GRH-FR-ODLT / GRU-FR-BLSS: the run the Runs panel should select, set when the
   * author arrived from somewhere that named one. Cleared once the panel has
   * taken it, so a later re-render does not drag the selection back.
   */
  const [selectGraduationRun, setSelectGraduationRun] = useState<{
    runId: string;
    nonce: number;
    override?: boolean;
  } | null>(null);
  /**
   * GIT-FR-FZMS: the branch the Git panel should select, set when the author
   * activated a status bar row that names a push. Cleared once the panel has
   * taken it, so a later mount does not select it again.
   */
  const [selectGitBranch, setSelectGitBranch] = useState<{
    branch: string;
    nonce: number;
  } | null>(null);
  /**
   * GRU-FR-MYFA: whether the graduation section is the active section of a
   * showing Runs panel, reported by the panel itself.
   *
   * Held in a ref as well as in state because what it is read for is the
   * **instant a graduation is started** — a callback fired from a modal, which
   * a stale closure would answer with whatever was true when it was created.
   */
  const graduationSectionActive = useRef(false);
  /**
   * NTF-FR-39: the graduation run the author is looking at, reported by the
   * bottom panel — null whenever Runs is not the visible active bottom surface
   * with its graduation section showing a run.
   */
  const [runInView, setRunInView] = useState<string | null>(null);

  /**
   * / CHG-FR-43 / CHG-FR-45: the push half of a graduation's
   * **Commit & Push**.
   *
   * It is the push the Changes panel performs and nothing else — the same
   * operation, the same transcript in the Git panel's output area, and the same
   * two token causes, one of which opens the picker and runs the push on
   * confirmation. What differs is where it is reported: the publication choice
   * has closed by the time the commit lands, so the outcome is said
   * in the shell's own transient notice rather than inline in a surface that is
   * no longer open.
   *
   * `retried` bounds the token-selection loop to one retry: a binding still
   * unresolved after the picker confirmed is a failure to report, not a reason
   * to prompt again.
   */
  const pushAfterGraduationCommit = async (retried = false): Promise<void> => {
    try {
      await api.pushCurrentBranch();
    } catch (e) {
      const raw = String(e);
      if (raw.includes("github_token_selection_required") && !retried) {
        const chosen = await new Promise<boolean>((resolve) =>
          openTokenPicker(null, resolve),
        );
        if (!chosen) {
          s.flashToast("Push cancelled — no GitHub token was selected.");
          return;
        }
        return pushAfterGraduationCommit(true);
      }
      if (raw.includes("github_token_missing")) {
        s.flashToast("Push needs a GitHub token. Add one in Global settings.");
        return;
      }
      s.flashToast(`Push failed: ${raw}`);
    }
  };

  /**
   * GIT-FR-FZMS: open the bottom panel on the Git surface with this branch
   * selected in its branches section. A nonce, so asking twice selects twice.
   */
  const openGitBranch = (branch: string) => {
    s.showBottom("git");
    setSelectGitBranch({ branch, nonce: Date.now() });
  };

  /**
   * GRU-FR-MYFA / NAW-FR-BJQX: open the bottom panel on the graduation section with
   * this run selected. The one route to a run, so every surface that names one
   * takes the author to the same place.
   */
  const openGraduationRun = (runId: string, override = true) => {
    dismissGraduationOverlays();
    s.showBottom("runs");
    // A nonce rather than a bare id, so routing to the same run twice routes
    // twice — and so the panel can tell a fresh request from the value it has
    // already taken, which is what stops the request wedging the section
    // control on graduation for the rest of the session (RUN-FR-10).
    setSelectGraduationRun({ runId, nonce: Date.now(), override });
  };

  /**
   * GRU-FR-MYFA / GRU-FR-MYFA: the route a **newly started graduation** takes.
   *
   * The panel opens on the run either way, because that is what GRU-FR-MYFA says
   * a successful start does. What is conditional is whether the new run takes
   * the author's **selection**: GRU-FR-MYFA gives that override to the explicit
   * act alone — a Graduate made while this section was the one they were
   * looking at. A Graduate from a hidden panel, or from agent output, moves
   * nothing, and the run appears on the next re-list like any other.
   */
  const openStartedGraduationRun = (runId: string) =>
    openGraduationRun(runId, graduationSectionActive.current);
  // Mirrored in a ref for the same reason the picker's is: a caller left
  // unsettled is a promise the Changes panel awaits forever.
  const commitRef = useRef<PendingCommit | null>(null);
  /**
   * CMW-FR-09: a commit is running inside the window right now, so nothing may
   * dismiss it. Held here rather than inside the modal because the openers that
   * close their sibling overlays live here — and a native menu accelerator
   * reaches them straight past the modal's scrim.
   */
  const committingRef = useRef(false);

  /** The same, for the graduation publication's own commit window. */

  const settleCommitWindow = (outcome: CommitOutcome | null) => {
    const pending = commitRef.current;
    commitRef.current = null;
    setCommitWindow(null);
    pending?.settle(outcome);
    // TAB-FR-22 / CHG-FR-41: on confirmed success alone, and from the paths the
    // commit RECORDED rather than the ones the window submitted (GTC-FR-19), so
    // a rename closes at both its locations. A dismissal or a refusal reports
    // nothing and closes nothing.
    // TAB-FR-22: never a fallback. An outcome that names no paths — including
    // one from a backend too old to report them — closes nothing at all rather
    // than closing every Diff tab.
    if (outcome) {
      void s.closeDiffTabsForCommittedPaths(outcome.committedPaths ?? []);
    }
  };

  const dismissCommitWindow = () => {
    // CMW-FR-09: dismissal is unavailable while a commit is in flight. Tearing
    // the window down here would answer the Changes panel with "nothing was
    // committed" while the commit was landing — and, for **Commit & Push**,
    // silently skip the push of a commit that really happened (CHG-FR-42).
    if (commitRef.current && !committingRef.current) settleCommitWindow(null);
  };

  /**
   * CHG-FR-59: the rollback confirmation, outstanding while the author reads it.
   *
   * Modelled exactly as the commit window is, and for the same reason: it is a
   * floating overlay of the main window and is mutually exclusive with the
   * others (SNV-FR-56), so opening it closes whatever else was open. `settle`
   * is how the Changes panel learns the answer — `false` for every dismissal,
   * which performs no operation at all.
   */
  interface PendingRollback {
    files: RollbackFile[];
    settle: (confirmed: boolean) => void;
  }
  const [rollbackWindow, setRollbackWindow] = useState<PendingRollback | null>(
    null,
  );
  // Mirrored in a ref for the same reason the commit window's is: a caller left
  // unsettled is a promise the Changes panel awaits forever.
  const rollbackRef = useRef<PendingRollback | null>(null);

  const settleRollbackWindow = (confirmed: boolean) => {
    const pending = rollbackRef.current;
    rollbackRef.current = null;
    setRollbackWindow(null);
    pending?.settle(confirmed);
  };

  const dismissRollbackWindow = () => {
    if (rollbackRef.current) settleRollbackWindow(false);
  };

  /**
   * CHG-FR-59 – CHG-FR-63: confirm a rollback and, only if the author confirms,
   * prepare and perform it.
   *
   * Nothing is quiesced and no session is touched before the confirmation is
   * answered — a dismissal must leave every check and every buffer exactly as
   * it was, which it cannot do if preparation has already cancelled writes.
   */
  const requestRollback = async (
    files: RollbackFile[],
    onConfirmed: () => void,
  ): Promise<RollbackResult | null> => {
    const confirmed = await new Promise<boolean>((resolve) => {
      s.setNewFileSeed(null);
      s.setNewFolderSeed(null);
      s.setNewTypedArtifactSeed(null);
      s.setSearchOpen(false);
      s.closeProgressOverlay();
      dismissTokenPicker();
      dismissTabMenu();
      dismissCommitWindow();
      dismissGraduationOverlays();
      pullRequest.dismissPullRequestWindow();
      rollbackRef.current?.settle(false);
      const pending = { files, settle: resolve };
      rollbackRef.current = pending;
      setRollbackWindow(pending);
    });
    if (!confirmed) return null;
    // CHG-FR-60: the panel shows its in-progress state from here — the point
    // the author committed to the operation, not the point they were asked.
    onConfirmed();
    return await s.performRollback(files.map((f) => f.path));
  };

  // CMW-FR-01 / NAW-FR-01 / SNV-FR-56: opening an overlay closes any other open
  // one rather than coexisting with it.
  const closeSiblingOverlays = () => {
    s.setNewFileSeed(null);
    s.setNewFolderSeed(null);
    s.setNewTypedArtifactSeed(null);
    s.setSearchOpen(false);
    s.closeProgressOverlay();
    dismissTokenPicker();
    dismissTabMenu();
    dismissGraduationOverlays();
    pullRequest.dismissPullRequestWindow();
  };

  /**
   * CPR-FR-FDVO: the Create a PR window. Opening it closes every other overlay
   * of the shell, and the token picker it asks for takes its place for a moment
   * (CPR-FR-VZUZ).
   */
  const pullRequest = usePullRequestWindow({
    closeOthers: () => {
      closeSiblingOverlays();
      dismissCommitWindow();
      dismissRollbackWindow();
      if (streamMergeRef.current) settleStreamMergeWindow(null);
      setAgentsRosterOpen(false);
    },
    requestToken: () =>
      new Promise<boolean>((resolve) => openTokenPicker(null, resolve)),
    projectPath: s.projectPath,
  });

  /**
   * GSD-FR-QMTF: open the start dialog for a draft. Opening it closes every
   * other overlay of the shell, as every opener here does (SNV-FR-56).
   */
  const openGraduationStart = (draftId: string, draftName: string) =>
    new Promise<boolean>((resolve) => {
      closeSiblingOverlays();
      dismissCommitWindow();
      dismissRollbackWindow();
      if (streamMergeRef.current) settleStreamMergeWindow(null);
      setAgentsRosterOpen(false);
      let answered = false;
      const pending: PendingGraduationStart = {
        draftId,
        draftName,
        nonce: ++graduationStartSeq.current,
        settle: (opened) => {
          if (answered) return;
          answered = true;
          resolve(opened);
        },
      };
      graduationStartRef.current = pending;
      setGraduationStart(pending);
    });

  /**
   * ABT-FR-KMVD / ABT-FR-GCPK: open the About panel. Opening it closes every
   * other overlay of the shell, as every opener here does (SNV-FR-56), and a
   * second activation while it is open changes nothing (ABT-FR-NVQT).
   */
  const openAbout = () => {
    closeSiblingOverlays();
    dismissCommitWindow();
    dismissRollbackWindow();
    if (streamMergeRef.current) settleStreamMergeWindow(null);
    setAgentsRosterOpen(false);
    // The chrome dropdowns and the context menus keep their open state to
    // themselves and close on a pointer press outside them. A native menu
    // activation raises no such press, so one is announced here.
    document.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    setAboutOpen(true);
  };

  const closeAbout = () => setAboutOpen(false);

  const openCommitWindow = (files: CommitFile[], hidden: CommitFile[]) =>
    new Promise<CommitOutcome | null>((resolve) => {
      closeSiblingOverlays();
      streamMergeRef.current?.settle(null);
      setStreamMergeWindow(null);
      streamMergeRef.current = null;
      commitRef.current?.settle(null);
      committingRef.current = false;
      const pending = { files, hidden, settle: resolve };
      commitRef.current = pending;
      setCommitWindow(pending);
    });

  return {
    picker,
    settlePicker,
    settingsWindowsRef,
    rehearsalTimers,
    agentsRosterOpen,
    setAgentsRosterOpen,
    tabMenu,
    setTabMenu,
    dismissTabMenu,
    dismissGraduationOverlays,
    openTokenPicker,
    dismissTokenPicker,
    commitWindow,
    streamMergeWindow,
    settleStreamMergeWindow,
    requestMergeCommit,
    selectGraduationRun,
    setSelectGraduationRun,
    selectGitBranch,
    setSelectGitBranch,
    openGitBranch,
    graduationSectionActive,
    runInView,
    setRunInView,
    pushAfterGraduationCommit,
    openGraduationRun,
    openStartedGraduationRun,
    committingRef,
    settleCommitWindow,
    dismissCommitWindow,
    rollbackWindow,
    settleRollbackWindow,
    dismissRollbackWindow,
    requestRollback,
    closeSiblingOverlays,
    openCommitWindow,
    graduationStart,
    openGraduationStart,
    closeGraduationStart,
    aboutOpen,
    openAbout,
    closeAbout,
    ...pullRequest,
  };
}
