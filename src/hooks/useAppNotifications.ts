import { useCallback, useEffect, useMemo, useRef } from "react";
import type {
  DraftChangeProposal,
  DraftSummary,
  PromptChangeProposal,
} from "../types";
import type { useAppOverlays } from "./useAppOverlays";

/**
 * The notification facility as the main window drives it
 * (`../../specifications/ui/NTF-notifications.md`), together with the tab
 * attention it produces.
 *
 * Held in one place because a raise and what the strip shows for it are the
 * same fact read twice: an address names a tab, the window suppresses a raise
 * for whatever the author is already looking at (NTF-FR-08), and what survives
 * that suppression is what marks the tab. Splitting them would let the strip
 * mark a tab for a raise the window decided not to make.
 */


import { REHEARSAL_DELAY_MS } from "../components/NotificationSettings";
import { useShellSession } from "./useShellSession";
import { useVerticalPanel } from "./useVerticalPanel";
import { useMenuEvents } from "./useMenuEvents";
import { useProjectFolders } from "./useProjectFolders";
import * as api from "../api";
import {
  loadAppPreferences,
  refreshAppPreferences,
} from "../state/appPreferences";
import { useProjectArtifacts } from "./useProjectArtifacts";
import { useNotifications } from "./useNotifications";
import {
  configureNotifications,
  notifyArrived,
  raiseNotification,
  retractNotification,
  readNotificationPermission,
  setNotificationsEnabled,
  withdrawForRoot,
} from "../state/notifications";
import { setSelectionFollowsTab } from "../state/selectionFollowsTab";
import {
  onAppPreferencesChanged,
  onDraftChangeProposalsChanged,
  onGithubPollingChanged,
  onGraduationRunChanged,
  onPromptChangeProposalsChanged,
  onSettingsRehearsalRequested,
  onSettingsWindowChanged,
} from "../events";
import {
  raiseKey,
  raiseStatement,
  raisesForRun,
  raiseTitle,
  stateLabelOf,
} from "../state/graduation";
import {
  noteProposalChanged,
  openReview,
  pendingOf,
  undecidedHunks,
} from "../state/draftProposals";
import { setFocusedHunk } from "../state/draftDiscussion";
import { newIssuesBody, newIssuesTitle } from "../state/githubPolling";
import {
  configurePromptReviewRoute,
  notePromptProposalChanged,
  openPromptReview,
  pendingOf as pendingPromptOf,
} from "../state/promptProposals";
import {
  mintAddress,
  parseAddress,
  resolveTabForAddress,
  targetForTab,
  type NotificationTarget,
} from "../state/notificationAddress";
import {
  hasRunIndication,
  retainIndications,
  runIndicationPulsing,
  useTabIndications,
} from "../state/tabIndications";
import type { TabAttention } from "../components/TabStrip";

export interface AppNotificationDeps {
  s: ReturnType<typeof useShellSession>;
  overlays: ReturnType<typeof useAppOverlays>;
  vpanel: ReturnType<typeof useVerticalPanel>;
  projectFolders: ReturnType<typeof useProjectFolders>;
  projectArtifacts: ReturnType<typeof useProjectArtifacts>;
  knownDrafts: DraftSummary[];
}

