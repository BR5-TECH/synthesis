/**
 * The Delivery widget of the Dashboard
 * (`../../specifications/ui/DSH-dashboard.md` DSH-FR-03, DSH-FR-AHRJ through
 * DSH-FR-YGWR, and DSH-FR-17, DSH-FR-18, DSH-FR-19 for this widget).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Dashboard } from "./Dashboard";
import { initialScrollLeft, layoutTimeline } from "./deliveryTimeline";
import { resetLogBufferForTest } from "../logging";
import { readStylesheet } from "../test/readStylesheet";
import type { DeliveryMilestone, DeliveryMilestones, RecentlyEditedArtifact } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listeners: Record<string, ((payload: unknown) => void)[]> = {};
function emitEvent(name: string, payload: unknown = {}) {
  (listeners[name] ?? []).forEach((handler) => handler(payload));
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    const wrapped = (payload: unknown) => handler({ payload });
    (listeners[name] ??= []).push(wrapped);
    return () => {
      listeners[name] = (listeners[name] ?? []).filter((h) => h !== wrapped);
    };
  }),
}));

/** 2026-11-14, at noon on the local clock. */
const clock = () => new Date(2026, 10, 14, 12, 0);

function milestone(
  itemId: string,
  name: string,
  due: string | null,
  extra: Partial<DeliveryMilestone> = {},
): DeliveryMilestone {
  return {
    itemId,
    name,
    due,
    state: "not_started",
    leafCounts: { notStarted: 0, drafting: 0, inProgress: 0, done: 0, dropped: 0 },
    blockedLeaves: 0,
    current: false,
    ...extra,
  };
}

/** The plan of the wireframe in DSH-dashboard.md. */
const PLAN: DeliveryMilestone[] = [
  milestone("m-alpha", "Alpha", "2026-09-30", {
    state: "done",
    leafCounts: { notStarted: 0, drafting: 0, inProgress: 0, done: 12, dropped: 0 },
  }),
  milestone("m-private", "Private beta", "2026-10-31", {
    state: "done",
    leafCounts: { notStarted: 0, drafting: 0, inProgress: 0, done: 8, dropped: 0 },
  }),
  milestone("m-beta", "Beta", "2027-03-31", {
    state: "in_progress",
    leafCounts: { notStarted: 6, drafting: 1, inProgress: 5, done: 7, dropped: 0 },
    blockedLeaves: 4,
    current: true,
  }),
  milestone("m-ga", "GA", "2027-06-30", {
    leafCounts: { notStarted: 9, drafting: 0, inProgress: 0, done: 0, dropped: 0 },
  }),
  milestone("m-v21", "v2.1", null, {
    leafCounts: { notStarted: 3, drafting: 0, inProgress: 0, done: 0, dropped: 0 },
  }),
];

function answer(extra: Partial<DeliveryMilestones> = {}): DeliveryMilestones {
  return {
    source: { kind: "file", flowId: "flows/release.flow.md", path: "flows/release.flow.md" },
    planName: "Payments v2",
    milestones: PLAN,
    readAt: null,
    stale: false,
    ...extra,
  };
}

let delivery: (refresh: boolean) => Promise<DeliveryMilestones>;
let recent: RecentlyEditedArtifact[];

function deliveryCalls(): boolean[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "load_delivery_milestones")
    .map((c) => (c[1] as { refresh: boolean }).refresh);
}

function renderDashboard() {
  const onShowDeliveryItem = vi.fn<(itemId: string | null) => void>();
  render(
    <Dashboard
      onOpenArtifact={vi.fn()}
      onOpenRuns={vi.fn()}
      onOpenGit={vi.fn()}
      onShowDeliveryItem={onShowDeliveryItem}
      clock={clock}
    />,
  );
  return { onShowDeliveryItem };
}

const widget = () => screen.findByTestId("dashboard-delivery");

beforeEach(() => {
  resetLogBufferForTest();
  for (const k in listeners) delete listeners[k];
  delivery = async () => answer();
  recent = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "list_recently_edited_artifacts":
        return Promise.resolve(recent);
      case "list_active_drafts":
      case "list_recent_agent_runs":
        return Promise.resolve([]);
      case "list_pending_git_activity":
        return Promise.resolve({
          modifiedArtifacts: null,
          modifiedSourceFiles: null,
          unpushedCommits: null,
          fetchableCommits: null,
        });
      case "load_delivery_milestones":
        return delivery((args as { refresh: boolean }).refresh);
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});
afterEach(cleanup);

