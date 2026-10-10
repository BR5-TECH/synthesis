import "./styles/colors_and_type.css";
import "./styles/components.css";
import "./styles/kit.css";

import { useEffect, useMemo, useRef, useState } from "react";
import { ProjectPicker } from "./components/ProjectPicker";
import { AboutPanel } from "./components/AboutPanel";
import { AppShell } from "./AppShell";
import { GitTransferProvider, useGitTransferState } from "./hooks/useGitTransfer";
import { useDraftEditingObserver } from "./hooks/useDraftEditingObserver";
import { useMainWindowState } from "./hooks/useMainWindowState";
import { useAppOverlays } from "./hooks/useAppOverlays";
import { useAppNotifications } from "./hooks/useAppNotifications";
import { NotificationToasts } from "./components/NotificationToasts";
import { useThemePreference } from "./hooks/useThemePreference";
import { useFontRoles } from "./hooks/useFontRoles";
import { useShellSession } from "./hooks/useShellSession";
import { useVerticalPanel } from "./hooks/useVerticalPanel";
import { useBottomPanel } from "./hooks/useBottomPanel";
import { useProjectFolders } from "./hooks/useProjectFolders";
import { useInFlightOperations } from "./hooks/useInFlightOperations";
import { useDiffTotals } from "./hooks/useDiffTotals";
import { useLineEndings } from "./hooks/useLineEndings";
import { useGithubPolling } from "./hooks/useGithubPolling";
import * as api from "./api";
import { useProjectArtifacts } from "./hooks/useProjectArtifacts";
import type { DraftSummary, PanelRevealRequest } from "./types";
import {
  onAgentTurnStateChanged,
  onDiscussionChanged,
  onDiscussionQuestionSetChanged,
} from "./events";
import { followAgentTurn } from "./state/discussionSession";
import { publishThread } from "./state/conversationThreads";
import { publishSet } from "./state/questionSets";
import { useDiscussionAnnouncement } from "./state/discussionAnnouncer";
import {
  publishKnownArtifacts,
  publishKnownDrafts,
} from "./state/ownerAvailability";

/**
 * SNV-FR-06: which edge the activity bar and vertical panel occupy. The side is
 * a persisted preference in the layout record, but the shell renders the left
 * edge only for now, so this names the one value the grid is built around
 * rather than reading a preference nothing can yet change. It reaches the
 * activity bar because the tooltips face inward from whichever edge the strip
 * is on (SNV-FR-49).
 */

