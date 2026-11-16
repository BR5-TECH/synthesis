/**
 * The No date track and the blocked marks of the Delivery widget
 * (`../../specifications/ui/DSH-dashboard.md` DSH-FR-NZXN, DSH-FR-HVOQ,
 * DSH-FR-NNWM, DSH-FR-OUPX). The widget renders a state it is given, so these
 * tests render it alone, with no backend.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { DeliveryWidget } from "./DashboardDelivery";
import { blocksFor, decl, sheet } from "../test/cssRules";
import { resetLogBufferForTest } from "../logging";
import type { DeliveryMilestone, DeliveryMilestones } from "../types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) =>
    cmd === "append_log_records" ? Promise.resolve() : Promise.reject(new Error(`unexpected ${cmd}`)),
}));

function milestone(itemId: string, due: string | null, extra: Partial<DeliveryMilestone> = {}): DeliveryMilestone {
  return {
    itemId,
    name: itemId.toUpperCase(),
    due,
    state: "not_started",
    leafCounts: { notStarted: 1, drafting: 0, inProgress: 0, done: 0, dropped: 0 },
    blockedLeaves: 0,
    current: false,
    ...extra,
  };
}

/** The dateless plan of the second wireframe in DSH-dashboard.md. */
const DATELESS: DeliveryMilestone[] = [
  milestone("m0", null, {
    state: "done",
    leafCounts: { notStarted: 0, drafting: 0, inProgress: 0, done: 4, dropped: 0 },
  }),
  milestone("m1", null, {
    state: "in_progress",
    leafCounts: { notStarted: 3, drafting: 0, inProgress: 0, done: 1, dropped: 0 },
    blockedLeaves: 3,
    current: true,
  }),
  milestone("m2", null),
  milestone("m3", null),
];

function renderWidget(milestones: DeliveryMilestone[]) {
  const data: DeliveryMilestones = {
    source: { kind: "file", flowId: "plans/workforce.flow", path: "plans/workforce.flow" },
    planName: "Workforce",
    milestones,
    readAt: null,
    stale: false,
  };
  const onShow = vi.fn<(itemId: string | null) => void>();
  render(
    <DeliveryWidget
      state={{ data, error: null, loading: false, stale: false }}
      today="2026-11-16"
      onShow={onShow}
    />,
  );
  return { onShow };
}

beforeEach(() => resetLogBufferForTest());
afterEach(cleanup);

describe("a plan with no dated milestone (DSH-FR-NZXN, DSH-FR-HVOQ)", () => {
  it("DSH-FR-NZXN: no axis and no Today line where no milestone has a due date", () => {
    renderWidget(DATELESS);
    expect(screen.queryByTestId("delivery-scroller")).toBeNull();
    expect(screen.queryByTestId("delivery-today")).toBeNull();
    expect(screen.queryByText("today")).toBeNull();
  });

  it("DSH-FR-HVOQ: the track is the whole content, in plan order, with no No date label", () => {
    renderWidget(DATELESS);
    const track = screen.getByTestId("delivery-no-date");
    expect(within(track).queryByText("No date")).toBeNull();
    const steps = within(track).getAllByRole("listitem");
    expect(steps.map((s) => within(s).getByRole("button").dataset.testid)).toEqual([
      "delivery-milestone-m0",
      "delivery-milestone-m1",
      "delivery-milestone-m2",
      "delivery-milestone-m3",
    ]);
  });

  it("DSH-FR-HVOQ, DSH-FR-DFDW: a milestone of the track opens it in the Map", async () => {
    const { onShow } = renderWidget(DATELESS);
    await userEvent.click(screen.getByTestId("delivery-milestone-m2"));
    expect(onShow).toHaveBeenCalledWith("m2");
  });

  it("DSH-FR-HVOQ: the first scroll of the track shows the current milestone", () => {
    // The current card sits 400px into the track on the screen; its own
    // offset parent is its step, so only the screen rectangles tell.
    const rect = vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
      const left = (this as HTMLElement).dataset?.current === "true" ? 450 : 50;
      return { left, x: left, top: 0, y: 0, right: left, bottom: 0, width: 0, height: 0, toJSON: () => ({}) };
    });
    const client = vi.spyOn(Element.prototype, "clientWidth", "get").mockReturnValue(300);
    const scroll = vi.spyOn(Element.prototype, "scrollWidth", "get").mockReturnValue(900);
    try {
      renderWidget(DATELESS);
      expect(screen.getByTestId("delivery-no-date-scroller").scrollLeft).toBe(300);
    } finally {
      rect.mockRestore();
      client.mockRestore();
      scroll.mockRestore();
    }
  });

  it("DSH-FR-HVOQ: under an axis the track carries the No date label", () => {
    renderWidget([milestone("ma", "2026-12-01"), milestone("mb", null)]);
    expect(screen.getByTestId("delivery-scroller")).toBeInTheDocument();
    const track = screen.getByTestId("delivery-no-date");
    expect(within(track).getByText("No date")).toBeInTheDocument();
    expect(within(track).getByTestId("delivery-milestone-mb")).toBeInTheDocument();
  });
});

describe("blocked work items (DSH-FR-NNWM, DSH-FR-OUPX)", () => {
  it("DSH-FR-NNWM: a dated milestone on the axis also shows its blocked mark", () => {
    renderWidget([milestone("ma", "2026-12-01", { blockedLeaves: 1 })]);
    expect(within(screen.getByTestId("delivery-scroller")).getByTestId("delivery-blocked-ma")).toBeInTheDocument();
  });

  it("DSH-FR-NNWM, DSH-FR-BBRC: the current milestone says its blocked work once, in its counts line", () => {
    renderWidget(DATELESS);
    expect(screen.queryByTestId("delivery-blocked-m1")).toBeNull();
    expect(within(screen.getByTestId("delivery-milestone-m1")).getByText(/0\sin\sprogress · 3\sblocked/)).toBeInTheDocument();
  });

  it("DSH-FR-NNWM: a milestone with a blocked work item shows ⊘ N blocked; one without shows no mark", () => {
    renderWidget([
      milestone("m1", null, { blockedLeaves: 2 }),
      milestone("m2", null),
    ]);
    expect(screen.getByTestId("delivery-blocked-m1")).toHaveTextContent(/^⊘\s2\sblocked$/);
    expect(screen.queryByTestId("delivery-blocked-m2")).toBeNull();
  });

  it("DSH-FR-OUPX: the count line ends in the sum of the blocked work items", () => {
    renderWidget(DATELESS);
    expect(screen.getByText("1 of 4 milestones done · 3 blocked")).toBeInTheDocument();
  });
});

describe("the track in the stylesheet (DSH-FR-HVOQ)", () => {
  const css = sheet("kit.css");
  it("DSH-FR-HVOQ: the track is one horizontal row that scrolls, not a column", () => {
    const [steps] = blocksFor(css, ".dash-delivery__steps");
    expect(decl(steps, "display")).toBe("flex");
    expect(decl(steps, "flex-direction")).toBeNull();
    const [scroller] = blocksFor(css, ".dash-delivery__track-scroller");
    expect(decl(scroller, "overflow-x")).toBe("auto");
  });

  it("DSH-FR-HVOQ: the widget's row grows to hold the axis and the track; its body never shrinks under them", () => {
    const [widget] = blocksFor(css, ".dash-delivery");
    expect(decl(widget, "min-height")).toBe("max-content");
    const [body] = blocksFor(css, ".dash-delivery__body");
    expect(decl(body, "flex")).toBe("none");
  });
});
