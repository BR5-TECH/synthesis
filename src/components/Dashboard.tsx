import { useCallback, useEffect } from "react";
import type { ReactNode } from "react";
import * as api from "../api";
import * as events from "../events";
import { Icon } from "./icons";
import { DashWidget, WidgetNotice, useWidget } from "./dashboardWidget";
import { localDate } from "./deliveryTimeline";
import {
  DeliveryWidget,
  DELIVERY_ERROR_TEXT,
  deliveryHasContent,
  useDeliveryRefreshEvents,
} from "./DashboardDelivery";
import type {
  ActiveDraftItem,
  AgentRunItem,
  DashboardWidgetName,
  DeliveryMilestones,
  OpenableArtifact,
  PendingGitActivity,
  RecentlyEditedArtifact,
} from "../types";

interface DashRowProps {
  name: string;
  meta?: string;
  onClick?: () => void;
  icon?: ReactNode;
  live?: boolean;
  badge?: { label: string; tone: "live" | "ok" | "warn" | "danger" | "accent" };
}

function DashRow({ name, meta, onClick, icon, live, badge }: DashRowProps) {
  return (
    <div
      className="dash-row"
      onClick={onClick}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      onKeyDown={
        onClick
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                onClick();
              }
            }
          : undefined
      }
    >
      {/* DSH-FR-07: the live indicator is on the row of the run that is in
          flight, not on the widget — a widget-level indicator would claim
          liveness for four finished runs beside one running one. */}
      {live && <span className="dot dot--live" />}
      {icon}
      <span className="dash-row__name">{name}</span>
      {badge && <span className={`badge badge--${badge.tone}`}>{badge.label}</span>}
      {meta && <span className="dash-row__meta">{meta}</span>}
    </div>
  );
}

/**
 * DSH-FR-13: how a run's state reads. The distinction that matters is between a
 * run that is doing something, one that ended well, and one that did not —
 * collapsing the last two onto one token would report a failure in the same
 * colour as a clean finish.
 */
export function runTone(
  state: AgentRunItem["state"],
): "live" | "ok" | "warn" | "danger" | "accent" {
  switch (state) {
    case "working":
    case "reviewing":
      return "live";
    case "completed":
      return "ok";
    case "failed":
    case "blocked":
      return "danger";
    case "discarded":
    case "interrupted":
      return "warn";
    default:
      // `queued`, and every state resting on a decision the author has not
      // taken: waiting is neither progress nor failure.
      return "accent";
  }
}

/** The four Pending Git counts, in the order DSH-FR-14 names them. */
export function pendingGitRows(
  activity: PendingGitActivity,
): { key: string; label: string; value: number | null }[] {
  return [
    { key: "modifiedArtifacts", label: "modified artifacts", value: activity.modifiedArtifacts },
    { key: "modifiedSourceFiles", label: "modified source files", value: activity.modifiedSourceFiles },
    { key: "unpushedCommits", label: "commits to push", value: activity.unpushedCommits },
    { key: "fetchableCommits", label: "commits to fetch", value: activity.fetchableCommits },
  ];
}

/**
 * DSH-FR-04: whether the Pending Git widget has content.
 *
 * A count the backend reports as unavailable is not content, and neither is a
 * zero: a project with nothing pending has nothing to say, and four zeroes said
 * out loud is noise on the one surface meant to be read at a glance.
 */
export function hasPendingGitContent(activity: PendingGitActivity | null): boolean {
  return (
    activity != null &&
    pendingGitRows(activity).some((row) => row.value != null && row.value > 0)
  );
}

