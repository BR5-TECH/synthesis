import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import * as api from "./../api";
import { EditSessionStore } from "../state/editSessions";
import { DraftSessionStore } from "../state/draftSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { SearchSessionStore } from "../state/searchSessions";
import { SpecMapSessionStore } from "../state/specMap/session";
import { useSpecMapDraftSync } from "./shell/useSpecMapDraftSync";
import {
  followTargetForTab,
  selectionFollowsTabEnabled,
} from "../state/selectionFollowsTab";
import { useExternalChanges } from "./useExternalChanges";
import { onProjectTreeChanged } from "../events";
import { logDebug, logWarn } from "../logging";
import type {
  NewFileSeed,
  NewFolderSeed,
  NewTypedArtifactSeed,
  NotesEntity,
  PanelSurface,
  Tab,
  TabCloseScope,
  WorktreeEntry,
} from "../types";
import {
  DASHBOARD_TAB,
  conversationTabId,
} from "./shell/tabRecords";
import { useBottomPanelSurface } from "./shell/useBottomPanelSurface";
import { useDraftsRevision } from "./shell/useDraftsRevision";
import { usePanelReveal } from "./shell/usePanelReveal";
import { useActiveWorktreeTracking } from "./shell/useActiveWorktreeTracking";
import { createCreationActions } from "./shell/creationActions";
import { createDraftTabActions } from "./shell/draftActions";
import { createLifecycleActions } from "./shell/lifecycleActions";
import { createSaveActions } from "./shell/saveActions";
import { createTabClosures } from "./shell/tabClosures";
import { createOpenActions } from "./shell/openActions";
import {
  createTabStripActions,
  eligibleMassCloseTargets,
} from "./shell/tabStripActions";
import { createNavigationActions } from "./shell/navigationActions";
import { createDiscussionActions } from "./shell/discussionActions";
import {
  createDocumentActions,
  useViewerStatePruning,
} from "./shell/documentActions";

export { conversationTabId };

/**
 * Owns the entire per-project IDE shell: which project is open, the open tabs,
 * the vertical/bottom panel state, the search overlay, the
 * transient toast, and the Global-settings dirty flag. These pieces are tightly
 * coupled — `resetShellState` (run on initial open and on project switch,
 * OVW-FR-04/05/11) clears all of them together — so they live in one hook rather
 * than several. `App` becomes a thin orchestrator that wires this to the chrome.
 *
 * The groups it is built from live under `./shell/`: the contiguous state
 * blocks as custom hooks, and the action groups as plain factories over state
 * this hook still owns.
 */
export interface ShellSessionStores {
  /**
   * The Flow store to use instead of a fresh one. The only reason to pass one is
   * to seed or observe Flow state directly, so the refusal paths (TAB-FR-13,
   * FLO-FR-29) can be driven through the real store rather than a stubbed-out
   * `flush`.
   */
  flows?: FlowSessionStore;
  /** The map session to use instead of a fresh one, for a test to seed or observe. */
  specMap?: SpecMapSessionStore;
}

