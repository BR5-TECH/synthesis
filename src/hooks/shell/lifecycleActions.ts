/**
 * The project and worktree lifecycle: what a teardown writes first, what a
 * reset clears, and the four transitions built on the pair (OVW-FR-04,
 * OVW-FR-05, OVW-FR-11, OVW-FR-12, SNV-FR-22, SNV-FR-25, SNV-FR-26).
 *
 * Held together because they are one sequence read four ways: flush, reset,
 * then whatever the particular transition does. A transition that skipped the
 * flush would tear down over unsaved work, and one that skipped the reset would
 * carry the outgoing content root's state into the incoming one.
 *
 * Plain closures rather than a hook: every piece of state stays owned by
 * `useShellSession`.
 */
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import * as api from "../../api";
import { flushDraftTemplate } from "../../state/draftTemplate";
import { resetPanelReveals } from "../../state/panelReveal";
import { clearAllComposers } from "../../state/discussionComposers";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { resetOwnerAvailability } from "../../state/ownerAvailability";
import { clearConversationThreads } from "../../state/conversationThreads";
import { clearQuestionSets } from "../../state/questionSets";
import { resetDraftProposals } from "../../state/draftProposals";
import { resetProposalHunks } from "../../state/proposalHunks";
import { resetDraftDiscussions } from "../../state/draftDiscussion";
import { resetPromptProposals } from "../../state/promptProposals";
import { resetSharedCommentIdentity } from "../useSharedCommentIdentity";
import { resetProjectIdentity } from "../../state/projectIdentity";
import type { EditSessionStore } from "../../state/editSessions";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { DraftSessionStore } from "../../state/draftSessions";
import type { SearchSessionStore } from "../../state/searchSessions";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { SwitchOutcome } from "../../components/WorktreeSelector";
import type {
  NewFileSeed,
  NewFolderSeed,
  NotesEntity,
  PanelRevealRequest,
  PanelSurface,
  ProjectHandle,
  Tab,
  WorktreeContext,
  WorktreeEntry,
} from "../../types";
import { DASHBOARD_TAB } from "./tabRecords";

export interface LifecycleDeps {
  sessions: EditSessionStore;
  flows: FlowSessionStore;
  drafts: DraftSessionStore;
  searches: SearchSessionStore;
  specMap: SpecMapSessionStore;
  focusBlocker: (artifactId: string) => void;
  focusDraft: (draftId: string) => void;
  flashToast: (message: string) => void;
  setScreen: (screen: "picker" | "ide") => void;
  setProjectName: (name: string) => void;
  setProjectPath: (path: string) => void;
  setActiveWorktree: Dispatch<SetStateAction<WorktreeEntry | null>>;
  setContentRootEpoch: Dispatch<SetStateAction<number>>;
  setTabs: (tabs: Tab[]) => void;
  setActiveTab: (id: string) => void;
  setPinnedTabs: (pinned: ReadonlySet<string>) => void;
  setPanelSurface: (surface: PanelSurface) => void;
  setNotesEntityOverride: (entity: NotesEntity | null) => void;
  setNewFileSeed: (seed: NewFileSeed | null) => void;
  setNewFolderSeed: (seed: NewFolderSeed | null) => void;
  setSearchOpen: (open: boolean) => void;
  setProgressOverlayOpen: (open: boolean) => void;
  setBottomVisible: (visible: boolean) => void;
  setPanelReveal: (request: PanelRevealRequest | null) => void;
  setRevealArtifactId: (id: string | null) => void;
  pendingReveal: MutableRefObject<PanelRevealRequest | null>;
  exitInFlight: MutableRefObject<boolean>;
}

export interface LifecycleActions {
  flushBeforeTeardown: () => Promise<boolean>;
  resetViewport: () => void;
  resetShellState: () => void;
  switchWorktree: (
    operation: () => Promise<WorktreeContext>,
  ) => Promise<SwitchOutcome>;
  loadProject: (handle: ProjectHandle) => void;
  openAnother: () => Promise<void>;
  requestCloseProject: () => Promise<void>;
  requestExit: () => Promise<void>;
}