export function useAppNotifications(deps: AppNotificationDeps) {
  const { s, overlays, vpanel, projectFolders, projectArtifacts, knownDrafts } =
    deps;
  const {
    dismissCommitWindow,
    dismissGraduationOverlays,
    dismissRollbackWindow,
    dismissPullRequestWindow,
    dismissTabMenu,
    dismissTokenPicker,
    openGraduationRun,
    rehearsalTimers,
    runInView,
    settingsWindowsRef,
  } = overlays;

  // ---- Notifications (NTF-notifications.md) ------------------------------
  //
  // The project's anchor path is the key an address names (NTF-FR-04), keyed
  // the same way the per-project settings slot is (GSS-FR-18) — so an address
  // minted in one worktree does not resolve in another.
  const projectKey = s.projectPath || null;
  const worktree = s.activeWorktree?.path ?? s.projectPath ?? null;

  /**
   * NTF-FR-08: what the main viewport's active tab is showing, when it is
   * something an address can name. A Diff, Search results, or History detail
   * tab yields null, because none of the three is addressable (NTF-FR-03).
   */
  const activeTarget = useMemo<NotificationTarget | null>(
    () => (s.activeT ? targetForTab(s.activeT) : null),
    [s.activeT],
  );

  // NTF-FR-12: seed the facility's gate at startup. It is *owned* by the
  // facility rather than by this component, so a change made in the
  // Notifications section reaches the next raise without a relaunch — which is
  // what NTF-FR-12 requires and what this component could not provide, being
  // several levels above the section that edits it.
  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (cancelled) return;
      // GSS-FR-32: absent means never chosen, and the default is ON.
      setNotificationsEnabled(prefs.notificationsEnabled ?? true);
      // GSS-FR-33 / SNV-FR-64: seed the follow gate on the same terms and for
      // the same reason — it is owned by the shell rather than by the Navigation
      // section that edits it, so a change made there reaches the next
      // activation without a relaunch (GLS-FR-28). `?? true` rather than
      // `=== true`, or every record written before the field existed would
      // present as opted out.
      setSelectionFollowsTab(prefs.selectionFollowsTab ?? true);
    });
    // NTF-FR-13: the state is *read* at startup. Nothing here requests it, so a
    // permission prompt never appears because the application started.
    void readNotificationPermission("startup");
    return () => {
      cancelled = true;
    };
  }, []);

  /**
   * NTF-FR-12 / GLS-FR-25 / GLS-FR-28: re-seed both gates when the settings
   * window writes them.
   *
   * The switches that govern them live in the Global settings **window** now
   * (SWN-FR-01), which is a webview of its own: `setNotificationsEnabled` there
   * reaches that window's copy of the facility and not this one's. Without
   * this, a switch turned off would take effect only at the next relaunch —
   * which is exactly what NTF-FR-12 says it must not.
   *
   * The operating system's own disposition is re-read alongside, because it may
   * have been requested from that window's Notifications section (GLS-FR-26)
   * and this window has no other way to hear about it. Re-reading is not
   * requesting: no prompt originates here (NTF-FR-13).
   */
  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisten.push(fn);
    };
    const reseed = () => {
      void refreshAppPreferences().then((prefs) => {
        if (cancelled) return;
        setNotificationsEnabled(prefs.notificationsEnabled ?? true);
        setSelectionFollowsTab(prefs.selectionFollowsTab ?? true);
      });
      void readNotificationPermission("settings changed");
    };
    void onAppPreferencesChanged(reseed).then(keep);
    // NTF-FR-WUUY: the author may change the permission in the operating
    // system's settings while away, so it is read again when the main window
    // gets focus. Reading is not requesting: no prompt originates here.
    const rereadPermission = () => {
      void readNotificationPermission("window focus");
    };
    window.addEventListener("focus", rereadPermission);
    unlisten.push(() => window.removeEventListener("focus", rereadPermission));
    // A settings window opening or closing is the other moment the author may
    // have changed one of these, the permission in particular, which no
    // preferences write announces.
    void onSettingsWindowChanged(reseed).then(keep);
    /**
     * GLS-FR-27 / NTF-FR-25: the Notifications section asked for a rehearsal.
     *
     * The wait belongs here rather than in the section: the whole point is that
     * the author leaves before it fires, and closing that window is one of the
     * ways they may leave — a timer held there would go with the window and
     * take the notification with it (NTF-FR-25, NTF-FR-17, GLS-FR-27). It then goes through the
     * ordinary facility on exactly the terms every other raise does, so it is
     * correctly suppressed when the author stays put (NTF-FR-25, NTF-FR-08).
     */
    void onSettingsRehearsalRequested((address) => {
      const timer = window.setTimeout(() => {
        rehearsalTimers.current.delete(timer);
        void raiseNotification({
          key: "notifications:rehearsal",
          // NTF-FR-FPLB: the title says what happened. The operating system
          // already shows the application's name above it.
          title: "Test notification",
          body: "Click it to come back to Global settings.",
          address,
        });
      }, REHEARSAL_DELAY_MS);
      rehearsalTimers.current.add(timer);
    }).then(keep);
    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
      for (const timer of rehearsalTimers.current) window.clearTimeout(timer);
      rehearsalTimers.current.clear();
    };
  }, []);

  // The facility reads the window through getters rather than a captured value,
  // so a raise made at any moment sees the state as it is then rather than as
  // it was when this effect last ran.
  const snapshotRef = useRef({
    focused: true,
    projectKey,
    worktree,
    activeTarget,
    visiblePanelSurface: null as string | null,
    visibleBottomSurface: null as string | null,
    visibleRun: null as string | null,
    openSettingsWindow: null as string | null,
  });
  snapshotRef.current = {
    // Overwritten at raise time by `configureNotifications` above; the value
    // here is only a placeholder for the shape.
    focused: true,
    projectKey,
    worktree,
    activeTarget,
    // NTF-FR-08: a panel surface counts as "already seen" only while its panel
    // is actually VISIBLE. `s.panelSurface` is never null — it keeps naming the
    // selected surface while the panel is hidden — so the hidden flag is what
    // decides, exactly as `bottomVisible` does for the bottom panel.
    visiblePanelSurface: vpanel.hidden ? null : s.panelSurface,
    visibleBottomSurface: s.bottomVisible ? s.bottomSurface : null,
    // NTF-FR-08 / NTF-FR-39: the run the author is actually looking at, which
    // the bottom panel reports because it is what knows both which surface is
    // showing and which run that surface has selected.
    visibleRun: s.bottomVisible ? runInView : null,
    // Overwritten at raise time like `focused` above, and for the same reason:
    // a settings window opening or closing does not re-render this component.
    openSettingsWindow: null,
  };
  /**
   * NTF-FR-27: the strip as the facility needs to see it when a raise arrives.
   *
   * A ref for the same reason the snapshot is one — the facility is configured
   * once and must see the tabs as they are at the moment of the raise, not as
   * they were when that effect last ran.
   */
  const stripRef = useRef({
    tabs: s.tabs,
    activeTab: s.activeTab,
    projectKey,
    worktree,
  });
  stripRef.current = {
    tabs: s.tabs,
    activeTab: s.activeTab,
    projectKey,
    worktree,
  };

  useEffect(() => {
    configureNotifications({
      // Focus is sampled HERE, at raise time, rather than being read into the
      // snapshot during render: React does not re-render on a window blur, so a
      // rendered value would be as stale as the last render — and the stale
      // direction is the harmful one, since a stale `focused: true` suppresses
      // the raise entirely and silently (NTF-FR-10).
      snapshot: () => ({
        ...snapshotRef.current,
        // NTF-FR-08 asks whether ANY window of the application holds focus, and
        // since SWN-FR-01 there may be a second one: a settings window taking
        // focus does not put the author somewhere else in the application.
        focused:
          (typeof document === "undefined" ? true : document.hasFocus()) ||
          settingsWindowsRef.current.focused,
        // NTF-FR-08 / SWN-FR-05: a raise naming the settings window that is at
        // that moment open is suppressed — the author has already been told by
        // the thing itself.
        openSettingsWindow: settingsWindowsRef.current.open,
      }),
      // NTF-FR-27: which open tab this address's own routing would activate.
      // Matched on the target rather than on the tab id, so it finds the tab
      // whatever the strip has named it — and finds the *editing* tab of a file
      // that also has a Diff or History detail tab open, `targetForTab` yielding
      // nothing for either of those (TAB-FR-06).
      resolveTab: (address) => resolveTabForAddress(address, stripRef.current),
    });
  }, []);

  /**
   * A stable signature of what the strip is showing, for the two effects below.
   *
   * `s.tabs` is rebuilt on every render (the dirty flag is read live off the
   * edit stores), so depending on the array itself would run both of these every
   * render. A tab's id names its target, so the ids are what actually decide
   * whether an address still resolves.
   */
  const stripSignature = `${projectKey ?? ""}\u0000${worktree ?? ""}\u0000${s.tabs
    .map((t) => t.id)
    .join("\u0001")}`;

  /**
   * NTF-FR-33: an indication whose tab is gone goes with it, and does not come
   * back when the target is reopened.
   *
   * Reconciled against the strip rather than hooked onto each close path,
   * because a tab leaves by more routes than the author closing it — a removed
   * file takes its tab (TAB-FR-19), a commit takes its Diff tabs (TAB-FR-22),
   * and a worktree change takes all of them (TAB-FR-14).
   */
  useEffect(() => {
    retainIndications((address) => {
      const parsed = parseAddress(address);
      // The same question the facility asked when it marked, asked the same
      // way — so an indication cannot survive a tab the raise-time resolver
      // would no longer find, nor be reaped while that resolver still would.
      return (
        !!parsed &&
        resolveTabForAddress(parsed, {
          tabs: s.tabs,
          activeTab: s.activeTab,
          projectKey,
          worktree,
        }) !== null
      );
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stripSignature]);

  /**
   * NTF-FR-26 / NTF-FR-34: the marked tabs, as the strip renders them.
   *
   * The facility holds indications by address (NTF-FR-30); this is the only
   * place they are mapped onto tab ids, and it is a read rather than a second
   * copy of the state. Several keys indicating one address yield one entry here,
   * which is what makes the emphasis aggregate (NTF-FR-31).
   */
  const indications = useTabIndications();
  const tabAttention = useMemo<ReadonlyMap<string, TabAttention>>(() => {
    const out = new Map<string, TabAttention>();
    if (!projectKey || !worktree || indications.addresses.size === 0) return out;
    for (const tab of s.tabs) {
      const target = targetForTab(tab);
      if (!target) continue;
      const address = mintAddress(projectKey, worktree, target);
      if (!indications.addresses.has(address)) continue;
      out.set(tab.id, indications.pulsing.has(address) ? "pulse" : "on");
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stripSignature, indications]);

  /**
   * NTF-FR-39 / SNV-FR-70: the activity bar's Runs toggle, as the strip renders
   * it.
   *
   * The same read the tabs get, one step simpler: the toggle renders **one**
   * emphasis however many runs are indicating it (NTF-FR-31), so what the strip
   * needs is the one answer rather than the set behind it.
   */
  const runsAttention: TabAttention | undefined = runIndicationPulsing(
    indications,
  )
    ? "pulse"
    : hasRunIndication(indications)
      ? "on"
      : undefined;

  /**
   * DCR-FR-18: a change was proposed to a draft, or one was decided.
   *
   * Subscribed at the **shell** rather than in the New Artifact tab that renders
   * the modal: a tab-scoped listener would hear nothing for a draft whose tab is
   * closed, which is exactly the case the author most needs telling about. The
   * payload carries the whole proposal, so the store folds it in without a read
   * and every marker that follows it — the tab's, the panel's, the comment
   * control's — is current at once.
   *
   * Whether the raise becomes a notification the author sees is
   * `NTF-notifications.md`'s policy and not this component's (DCR-FR-19): it is
   * posted while the window is unfocused, and while the window is focused but
   * that draft's tab is not the active one — and suppressed while it is, because
   * the tab has already told the author itself (DCR-FR-03).
   *
   * The refs are read at delivery time rather than captured, for the reason the
   * activation listener above uses them: the subscription is established once
   * and must not be torn down and rebuilt on every project change, or an event
   * landing in that gap would be lost.
   */
  const rootRef = useRef({ projectKey, worktree });
  rootRef.current = { projectKey, worktree };
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onDraftChangeProposalsChanged((payload) => {
      noteProposalChanged(payload);
      // DRP-FR-19: the panel's marker rides on `list_drafts`' own summary, so a
      // proposal recorded or decided anywhere re-lists it — which is also what
      // keeps the panel's cost at one call however many drafts it shows.
      s.bumpDrafts();
      // DCR-FR-18: nothing is raised when a proposal is *decided* — the author
      // decided it.
      if (payload.proposal.state !== "pending") return;
      const { projectKey: key, worktree: tree } = rootRef.current;
      if (!key || !tree) return;
      void raiseNotification({
        // NTF-FR-07: one slot per draft, so an agent that proposes, is declined,
        // and proposes again replaces its own notification rather than stacking
        // a second beside it.
        key: `draft-proposal:${payload.draftId}`,
        title: `${proposerHandle(payload.proposal)} proposed a change`,
        body: payload.proposal.path,
        address: mintAddress(key, tree, {
          kind: "draft",
          draftId: payload.draftId,
        }),
      });
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
   * NTF-FR-JLXL / NTF-FR-VALU: a GitHub poll found new ready tasks.
   *
   * Raised once per poll, with one key, so a later poll's raise replaces the
   * earlier one rather than stacking beside it. A change that carries no new
   * issue — a failed, stale, or discarded poll, a settings change, a claim —
   * raises nothing, because the backend reports no new issue for it
   * (GPP-FR-RRPB). The address is the Git bottom surface of the open project
   * and its active worktree.
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onGithubPollingChanged((payload) => {
      const issues = payload.newIssues ?? [];
      if (issues.length === 0) return;
      const { projectKey: key, worktree: tree } = rootRef.current;
      if (!key || !tree) return;
      void raiseNotification({
        key: "github-ready-tasks",
        title: newIssuesTitle(issues),
        body: newIssuesBody(issues),
        address: mintAddress(key, tree, { kind: "bottom", surface: "git" }),
      });
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
   * PCR-FR-17 / PCR-FR-26: a change was proposed to a prompt artifact, or one
   * was decided.
   *
   * Subscribed at the **shell** rather than in the Editor tab that renders the
   * review, for the reason the draft subscription above is: a tab-scoped
   * listener would hear nothing for a file whose tab is closed, which is exactly
   * the case the author most needs telling about (PCR-FR-17). The payload
   * carries the whole proposal, so the store folds it in without a read and
   * every surface that follows it — the tab's indication, the review, the
   * control in the rail — is current at once.
   *
   * The subscriber does two things and no others (PCR-FR-17): it **raises** on a
   * recording, and it **retracts** on a decision. It reaches no notification
   * command of its own.
   */
  /**
   * PCR-FR-18: the route every proposal control takes to its review — opening or
   * focusing the artifact's Editor tab first, so the review is never rendered
   * anywhere but over the tab for its own file.
   *
   * Registered here rather than threaded through the comment rail, on exactly
   * the terms the notification facility is configured: the control sits several
   * layers below the tab strip that knows how to open a tab.
   */
  const openArtifactTab = useCallback(
    (artifactId: string) => {
      const found = projectArtifacts.artifacts.find(
        (a) => a.id === artifactId || a.path === artifactId,
      );
      // TAB-FR-05: `openArtifact` jumps focus to the tab already on this
      // artifact rather than creating a second one.
      s.openArtifact(
        found
          ? { id: found.id, name: found.name, artifactType: found.artifactType }
          : { id: artifactId, name: artifactId },
      );
    },
    [projectArtifacts.artifacts, s],
  );
  useEffect(() => {
    configurePromptReviewRoute(openArtifactTab);
    return () => configurePromptReviewRoute(null);
  }, [openArtifactTab]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onPromptChangeProposalsChanged((payload) => {
      notePromptProposalChanged(payload);
      const key = promptRaiseKey(payload.proposal.id);
      // PCR-FR-26: a decided proposal — accepted, rejected, or accepted with its
      // comment still owed — is settled however it was settled, so the raise is
      // taken back. Unconditionally rather than after working out what became of
      // it: a raise that was suppressed, or a notification already reached,
      // retracts to a no-op (NTF-FR-38).
      if (payload.proposal.state !== "pending") {
        retractNotification(key);
        return;
      }
      const { projectKey: openKey, worktree: tree } = rootRef.current;
      if (!openKey || !tree) return;
      void raiseNotification({
        // PCR-FR-17: derived from the **proposal's own id**, so two proposals
        // against one artifact never share a key and one proposal's retraction
        // never reaches another's notification.
        key,
        // PCR-FR-17: a title naming the agent **and the file**, and a body
        // naming the prompt — so a notification read in a centre beside a dozen
        // others says which file is waiting without being opened.
        title: `${promptProposerHandle(payload.proposal)} proposed a change to ${basename(payload.proposal.path)}`,
        body: payload.proposal.path,
        // NTF-FR-03: the project-file address, so activating it opens or
        // focuses that artifact's Editor tab (PCR-FR-18).
        address: mintAddress(openKey, tree, {
          kind: "file",
          path: payload.proposal.path,
        }),
      });
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
   * GRU-FR-BLSS: a run entering a state the author is being waited on for — or
   * one that finished — calls them back.
   *
   * Subscribed at the shell for the reason the proposal raise above is: a run is
   * unattended by design, so a listener scoped to the Runs panel would hear
   * nothing for exactly the runs the author most needs telling about. Keyed by
   * the run, so a run raising twice replaces its own notification rather than
   * stacking a second beside it (NTF-FR-07), and addressed to the run so
   * activating it lands on the section with that run selected.
   *
   * Progress raises nothing: a run that is merely still working is not news
   * (NTF-FR-24), which `raisesOn` is the whole of.
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onGraduationRunChanged((payload) => {
      // DRP-FR-35 / NAW-FR-44: every surface showing a draft's lock re-reads it
      // on the same signal, so the panel and any open tab follow the run.
      s.bumpDrafts();
      const { projectKey: key, worktree: tree } = rootRef.current;
      if (!key || !tree) return;
      void api
        .getGraduationRun(payload.runId)
        .then((run) => {
          // GRU-FR-BLSS: `interrupted` is the one state the run itself has to be
          // read to decide — a stop the author caused is not news to them.
          if (!raisesForRun(run)) return;
          void raiseNotification({
            key: raiseKey(run),
            title: raiseTitle(run, stateLabelOf(run)),
            body: raiseStatement(run),
            address: mintAddress(key, tree, {
              kind: "run",
              runId: run.id,
            }),
          });
        })
        // A run that has gone between the event and the read has nothing to
        // call anyone back to.
        .catch(() => {});
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // NTF-FR-14: reaching what a notification addresses by a route of the
  // author's own withdraws it and clears any indication naming it, so neither
  // outlives the thing it was about being seen.
  useEffect(() => {
    if (!projectKey || !worktree || !activeTarget) return;
    notifyArrived(mintAddress(projectKey, worktree, activeTarget));
  }, [projectKey, worktree, activeTarget]);

  /**
   * NTF-FR-14: the window regaining OS focus reaches what its already-active tab
   * is a view onto — and only that.
   *
   * The effect above fires on the active tab *changing*, which is not what
   * happens here: the author left the window with a tab active, something raised
   * about that very tab and posted (the window being unfocused, NTF-FR-08), and
   * they came back to find it already in front of them. An address naming any
   * other tab is untouched, focus alone having brought them no nearer to it —
   * which is also why this clears no indication in practice, the active tab
   * never carrying one (NTF-FR-29).
   */
  useEffect(() => {
    if (!projectKey || !worktree || !activeTarget) return;
    const address = mintAddress(projectKey, worktree, activeTarget);
    const onFocus = () => notifyArrived(address);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [projectKey, worktree, activeTarget]);
  useEffect(() => {
    if (!projectKey || !worktree || !s.panelSurface) return;
    notifyArrived(
      mintAddress(projectKey, worktree, {
        kind: "panel",
        surface: s.panelSurface as never,
      }),
    );
  }, [projectKey, worktree, s.panelSurface]);
  useEffect(() => {
    if (!projectKey || !worktree || !s.bottomVisible) return;
    notifyArrived(
      mintAddress(projectKey, worktree, {
        kind: "bottom",
        surface: s.bottomSurface as never,
      }),
    );
  }, [projectKey, worktree, s.bottomVisible, s.bottomSurface]);
  /**
   * NTF-FR-14 / NTF-FR-39: the author **reaches** a run by opening Runs on it,
   * whether they got there from the notification or on their own — so the
   * toggle's mark for that run is cleared and its notification withdrawn.
   *
   * Reaching one run leaves another's mark standing, which falls out of the
   * address naming the run rather than the surface.
   */
  useEffect(() => {
    if (!projectKey || !worktree || !s.bottomVisible || !runInView) return;
    notifyArrived(
      mintAddress(projectKey, worktree, { kind: "run", runId: runInView }),
    );
  }, [projectKey, worktree, s.bottomVisible, runInView]);

  // NTF-FR-15: a content root the application is no longer reading leaves
  // nothing pointing into it. Keyed on the epoch the shell already bumps on a
  // worktree change, and on the project path for a project switch or close.
  const previousRoot = useRef<{ key: string; worktree: string } | null>(null);
  useEffect(() => {
    const current =
      projectKey && worktree ? { key: projectKey, worktree } : null;
    const previous = previousRoot.current;
    if (
      previous &&
      (previous.key !== current?.key || previous.worktree !== current?.worktree)
    ) {
      withdrawForRoot(previous.key, previous.worktree);
    }
    previousRoot.current = current;
  }, [projectKey, worktree, s.contentRootEpoch]);

  const notifications = useNotifications(
    {
      projectKey,
      worktree,
      // A file resolves when the Library's loaded tree still holds it. The
      // shell already knows every artifact it can open, so this costs no read.
      // Both predicates read what the shell already holds, so resolving an
      // activation costs no backend round-trip and cannot hang the routing.
      fileExists: (path) =>
        projectArtifacts.artifacts.some(
          (a) => a.id === path || a.path === path,
        ),
      draftExists: (draftId) => knownDrafts.some((d) => d.id === draftId),
    },
    {
      openFile: (path) => {
        const found = projectArtifacts.artifacts.find(
          (a) => a.id === path || a.path === path,
        );
        // TAB-FR-05: `openArtifact` jumps focus to the tab already on this
        // artifact rather than creating a second one.
        s.openArtifact(
          found
            ? { id: found.id, name: found.name, artifactType: found.artifactType }
            : { id: path, name: path },
        );
        // PCR-FR-18: a project-file address is raised to call the author back to
        // a change an agent has proposed, so the review is what they land on —
        // that being what they were called back for. Opened here rather than by
        // the arrival itself (PCR-FR-16), because a proposal for a file with no
        // tab open has nowhere to arrive and must not sit in the open slot
        // waiting to ambush whoever next opens that file for their own reasons.
        const pendingPrompt = pendingPromptOf(found ? found.id : path);
        if (pendingPrompt !== undefined) openPromptReview(pendingPrompt.id);
      },
      openDraft: (draftId) => {
        const draft = knownDrafts.find((d) => d.id === draftId);
        if (!draft) return;
        s.openDraft({ id: draft.id, name: draft.name });
        // DCR-FR-19: a draft address is raised to call the author back to a
        // change an agent has proposed, so the document lands on the first
        // change still to decide — that being what they were called back for.
        // Set here rather than by the arrival itself (DCR-FR-03), because a
        // proposal for a draft with no tab open has nowhere to arrive and must
        // not sit waiting to move the document under whoever next opens that
        // draft for their own reasons.
        const pending = pendingOf(draftId);
        if (pending !== undefined) {
          const first = undecidedHunks(pending)[0];
          if (first) setFocusedHunk(draftId, first.id);
          openReview(pending.id);
        }
      },
      openDashboard: () => s.goHome(),
      openPanelSurface: (surface) => s.setPanelSurface(surface as never),
      openBottomSurface: (surface) => s.showBottom(surface as never),
      // NTF-FR-17 / GRU-FR-BLSS: the run address opens the bottom panel on the
      // graduation section with that run selected — the same navigation the
      // author's own route from a draft performs.
      openRun: (runId) => openGraduationRun(runId),
      openGlobalSettings: () => s.openGlobalSettings(),
      openProjectSettings: () => s.openSettings(),
    },
  );

  // SNV-FR-24: File → New File opens the New File modal (NFI), File → New Artifact
  // the New Artifact modal (NAW), and File → New Folder the New Folder modal (NFW),
  // each with nothing seeded — the user picks everything, so the file modal's
  // location and the folder modal's parent both start at the project root
  // (NFI-FR-06 / NFW-FR-06). Close Project returns to the picker.
  // SNV-FR-25 / EDT-FR-33: Close Project writes every artifact's pending changes
  // before the backend teardown; a blocked write cancels the close.
  // SNV-FR-29 / SNV-FR-31: Save writes the active tab's content; Save All writes
  // everything holding unsaved changes. Both only ever fire while their menu
  // item is enabled (SNV-FR-28 / SNV-FR-30).
  useMenuEvents({
    newFile: () => {
      dismissTokenPicker();
      dismissCommitWindow();
      dismissRollbackWindow();
      dismissPullRequestWindow();
      dismissTabMenu();
      dismissGraduationOverlays();
      // The Location select is fed from the same published folder list the New
      // Folder window uses, so it refreshes on the same terms (NFI NFR).
      projectFolders.ensureFresh();
      s.openNewFile({});
    },
    newFolder: () => {
      dismissTokenPicker();
      dismissCommitWindow();
      dismissRollbackWindow();
      dismissPullRequestWindow();
      dismissTabMenu();
      dismissGraduationOverlays();
      // Refresh the parent list only if it may have gone stale since the Library
      // last published it; in the common case this is a no-op (NFW NFR).
      projectFolders.ensureFresh();
      s.openNewFolder({});
    },
    // SNV-FR-24 / NTA-FR-07: New Artifact opens the typed-artifact window with
    // the location at the project root, the name empty, and the artifact type
    // unchosen. It creates **no draft** and opens no New Artifact tab
    // (NTA-FR-14) — a draft is created in the Drafts panel alone (DRP-FR-06,
    // DRP-FR-26), which is the whole of what this item and that tab share
    // besides a name.
    newArtifact: () => {
      dismissTokenPicker();
      dismissCommitWindow();
      dismissRollbackWindow();
      dismissPullRequestWindow();
      dismissTabMenu();
      dismissGraduationOverlays();
      // The Location select is fed from the same published folder list the New
      // File and New Folder windows use, so it refreshes on the same terms and
      // opening the window costs no filesystem walk (NTA NFR).
      projectFolders.ensureFresh();
      s.openNewTypedArtifact({});
    },
    // ABT-FR-QZHW: while the About panel is open the window behind it takes no
    // interaction, so the four actions that act on the active tab do nothing.
    save: () => {
      if (!overlays.aboutOpen) void s.requestSave();
    },
    saveAll: () => {
      if (!overlays.aboutOpen) void s.requestSaveAll();
    },
    closeProject: () => void s.requestCloseProject(),
    exit: () => void s.requestExit(),
    // SNV-FR-43: Find and Find & Replace toggle the active Editor tab's panel
    // (EFR-FR-AYNZ / EFR-FR-BJUY). Like Save, both only ever fire while their menu
    // item is enabled.
    find: () => {
      if (!overlays.aboutOpen) s.requestFind("find");
    },
    findReplace: () => {
      if (!overlays.aboutOpen) s.requestFind("replace");
    },
    // ABT-FR-KMVD: About is offered whether or not a project is open, so this
    // handler lives with the window-level subscription rather than the shell's.
    about: () => overlays.openAbout(),
  });

  return { notifications, tabAttention, runsAttention };
}

/** DCR-FR-18: who a raise says proposed the change. */
/**
 * PCR-FR-17: the key a proposal against a prompt artifact is raised under.
 *
 * Derived from the proposal's **own id** rather than from its artifact, so two
 * proposals against one file never share a key — and one proposal's retraction
 * never withdraws another's notification (NTF-FR-38).
 */
function promptRaiseKey(proposalId: string): string {
  return `prompt-proposal:${proposalId}`;
}

/** The file's own name, for a title that has to name it in a few words. */
function basename(path: string): string {
  const tail = path.split("/").pop();
  return tail === undefined || tail === "" ? path : tail;
}

function promptProposerHandle(proposal: PromptChangeProposal): string {
  return proposal.agent.kind === "agent"
    ? `@${proposal.agent.handle}`
    : proposal.agent.login;
}

function proposerHandle(proposal: DraftChangeProposal): string {
  return proposal.agent.kind === "agent"
    ? `@${proposal.agent.handle}`
    : proposal.agent.login;
}