interface DashboardProps {
  onOpenArtifact: (node: OpenableArtifact) => void;
  onOpenRuns: () => void;
  onOpenGit: () => void;
  /**
   * DSH-FR-12: open an Active workstreams item's draft in a New Artifact tab.
   * By **id**, so the name the widget last read cannot decide which draft opens
   * (NAW-FR-03).
   */
  onOpenDraft?: (draftId: string) => void;
  /**
   * DSH-FR-06 / DSH-FR-07: open the Runs bottom panel for one run. Falls back to
   * `onOpenRuns` where the shell supplies no per-run route.
   */
  onOpenRun?: (runId: string) => void;
  /**
   * DSH-FR-DFDW / MDL-FR-JIKK: open the Map tab on its Delivery lens. With an
   * item id, the lens selects that milestone; with null (the widget header),
   * it opens on the lens alone.
   */
  onShowDeliveryItem?: (itemId: string | null) => void;
  /**
   * The clock the Delivery widget reads today from (DSH-FR-NZXN,
   * DSH-FR-NNWM). Tests give a fixed one, so no result depends on real time.
   */
  clock?: () => Date;
}

const systemClock = () => new Date();

export function Dashboard({
  onOpenArtifact,
  onOpenRuns,
  onOpenGit,
  onOpenDraft,
  onOpenRun,
  onShowDeliveryItem,
  clock = systemClock,
}: DashboardProps) {
  const recent = useWidget<RecentlyEditedArtifact[]>(
    "recently_edited",
    api.listRecentlyEditedArtifacts,
  );
  const drafts = useWidget<ActiveDraftItem[]>("active_drafts", api.listActiveDrafts);
  const runs = useWidget<AgentRunItem[]>("agent_activity", api.listRecentAgentRuns);
  const git = useWidget<PendingGitActivity>("pending_git", api.listPendingGitActivity);
  // DSH-FR-YAZR: the Delivery widget reads with `refresh` false when the
  // Dashboard renders.
  const loadDelivery = useCallback(() => api.loadDeliveryMilestones(false), []);
  const delivery = useWidget<DeliveryMilestones>("delivery", loadDelivery, DELIVERY_ERROR_TEXT);
  const today = localDate(clock());

  // DSH-FR-17: every loader is invoked asynchronously and none is waited for.
  // Widgets render as their results arrive, in whatever order they arrive.
  const { refresh: refreshRecent } = recent;
  const { refresh: refreshDrafts } = drafts;
  const { refresh: refreshRuns } = runs;
  const { refresh: refreshGit } = git;
  const { refresh: refreshDelivery } = delivery;
  useEffect(() => {
    refreshRecent();
    refreshDrafts();
    refreshRuns();
    refreshGit();
    refreshDelivery();
  }, [refreshRecent, refreshDrafts, refreshRuns, refreshGit, refreshDelivery]);

  // DSH-FR-LGJT: the Delivery widget's own refresh events.
  useDeliveryRefreshEvents(refreshDelivery);

  // DSH-FR-15 / DSH-FR-16: each refresh event names one loader to re-invoke.
  // This surface starts no timer of its own and polls nothing — the 5-minute
  // interval belongs to the backend (PST-FR-36), and the two filesystem-driven
  // channels follow the disk (PST-FR-35).
  useEffect(() => {
    let cancelled = false;
    const pending = [
      events.onDashboardRecentlyEditedChanged(() => refreshRecent()),
      events.onDashboardActiveDraftsChanged(() => refreshDrafts()),
      events.onDashboardAgentActivityChanged(() => refreshRuns()),
      events.onDashboardPendingGitChanged(() => refreshGit()),
    ];
    void Promise.all(pending).then((unlisteners) => {
      if (cancelled) unlisteners.forEach((un) => un());
    });
    return () => {
      cancelled = true;
      pending.forEach((p) => void p.then((un) => un()).catch(() => {}));
    };
  }, [refreshRecent, refreshDrafts, refreshRuns, refreshGit]);

  // DSH-FR-18: a background refresh the author never asked for still reaches
  // them, instead of ending in a log. It names one widget and touches no other.
  const { fail: failRecent } = recent;
  const { fail: failDrafts } = drafts;
  const { fail: failRuns } = runs;
  const { fail: failGit } = git;
  useEffect(() => {
    let cancelled = false;
    const failers: Record<DashboardWidgetName, (reason: string) => void> = {
      recently_edited: failRecent,
      active_drafts: failDrafts,
      agent_activity: failRuns,
      pending_git: failGit,
    };
    const pending = events.onDashboardRefreshFailed((payload) => {
      failers[payload.widget]?.(payload.error);
    });
    void pending.then((un) => {
      if (cancelled) un();
    });
    return () => {
      cancelled = true;
      void pending.then((un) => un()).catch(() => {});
    };
  }, [failRecent, failDrafts, failRuns, failGit]);

  // DSH-FR-19: a change of active worktree discards every outstanding result
  // rather than rendering it against the new root, and re-invokes every loader.
  // The Dashboard holds no cached copy of any widget's data across it.
  const { reset: resetRecent } = recent;
  const { reset: resetDrafts } = drafts;
  const { reset: resetRuns } = runs;
  const { reset: resetGit } = git;
  const { reset: resetDelivery } = delivery;
  useEffect(() => {
    let cancelled = false;
    const pending = events.onWorktreeContextChanged(() => {
      resetRecent();
      resetDrafts();
      resetRuns();
      resetGit();
      resetDelivery();
      refreshRecent();
      refreshDrafts();
      refreshRuns();
      refreshGit();
      refreshDelivery();
    });
    void pending.then((un) => {
      if (cancelled) un();
    });
    return () => {
      cancelled = true;
      void pending.then((un) => un()).catch(() => {});
    };
  }, [
    resetRecent,
    resetDrafts,
    resetRuns,
    resetGit,
    resetDelivery,
    refreshRecent,
    refreshDrafts,
    refreshRuns,
    refreshGit,
    refreshDelivery,
  ]);

  // DSH-FR-06: route by artifact kind — a Flow opens the Flow tab, every other
  // (markdown) artifact opens the Editor. The id is the backend artifact id
  // (ASC-FR-13), so the Editor or Flow loads it live.
  const openRecent = (item: RecentlyEditedArtifact) =>
    onOpenArtifact({
      id: item.id,
      name: item.name,
      artifactType: item.kind === "flow" ? "flow" : undefined,
    });

  const recentItems = recent.state.data ?? [];
  const draftItems = drafts.state.data ?? [];
  const runItems = runs.state.data ?? [];
  const gitActivity = git.state.data;

  // DSH-FR-04 / DSH-FR-08. A widget with no content is hidden; a widget that is
  // loading or has failed is not, an error being content and a loading widget
  // being the only honest way to say "not yet". The whole-Dashboard empty state
  // is reached only once every loader has answered and answered with nothing.
  const draftsVisible = drafts.state.loading || drafts.state.error !== null || draftItems.length > 0;
  const runsVisible = runs.state.loading || runs.state.error !== null || runItems.length > 0;
  const gitVisible =
    git.state.loading || git.state.error !== null || hasPendingGitContent(gitActivity);
  const recentHasOwnContent =
    recent.state.loading || recent.state.error !== null || recentItems.length > 0;
  const deliveryVisible = deliveryHasContent(delivery.state);
  const dashboardHasContent =
    recentHasOwnContent || draftsVisible || runsVisible || gitVisible || deliveryVisible;

  if (!dashboardHasContent) {
    return (
      <div className="dash dash--empty">
        <div className="dash-empty">
          <div className="dash-empty__title">Nothing here yet</div>
          <div className="dash-empty__hint">
            Open a file from the Project panel, or start a draft in the Drafts
            panel, and this page will fill in.
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="dash">
      {/* DSH-FR-09: the Recently edited widget stays visible while the Dashboard
          has content; when its list is empty it renders an empty state rather
          than hiding (the DSH-FR-04 exception). */}
      <DashWidget
        title="Recently edited"
        count={recentItems.length > 0 ? recentItems.length : undefined}
        stale={recent.state.stale}
      >
        {recent.state.error !== null ? (
          <WidgetNotice kind="error">Recent edits could not be read</WidgetNotice>
        ) : recent.state.loading ? (
          <WidgetNotice kind="loading">Loading…</WidgetNotice>
        ) : recentItems.length === 0 ? (
          <WidgetNotice kind="empty">No recent edits yet</WidgetNotice>
        ) : (
          recentItems.map((item) => (
            <DashRow
              key={item.id}
              icon={
                item.kind === "flow" ? (
                  <Icon.Diamond className="icon" size={13} />
                ) : (
                  <Icon.File className="icon" size={13} />
                )
              }
              name={item.name}
              onClick={() => openRecent(item)}
            />
          ))
        )}
      </DashWidget>

      {/* DSH-FR-11 / DSH-FR-12: the widget's items are the project's active
          drafts, and activating one opens it in a New Artifact tab — no Project
          panel filter and no other surface. */}
      {draftsVisible && (
        <DashWidget
          title="Active workstreams"
          count={draftItems.length > 0 ? draftItems.length : undefined}
          stale={drafts.state.stale}
        >
          {drafts.state.error !== null ? (
            <WidgetNotice kind="error">Active drafts could not be read</WidgetNotice>
          ) : drafts.state.loading ? (
            <WidgetNotice kind="loading">Loading…</WidgetNotice>
          ) : (
            draftItems.map((item) => (
              <DashRow
                key={item.draftId}
                icon={<Icon.Layers className="icon" size={13} />}
                name={item.name}
                meta={item.status}
                onClick={() => onOpenDraft?.(item.draftId)}
              />
            ))
          )}
        </DashWidget>
      )}

      {/* DSH-FR-13: the five most recently updated graduation runs, a queued run
          among them where its update instant places it. */}
      {runsVisible && (
        <DashWidget
          title="Last / current agent activity"
          count={runItems.length > 0 ? runItems.length : undefined}
          stale={runs.state.stale}
        >
          {runs.state.error !== null ? (
            <WidgetNotice kind="error">Agent activity could not be read</WidgetNotice>
          ) : runs.state.loading ? (
            <WidgetNotice kind="loading">Loading…</WidgetNotice>
          ) : (
            runItems.map((item) => (
              <DashRow
                key={item.runId}
                live={item.state === "working"}
                name={item.draftName ?? item.draftId}
                badge={{ label: item.state.replace(/_/g, " "), tone: runTone(item.state) }}
                meta={item.stage}
                onClick={() => (onOpenRun ? onOpenRun(item.runId) : onOpenRuns())}
              />
            ))
          )}
        </DashWidget>
      )}

      {/* DSH-FR-14: four counts. A count the backend reports as unavailable —
          the two commit counts on a branch with no upstream — is rendered as
          unavailable rather than as zero. */}
      {gitVisible && (
        <DashWidget title="Pending Git activity" stale={git.state.stale}>
          {git.state.error !== null ? (
            <WidgetNotice kind="error">Pending Git activity could not be read</WidgetNotice>
          ) : git.state.loading ? (
            <WidgetNotice kind="loading">Loading…</WidgetNotice>
          ) : (
            pendingGitRows(gitActivity as PendingGitActivity).map((row) => (
              <DashRow
                key={row.key}
                icon={<Icon.Branch className="icon" size={13} />}
                name={row.label}
                meta={row.value == null ? "unavailable" : String(row.value)}
                onClick={onOpenGit}
              />
            ))
          )}
        </DashWidget>
      )}

      {/* DSH-FR-AHRJ: the Delivery widget is the full-width last row, below the
          grid of the other widgets, and its place is fixed. */}
      {deliveryVisible && (
        <DeliveryWidget
          state={delivery.state}
          today={today}
          onShow={(itemId) => onShowDeliveryItem?.(itemId)}
        />
      )}
    </div>
  );
}
