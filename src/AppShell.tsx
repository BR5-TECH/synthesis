/**
 * Everything the main window draws once a project is open: the chrome, the tab
 * strip, the viewport, the two panels, and every modal and floating overlay
 * over the lot.
 *
 * A presentational component over `App`'s own state — it owns none of it and
 * holds no hook of its own. Split out so `App.tsx` reads as the state the
 * window is, with the markup that renders it beside it.
 *
 * The shell session and the overlay group arrive whole rather than field by
 * field, and are unpacked here under the names the markup already used: the
 * hooks that produce them are what decide those names, so a field added to one
 * of them reaches the markup without a prop being threaded through for it.
 */


import {
  TopChrome,
  ActivityBar,
  BottomPanelResizer,
  PanelResizer,
  VPanel,
} from "./components/Shell";
import { TabStrip } from "./components/TabStrip";
import { Viewport } from "./components/Viewport";
import { BottomPanel } from "./components/BottomPanel";
import { StatusBar } from "./components/StatusBar";
import { Toast } from "./components/Toast";
import { NotificationStatement } from "./components/NotificationStatement";
import { NewFileModal } from "./components/NewFileModal";
import { AboutPanel } from "./components/AboutPanel";
import { NewFolderModal } from "./components/NewFolderModal";
import { NewTypedArtifactModal } from "./components/NewTypedArtifactModal";
import { GithubTokenPicker } from "./components/GithubTokenPicker";
import { CommitMessageModal } from "./components/CommitMessageModal";
import { RollbackConfirm } from "./components/RollbackConfirm";
import { ShellGraduationStart } from "./components/ShellGraduationStart";
import type { GithubPollingController } from "./hooks/useGithubPolling";
import { useAppOverlays } from "./hooks/useAppOverlays";
import { useThemePreference } from "./hooks/useThemePreference";
import { useShellSession } from "./hooks/useShellSession";
import type { Indentation } from "./state/indentation";
import { useVerticalPanel } from "./hooks/useVerticalPanel";
import { useBottomPanel } from "./hooks/useBottomPanel";
import { useProjectFolders } from "./hooks/useProjectFolders";
import { useInFlightOperations } from "./hooks/useInFlightOperations";
import { createOperationActions } from "./hooks/shell/operationActions";
import { useDiffTotals } from "./hooks/useDiffTotals";
import { useLineEndings } from "./hooks/useLineEndings";
import { nextPanelToggleState } from "./state/panelToggle";
import {
  NEW_AGENT_ITEM,
  formatSectionAddress,
} from "./settingsWindow";
import { useNotifications } from "./hooks/useNotifications";
import type { useDiscussionAnnouncement } from "./state/discussionAnnouncer";
import type { TabAttention } from "./components/TabStrip";

/**
 * SNV-FR-06: which edge the activity bar and vertical panel occupy. The side is
 * a persisted preference in the layout record, but the shell renders the left
 * edge only for now, so this names the one value the grid is built around
 * rather than reading a preference nothing can yet change. It reaches the
 * activity bar because the tooltips face inward from whichever edge the strip
 * is on (SNV-FR-49).
 */
const PANEL_SIDE = "left" as const;

export interface AppShellProps {
  s: ReturnType<typeof useShellSession>;
  overlays: ReturnType<typeof useAppOverlays>;
  vpanel: ReturnType<typeof useVerticalPanel>;
  bpanel: ReturnType<typeof useBottomPanel>;
  notifications: ReturnType<typeof useNotifications>;
  operations: ReturnType<typeof useInFlightOperations>;
  diffTotals: ReturnType<typeof useDiffTotals>;
  projectFolders: ReturnType<typeof useProjectFolders>;
  conversationAnnouncement: ReturnType<typeof useDiscussionAnnouncement>;
  themePref: ReturnType<typeof useThemePreference>["themePref"];
  selectTheme: ReturnType<typeof useThemePreference>["selectTheme"];
  lineEndings: ReturnType<typeof useLineEndings>["lineEndings"];
  selectLineEndings: ReturnType<typeof useLineEndings>["selectLineEndings"];
  indentTarget: string | null;
  indentation: Indentation | null;
  shellRef: React.RefObject<HTMLDivElement | null>;
  changePaths: React.MutableRefObject<Set<string> | null>;
  tabAttention: ReadonlyMap<string, TabAttention>;
  runsAttention: TabAttention | undefined;
  /** GIT-FR-OGHO: the polling view and the Ready tasks actions. */
  githubPolling: GithubPollingController;
}

