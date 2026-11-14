/**
 * The Delivery widget of the Dashboard (`DSH-dashboard.md` DSH-FR-AHRJ through
 * DSH-FR-YGWR): the project's milestones on one time axis, with today marked
 * and the current milestone picked out.
 *
 * The Dashboard owns the widget's loader state (`useWidget`), so the widget
 * takes part in DSH-FR-04, DSH-FR-08, and DSH-FR-19 like the others. This file
 * renders that state and holds the refresh events of DSH-FR-LGJT.
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../api";
import * as events from "../events";
import { logDebug } from "../logging";
import { deliveryErrorCode, deliveryErrorMessage } from "../state/deliveryErrors";
import type { DeliveryMilestone, DeliveryMilestones, FlowItemState } from "../types";
import { WidgetNotice, type WidgetErrorText, type WidgetState } from "./dashboardWidget";
import {
  MILESTONE_MIN_WIDTH,
  initialScrollLeft,
  isDated,
  isPastDue,
  layoutTimeline,
  leafSegments,
  leafTotal,
  milestoneCountLine,
} from "./deliveryTimeline";

/** FLO-FR-KQCE: each state has a shape and a word, so no state is said in colour alone. */
const STATE_LOOK: Readonly<Record<FlowItemState, { shape: string; word: string }>> = {
  not_started: { shape: "○", word: "Not started" },
  drafting: { shape: "◔", word: "Drafting" },
  in_progress: { shape: "◑", word: "In progress" },
  done: { shape: "✓", word: "Done" },
  dropped: { shape: "–", word: "Dropped" },
};

/** The words of a leaf state in the bar's accessible name. */
const LEAF_WORD: Readonly<Record<FlowItemState, string>> = {
  done: "done",
  in_progress: "in progress",
  drafting: "drafting",
  not_started: "not started",
  dropped: "dropped",
};

/**
 * The height of one label row above the axis. It holds the current milestone
 * with its counts line on two lines (DSH-FR-BBRC).
 */
const ROW_HEIGHT = 112;

/** DSH-FR-YGWR: a refused load shows its error text. The log carries the code alone. */
export const DELIVERY_ERROR_TEXT: WidgetErrorText = {
  describe: deliveryErrorMessage,
  code: deliveryErrorCode,
};

/** Whether an answer has the shape of `DeliveryMilestones`. */
function isAnswer(data: unknown): data is DeliveryMilestones {
  return (
    !!data &&
    typeof data === "object" &&
    Array.isArray((data as DeliveryMilestones).milestones) &&
    !!(data as DeliveryMilestones).source
  );
}

/**
 * DSH-FR-YAZR / DSH-FR-04: whether the widget is shown. It is hidden while the
 * source is `none` or the plan holds no milestone. A loading or failed widget
 * is shown, because an error is content (DSH-FR-18).
 */
export function deliveryHasContent(state: WidgetState<DeliveryMilestones>): boolean {
  if (state.loading || state.error !== null) return true;
  const data = state.data;
  return isAnswer(data) && data.source.kind !== "none" && data.milestones.length > 0;
}

/**
 * DSH-FR-LGJT: an open Dashboard reads again with `refresh` false on five
 * events, and with `refresh` true on `"github polling changed"`.
 */
export function useDeliveryRefreshEvents(refresh: (other?: () => Promise<DeliveryMilestones>) => void) {
  useEffect(() => {
    const again = () => refresh();
    const pending = [
      listen(events.DELIVERY_SOURCE_CHANGED, again),
      listen(events.GITHUB_DELIVERY_PLAN_CHANGED, again),
      events.onDraftsChanged(again),
      events.onGraduationRunChanged(again),
      events.onDashboardRecentlyEditedChanged(again),
      events.onGithubPollingChanged(() => refresh(() => api.loadDeliveryMilestones(true))),
    ];
    // One unlisten for each listener: the cleanup waits for a listen that
    // has not answered yet, so no unlisten runs twice.
    return () => {
      pending.forEach((p) => void p.then((un) => un()).catch(() => {}));
    };
  }, [refresh]);
}