export function useShellSession(stores: ShellSessionStores = {}) {
  // EDT-FR-28: the edit state of every artifact touched in this application
  // session. Owned here rather than by the Editor so it outlives any one tab
  // (EDT-FR-29/EDT-FR-30) and so the shell can flush it before a teardown
  // (EDT-FR-33). Never persisted; cleared when the project closes.
  const [sessions] = useState(() => new EditSessionStore());
  // FLO-FR-26..FLO-FR-31: the graph and unsaved state of every Flow open on a
  // Flow canvas. Separate from `sessions` because a Flow's editing state belongs
  // to its tab and is dropped when that tab closes (FLO-FR-28), where an
  // artifact's outlives it.
  const [flows] = useState(() => stores.flows ?? new FlowSessionStore());
  // SCH-FR-11: the finished result set of every open Search results tab, kept
  // outside the viewport so an inactive tab's results survive being unmounted
  // and returning to it re-dispatches nothing.
  const [searches] = useState(() => new SearchSessionStore());
  // SMP-FR-QNUH: the Map tab's session, outside the viewport for the same reason.
  const [specMap] = useState(() => stores.specMap ?? new SpecMapSessionStore());
  /**
   * NAW-FR-09 / NAW-FR-11 / NAW-FR-12: the selection and unsaved buffer of every
   * draft touched this session. Owned here rather than by the New Artifact tab
   * for the same reason `sessions` is: the tab unmounts on every switch, and a
   * teardown has to be able to flush a dirty draft it cannot otherwise reach.
   */
  const [drafts] = useState(() => new DraftSessionStore());
  // EXC-FR-LKHZ / EXC-FR-UWYK: divergence detection for every artifact with a
  // session, not just the one whose tab happens to be active.
  useExternalChanges(sessions);
  // FLO-FR-31: the same channel for Flows, which answer it differently — a clean
  // Flow tab reloads its graph, a dirty one keeps its edits.
  useExternalChanges(flows);
  // Both stores live outside React, and the File menu's Save enablement is a
  // function of what they hold (SNV-FR-28 / SNV-FR-30) — so the shell re-renders
  // when either changes. Only observable transitions notify (a dirty flag
  // flipping, a save landing); keystrokes and canvas drags do not.
  useSyncExternalStore(sessions.subscribe, sessions.getVersion);
  useSyncExternalStore(flows.subscribe, flows.getVersion);
  useSyncExternalStore(drafts.subscribe, drafts.getVersion);

  const [screen, setScreen] = useState<"picker" | "ide">("picker");
  const [projectName, setProjectName] = useState("");
  // Identity anchor of the open project. The top-chrome switcher uses it to
  // flag the current entry as non-actionable (SNV-FR-20) and it keys the shell
  // subtree so a project switch remounts every project-scoped surface
  // (OVW-FR-11). Stable across a change of active worktree.
  const [projectPath, setProjectPath] = useState("");
  // WTC-FR-03 / WTS-FR-02: the worktree rooting the project, or null while its
  // content root is not inside a Git repository — which is what hides the
  // worktree selector. Also part of the shell key, so a worktree switch
  // remounts the surfaces bound to the content root (OVW-FR-12).
  const [activeWorktree, setActiveWorktree] = useState<WorktreeEntry | null>(
    null,
  );
  /**
   * OVW-FR-12 / LIB-FR-13 / CHG-FR-28 / GIT-FR-11: how many times the content
   * root has been (re)mounted for this project. It keys the shell subtree, so
   * every surface bound to the content root reloads on a switch.
   *
   * A counter rather than the worktree's path, because an **in-place checkout**
   * re-roots onto the *same directory* with different content — the path alone
   * would leave the Library, the Changes panel and the Git panel showing the
   * previous branch's state.
   */
  const [contentRootEpoch, setContentRootEpoch] = useState(0);

  const [tabs, setTabs] = useState<Tab[]>([DASHBOARD_TAB]);
  const [activeTab, setActiveTab] = useState("dashboard");
  /**
   * TAB-FR-35: which tabs are pinned, by tab id.
   *
   * Held in memory alone. No operation records it and nothing about it survives
   * a relaunch, a project switch, or the worktree change of TAB-FR-14 — this
   * state is created with the session and goes with it.
   */
  const [pinnedTabs, setPinnedTabs] = useState<ReadonlySet<string>>(
    () => new Set<string>(),
  );
  /**
   * The live tab list, readable synchronously between two awaited closes.
   *
   * `closeTab` awaits a write before it removes a tab, and a mass close
   * (TAB-FR-38) runs several of them back to back. The `tabs` binding a closure
   * captured before the first await names the strip as it stood then, so a
   * second close reading it would decide against tabs the first one already
   * removed. The ref is written from inside `setTabs` — where the next strip is
   * computed against live state — so it is current the instant a close commits
   * rather than after the render that follows it.
   */
  const tabsRef = useRef<Tab[]>(tabs);
  useEffect(() => {
    tabsRef.current = tabs;
  }, [tabs]);
  /**
   * TAB-FR-35: a tab's pinned state belongs to the tab and is dropped with it.
   *
   * Pruned against the strip rather than cleared at each close site, because a
   * tab leaves the strip by six different routes — its own close control, the
   * context menu's **Close tab**, a mass close, the path removal of TAB-FR-19,
   * the commit closure of TAB-FR-22, and the worktree change of TAB-FR-14 — and
   * a pin surviving any one of them would reappear on a tab the author reopens
   * later. One rule against the live strip covers every route, present and
   * future.
   */
  useEffect(() => {
    setPinnedTabs((current) => {
      if (current.size === 0) return current;
      const open = new Set(tabs.map((t) => t.id));
      const next = new Set([...current].filter((id) => open.has(id)));
      return next.size === current.size ? current : next;
    });
  }, [tabs]);
  const [panelSurface, setPanelSurface] = useState<PanelSurface>("library");
  // LCM-FR-05: a transient Notes scope set by the Library's "Notes" context-menu
  // action so the panel can show an artifact's notes without that artifact being
  // the active tab. Cleared when the active tab changes (NTS-FR-08) or when a
  // panel surface is chosen from the activity bar, so it never goes stale.
  const [notesEntityOverride, setNotesEntityOverride] =
    useState<NotesEntity | null>(null);


  // SNV-FR-08 / SNV-FR-46 / SNV-FR-47: the bottom panel, its surface, and the
  // per-project persistence of both.
  const {
    bottomVisible,
    bottomSurface,
    setBottomVisible,
    showBottom,
    toggleBottomSurface,
    hideBottom,
  } = useBottomPanelSurface(screen, projectPath);

  // DRP-FR-05 / DRS-FR-22: the counter every draft listing reloads on.
  const { draftsRevision, bumpDrafts } = useDraftsRevision();

  // NFW-FR-01: the New Folder modal's invocation context, or null when closed.
  // Another floating overlay, so opening it closes the others — see
  // `openNewFolder`.
  const [newFolderSeed, setNewFolderSeed] = useState<NewFolderSeed | null>(null);
  // NFI-FR-01: the New File modal's invocation context, or null when closed.
  // Another floating overlay, so opening it closes the others — see `openNewFile`.
  const [newFileSeed, setNewFileSeed] = useState<NewFileSeed | null>(null);
  /**
   * NTA-FR-01: the New Artifact modal's invocation context, or null when closed.
   *
   * The third of the creation windows and another floating overlay, so opening
   * it closes the others — see `openNewTypedArtifact`. Deliberately unrelated to
   * the New Artifact *tab*, which is a draft's workspace and no overlay at all
   * (NAW-FR-01): the two share a name in the menus and nothing else.
   */
  const [newTypedArtifactSeed, setNewTypedArtifactSeed] =
    useState<NewTypedArtifactSeed | null>(null);

  // SNV-FR-64 .. SNV-FR-68: the vertical panel's pending reveal-and-select.
  const {
    panelReveal,
    setPanelReveal,
    pendingReveal,
    requestPanelReveal,
    setRevealArtifactId,
  } = usePanelReveal();

  /**
   * CMT-FR-36: the thread an Editor tab should land on, set by the Comments
   * panel's click-through and cleared by the Editor once it has taken it.
   */
  const [focusThread, setFocusThread] = useState<{
    artifactId: string;
    threadId: string;
  } | null>(null);
  const [searchOpen, setSearchOpen] = useState(false);
  // STB-FR-12 / NAW-FR-01: the status bar's in-flight operations overlay is a
  // floating overlay of the main window, so it is mutually exclusive with every
  // other one. Like the rest, exclusion is enforced in the *opening* logic
  // rather than by listeners coordinating after the fact.
  const [progressOverlayOpen, setProgressOverlayOpen] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  // NTS-FR-08: when the active tab changes, drop any Library-initiated Notes
  // override so the panel rescopes to the active tab's entity.
  useEffect(() => {
    setNotesEntityOverride(null);
  }, [activeTab]);

  /**
   * SNV-FR-65: whether the transition that is about to render was an
   * **activation** — the author arriving at a different tab — rather than focus
   * landing somewhere because a tab was removed under them.
   *
   * The distinction cannot be recovered from `activeTab` alone: a click on a tab
   * and the Dashboard that replaces an emptied strip both look like "the active
   * tab changed". So the activation routes arm this and the removal routes do
   * not, which is why `activateTab` exists beside the raw setter rather than
   * replacing it. Every `setActiveTab` call left in this file is deliberate: the
   * close fallbacks (TAB-FR-15), the automatic closures (TAB-FR-19, TAB-FR-22),
   * and the fresh-open and worktree-change mounts (OVW-FR-04, OVW-FR-12) are all
   * named non-triggers.
   */
  const followArmed = useRef(false);

  /**
   * SNV-FR-65: activate a tab, arming the panel to follow when this is a
   * transition to a *different* tab. Re-activating the tab that is already
   * active is explicitly not a trigger, so the comparison is made inside the
   * updater — against the live value rather than this closure's, which an
   * awaited flush elsewhere may have left stale.
   */
  const activateTab = useCallback((id: string) => {
    setActiveTab((current) => {
      if (current !== id) followArmed.current = true;
      return id;
    });
  }, []);

  /**
   * SNV-FR-64 / SNV-FR-66: the panel follows the newly-activated tab to the item
   * it is a view onto.
   *
   * Keyed on `activeTab` alone and reading `tabs` from the same render: the
   * routes that open a tab set both in one handler, so React has batched them
   * into a single render and the tab is already in the array by the time this
   * runs. Depending on `tabs` as well would re-run the whole thing on every
   * unrelated strip mutation — a dirty marker, a rename — with the arm flag long
   * since spent, which is only wasted work today and a re-assert waiting to
   * happen the moment the flag is not.
   *
   * The gate is read here rather than captured, so a switch turned on mid-session
   * takes effect at the next activation and not at the next relaunch
   * (GLS-FR-28). SNV-FR-69: turning it off changes nothing that has already
   * happened — this simply stops running.
   */
  useEffect(() => {
    if (!followArmed.current) return;
    followArmed.current = false;
    if (!selectionFollowsTabEnabled()) return;
    const target = followTargetForTab(tabs.find((t) => t.id === activeTab));
    // SNV-FR-66 / SNV-FR-67: a tab with no item to name, and one whose item
    // cannot be identified, both leave the panel exactly as it is. The panel
    // itself decides the rest of SNV-FR-67 — an id it does not hold reveals
    // nothing and reports nothing.
    if (!target) return;
    /**
     * A creation reveal already standing for this very item is not something to
     * supersede. Creating a file opens its tab *and* reveals the new node, and
     * the tab open is an activation — so this effect runs last and would replace
     * the optimistic request with a strict one, against a tree that cannot hold
     * the node yet because it was written a moment ago. The lens would then
     * never relax and the author would be left looking at a panel that does not
     * show what they just made.
     *
     * Narrowed to an *optimistic* predecessor deliberately: a follow must still
     * supersede an ordinary one, or re-activating a tab after a detour would not
     * re-select it (SNV-FR-68).
     */
    const pending = pendingReveal.current;
    if (
      pending?.optimistic &&
      pending.panel === target.panel &&
      pending.id === target.id
    )
      return;

    // SNV-FR-67: a follow names something that exists now or not at all, so the
    // panel must resolve it before touching anything. And the author's focus
    // intent is the tab they just activated, not the panel — see the `focus`
    // field's own note.
    requestPanelReveal(target.panel, target.id, {
      optimistic: false,
      focus: false,
    });
    logDebug(
      ["frontend"],
      "vertical panel following active tab",
      { panel: target.panel, tabId: activeTab },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab]);

  // WTS-FR-25 / WTS-FR-32: keep the chrome control's worktree label current.
  useActiveWorktreeTracking(setActiveWorktree);

  // Cmd/Ctrl+K toggles the search overlay. Opening it closes the New Artifact and
  // New Folder modals so at most one overlay is mounted (single-overlay
  // invariant, NAW-FR-01 / NFW-FR-01).
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen((s) => {
          if (!s) {
            setNewFileSeed(null);
            setNewFolderSeed(null);
            setNewTypedArtifactSeed(null);
            setProgressOverlayOpen(false);
          }
          return !s;
        });
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // TAB-FR-04 / TAB-FR-05 / TAB-FR-06 / SCH-FR-08 / SCH-FR-09: the routes that
  // put content in front of the author.
  const {
    openArtifact,
    openDiff,
    openSearchResults,
    activateSearchHit,
    openSpecMap,
    activateLibraryFile,
  } = createOpenActions({
    tabs,
    setTabs,
    activateTab,
    sessions,
    flows,
    setPanelSurface,
    setRevealArtifactId,
    showBottom,
    specMap,
    activeTab,
  });
  /**
   * TAB-FR-37 / TAB-FR-39: the eligible set, held here so the strip's menu can
   * read the same rule the close will run by without a fresh function on every
   * render re-deciding enablement it has already decided.
   */
  const massCloseTargets = useCallback(
    (scope: TabCloseScope, targetId: string): string[] =>
      eligibleMassCloseTargets(tabs, pinnedTabs, scope, targetId),
    [tabs, pinnedTabs],
  );

  // TAB-FR-10 / TAB-FR-34 / TAB-FR-38: the closes and pins the author performs.
  const { closeTab, setTabPinned, closeTabGroup } = createTabStripActions({
    tabsRef,
    activeTab,
    pinnedTabs,
    setPinnedTabs,
    setTabs,
    setActiveTab,
    activateTab,
    sessions,
    flows,
    drafts,
    searches,
    specMap,
    massCloseTargets,
  });
  /**
   * Focus the tab showing the content whose write was blocked, so the user sees
   * the modal, confirmation, or error that stopped it (EDT-FR-32 / EDT-FR-36).
   * When nothing is showing it, a tab is opened for it — a Save All can reach an
   * artifact no tab is on, and a blocker the user cannot see is one they cannot
   * resolve.
   */
  const focusBlocker = (artifactId: string) => {
    const blocking = tabs.find((t) => t.artifactId === artifactId);
    if (blocking) {
      activateTab(blocking.id);
      return;
    }
    // The id is the artifact's project-relative path (ASC-FR-13); its last
    // segment is the file name the tab is labelled with.
    openArtifact({
      id: artifactId,
      name: artifactId.split("/").pop() || artifactId,
    });
  };

  /** Focus the New Artifact tab whose write blocked, so the error is visible. */
  const focusDraft = (draftId: string) => {
    const tab = tabs.find((t) => t.draftId === draftId);
    if (tab) activateTab(tab.id);
  };


  // SNV-FR-28 .. SNV-FR-31 / SNV-FR-43: what the active tab owns, and the File
  // and Find menu actions scoped to it.
  const activeSaveTab = tabs.find((t) => t.id === activeTab);
  const {
    saveEnabled,
    saveAllEnabled,
    findEnabled,
    requestSave,
    requestSaveAll,
    requestFind,
  } = createSaveActions({
    activeSaveTab,
    sessions,
    flows,
    drafts,
    focusBlocker,
    focusDraft,
  });

  // Push the two enabled states to the native menu (SNV-FR-28 / SNV-FR-30).
  // Greying the item is what disables its accelerator too, so this is also what
  // makes ⌘S a no-op on a Dashboard tab. Best-effort: a menu that cannot be
  // updated must not break the shell's render.
  useEffect(() => {
    void api.setSaveMenuState(saveEnabled, saveAllEnabled).catch(() => {});
  }, [saveEnabled, saveAllEnabled]);

  useEffect(() => {
    void api.setFindMenuState(findEnabled).catch(() => {});
  }, [findEnabled]);

  /**
   * SWN-FR-13 / SET-FR-01: open the Project settings child window.
   *
   * A window rather than a tab (SWN-FR-01, TAB-FR-03): nothing is added to the
   * strip, nothing is focused in it, and the never-empty invariant is untouched
   * (SWN-FR-18). Everything about which window ends up on screen — focusing the
   * one already open, closing the other one first, refusing while a save sweep
   * runs — is the backend's single decision (SWN-FR-05 through SWN-FR-07,
   * SWN-FR-12), so this side asks and does not arbitrate.
   */
  const openSettings = (section: string | null = null) => {
    void api.openSettingsWindow("project", section).catch((e) => {
      logWarn(["frontend"], "Project settings window could not be opened", {
        reason: e instanceof Error ? e.message : String(e),
      });
    });
  };

  /** SWN-FR-13 / GLS-FR-01: the same for the Global settings child window. */
  const openGlobalSettings = (section: string | null = null) => {
    void api.openSettingsWindow("global", section).catch((e) => {
      logWarn(["frontend"], "Global settings window could not be opened", {
        reason: e instanceof Error ? e.message : String(e),
      });
    });
  };

  /**
   * SNV-FR-41: the Home affordance opens the Dashboard in the leading position
   * of the strip and focuses it. The presence test lives inside the updater
   * rather than reading `tabs` from this closure: two calls landing before a
   * re-render would otherwise both see the strip as it was before the first one
   * and prepend a second Dashboard — the very duplicate SNV-FR-09 exists to
   * rule out.
   */
  const goHome = () => {
    setTabs((ts) =>
      ts.some((t) => t.id === "dashboard") ? ts : [DASHBOARD_TAB, ...ts],
    );
    activateTab("dashboard");
  };

  const openRuns = () => showBottom("runs");
  const openGit = () => showBottom("git");

  // Transient confirmation toast, auto-dismissed.
  const flashToast = (message: string) => {
    setToast(message);
    setTimeout(() => setToast(null), 3000);
  };


  // NAW-new-artifact.md / DRP-drafts-panel.md: the New Artifact tab's lifecycle.
  const {
    openDraft,
    createDraft,
    createDraftForMapNode,
    renameDraftTab,
    openDraftById,
    dropDraftTab,
  } = createDraftTabActions({
    drafts,
    tabsRef,
    setTabs,
    setActiveTab,
    activateTab,
    bumpDrafts,
    flashToast,
    specMap,
  });
  // SMD-FR-LKVU: planned chips follow their drafts through the drafts revision.
  useSpecMapDraftSync(specMap, draftsRevision);

  // TAB-FR-19 / TAB-FR-22 / TAB-FR-41 / CHG-FR-60 – CHG-FR-63: the closures the
  // shell performs because the content behind a tab moved.
  const {
    closeTabsForRemovedPaths,
    closeDiffTabsForCommittedPaths,
    performRollback,
  } = createTabClosures({ tabs, setTabs, setActiveTab, sessions, flows });

  // TAB-FR-LKCT / TAB-FR-KUIN: the viewer tabs of the Documents collection.
  const { openDocument, closeTabsForRemovedDocuments } = createDocumentActions({
    tabsRef,
    setTabs,
    setActiveTab,
    activateTab,
  });
  useViewerStatePruning(tabs);

  // TAB-FR-19: the strip follows `"project tree changed"` for the removed paths
  // it carries. Held through a ref and subscribed once, so the listener is not
  // torn down and rebuilt on every render — the handler closes over live state
  // through the setters it calls rather than through the render it was created
  // in, so a stale identity would be a leak with no upside.
  const closeRemovedRef = useRef(closeTabsForRemovedPaths);
  closeRemovedRef.current = closeTabsForRemovedPaths;
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void onProjectTreeChanged((payload) => {
      // `removedPaths` is always sent (ASC-FR-22); the fallback keeps a payload
      // from an older backend from throwing here.
      closeRemovedRef.current(payload.removedPaths ?? []);
    }).then((fn) => {
      if (cancelled) fn();
      else stop = fn;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);


  // NFW-FR-01 / NFI-FR-01 / NTA-FR-01: the three creation windows.
  const {
    openNewFolder,
    openNewFile,
    openNewTypedArtifact,
    submitNewFile,
    submitNewFolder,
    submitNewTypedArtifact,
  } = createCreationActions({
    openArtifact,
    setRevealArtifactId,
    setSearchOpen,
    setProgressOverlayOpen,
    setNewFileSeed,
    setNewFolderSeed,
    setNewTypedArtifactSeed,
  });

  /**
   * STB-FR-10 / STB-FR-12: open the in-flight operations overlay, closing every
   * other floating overlay of the main window (NAW-FR-01).
   */
  const openProgressOverlay = useCallback(() => {
    setSearchOpen(false);
    setNewFileSeed(null);
    setNewFolderSeed(null);
    setNewTypedArtifactSeed(null);
    setProgressOverlayOpen(true);
  }, []);

  /**
   * Stable identity, deliberately: the status bar hangs the overlay's
   * outside-pointer and Escape listeners off this callback, so a fresh function
   * each render would tear those listeners down and re-attach them on every
   * shell re-render — and a re-attach landing between a pointerdown and its
   * click is exactly the window in which a dismissal goes missing.
   */
  const closeProgressOverlay = useCallback(
    () => setProgressOverlayOpen(false),
    [],
  );


  /**
   * SNV-FR-26: held across a re-fired quit request, so the writes in flight
   * finish and answer once instead of a second pass racing them.
   */
  const exitInFlight = useRef(false);

  // OVW-FR-04 / OVW-FR-05 / OVW-FR-11 / OVW-FR-12 / SNV-FR-22 / SNV-FR-25 /
  // SNV-FR-26: the project and worktree lifecycle.
  const {
    flushBeforeTeardown,
    switchWorktree,
    loadProject,
    openAnother,
    requestCloseProject,
    requestExit,
  } = createLifecycleActions({
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
  });

  // NTS-FR-11 / CMP-FR-10 / CMP-FR-11 / LCM-FR-05: the panel click-throughs.
  const { revealArtifact, showNotesFor } = createNavigationActions({
    openArtifact,
    setPanelSurface,
    setRevealArtifactId,
    setNotesEntityOverride,
  });
  // CVP-FR-06 / TAB-FR-QXMV: the one route that reveals a discussion, and the
  // tab work behind it.
  const { revealDiscussion, bindConversationTab, dropNoteConversationTab } =
    createDiscussionActions({
      tabsRef,
      setTabs,
      setActiveTab,
      activateTab,
      openArtifact,
      openDraft,
      sessions,
      setFocusThread,
    });
  // EDT-FR-04 / FLO-FR-26: the tab strip's dirty indicator reads live off the
  // stores rather than off the tab record, so it appears the moment an artifact
  // or a Flow is edited and clears the moment it is written.
  const tabsView: Tab[] = tabs.map((t) => {
    if (!t.artifactId) return t;
    const dirty = t.kind === "flow"
      ? !!flows.get(t.artifactId)?.dirty
      : !!sessions.get(t.artifactId)?.dirty;
    return dirty === !!t.dirty ? t : { ...t, dirty };
  });

  const activeT = tabsView.find((t) => t.id === activeTab);
  const activeEntity =
    activeT && activeT.id.startsWith("art:") ? activeT.label : null;
  /**
   * SNV-FR-48 / HVW-FR-01: whether the History toggle is live. The version list
   * describes the active tab's artifact, so only a tab that *has* one — an
   * Editor or a Flow tab — gives it something to describe. A Diff or Search
   * tab is derived from artifacts without being bound to one, and the
   * Dashboard is bound to nothing at all.
   */
  const historyEnabled = activeT?.kind === "editor" || activeT?.kind === "flow";
  /**
   * NTS-FR-02: the entity the Notes panel's entity position binds to. An `art:`
   * tab opened from a real filesystem node carries `artifactId` — the entity id
   * a note's scope names (ASC-FR-13). A tab without one (a canned Dashboard row)
   * is not something a note can attach to, so the panel has nothing to bind to
   * and renders project-wide instead (NTS-FR-03).
   */
  const activeNotesEntity: NotesEntity | null =
    activeT?.artifactId && activeT.id.startsWith("art:")
      ? { id: activeT.artifactId, name: activeT.label }
      : null;
  // LCM-FR-05: the Notes panel scopes to the Library-chosen artifact when one is
  // pending, otherwise to the active tab's entity (NTS-FR-02 / NTS-FR-03).
  const notesEntity = notesEntityOverride ?? activeNotesEntity;

  return {
    // identity / navigation
    screen,
    projectName,
    projectPath,
    // worktree context (WTS-worktree-selector.md / OVW-FR-12)
    activeWorktree,
    contentRootEpoch,
    switchWorktree,
    // tabs
    tabs: tabsView,
    activeTab,
    activeT,
    /**
     * SNV-FR-65: the activation entry point. The tab strip, and every in-app
     * route that lands the author on a tab, goes through this so the vertical
     * panel can follow (SNV-FR-64); the raw setter is not exported, because a
     * caller reaching for it would silently opt out of that.
     */
    activateTab,
    openArtifact,
    openDiff,
    // SMP-FR-HZRA / LIB-FR-SJDC: the Map tab, and the Project panel's file
    // clicks, which reach the map while it is the active tab.
    openSpecMap,
    activateLibraryFile,
    // DPN-FR-CDFO / TAB-FR-KUIN: the viewer tabs of the Documents collection.
    openDocument,
    closeTabsForRemovedDocuments,
    // TAB-FR-22: the strip closes exactly the Diff tabs a commit has finished
    // with, from the paths the commit recorded.
    closeDiffTabsForCommittedPaths,
    // CHG-FR-60 – CHG-FR-63 / TAB-FR-41: the rollback's preparation, its call,
    // and the per-path application of what it confirmed.
    performRollback,
    // search (SCH-search.md)
    openSearchResults,
    activateSearchHit,
    closeTab,
    // TAB-FR-34 / TAB-FR-35: the pinned set the strip draws, and the two menu
    // actions that change it. Session state throughout — nothing records it.
    pinnedTabs,
    setTabPinned,
    // TAB-FR-37 / TAB-FR-38 / TAB-FR-39: the mass closes, and the eligible set
    // the menu reads to decide which of them can act.
    massCloseTargets,
    closeTabGroup,
    openSettings,
    openGlobalSettings,
    goHome,
    // vertical panel
    panelSurface,
    setPanelSurface,
    notesEntity,
    setNotesEntityOverride,
    showNotesFor,
    revealArtifact,
    revealDiscussion,
    bindConversationTab,
    dropNoteConversationTab,
    focusThread,
    clearFocusThread: () => setFocusThread(null),
    // bottom panel
    bottomVisible,
    bottomSurface,
    toggleBottomSurface,
    // NTF-FR-17: an activated notification *opens* the surface its address
    // names rather than toggling it — a bottom address arriving while that
    // surface is already showing must not hide the panel.
    showBottom,
    hideBottom,
    activeEntity,
    historyEnabled,
    openRuns,
    openGit,
    // search + toast
    // drafts / New Artifact tab (NAW-new-artifact.md / DRP-drafts-panel.md)
    openDraft,
    openDraftById,
    createDraft,
    // SMD-FR-HVBE: the map's New draft.
    createDraftForMapNode,
    renameDraftTab,
    dropDraftTab,
    draftsRevision,
    bumpDrafts,
    // new folder modal (NFW)
    newFolderSeed,
    openNewFolder,
    submitNewFolder,
    setNewFolderSeed,

    newFileSeed,
    openNewFile,
    submitNewFile,
    setNewFileSeed,

    // new (typed) artifact modal (NTA)
    newTypedArtifactSeed,
    openNewTypedArtifact,
    submitNewTypedArtifact,
    setNewTypedArtifactSeed,
    /**
     * The pending reveal-and-select for a vertical panel, from a creation or
     * from the active tab being followed (LIB-FR-18, DRP-FR-34, CHG-FR-54,
     * SNV-FR-64).
     */
    panelReveal,
    searchOpen,
    setSearchOpen,
    // in-flight operations overlay (STB-FR-10 / STB-FR-12)
    progressOverlayOpen,
    openProgressOverlay,
    closeProgressOverlay,
    toast,
    flashToast,
    // global settings dirty flag
    // artifact edit sessions (EDT-FR-28) + Flow sessions (FLO-FR-26) + the
    // draft selections and buffers of NAW-FR-09 / NAW-FR-11
    sessions,
    flows,
    drafts,
    // finished Search results tab result sets (SCH-FR-11)
    searches,
    // the Map tab's session (SMP-FR-QNUH)
    specMap,
    flushBeforeTeardown,
    // File → Save / Save All (SNV-FR-28..31)
    saveEnabled,
    saveAllEnabled,
    requestSave,
    requestSaveAll,
    requestFind,
    findEnabled,
    // lifecycle
    loadProject,
    openAnother,
    requestCloseProject,
    requestExit,
  };
}