export function AppShell(props: AppShellProps) {
  const {
    s,
    overlays,
    vpanel,
    bpanel,
    notifications,
    operations,
    diffTotals,
    projectFolders,
    conversationAnnouncement,
    themePref,
    selectTheme,
    lineEndings,
    selectLineEndings,
    indentTarget,
    indentation,
    shellRef,
    changePaths,
    tabAttention,
    runsAttention,
    githubPolling,
  } = props;
  const {
    picker,
    settlePicker,
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
    selectGitBranch,
    setSelectGitBranch,
    openGitBranch,
    setRunInView,
    openGraduationRun,
    openStartedGraduationRun,
    committingRef,
    settleCommitWindow,
    dismissCommitWindow,
    rollbackWindow,
    settleRollbackWindow,
    dismissRollbackWindow,
    requestRollback,
    openCommitWindow,
    graduationStart,
    openGraduationStart,
    closeGraduationStart,
  } = overlays;

  // STB-FR-RWPD: a status bar row with a destination opens the surface that owns
  // its target, by that surface's own route.
  const { activateOperation } = createOperationActions({
    openGraduationRun: (runId) => openGraduationRun(runId),
    revealDiscussion: s.revealDiscussion,
    openGitBranch,
  });

  /**
   * SNV-FR-56: a floating overlay that is about to open closes every other one,
   * so at most one is mounted. The map's dialogs and the Add documents choice
   * of the Documents panel both ask for it.
   */
  const closeOtherOverlays = () => {
    s.closeProgressOverlay();
    s.setNewFileSeed(null);
    s.setNewFolderSeed(null);
    s.setNewTypedArtifactSeed(null);
    dismissTokenPicker();
    dismissCommitWindow();
    dismissRollbackWindow();
    dismissTabMenu();
    dismissGraduationOverlays();
    setAgentsRosterOpen(false);
    s.setSearchOpen(false);
  };

  return (
    <div className="app">
      <div
        // OVW-FR-11: switching the active project tears down and reopens the
        // main window. Keying the shell subtree on the project path remounts
        // every project-scoped surface (Library, Dashboard, …) so each
        // re-fetches for the newly-activated backend project.
        //
        // OVW-FR-12 / LIB-FR-13 / CHG-FR-28 / GIT-FR-11: the mount epoch is
        // part of that key too, because those same surfaces read from the
        // content root rather than from the project. A worktree switch remounts
        // them so each reloads against the new checkout — while the layout,
        // which lives outside this subtree, keeps its shape. The epoch, not the
        // worktree path: an in-place branch checkout re-roots onto the same
        // directory with different content, and a path-keyed subtree would sit
        // there showing the previous branch.
        key={`${s.projectPath}::${s.contentRootEpoch}`}
        // The ref is deliberately OUTSIDE the keyed remount's concerns: it
        // measures the shell element, which exists for as long as the IDE is
        // mounted, so the panel width survives a project or worktree switch
        // (SNV-FR-08, PSS-FR-06) even though the surfaces inside it do not.
        ref={shellRef}
        className="shell"
        style={{
          // SNV-FR-51 / SNV-FR-52: the bottom track is the persisted height
          // clamped against the live shell, so a panel sized on a tall window
          // is brought back under the 75% ceiling on a short one.
          gridTemplateRows: s.bottomVisible
            ? `44px 1fr ${bpanel.heightPx}px`
            : "44px 1fr 0",
          // SNV-FR-34 / SNV-FR-35: the panel track is the clamped resolution of
          // the persisted fraction against the live shell width, so the panel
          // keeps its proportion as the window resizes. SNV-FR-45: a hidden
          // panel collapses the track to nothing and the viewport takes its
          // space; the fraction itself is untouched, so reopening restores it.
          gridTemplateColumns: `44px ${vpanel.hidden ? 0 : vpanel.widthPx}px 1fr`,
        }}
      >
        <TopChrome
          projectName={s.projectName}
          projectPath={s.projectPath}
          onSwitchProject={s.loadProject}
          // OVW-FR-11 / EDT-FR-33: a switch writes the outgoing project's
          // pending edits before it opens the next project, and is cancelled if
          // one of those writes is blocked.
          onBeforeSwitchProject={s.flushBeforeTeardown}
          onOpenAnother={() => void s.openAnother()}
          // WTS-worktree-selector.md: the worktree selector sits immediately
          // after the project switcher (SNV-FR-32), and is absent entirely
          // while the project is not inside a Git repository (WTS-FR-02).
          activeWorktree={s.activeWorktree}
          onSwitchWorktree={s.switchWorktree}
          // WSS-FR-JVUF: the work-stream selector follows the worktree
          // selector, a stream being a checkout of the same repository.
          onRequestMergeCommit={requestMergeCommit}
          onOpenChanges={() => s.setPanelSurface("changes")}
          // WSS-FR-AWRS: the same route a `run` address takes.
          onOpenRun={(runId) => openGraduationRun(runId)}
          themePref={themePref}
          onSelectTheme={selectTheme}
          searchOpen={s.searchOpen}
          // NAW-FR-01 (single-overlay invariant): opening the search overlay —
          // by click, focus, or Tab-wrapping out of the modal — closes the New
          // Artifact and New Folder modals so they never coexist. The reverse
          // direction is handled in `openNewArtifact` / `openNewFolder`.
          // Enforced in opening logic, not via listener coordination.
          setSearchOpen={(open) => {
            if (open) {
              s.setNewFileSeed(null);
              s.setNewFolderSeed(null);
              s.setNewTypedArtifactSeed(null);
              s.closeProgressOverlay();
              dismissTokenPicker();
              dismissCommitWindow();
              dismissRollbackWindow();
              dismissTabMenu();
              dismissGraduationOverlays();
              setAgentsRosterOpen(false);
            }
            s.setSearchOpen(open);
          }}
          // STB-FR-12 / NAW-FR-01 / NFW-FR-01: the project switcher and
          // worktree selector dropdowns are floating overlays too, so opening
          // one closes every other — the creation modals included. They are
          // reachable from a mounted modal by keyboard (neither modal traps
          // focus), which is the case that would otherwise leave two overlays
          // mounted at once.
          onOverlayOpening={() => {
            s.closeProgressOverlay();
            s.setNewFileSeed(null);
            s.setNewFolderSeed(null);
            s.setNewTypedArtifactSeed(null);
            dismissTokenPicker();
            dismissCommitWindow();
            dismissRollbackWindow();
            setAgentsRosterOpen(false);
            // TAB-FR-32 / SNV-FR-56: the tab context menu is one of the
            // window's floating overlays, so a dropdown opening takes it down.
            dismissTabMenu();
          }}
          // SCH-FR-08 / SCH-FR-09: escalate the overlay to a full results tab,
          // and follow an overlay result to its surface.
          onOpenSearchResults={s.openSearchResults}
          onActivateSearchHit={s.activateSearchHit}
          // WTS-FR-35 / GHA-FR-16: the refresh control's remote leg was blocked
          // on token selection. The same picker the Git panel opens (GHA-FR-16),
          // through the same shell-owned single instance.
          onRequestGithubToken={() =>
            new Promise<boolean>((resolve) => openTokenPicker(null, resolve))
          }
          // WTS-FR-35 / GHA-FR-19: no token stored at all — nothing to choose
          // between, so the note routes to where one is added — the Global
          // settings window's GitHub section (SWN-FR-13, GIT-FR-10).
          onOpenGlobalSettings={() => s.openGlobalSettings("github")}
          // AGT-FR-06 / AGT-FR-07: the roster is how an agent is reached to be
          // looked at or changed, and how a new one is described. Both open the
          // Global settings **window** on its Agents section with an editor
          // already open (SWN-FR-13), focusing that window where it is already
          // open and closing Project settings first where that one is
          // (SWN-FR-06, SWN-FR-07). The editor travels as part of the section
          // address, because the window it opens is a window of its own and
          // shares no state with this one.
          onOpenAgentInSettings={(agentId) => {
            s.openGlobalSettings(
              formatSectionAddress("agents", agentId ?? NEW_AGENT_ITEM),
            );
          }}
          // AGT-FR-08: a project enrolling nothing names where agents are
          // enrolled, which is the Project settings window's Agents section.
          onOpenProjectSettings={() => s.openSettings("agents")}
          agentsRosterOpen={agentsRosterOpen}
          setAgentsRosterOpen={setAgentsRosterOpen}
        />

        <ActivityBar
          runsAttention={runsAttention}
          panelSurface={s.panelSurface}
          panelHidden={vpanel.hidden}
          onSelectPanelSurface={(surface) => {
            // A surface chosen from the activity bar drops any Library-initiated
            // Notes override so the panel shows the active tab's scope.
            s.setNotesEntityOverride(null);
            // SNV-FR-45: the toggle both selects and closes. Hiding keeps the
            // selected surface, so reopening returns to it.
            const next = nextPanelToggleState(
              { hidden: vpanel.hidden, surface: s.panelSurface },
              surface,
            );
            vpanel.setHidden(next.hidden);
            s.setPanelSurface(next.surface);
          }}
          bottomSurface={s.bottomSurface}
          bottomVisible={s.bottomVisible}
          onSelectBottomSurface={s.toggleBottomSurface}
          historyEnabled={s.historyEnabled}
          // SNV-FR-06: the strip sits on the vertical panel's edge, and its
          // tooltips face inward from there.
          side={PANEL_SIDE}
        />

        {/* SNV-FR-45: a hidden panel renders nothing at all — no content, no
            splitter — leaving only the activity bar on that edge. */}
        {!vpanel.hidden && (
        <div className="vpanel">
          <VPanel
            surface={s.panelSurface}
            // LIB-FR-03 / LIB-FR-SJDC: a file click opens the file, except a
            // Spec file clicked while the Map tab is active, which selects
            // its node on the map.
            onOpenArtifact={s.activateLibraryFile}
            // LIB-FR-VMAQ / SMP-FR-HZRA
            onViewSpecMap={s.openSpecMap}
            onShowNotes={s.showNotesFor}
            // NTS-FR-11: a Notes group header reveals its artifact in the
            // Library and opens it in its natural surface.
            onRevealArtifact={s.revealArtifact}
            // LCM-FR-10 / NFI-FR-07: open the New File modal with the
            // right-clicked folder as the STARTING location — editable, so the
            // folder is a starting point rather than a commitment.
            onNewFile={(folder) => {
              dismissTokenPicker();
              dismissCommitWindow();
              dismissRollbackWindow();
              dismissTabMenu();
              dismissGraduationOverlays();
              projectFolders.ensureFresh();
              s.openNewFile({ initialLocation: folder.path });
            }}
            // LCM-FR-08 / NTA-FR-08: open the New Artifact modal with the
            // right-clicked folder as the STARTING location — editable, so the
            // folder is a starting point rather than a commitment — and with
            // the artifact type left UNCHOSEN whatever type that folder carries
            // (NTA-FR-06). Confirming there creates one empty typed file; it
            // creates no draft, applies no draft template, and opens no New
            // Artifact tab (NTA-FR-14).
            onNewArtifact={(folder) => {
              dismissTokenPicker();
              dismissCommitWindow();
              dismissRollbackWindow();
              dismissTabMenu();
              dismissGraduationOverlays();
              projectFolders.ensureFresh();
              s.openNewTypedArtifact({ initialLocation: folder.path });
            }}
            // LCM-FR-09 / NFW-FR-07: open the New Folder modal with the
            // right-clicked folder as the STARTING parent — editable, and with
            // the artifact type left unset whatever type that folder carries
            // (NFW-FR-05).
            onNewFolder={(folder) => {
              dismissTokenPicker();
              dismissCommitWindow();
              dismissRollbackWindow();
              dismissTabMenu();
              dismissGraduationOverlays();
              projectFolders.ensureFresh();
              s.openNewFolder({ initialParent: folder.path });
            }}
            // NFW-FR-04: the Library is the surface that scans, so its tree is
            // what feeds the New Folder window's parent list.
            onTreeLoaded={projectFolders.publishTree}
            panelReveal={s.panelReveal}
            activeEntity={s.notesEntity}
            // CMP-FR-10 / NTS-FR-26: a Comments row and a note's Discuss reveal
            // a discussion by the one route.
            onRevealDiscussion={s.revealDiscussion}
            onNoteDeleted={s.dropNoteConversationTab}
            // SNV-FR-67: the panel publishes each comparison it loads, so the
            // shell can resolve a Diff tab's file before activating it.
            onChangeSetLoaded={(paths) => {
              changePaths.current = new Set(paths);
            }}
            // CHG-FR-18: a click in the Changes panel opens the file's diff as
            // a main-viewport tab.
            onOpenDiff={s.openDiff}
            // CHG-FR-39 / CMW-FR-03: the panel's Commit and Commit & Push
            // actions open the commit message window carrying the commit set.
            onRequestCommitMessage={openCommitWindow}
            // CHG-FR-59 – CHG-FR-63: the rollback's confirmation, preparation,
            // and the per-path application of what the backend confirmed.
            onRequestRollback={requestRollback}
            // CHG-FR-45 / GHA-FR-16: a push blocked on token selection opens
            // the same shell-owned picker the Git panel opens.
            onRequestGithubToken={() =>
              new Promise<boolean>((resolve) => openTokenPicker(null, resolve))
            }
            // CHG-FR-45 / GHA-FR-19: no token stored at all — nothing to choose
            // between, so the note routes to where one is added — the Global
            // settings window's GitHub section (SWN-FR-13, GIT-FR-10).
            onOpenGlobalSettings={() => s.openGlobalSettings("github")}
            // DRP-FR-06 / DRP-FR-09 / DRP-FR-12: the Drafts surface creates,
            // opens, and abandons drafts; the tab strip follows each.
            onOpenDraft={s.openDraft}
            // DRP-FR-26 / DRS-FR-07: the folder the draft is FILED in, passed
            // separately from the destination root its files are proposed
            // beneath at graduation. A draft created in the panel's `UI` folder
            // still graduates to the project root.
            onCreateDraft={(folder) => s.createDraft({ folder })}
            onDraftDeleted={s.dropDraftTab}
            // DRP-FR-11 / NAW-FR-04: renaming a draft from the panel renames
            // its open tab, exactly as renaming it from the tab does.
            onDraftRenamed={s.renameDraftTab}
            // DRP-FR-18 / NAW-FR-16: a draft archived or restored from the
            // panel keeps its tab open, and that tab follows the change into
            // or out of its archived marker.
            onDraftChanged={s.bumpDrafts}
            // DRP-FR-35: a row's graduation marker is a route to the run.
            onOpenRun={(runId) => openGraduationRun(runId)}
            // DRP-FR-YYZU: a GitHub-shadow row's Graduate… opens the start
            // dialog the shell mounts.
            onGraduateDraft={(draftId, draftName) =>
              void openGraduationStart(draftId, draftName)
            }
            draftsRevision={s.draftsRevision}
            // DPN-FR-CDFO: a document row opens its viewer tab, or focuses the
            // open one (TAB-FR-QXRF).
            onOpenDocument={s.openDocument}
            // DPN-FR-ZHEJ / TAB-FR-KUIN: a removal that took documents out of
            // the collection closes their viewer tabs.
            onDocumentsRemoved={s.closeTabsForRemovedDocuments}
            // DPN-FR-ZMBQ / SNV-FR-56: the Add documents choice is an overlay.
            onOverlayOpening={closeOtherOverlays}
          />
          {/* SNV-FR-33: the drag handle on the panel's inner edge. */}
          <PanelResizer
            resizable={vpanel.resizable}
            dragging={vpanel.dragging}
            onResizeStart={vpanel.onResizeStart}
          />
        </div>
        )}

        <div className="viewport">
          <TabStrip
            tabs={s.tabs}
            activeId={s.activeTab}
            // SNV-FR-65: a click or a keyboard move across the strip is the
            // plainest activation there is, and what the vertical panel follows.
            onActivate={s.activateTab}
            onClose={s.closeTab}
            onHome={s.goHome}
            // NTF-FR-26: which tabs need attention. The strip draws it; the
            // facility decides it (NTF-FR-30).
            attention={tabAttention}
            // TAB-FR-34 / TAB-FR-35: the pinned set and the two menu actions
            // that change it. Session state — nothing here records any of it.
            pinned={s.pinnedTabs}
            onSetPinned={s.setTabPinned}
            // TAB-FR-37 / TAB-FR-38 / TAB-FR-39: the mass closes, and the
            // eligible set the menu reads to decide which of them can act — the
            // same rule the close itself runs by.
            onCloseGroup={(scope, targetId) =>
              void s.closeTabGroup(scope, targetId)
            }
            massCloseTargets={s.massCloseTargets}
            // TAB-FR-32 / SNV-FR-56: the menu's state, and the closing of every
            // other floating overlay when it opens.
            menu={tabMenu}
            onMenuChange={setTabMenu}
            onOverlayOpening={() => {
              s.closeProgressOverlay();
              s.setNewFileSeed(null);
              s.setNewFolderSeed(null);
              s.setNewTypedArtifactSeed(null);
              dismissTokenPicker();
              dismissCommitWindow();
              dismissRollbackWindow();
              dismissTabMenu();
              dismissGraduationOverlays();
              setAgentsRosterOpen(false);
              s.setSearchOpen(false);
            }}
          />
          <div className="viewport__body">
            <Viewport
              activeTab={s.activeTab}
              activeT={s.activeT}
              onOpenArtifact={s.openArtifact}
              onOpenRuns={s.openRuns}
              onOpenGit={s.openGit}
              sessions={s.sessions}
              flows={s.flows}
              drafts={s.drafts}
              // CMT-FR-36: the thread a Comments row sent this tab to.
              focusThread={s.focusThread}
              onThreadFocused={s.clearFocusThread}
              // CVP-FR-60: a note's opening post binds its tab to the discussion.
              onConversationOpened={s.bindConversationTab}
              // SCH-FR-09 / SCH-FR-11: the full results page follows a result
              // by the same routing rule the overlay does, and keeps its
              // finished result set in the shell-owned store.
              onActivateSearchHit={s.activateSearchHit}
              searches={s.searches}
              // DRP-FR-05 / NAW-FR-04 / NAW-FR-24: the Drafts panel and the tab
              // strip follow what the workspace does to its draft.
              onDraftChanged={s.bumpDrafts}
              draftsRevision={s.draftsRevision}
              onDraftRenamed={s.renameDraftTab}
              // NAW-FR-28: Archive closes the tab through the shell's ordinary
              // close, which writes the pending file and keeps the draft.
              onCloseTab={(tabId) => void s.closeTab(tabId)}
              // NAW-FR-20 / GRU-FR-MYFA: graduating enqueues a run and does not
              // end the draft, so the tab stays open — read-only — and the
              // shell routes to the run in the Runs panel.
              onGraduationStarted={(runId) => openStartedGraduationRun(runId)}
              // / CHG-FR-45: the start preflight's optional push
              // blocked on token selection opens the same picker every other
              // GitHub operation does, and resolves with whether one was chosen.
              onOpenRun={(runId) => openGraduationRun(runId)}
              // DSH-FR-12 / NAW-FR-03: the Dashboard's Active workstreams rows
              // open an existing draft in a New Artifact tab, by id — so a
              // draft renamed since the widget last read it is still the one
              // that opens.
              onOpenDraft={(draftId) => void s.openDraftById(draftId)}
              // SMP-FR-QNUH / SMD-FR-HVBE: the Map tab renders from the shell's
              // map session, and its New draft runs the Drafts panel's creation.
              specMap={s.specMap}
              onNewDraftForMapNode={(nodeId) => void s.createDraftForMapNode(nodeId)}
              // SNV-FR-56: a map dialog closes every other floating overlay.
              onOverlayOpening={closeOtherOverlays}
            />
          </div>
        </div>

        {s.bottomVisible && (
          <BottomPanel
            surface={s.bottomSurface}
            activeEntity={s.activeEntity}
            // SNV-FR-50: the drag handle on the panel's top edge.
            resizer={
              <BottomPanelResizer
                dragging={bpanel.dragging}
                onResizeStart={bpanel.onResizeStart}
              />
            }
            // SNV-FR-47: the panel's hide control closes it exactly as
            // re-activating the active surface's toggle does.
            onHide={s.hideBottom}
            // GRU-FR-RZDI: the section routes to a run's source draft.
            onOpenDraft={(draftId) => void s.openDraftById(draftId)}
            // GRU-FR-QLRQ / SWN-FR-13: a run stopped at its time limit routes
            // to the Project settings section that sets the limit.
            onOpenSettings={(section) => s.openSettings(section)}
            // GLW-FR-ALZI / SNV-FR-56: the graduation log window is one of the
            // window's floating overlays, so opening it closes every other.
            onOverlayOpening={() => {
              s.closeProgressOverlay();
              s.setNewFileSeed(null);
              s.setNewFolderSeed(null);
              s.setNewTypedArtifactSeed(null);
              dismissTokenPicker();
              dismissCommitWindow();
              dismissRollbackWindow();
              setAgentsRosterOpen(false);
              dismissTabMenu();
            }}
            // GRH-FR-ODLT: the run the panel should select, where the author
            // arrived from somewhere that named one.
            selectGraduationRun={selectGraduationRun}
            // GIT-FR-FZMS: the branch the Git panel should select, where the
            // author arrived from a status bar row naming a push.
            selectGitBranch={selectGitBranch}
            onGitBranchSelected={() => setSelectGitBranch(null)}
            // NTF-FR-39: which run the author is looking at.
            onGraduationRunInView={setRunInView}
            // GIT-FR-06: a checkout from the Git panel's branches section is
            // the same operation the selector performs in place, so it drives
            // the identical switch transition.
            onSwitchWorktree={s.switchWorktree}
            // WTC-FR-21: only the repository's own checkout may move to
            // another branch.
            canCheckOutBranches={!!s.activeWorktree?.isPrimary}
            // GHA-FR-16: a GitHub operation blocked on token selection opens
            // the picker and resolves with whether one was chosen.
            onRequestGithubToken={() =>
              // Nothing is bound yet — that is why the operation was blocked —
              // so the picker opens with no preselection of its own.
              new Promise<boolean>((resolve) => openTokenPicker(null, resolve))
            }
            // GIT-FR-OGHO: the Ready tasks section renders the window's polling
            // view; GIT-FR-HNGQ routes its configuration states to the GitHub
            // polling section of Project settings.
            readyTasks={{
              polling: githubPolling,
              onOpenProjectSettings: () => s.openSettings("github-polling"),
            }}
          />
        )}
      </div>

      {/* SNV-FR-42 / STB-FR-01: the sixth zone. A sibling of `.shell` rather
          than a track inside it, so it spans the full window width below every
          other zone — and, being outside the keyed subtree, it is not torn down
          and remounted by a project or worktree switch (STB-FR-02). */}
      <StatusBar
        // STB-FR-04: each button opens its native settings child window, and
        // neither creates a tab. Wrapped rather than passed straight through:
        // the control hands its click event to the handler, and the handler's
        // first parameter is the section to open on.
        onGlobalSettings={() => s.openGlobalSettings()}
        onSettings={() => s.openSettings()}
        operations={operations}
        overlayOpen={s.progressOverlayOpen}
        onOpenOverlay={() => {
          dismissTokenPicker();
          dismissCommitWindow();
          dismissRollbackWindow();
          dismissTabMenu();
          dismissGraduationOverlays();
          s.openProgressOverlay();
        }}
        onCloseOverlay={s.closeProgressOverlay}
        onActivateOperation={activateOperation}
        lineEndings={lineEndings}
        onSelectLineEndings={selectLineEndings}
        indentation={indentation}
        onSelectIndentation={(next) => {
          // STB-FR-23 / STB-FR-24: the override belongs to the artifact's
          // retained edit state, so it survives the tab closing and reopening.
          if (indentTarget) s.sessions.setIndentation(indentTarget, next);
        }}
        diffTotals={diffTotals}
      />

      {/* ABT-about-panel.md: the About panel, mounted only while open. One of
          the floating overlays of SNV-FR-56 (ABT-FR-GCPK). */}
      {overlays.aboutOpen && <AboutPanel onClose={overlays.closeAbout} />}

      {/* NFI-new-file.md: the New File modal, mounted only while creating (a
          closed modal is unmounted so it re-centers fresh on each open,
          NFI-FR-02). */}
      {s.newFileSeed && (
        <NewFileModal
          seed={s.newFileSeed}
          folders={projectFolders.folders}
          onClose={() => s.setNewFileSeed(null)}
          onSubmit={s.submitNewFile}
        />
      )}

      {/* NFW-new-folder.md: the New Folder modal, mounted only while creating (a
          closed modal is unmounted so it re-centers fresh on each open,
          NFW-FR-02). */}
      {s.newFolderSeed && (
        <NewFolderModal
          seed={s.newFolderSeed}
          folders={projectFolders.folders}
          onClose={() => s.setNewFolderSeed(null)}
          onSubmit={s.submitNewFolder}
        />
      )}

      {/* NTA-new-typed-artifact.md: the New Artifact modal, mounted only while
          creating (a closed modal is unmounted so it re-centers fresh on each
          open, NTA-FR-02). Not the New Artifact tab (NAW-FR-01) — this window
          creates a typed project file and no draft (NTA-FR-14). */}
      {s.newTypedArtifactSeed && (
        <NewTypedArtifactModal
          seed={s.newTypedArtifactSeed}
          folders={projectFolders.folders}
          onClose={() => s.setNewTypedArtifactSeed(null)}
          onSubmit={s.submitNewTypedArtifact}
        />
      )}

      {/* CMW-commit-message.md: the commit message window, mounted only while
          a commit is being authored so it re-centers fresh on each open
          (CMW-FR-02) and starts with an empty message (CMW-FR-10). */}
      {commitWindow && (
        <CommitMessageModal
          files={commitWindow.files}
          hidden={commitWindow.hidden}
          onClose={() => settleCommitWindow(null)}
          onCommitted={(outcome) => settleCommitWindow(outcome)}
          onCommittingChange={(committing) => {
            committingRef.current = committing;
          }}
        />
      )}

      {/* CMW-FR-KRVP / CMW-FR-ZDMT: the merge's own commit message. The window
          supplies the message alone and carries no file set — the stream, its
          paths and its target branch are all the merge's (per WKS-FR-GKPX). */}
      {streamMergeWindow && (
        <CommitMessageModal
          files={[]}
          streamMerge={{
            streamId: streamMergeWindow.streamId,
            streamName: streamMergeWindow.streamName,
          }}
          onClose={() => settleStreamMergeWindow(null)}
          onCommitted={() => settleStreamMergeWindow(null)}
          onMergeMessage={(message) => settleStreamMergeWindow(message)}
        />
      )}

      {/* GSD-FR-QMTF: the start dialog for a GitHub-shadow row of the Drafts
          panel and for the Ready tasks section of the Git panel. Keyed on the
          request, so each opening mounts fresh and reports its own opening. */}
      {graduationStart && (
        <ShellGraduationStart
          key={graduationStart.nonce}
          draftId={graduationStart.draftId}
          draftName={graduationStart.draftName}
          onOpened={() => graduationStart.settle(true)}
          onClose={closeGraduationStart}
          onStarted={(run) => {
            closeGraduationStart();
            s.bumpDrafts();
            openStartedGraduationRun(run.id);
          }}
        />
      )}

      {/* CHG-FR-59: the rollback confirmation, mounted only while one is
          outstanding so it opens fresh each time and nothing it listed can go
          stale behind it. */}
      {rollbackWindow && (
        <RollbackConfirm
          files={rollbackWindow.files}
          onSettle={settleRollbackWindow}
        />
      )}

      {/* GHA-github-authentication.md: the token picker, mounted only while a
          choice is outstanding so it opens fresh each time (GHA-FR-15). */}
      {picker && (
        <GithubTokenPicker
          projectName={s.projectName}
          currentTokenId={picker.currentTokenId}
          onCancel={() => settlePicker(false)}
          onConfirm={() => settlePicker(true)}
        />
      )}

      {/* NTF-FR-20 / NTF-FR-21: what an activation says when its address
          cannot be reached. Anchored in the shell rather than in a tab, and
          outside the single-overlay rule (SNV-FR-56) — it takes no focus and
          intercepts no pointer event. */}
      <NotificationStatement
        message={notifications.statement}
        onDismiss={notifications.dismissStatement}
      />

      {/* CVP-FR-43: every reveal is announced, so a conversation is followed
          without being watched. Keyed on the sequence so a repeated message
          still reads. */}
      <div
        className="sr-only"
        role="status"
        aria-live="polite"
        data-testid="conversation-announcer"
      >
        <span key={conversationAnnouncement.seq}>
          {conversationAnnouncement.message}
        </span>
      </div>

      <Toast message={s.toast} />
    </div>
  );
}