describe("Dashboard — Delivery widget: place and visibility", () => {
  it("DSH-FR-YAZR: reads with refresh false when the Dashboard renders", async () => {
    renderDashboard();
    await widget();
    expect(deliveryCalls()).toEqual([false]);
  });

  it("DSH-FR-03, DSH-FR-AHRJ: the widget is the last row of the Dashboard, and it spans the full width", async () => {
    recent = [{ id: "a.md", name: "a.md", kind: "markdown", modifiedAt: "2026-11-14T09:00:00.000Z" }];
    renderDashboard();
    const el = await widget();
    await screen.findByText("a.md");
    expect(el.parentElement?.lastElementChild).toBe(el);
    expect(el).toHaveClass("dash-widget", "dash-delivery");
    const sheet = readStylesheet("kit.css").replace(/\/\*[\s\S]*?\*\//g, "");
    expect(sheet).toMatch(/\.dash-delivery\s*\{[^}]*grid-column:\s*1\s*\/\s*-1/);
  });

  it("DSH-FR-YAZR: hidden while the source is none, and the Dashboard shows its empty state", async () => {
    // Milestones in the answer do not matter: a none source hides the widget.
    delivery = async () => answer({ source: { kind: "none", reason: "no_source" }, planName: null });
    renderDashboard();
    expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
    expect(screen.queryByTestId("dashboard-delivery")).toBeNull();
  });

  it("DSH-FR-YAZR: hidden while the plan holds no milestone", async () => {
    recent = [{ id: "a.md", name: "a.md", kind: "markdown", modifiedAt: "2026-11-14T09:00:00.000Z" }];
    delivery = async () => answer({ milestones: [] });
    renderDashboard();
    await screen.findByText("a.md");
    await waitFor(() => expect(deliveryCalls()).toHaveLength(1));
    await waitFor(() => expect(screen.queryByTestId("dashboard-delivery")).toBeNull());
  });

  it("DSH-FR-17: a slow delivery read leaves its own widget loading while the others render", async () => {
    recent = [{ id: "a.md", name: "a.md", kind: "markdown", modifiedAt: "2026-11-14T09:00:00.000Z" }];
    delivery = () => new Promise(() => {});
    renderDashboard();
    expect(await screen.findByText("a.md")).toBeInTheDocument();
    expect(within(await widget()).getByText("Loading…")).toBeInTheDocument();
  });
});

describe("Dashboard — Delivery widget: content", () => {
  it("DSH-FR-OUPX: the header shows DELIVERY, the plan name, and the count line with the blocked work items", async () => {
    renderDashboard();
    const el = await widget();
    expect(await within(el).findByText("Delivery")).toBeInTheDocument();
    expect(within(el).getByText("· Payments v2")).toBeInTheDocument();
    expect(within(el).getByText("2 of 5 milestones done · 4 blocked")).toBeInTheDocument();
  });

  it("DSH-FR-OUPX: every milestone done or dropped reads All milestones done", async () => {
    delivery = async () =>
      answer({
        milestones: [
          milestone("a", "Alpha", "2026-09-30", { state: "done" }),
          milestone("b", "Beta", "2026-10-30", { state: "dropped" }),
        ],
      });
    renderDashboard();
    const el = await widget();
    expect(await within(el).findByText("All milestones done")).toBeInTheDocument();
  });

  it("DSH-FR-NNWM: a milestone shows its state glyph and word, name, due date, leaf bar, and done/total", async () => {
    renderDashboard();
    const card = await screen.findByTestId("delivery-milestone-m-beta");
    expect(within(card).getByText("◑")).toBeInTheDocument();
    expect(within(card).getByText("In progress:")).toBeInTheDocument();
    expect(within(card).getByText("Beta")).toBeInTheDocument();
    expect(within(card).getByText("2027-03-31")).toBeInTheDocument();
    expect(within(card).getByText("7/19")).toBeInTheDocument();
    const bar = within(card).getByRole("img");
    expect(bar).toHaveAttribute("aria-label", "7 done, 5 in progress, 1 drafting, 6 not started, 0 dropped");
    // MDL-FR-GDUP order; a state with no leaf has no segment.
    expect([...bar.children].map((s) => s.getAttribute("data-state"))).toEqual([
      "done",
      "in_progress",
      "drafting",
      "not_started",
    ]);
  });

  it("DSH-FR-NNWM: a due date before today on a milestone that is not done is marked as past", async () => {
    delivery = async () =>
      answer({
        milestones: [
          milestone("late", "Late", "2026-10-01", { state: "in_progress", current: true }),
          milestone("ok", "Done early", "2026-10-01", { state: "done" }),
        ],
      });
    renderDashboard();
    const late = await screen.findByTestId("delivery-milestone-late");
    expect(within(late).getByText("· past due")).toBeInTheDocument();
    expect(late.querySelector(".dash-delivery__due")).toHaveAttribute("data-past", "true");
    const ok = screen.getByTestId("delivery-milestone-ok");
    expect(within(ok).queryByText("· past due")).toBeNull();
  });

  it("DSH-FR-BBRC: the current milestone takes the accent border and the in progress · blocked line", async () => {
    renderDashboard();
    const beta = await screen.findByTestId("delivery-milestone-m-beta");
    expect(beta).toHaveAttribute("data-current", "true");
    expect(within(beta).getByText("5 in progress · 4 blocked")).toBeInTheDocument();
    const ga = screen.getByTestId("delivery-milestone-m-ga");
    expect(ga).not.toHaveAttribute("data-current");
    expect(within(ga).queryByText(/blocked/)).toBeNull();
  });

  it("DSH-FR-BBRC: the in progress · blocked line can break only after the middle dot", async () => {
    renderDashboard();
    const beta = await screen.findByTestId("delivery-milestone-m-beta");
    const line = beta.querySelector(".dash-delivery__current-line")!;
    // Each count keeps its words together; the one breakable space follows `·`.
    expect(line.textContent).toBe("5\u00a0in\u00a0progress\u00a0· 4\u00a0blocked");
  });

  it("DSH-FR-PLRL, DSH-FR-NZXN: dated milestones sit on the axis with month labels, and a Today line is labelled today", async () => {
    renderDashboard();
    const el = await widget();
    const scroller = await within(el).findByTestId("delivery-scroller");
    for (const id of ["m-alpha", "m-private", "m-beta", "m-ga"]) {
      expect(within(scroller).getByTestId(`delivery-milestone-${id}`)).toBeInTheDocument();
    }
    expect(within(scroller).getByText("Jan 2027")).toBeInTheDocument();
    expect(within(scroller).getByText("Mar")).toBeInTheDocument();
    expect(within(scroller).getByTestId("delivery-today")).toHaveTextContent("today");
  });

  it("DSH-FR-HVOQ: milestones with no due date sit under the axis in a labelled No date track", async () => {
    renderDashboard();
    const group = await screen.findByTestId("delivery-no-date");
    expect(within(group).getByText("No date")).toBeInTheDocument();
    expect(within(group).getByText("v2.1")).toBeInTheDocument();
    expect(within(group).getByText("0/3")).toBeInTheDocument();
    const scroller = screen.getByTestId("delivery-scroller");
    expect(scroller.compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(within(scroller).queryByText("v2.1")).toBeNull();
  });
});

describe("Dashboard — Delivery widget: scroll (DSH-FR-EWPI)", () => {
  let restore: () => void;
  const scrolls: number[] = [];
  beforeEach(() => {
    scrolls.length = 0;
    const width = Object.getOwnPropertyDescriptor(Element.prototype, "clientWidth");
    const left = Object.getOwnPropertyDescriptor(Element.prototype, "scrollLeft");
    Object.defineProperty(Element.prototype, "clientWidth", { configurable: true, get: () => 300 });
    Object.defineProperty(Element.prototype, "scrollLeft", {
      configurable: true,
      get: () => scrolls[scrolls.length - 1] ?? 0,
      set: (value: number) => scrolls.push(value),
    });
    restore = () => {
      if (width) Object.defineProperty(Element.prototype, "clientWidth", width);
      if (left) Object.defineProperty(Element.prototype, "scrollLeft", left);
    };
  });
  afterEach(() => restore());

  it("DSH-FR-EWPI: the first scroll shows the current milestone", async () => {
    renderDashboard();
    await screen.findByTestId("delivery-milestone-m-beta");
    const expected = initialScrollLeft(layoutTimeline(PLAN, "2026-11-14", 300), 300);
    expect(expected).toBeGreaterThan(0);
    await waitFor(() => expect(scrolls[scrolls.length - 1]).toBeCloseTo(expected));
  });

  it("DSH-FR-EWPI: a new answer after the author scrolled shows the current milestone again", async () => {
    renderDashboard();
    await screen.findByTestId("delivery-milestone-m-beta");
    const expected = initialScrollLeft(layoutTimeline(PLAN, "2026-11-14", 300), 300);
    await waitFor(() => expect(scrolls[scrolls.length - 1]).toBeCloseTo(expected));
    screen.getByTestId("delivery-scroller").scrollLeft = 0;
    await waitFor(() => expect((listeners["drafts-changed"] ?? []).length).toBeGreaterThan(0));
    emitEvent("drafts-changed");
    await waitFor(() => expect(deliveryCalls()).toHaveLength(2));
    await waitFor(() => expect(scrolls[scrolls.length - 1]).toBeCloseTo(expected));
  });

  it("DSH-FR-EWPI: the first scroll shows today where no milestone is current", async () => {
    const plan = PLAN.map((m) => ({ ...m, current: false }));
    delivery = async () => answer({ milestones: plan });
    renderDashboard();
    await screen.findByTestId("delivery-milestone-m-beta");
    const layout = layoutTimeline(plan, "2026-11-14", 300);
    expect(layout.focusX).toBe(layout.todayX);
    await waitFor(() => expect(scrolls[scrolls.length - 1]).toBeCloseTo(initialScrollLeft(layout, 300)));
  });
});

describe("Dashboard — Delivery widget: activation (DSH-FR-DFDW)", () => {
  it("DSH-FR-DFDW: activating a milestone asks for the Delivery lens with that milestone", async () => {
    const { onShowDeliveryItem } = renderDashboard();
    await userEvent.click(await screen.findByTestId("delivery-milestone-m-ga"));
    expect(onShowDeliveryItem).toHaveBeenCalledWith("m-ga");
    await userEvent.click(screen.getByTestId("delivery-milestone-m-v21"));
    expect(onShowDeliveryItem).toHaveBeenLastCalledWith("m-v21");
  });

  it("DSH-FR-DFDW: activating the header asks for the Delivery lens alone", async () => {
    const { onShowDeliveryItem } = renderDashboard();
    const el = await widget();
    await userEvent.click(await within(el).findByRole("button", { name: /Delivery/ }));
    expect(onShowDeliveryItem).toHaveBeenCalledWith(null);
  });
});

describe("Dashboard — Delivery widget: refresh, stale, and error", () => {
  it("DSH-FR-LGJT: five events read again with refresh false, and github polling changed with refresh true", async () => {
    renderDashboard();
    await screen.findByTestId("delivery-milestone-m-beta");
    const events = [
      "delivery-source-changed",
      "github-delivery-plan-changed",
      "drafts-changed",
      "graduation-run-changed",
      "dashboard-recently-edited-changed",
    ];
    await waitFor(() => expect(events.every((e) => (listeners[e] ?? []).length > 0)).toBe(true));
    for (const [index, name] of events.entries()) {
      emitEvent(name);
      await waitFor(() => expect(deliveryCalls()).toHaveLength(index + 2));
    }
    expect(deliveryCalls()).toEqual([false, false, false, false, false, false]);

    emitEvent("github-polling-changed", { newIssues: [] });
    await waitFor(() => expect(deliveryCalls()).toHaveLength(7));
    expect(deliveryCalls()[6]).toBe(true);
  });

  it("DSH-FR-18: a refresh keeps the milestones, marked as refreshing", async () => {
    renderDashboard();
    await screen.findByTestId("delivery-milestone-m-beta");
    let settle!: (v: DeliveryMilestones) => void;
    delivery = () => new Promise((res) => (settle = res));
    emitEvent("drafts-changed");
    const el = await widget();
    expect(await within(el).findByText("Refreshing…")).toBeInTheDocument();
    expect(el).toHaveAttribute("data-stale", "true");
    expect(within(el).getByText("Beta")).toBeInTheDocument();
    settle(answer({ planName: "Payments v3" }));
    expect(await within(el).findByText("· Payments v3")).toBeInTheDocument();
    expect(within(el).queryByText("Refreshing…")).toBeNull();
  });

  it("DSH-FR-YGWR: a stale answer shows its milestones with the stale marking and the time of the read", async () => {
    const readAt = new Date(2026, 10, 14, 9, 5).toISOString();
    delivery = async () => answer({ stale: true, readAt });
    renderDashboard();
    const el = await widget();
    expect(await within(el).findByTestId("delivery-stale")).toHaveTextContent("Stale · read 2026-11-14 09:05");
    expect(el).toHaveAttribute("data-stale", "true");
    expect(within(el).getByText("Beta")).toBeInTheDocument();
  });

  it("DSH-FR-YGWR: a stale answer with no read time is marked stale", async () => {
    delivery = async () => answer({ stale: true, readAt: null });
    renderDashboard();
    expect(await screen.findByTestId("delivery-stale")).toHaveTextContent(/^Stale$/);
  });

  it("DSH-FR-YGWR, DSH-FR-18: a refused load renders the error state with the error text", async () => {
    delivery = () =>
      Promise.reject({ code: "flow_invalid", message: "The delivery Flow file is not valid." });
    renderDashboard();
    const el = await widget();
    expect(await within(el).findByRole("alert")).toHaveTextContent(
      "The delivery could not be read. The delivery Flow file is not valid.",
    );
  });

  it("DSH-FR-YGWR: the log record of a refused load carries the code and not the error text", async () => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      delivery = () =>
        Promise.reject({ code: "flow_invalid", message: "The Flow file Secret plan is not valid." });
      renderDashboard();
      await vi.waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
      await vi.runOnlyPendingTimersAsync();
      const sent = JSON.stringify(invokeMock.mock.calls.filter((c) => c[0] === "append_log_records"));
      expect(sent).toContain("flow_invalid");
      expect(sent).not.toContain("Secret plan");
    } finally {
      vi.useRealTimers();
    }
  });

  it("DSH-FR-18: a refusal after an answer is shown, and the next answer clears it", async () => {
    renderDashboard();
    await screen.findByTestId("delivery-milestone-m-beta");
    delivery = () => Promise.reject("no_project_open");
    emitEvent("delivery-source-changed");
    const el = await widget();
    expect(await within(el).findByRole("alert")).toHaveTextContent("No project is open.");
    delivery = async () => answer();
    emitEvent("delivery-source-changed");
    expect(await within(el).findByText("Beta")).toBeInTheDocument();
    expect(within(el).queryByRole("alert")).toBeNull();
  });

  it("DSH-FR-19: an earlier read returning after a later one is discarded", async () => {
    const settle: ((v: DeliveryMilestones) => void)[] = [];
    delivery = () => new Promise((res) => settle.push(res));
    renderDashboard();
    await waitFor(() => expect(settle).toHaveLength(1));
    await waitFor(() => expect((listeners["drafts-changed"] ?? []).length).toBeGreaterThan(0));
    emitEvent("drafts-changed");
    await waitFor(() => expect(settle).toHaveLength(2));
    settle[1](answer({ planName: "Newer" }));
    expect(await screen.findByText("· Newer")).toBeInTheDocument();
    await act(async () => {
      settle[0](answer({ planName: "Older" }));
    });
    expect(screen.queryByText("· Older")).toBeNull();
    expect(screen.getByText("· Newer")).toBeInTheDocument();
  });

  it("DSH-FR-19: a read outstanding across a worktree change is discarded", async () => {
    const settle: ((v: DeliveryMilestones) => void)[] = [];
    delivery = () => new Promise((res) => settle.push(res));
    renderDashboard();
    await waitFor(() => expect(settle).toHaveLength(1));
    await waitFor(() => expect((listeners["worktree-context-changed"] ?? []).length).toBeGreaterThan(0));
    emitEvent("worktree-context-changed", { activeWorktreePath: "/p2" });
    await waitFor(() => expect(settle).toHaveLength(2));
    settle[1](answer({ planName: "Incoming" }));
    expect(await screen.findByText("· Incoming")).toBeInTheDocument();
    await act(async () => {
      settle[0](answer({ planName: "Outgoing" }));
    });
    expect(screen.queryByText("· Outgoing")).toBeNull();
    expect(screen.getByText("· Incoming")).toBeInTheDocument();
  });
});