function App() {
  const { themePref, selectTheme } = useThemePreference();
  // OVW-FR-14: the three typographic roles, applied to the application root
  // before any surface renders and re-applied the moment the Appearance section
  // changes one — so both the Project picker and the main window render in
  // them from first paint. Nothing here edits them: since SWN-FR-01 the
  // Appearance section is a child window of its own, and its writes reach this
  // window as an announcement the hook itself follows (GLS-FR-20).
  useFontRoles();
  const s = useShellSession();

  /**
   * DFI-FR-XKRM / DFI-FR-JDWS: the drafts a related surface is visible for.
   *
   * Reduced from the surfaces themselves so two surfaces of one draft hold one
   * interval: the active tab where it is a draft's, and the active conversation
   * tab where it is about a draft (per `CVP-conversation-presentation.md`).
   */
  const conversationDraftIds = useMemo(
    () =>
      s.tabs
        .filter(
          (tab) =>
            tab.id === s.activeTab &&
            tab.kind === "conversation" &&
            tab.ownerTarget?.kind === "draft",
        )
        .map((tab) => (tab.ownerTarget as { draftId: string }).draftId),
    [s.tabs, s.activeTab],
  );
  const activeDraftTab = s.tabs.find(
    (tab) => tab.id === s.activeTab && tab.kind === "draft",
  );
  useDraftEditingObserver({
    activeDraftId: activeDraftTab?.draftId,
    conversationDraftIds,
  });


  // STB-FR-15: the in-flight operation set the centre region and its overlay
  // render from. Mounted here rather than inside the status bar so it survives
  // the shell subtree's remount on a project or worktree switch.
  const operations = useInFlightOperations();

  // STB-FR-25 / STB-FR-30: the uncommitted diff summary, reloaded against the
  // new content root whenever the active worktree changes.
  const diffTotals = useDiffTotals(s.contentRootEpoch);

  // NFW-FR-04: the Parent Folder select's option list. Held here rather than in
  // the modal because the modal must open without scanning (NFW NFR): the
  // Library publishes the tree it already loaded into this, and the modal reads
  // the flattened folder set from it.
  const projectFolders = useProjectFolders(
    `${s.projectPath}::${s.contentRootEpoch}`,
  );

  // STB-FR-16..FR-19 / SET-FR-10 / SET-FR-11: one shared project-wide
  // line-ending preference with two equal editors — this status bar and the
  // Project settings window's Project section. Persisting a change marks the
  // artifact of every open Editor tab dirty (STB-FR-18 / EDT-FR-41), so the
  // pending conversion is visible and lands when the user saves rather than
  // riding invisibly on some later unrelated edit.
  const { lineEndings, selectLineEndings } = useLineEndings(
    s.projectPath,
    () => s.sessions.markOpenTabsDirty(),
  );

  // STB-FR-21 / STB-FR-22: the indentation control describes the artifact the
  // ACTIVE tab is editing — an Editor tab, or a Diff tab, whose target is that
  // same artifact's editing session (DFV-FR-42) and whose Source surface a Tab
  // keypress lands in. Every other tab type — Dashboard, Flow, Search results,
  // History detail, Global settings, Project settings — owns no artifact, so
  // there is nothing for it to describe and it renders its neutral disabled
  // state. So does a Diff tab whose comparison deletes the file, which has no
  // target to indent; that one shows as an artifact with no session behind it.
  const indentTarget =
    (s.activeT?.kind === "editor" || s.activeT?.kind === "diff") &&
    s.activeT.artifactId
      ? s.activeT.artifactId
      : null;
  const indentation = indentTarget
    ? (s.sessions.get(indentTarget)?.indentation ?? null)
    : null;

  // SNV-FR-08..13 / SNV-FR-38..40: once the main shell is mounted, restore the
  // persisted main-window dimensions/maximized state — or default to maximized
  // on first launch / unfit geometry — and apply the user-global full-screen
  // preference over it.
  //
  // Both this and `useVerticalPanel` take the project path, because both read
  // and write the *per-project* layout slot: a switch (OVW-FR-11) must re-read
  // the new project's own layout rather than leave the previous project's
  // applied — and, worse, written into the new project's slot (SNV-FR-35, SNV-FR-40, OVW-FR-11).
  useMainWindowState(s.screen === "ide", s.projectPath);

  /**
   * SNV-FR-56 / CMW-FR-01 / GHA-FR-21: the modal action surfaces of this window
   * and the mutual exclusion that binds them.
   */
  const overlays = useAppOverlays(s);

  /**
   * GIT-FR-CVSB / GIT-FR-SLRD: the GitHub polling schedule. Held by the window
   * rather than by the Git panel, which is mounted only while it shows, so the
   * launch poll and the timed polls run whatever the bottom panel shows.
   */
  const githubPolling = useGithubPolling({
    active: s.screen === "ide",
    projectKey: s.projectPath || null,
    contentRootEpoch: s.contentRootEpoch,
    draftsRevision: s.draftsRevision,
    openGraduationStart: overlays.openGraduationStart,
  });

  // SNV-FR-33..37: the vertical panel's draggable width. The ref is on the
  // `.shell` grid itself, because the panel's width is a fraction of the
  // shell's inner width and the drag needs that element's live geometry.
  const shellRef = useRef<HTMLDivElement>(null);
  const vpanel = useVerticalPanel(shellRef, s.screen === "ide", s.projectPath);
  // SNV-FR-50 – SNV-FR-53: the bottom panel's height, its drag, and the 75%
  // ceiling that keeps it from crowding the viewport out.
  const bpanel = useBottomPanel(shellRef, s.screen === "ide", s.projectPath);

  /**
   * SNV-FR-67: whether a reveal target is *known to be absent* from the panel
   * that would show it.
   *
   * Deliberately three-valued underneath: each source answers `null` for "I have
   * not loaded anything yet", and only a definite `false` refuses. Refusing on
   * "unknown" would trade a narrow SNV-FR-67 gap for a much broader SNV-FR-66
   * regression — every follow before the relevant panel had ever rendered would
   * silently do nothing, which is far more visible than the case being guarded.
   */
  const revealTargetMissing = (request: PanelRevealRequest): boolean => {
    if (request.panel === "library") return projectFolders.hasPath(request.id) === false;
    // The drafts list the shell already holds for notification routing.
    if (request.panel === "drafts")
      return (
        draftsLoaded.current && !knownDrafts.some((d) => d.id === request.id)
      );
    if (request.panel === "changes")
      return changePaths.current !== null && !changePaths.current.has(request.id);
    return false;
  };

  /**
   * SNV-FR-64, first half: the panel a reveal names becomes the vertical panel's
   * active surface, opening the panel at its persisted width if it was hidden —
   * exactly as activating that surface's own activity-bar toggle does
   * (SNV-FR-45). The second half, the reveal itself, is each panel's own.
   *
   * It lives here rather than in `useShellSession` because un-hiding the panel is
   * `useVerticalPanel`'s to do and the shell session cannot reach it. Keyed on
   * the nonce, so two reveals of the same item in the same panel both run.
   *
   * `setHidden` is called only while the panel actually *is* hidden: it persists
   * layout preferences, and re-writing `hidden: false` on every tab change would
   * be a backend round-trip per activation for no change at all.
   */
  /**
   * SNV-FR-67: the paths the Changes panel's last-loaded comparison held, so the
   * shell can tell whether a Diff tab's file is in it without mounting the
   * panel. `null` until that panel has published one — see `revealTargetMissing`
   * on why that is not the same as an empty set.
   */
  const changePaths = useRef<Set<string> | null>(null);

  const reveal = s.panelReveal;
  const revealNonce = reveal?.nonce;
  useEffect(() => {
    if (!reveal) return;
    // SNV-FR-67: resolve BEFORE moving anything. The panel itself cannot answer
    // this — it is not mounted until the surface switch, and the switch is
    // exactly what the requirement forbids for a target that does not resolve
    // ("keeps its active surface … is neither opened nor hidden"). So the
    // shell asks the panel's own published contents instead.
    if (!reveal.optimistic && revealTargetMissing(reveal)) return;
    if (vpanel.hidden) vpanel.setHidden(false);
    s.setPanelSurface(reveal.panel);
    // Keyed on the nonce alone: `reveal` is a fresh object each render and
    // `vpanel`/`s` are re-created every time, so depending on them would re-run
    // this on every render and drag the panel back off whatever the author
    // switched to (SNV-FR-68).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealNonce]);

  // The artifact set an address's `file/` target is resolved against
  // (NTF-FR-19). Already loaded for the shell, so this adds no read.
  const projectArtifacts = useProjectArtifacts();

  // CVP-FR-43: the live region every discussion route announces through.
  const conversationAnnouncement = useDiscussionAnnouncement();

  /**
   * DQA-FR-JWEF, CMS-FR-BQEN: a discussion gained a question set, or stopped
   * holding one.
   *
   * Subscribed once for the shell rather than per presentation: a discussion is
   * rendered in five of them and the author moves it between them (CVP-FR-47),
   * so a subscription per presentation would announce one set several times and
   * miss it entirely while none is mounted.
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onDiscussionQuestionSetChanged(({ discussionId, set }) => {
      publishSet(discussionId, set);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onDiscussionChanged((thread) => {
      // CVP-FR-36: every surface of the discussion redraws from the payload. What
      // counts as unread is decided by the surface that renders it, from the
      // session store (CVP-FR-37), so nothing else is marked here.
      publishThread(thread);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /**
   * CVP-FR-HWTN: the outstanding turns of every discussion follow their events
   * here, once for the shell. A surface follows them only while it is mounted,
   * so a turn that ended while its discussion was not shown would otherwise
   * stay pending in the session store.
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onAgentTurnStateChanged(followAgentTurn)
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /**
   * CVP-FR-45: an owner is unavailable when the thing the discussion is about no
   * longer resolves in the active project. Published whenever the shell's
   * artifact set moves, which is the same moment the strip learns a file has
   * gone. A tab, a Comments row, and the reveal route all read the one answer. A
   * draft is not judged here: a deleted draft takes its conversations outright
   * (CVP-FR-51).
   */
  useEffect(() => {
    if (!projectArtifacts.loaded) return;
    publishKnownArtifacts(
      projectArtifacts.artifacts.flatMap((a) => [a.id, a.path]),
    );
  }, [projectArtifacts.artifacts, projectArtifacts.loaded]);

  // The same for a `draft/` target. Reloaded on the revision the shell already
  // bumps whenever the draft set changes (DRS-FR-22).
  const [knownDrafts, setKnownDrafts] = useState<DraftSummary[]>([]);
  /**
   * SNV-FR-67: whether that list has actually been read for the current content
   * root. An empty list is only evidence a draft is absent once it is known to
   * be an *answer* rather than the initial value — `[]` is both.
   */
  const draftsLoaded = useRef(false);
  // Only a change of content root makes the previous answer meaningless. A
  // re-list on a revision bump keeps it until the new one lands: "briefly
  // unknown" would let a follow through that the last answer had ruled out.
  useEffect(() => {
    draftsLoaded.current = false;
  }, [s.projectPath, s.contentRootEpoch]);
  useEffect(() => {
    let cancelled = false;
    void api
      .listDrafts()
      .then((hierarchy) => {
        if (cancelled) return;
        setKnownDrafts(hierarchy.drafts);
        publishKnownDrafts(hierarchy.drafts);
        draftsLoaded.current = true;
      })
      .catch(() => {
        // A drafts read that fails leaves the set empty, so a draft address
        // states that it could not be reached rather than opening nothing. It is
        // deliberately NOT marked loaded: a failed read is not evidence that a
        // draft is absent, and a follow must not be refused on account of one.
        if (!cancelled) setKnownDrafts([]);
      });
    return () => {
      cancelled = true;
    };
  }, [s.projectPath, s.contentRootEpoch, s.draftsRevision]);

  // NTF-notifications.md: the facility this window drives, and the attention
  // the tab strip shows for what it raised.
  const { notifications, tabAttention, runsAttention } = useAppNotifications({
    s,
    overlays,
    vpanel,
    projectFolders,
    projectArtifacts,
    knownDrafts,
  });

  // GIT-FR-QMYB: the window's one push — running state, transcript, and the
  // branch's standing — lives above the keyed shell subtree, so the top-chrome
  // control, the Git panel, and the Changes panel share it, and a push started
  // with the Git panel closed is in its output area when it opens. A project or
  // worktree change discards it (GIT-FR-09).
  const gitTransfer = useGitTransferState({
    enabled: s.screen !== "picker",
    inRepository: s.activeWorktree !== null,
    resetKey: `${s.projectPath}::${s.contentRootEpoch}`,
  });

  if (s.screen === "picker") {
    return (
      <div className="app">
        <ProjectPicker onOpen={s.loadProject} />
        {/* PPK-FR-EWQH / ABT-FR-ZEJM: the About panel overlays and blocks the
            picker, which stays mounted beneath it and so is unchanged when the
            panel closes. */}
        {overlays.aboutOpen && <AboutPanel onClose={overlays.closeAbout} />}
        {/* NTF-FR-20: the toast of an activation made while no project is open
            shows over the picker. */}
        <NotificationToasts onActivate={notifications.activate} />
      </div>
    );
  }

  return (
    <GitTransferProvider value={gitTransfer}>
      {/* NTF-FR-HZNF / NTF-FR-21: the notification toast stack. Mounted at the
          window root, beside the shell, so it shows over the picker too. It
          takes no focus and is outside the single-overlay rule (SNV-FR-56). */}
      <NotificationToasts onActivate={notifications.activate} />
      <AppShell
        s={s}
        overlays={overlays}
        vpanel={vpanel}
        bpanel={bpanel}
        operations={operations}
        diffTotals={diffTotals}
        projectFolders={projectFolders}
        conversationAnnouncement={conversationAnnouncement}
        themePref={themePref}
        selectTheme={selectTheme}
        lineEndings={lineEndings}
        selectLineEndings={selectLineEndings}
        indentTarget={indentTarget}
        indentation={indentation}
        shellRef={shellRef}
        changePaths={changePaths}
        tabAttention={tabAttention}
        runsAttention={runsAttention}
        githubPolling={githubPolling}
      />
    </GitTransferProvider>
  );
}

export default App;