/** DSH-FR-YGWR: the time of a read, as the author's clock reads it. */
function readTime(readAt: string | null): string | null {
  if (!readAt) return null;
  const at = new Date(readAt);
  if (Number.isNaN(at.getTime())) return null;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())} ${pad(at.getHours())}:${pad(at.getMinutes())}`;
}

/** DSH-FR-NNWM: the 6px bar of the leaf states, and `done/total`. */
function LeafBar({ milestone }: { milestone: DeliveryMilestone }) {
  const counts = milestone.leafCounts;
  const segments = leafSegments(counts);
  const total = leafTotal(counts);
  const label = segments.map((s) => `${s.count} ${LEAF_WORD[s.state]}`).join(", ");
  return (
    <div className="dash-delivery__leaves">
      <span className="dash-delivery__bar" role="img" aria-label={label} title={label}>
        {segments
          .filter((s) => s.count > 0)
          .map((s) => (
            <span
              key={s.state}
              className="dash-delivery__segment"
              data-state={s.state}
              style={{ flexGrow: s.count }}
            />
          ))}
      </span>
      <span className="dash-delivery__fraction">
        {counts.done}/{total}
      </span>
    </div>
  );
}

/** DSH-FR-NNWM / DSH-FR-BBRC: one milestone, on the axis or in the No date group. */
function MilestoneCard({
  milestone,
  today,
  onShow,
  style,
}: {
  milestone: DeliveryMilestone;
  today: string;
  onShow: (itemId: string) => void;
  style?: CSSProperties;
}) {
  const look = STATE_LOOK[milestone.state] ?? STATE_LOOK.not_started;
  const past = isPastDue(milestone, today);
  return (
    <button
      type="button"
      className="dash-delivery__milestone"
      data-current={milestone.current ? "true" : undefined}
      data-testid={`delivery-milestone-${milestone.itemId}`}
      style={style}
      onClick={() => onShow(milestone.itemId)}
    >
      <span className="dash-delivery__name-line">
        <span className="dash-delivery__glyph" data-state={milestone.state} title={look.word} aria-hidden="true">
          {look.shape}
        </span>
        <span className="sr-only">{look.word}: </span>
        <span className="dash-delivery__name">{milestone.name}</span>
      </span>
      {isDated(milestone) && (
        <span className="dash-delivery__due" data-past={past ? "true" : undefined}>
          {milestone.due}
          {past && <span className="dash-delivery__past"> · past due</span>}
        </span>
      )}
      <LeafBar milestone={milestone} />
      {milestone.current && (
        // DSH-FR-BBRC: the line wraps only after `·`, so a card of the
        // axis keeps its width and shows the two counts on two lines.
        <span className="dash-delivery__current-line">
          {`${milestone.leafCounts.inProgress}\u00a0in\u00a0progress\u00a0· ${milestone.blockedLeaves}\u00a0blocked`}
        </span>
      )}
    </button>
  );
}

/** DSH-FR-PLRL / DSH-FR-NZXN / DSH-FR-EWPI: the scrolling time axis. */
function Timeline({
  answer,
  milestones,
  today,
  onShow,
}: {
  /** The answer the milestones come from. Each new answer scrolls again. */
  answer: DeliveryMilestones;
  milestones: DeliveryMilestone[];
  today: string;
  onShow: (itemId: string) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState(0);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    setViewport(el.clientWidth);
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => setViewport(el.clientWidth));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const layout = useMemo(() => layoutTimeline(milestones, today, viewport), [milestones, today, viewport]);

  // DSH-FR-EWPI: each render of an answer first shows the current milestone,
  // or today where none is current.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    el.scrollLeft = initialScrollLeft(layout, el.clientWidth);
  }, [layout, answer]);

  const labelsHeight = layout.rows * ROW_HEIGHT;
  return (
    <div className="dash-delivery__scroller" ref={scroller} data-testid="delivery-scroller">
      <div className="dash-delivery__track" style={{ width: layout.width }}>
        <div className="dash-delivery__labels" style={{ height: labelsHeight }}>
          {layout.placed.map((p) => (
            <MilestoneCard
              key={p.milestone.itemId}
              milestone={p.milestone}
              today={today}
              onShow={onShow}
              style={{
                left: p.labelLeft,
                top: p.row * ROW_HEIGHT,
                width: MILESTONE_MIN_WIDTH - 8,
              }}
            />
          ))}
        </div>
        <div className="dash-delivery__axis" aria-hidden="true">
          {layout.placed.map((p) => (
            <span
              key={p.milestone.itemId}
              className="dash-delivery__tick"
              data-current={p.milestone.current ? "true" : undefined}
              style={{ left: p.x }}
            />
          ))}
        </div>
        <div className="dash-delivery__months" aria-hidden="true">
          {layout.months.map((m) => (
            <span key={m.label + m.x} className="dash-delivery__month" style={{ left: m.x }}>
              {m.label}
            </span>
          ))}
        </div>
        <div
          className="dash-delivery__today"
          data-testid="delivery-today"
          style={{ left: layout.todayX }}
        >
          <span className="dash-delivery__today-label">today</span>
        </div>
      </div>
    </div>
  );
}

interface DeliveryWidgetProps {
  state: WidgetState<DeliveryMilestones>;
  /** Today's calendar date, `YYYY-MM-DD`. */
  today: string;
  /** DSH-FR-DFDW: show one milestone in the Delivery lens, or the lens alone for null. */
  onShow: (itemId: string | null) => void;
}

/** DSH-FR-AHRJ: the full-width last row of the Dashboard. */
export function DeliveryWidget({ state, today, onShow }: DeliveryWidgetProps) {
  const data = isAnswer(state.data) ? state.data : null;
  const milestones = data?.milestones ?? [];
  const undated = milestones.filter((m) => !isDated(m));
  const answerStale = data?.stale === true;
  const readAt = answerStale ? readTime(data?.readAt ?? null) : null;

  const show = (itemId: string | null) => {
    logDebug(["frontend"], "dashboard delivery item activated", { milestone: itemId !== null });
    onShow(itemId);
  };

  return (
    <section
      className="dash-widget dash-delivery"
      data-testid="dashboard-delivery"
      data-stale={state.stale || answerStale ? "true" : undefined}
      aria-label="Delivery"
    >
      <div className="dash-widget__head">
        <button type="button" className="dash-delivery__head" onClick={() => show(null)}>
          <span className="dash-widget__title dash-delivery__title">Delivery</span>
          {data?.planName && <span className="dash-delivery__plan">· {data.planName}</span>}
        </button>
        {state.stale ? (
          <span className="dash-widget__stale" title="Refreshing…">
            Refreshing…
          </span>
        ) : (
          answerStale && (
            <span className="dash-widget__stale" data-testid="delivery-stale">
              {readAt ? `Stale · read ${readAt}` : "Stale"}
            </span>
          )
        )}
        {milestones.length > 0 && (
          <span className="dash-widget__count dash-delivery__count">{milestoneCountLine(milestones)}</span>
        )}
      </div>
      <div className="dash-widget__body dash-delivery__body">
        {state.error !== null ? (
          <WidgetNotice kind="error">The delivery could not be read. {state.error}</WidgetNotice>
        ) : state.loading || !data ? (
          <WidgetNotice kind="loading">Loading…</WidgetNotice>
        ) : (
          <div className="dash-delivery__content">
            {/* DSH-FR-NZXN: the axis shows today also when no milestone is dated. */}
            <Timeline answer={data} milestones={milestones} today={today} onShow={(id) => show(id)} />
            {undated.length > 0 && (
              <div className="dash-delivery__nodate" data-testid="delivery-no-date">
                <span className="dash-delivery__nodate-title">No date</span>
                {undated.map((m) => (
                  <MilestoneCard key={m.itemId} milestone={m} today={today} onShow={(id) => show(id)} />
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}