export function createLifecycleActions(deps: LifecycleDeps): LifecycleActions {
  const {
    sessions,
    flows,
    drafts,
    searches,
    specMap,
    focusBlocker,
    focusDraft,
    flashToast,
    setScreen,
    setProjectName,
    setProjectPath,
    setActiveWorktree,
    setContentRootEpoch,
    setTabs,
    setActiveTab,
    setPinnedTabs,
    setPanelSurface,
    setNotesEntityOverride,
    setNewFileSeed,
    setNewFolderSeed,
    setSearchOpen,
    setProgressOverlayOpen,
    setBottomVisible,
    setPanelReveal,
    setRevealArtifactId,
    pendingReveal,
    exitInFlight,
  } = deps;

  /**
   * EDT-FR-33 / FLO-FR-29: write every artifact and Flow holding unsaved
   * changes before a teardown (project close, project switch, worktree change,
   * application exit). A blocked write cancels the teardown and focuses the tab holding the
   * blocker, so nothing is torn down over an unresolved divergence, empty-save
   * confirmation, or error. Returns whether the caller may proceed.
   */
  const flushBeforeTeardown = async (): Promise<boolean> => {
    const artifacts = await sessions.flushAll();
    if (!artifacts.ok) {
      if (artifacts.artifactId) focusBlocker(artifacts.artifactId);
      return false;
    }
    const graphs = await flows.flushAll();
    if (!graphs.ok) {
      if (graphs.artifactId) focusBlocker(graphs.artifactId);
      return false;
    }
    // NAW-FR-27 / NAW-FR-28: a dirty draft is written here too, and awaited —
    // an unawaited save races the teardown that follows and can land against
    // the wrong content root, or after the project has been closed under it.
    const pending = await drafts.flushAll();
    if (!pending.ok) {
      if (pending.artifactId) focusDraft(pending.artifactId);
      return false;
    }
    // SET-FR-17: the Project settings window's Draft template section writes
    // itself, and this teardown closes the tab that holds it. Awaited for the
    // reason the draft flush above is — an unawaited write races what follows
    // and can land against the wrong content root — but never a veto: the
    // section may not be on screen, and a template that could not be written is
    // reported where the author can act on it (SET-FR-18) rather than by
    // refusing to close the project.
    await flushDraftTemplate();
    return true;
  };

  /**
   * Return the viewport to its fresh-open state (OVW-FR-04 / OVW-FR-05):
   * Dashboard the focused and only tab, Library the vertical-panel surface,
   * transient surfaces cleared, and no editing session outliving the tabs that
   * held it.
   *
   * Shared by a project switch (OVW-FR-11) and a worktree switch (OVW-FR-12).
   * The difference is everything it deliberately leaves alone: project
   * identity, and the shell's layout — a worktree switch does not tear the main
   * window down, so the panel geometry keeps its shape.
   *
   * EDT-FR-28 / TAB-FR-14 / FLO-FR-29: retained edit state is discarded here
   * rather than carried across. An artifact's session is keyed by its
   * project-relative path, which names a *different file* in a different
   * content root — so carrying it over would resurface one checkout's undo
   * history on another's bytes. Callers flush first (`flushBeforeTeardown`), so
   * nothing dropped here was unsaved.
   */
  const resetViewport = () => {
    setTabs([DASHBOARD_TAB]);
    /**
     * TAB-FR-35: nothing about a pinned tab survives a project switch or the
     * worktree change of TAB-FR-14.
     *
     * Cleared outright rather than left to the prune-by-id effect, which keeps
     * a pin whose tab id is still in the strip — and the Dashboard's is, since
     * the teardown puts a fresh one back in the same position under the same
     * id. A pinned Dashboard would otherwise be the one tab whose pin outlived
     * the switch that was supposed to take every tab with it.
     */
    setPinnedTabs(new Set<string>());
    /**
     * SNV-FR-67: a request minted against the outgoing project or worktree names
     * a content root no panel is reading any more. Dropped here rather than left
     * standing, or the incoming Library would mount, find the previous root's
     * path in the prop, and expand phantom ancestors against a tree that has
     * never held it.
     */
    pendingReveal.current = null;
    setPanelReveal(null);
    resetPanelReveals();
    // SNV-FR-65: the tab the viewport *mounts* on when a project opens, the
    // active project is switched, or the active worktree changes is not a
    // transition from one active tab to another (OVW-FR-04, OVW-FR-11,
    // OVW-FR-12), so it arms nothing.
    setActiveTab("dashboard");
    setPanelSurface("library");
    setNotesEntityOverride(null);
    setNewFileSeed(null);
    setNewFolderSeed(null);
    setRevealArtifactId(null);
    setSearchOpen(false);
    setProgressOverlayOpen(false);
    sessions.clear();
    flows.clear();
    drafts.clear();
    // ACT-FR-14: a half-written Discuss message names an item of the outgoing
    // content root, so it goes with the rest of the session state rather than
    // reappearing over a file of the same path in the incoming one.
    clearAllComposers();
    /**
     * CVP-FR-49 / CVP-FR-50: every discussion surface goes, exactly as
     * application exit takes them. The session stores drop the unsent composer
     * text, the pending attachments, the scroll positions, the unread state, and
     * the question answers, and the focus registry drops every surface and held
     * request. The tabs are closed with the strip. Nothing is retained as an
     * unavailable-owner record, and no conversation is written to: both calls
     * are in-memory clears that invoke no operation of the storage module.
     */
    clearAllDiscussionSessions();
    resetDiscussionFocus();
    resetOwnerAvailability();
    clearConversationThreads();
    // CVP-FR-49, DQA-FR-ZUPB: the sets on screen and the answers entered against
    // them name discussions of the outgoing content root, so both go with it.
    clearQuestionSets();
    resetSharedCommentIdentity();
    resetProjectIdentity();
    /**
     * DCR-FR-33: the proposals every surface reads a draft's pending change
     * from, and the readings they were taken by, name drafts of the outgoing
     * content root and do not survive it (DCP-FR-02). Dropped here so the
     * incoming tree's first surface takes a reading of its own rather than
     * rendering an indication, an arrival, or a rail control from the tree the
     * window has left — and so the review standing over one goes with it
     * (DCR-FR-22).
     */
    resetDraftProposals();
    // DCR-FR-33: the changes those proposals hold, and where the author had got
    // to in each draft's two columns, belong to the same content root and go
    // with it. Left standing, the incoming tree's first review would draw the
    // outgoing tree's changes over a document that never held them.
    resetProposalHunks();
    resetDraftDiscussions();
    // PCR-FR-28: the readings of PCR-FR-27 belong to the content root they were
    // taken from, so they go with the project and with the worktree — together
    // with the review standing over one (PCR-FR-25).
    resetPromptProposals();
    // TAB-FR-14: a result set names paths in the outgoing content root, so it
    // does not survive into the new one either.
    searches.clear();
    // SMP-FR-MEZK: the map session belongs to the outgoing project and content
    // root — its index, its edits, and its draft placements go with them.
    specMap.clear();
  };

  // Reset the per-project shell to its defaults (OVW-FR-04 / OVW-FR-05):
  // Dashboard the focused tab, Library the vertical-panel surface, panels and
  // transient surfaces cleared. Shared by initial open and project switch so
  // neither carries over the previous project's tabs or panel state.
  const resetShellState = () => {
    setProjectName("");
    setProjectPath("");
    setActiveWorktree(null);
    setContentRootEpoch(0);
    resetViewport();
    setBottomVisible(false);
  };

  /**
   * OVW-FR-12 / WTS-FR-22 / GIT-FR-06: the worktree-switch transition — the one
   * implementation both the worktree selector and the Git panel's branches
   * section route through, which is what makes them alternative routes to one
   * behaviour rather than two behaviours.
   *
   * The order is fixed. Pending changes are written first, and a write that
   * cannot proceed safely cancels the switch **before anything else happens**:
   * no operation is invoked, no tab closes, the active worktree does not change
   * (WTS-FR-23, TAB-FR-14, per EDT-FR-32). Then every tab closes and all
   * retained editing state is discarded. Only then is the operation invoked.
   *
   * A failed operation leaves the window in that same fresh state, still on the
   * previous active worktree, and hands the typed error back for the calling
   * surface to render inline (WTS-FR-24).
   */
  const switchWorktree = async (
    operation: () => Promise<WorktreeContext>,
  ): Promise<SwitchOutcome> => {
    if (!(await flushBeforeTeardown())) {
      return { ok: false, cancelled: true };
    }
    resetViewport();
    try {
      const context = await operation();
      const next =
        context.worktrees.find((w) => w.isActive) ??
        (await api.getActiveWorktree());
      setActiveWorktree(next ?? null);
      setContentRootEpoch((n) => n + 1);
      return { ok: true };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  };

  // Mount the main window for a project at its defaults. Used both for the
  // initial open from the picker and for an in-window switch via the top-chrome
  // switcher (OVW-FR-11), which tears the current window down and reopens it for
  // the newly-opened handle, discarding the previous project's state.
  const loadProject = (handle: ProjectHandle) => {
    // Clear the previous project's identity and shell state first, then apply
    // the new project's identity, so the new name/path win the batched update.
    resetShellState();
    setProjectName(handle.name);
    setProjectPath(handle.path);
    setScreen("ide");
    // WTS-FR-02 / WTS-FR-03: label the chrome control from the worktree the
    // open resolved to. A project outside a Git repository rejects with the
    // typed "not a git repository" error, which leaves this null and renders no
    // selector at all.
    void api
      .getActiveWorktree()
      .then((entry) => setActiveWorktree(entry ?? null))
      .catch(() => setActiveWorktree(null));
    // WTC-FR-17: the project could not resume where it left off, because that
    // worktree's directory is gone. Said out loud — otherwise the user is
    // silently rerouted to the primary worktree with no indication that the one
    // they were working in has disappeared.
    if (handle.rememberedWorktreeUnavailable) {
      flashToast(
        "That worktree is no longer on disk — opened the main one instead",
      );
    }
  };

  // SNV-FR-22 / OVW-FR-11 / OVW-FR-02: leave the main window for the Project
  // picker so a project outside the recent list can be opened. Pending Editor
  // changes are written first, and a blocked write cancels the departure
  // (EDT-FR-33).
  const openAnother = async () => {
    if (!(await flushBeforeTeardown())) return;
    resetShellState();
    setScreen("picker");
  };

  /**
   * SNV-FR-25: File → Close Project. Pending Editor changes are written before
   * anything is torn down (EDT-FR-33), then the backend drops the project's
   * in-memory state and watcher (PST-FR-14 / ASC-FR-14) and the window returns
   * to the Project picker. A blocked write cancels the close outright: the
   * project stays open with the blocking tab focused (EDT-FR-32).
   */
  const requestCloseProject = async () => {
    if (!(await flushBeforeTeardown())) return;
    try {
      await api.closeProject();
    } catch {
      // Best-effort: a teardown failure must not strand the user in a project
      // they asked to leave — the frontend state is reset either way.
    }
    resetShellState();
    setScreen("picker");
  };

  /**
   * SNV-FR-26: the application is quitting and the backend is holding the exit.
   * Pending Editor changes are written first (EDT-FR-33); a blocked write
   * cancels the quit and leaves the application running with the blocking tab
   * focused (EDT-FR-32).
   *
   * A user who asks to quit again because nothing visibly happened re-fires this
   * request while the first one is still writing. Ignoring the repeat is what
   * makes that safe: the hold the backend is already holding stands either way,
   * so the writes in flight finish and answer once, instead of a second pass
   * racing them and quitting the application out from under them.
   */
  const requestExit = async () => {
    if (exitInFlight.current) return;
    exitInFlight.current = true;
    try {
      const ok = await flushBeforeTeardown();
      // Answer either way: `false` releases the hold so the *next* quit is held
      // and flushed too, instead of slipping through unflushed.
      await api.finishExit(ok);
    } finally {
      exitInFlight.current = false;
    }
  };

  return {
    flushBeforeTeardown,
    resetViewport,
    resetShellState,
    switchWorktree,
    loadProject,
    openAnother,
    requestCloseProject,
    requestExit,
  };
}
