import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { findItem, serializeFlowDocument } from "../state/flowDocument";
import { resetToasts } from "../state/toasts";
import type { FlowDeliveryState } from "../types";
import {
  callsOf,
  clearFlowStores,
  flowBackend,
  itemState,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { itemEl, panel, selectItem } from "../test/flowFixtures";
import { emitEvent, resetListeners } from "../test/flowMocks";
import {
  blocks,
  issueBase,
  issueRef,
  item,
  milestoneRef,
  plan,
  samplePlan,
} from "../test/flowPlanFixtures";
import { DELIVERY_REST_MS } from "./Flow/useDeliveryState";
import { STATE_LOOK } from "./Flow/chips";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../test/flowMocks")).listen,
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
  clearFlowStores();
});

const chipOf = (id: string) => within(itemEl(id)).queryByTestId("flow-status-chip");

describe("the status chip (FLO-FR-KQCE)", () => {
  it("FLO-FR-KQCE: each item shows the state the delivery state gives it, in shape and in words", async () => {
    const b = flowBackend(samplePlan());
    b.delivery = {
      items: [
        itemState("t1", "not_started"),
        itemState("t2", "drafting"),
        itemState("t3", "in_progress"),
        itemState("x1", "done"),
        itemState("f2", "dropped"),
      ],
    };
    await renderFlow(b);

    await waitFor(() => expect(chipOf("t1")).not.toBeNull());
    const expected: Array<[string, keyof typeof STATE_LOOK]> = [
      ["t1", "not_started"],
      ["t2", "drafting"],
      ["t3", "in_progress"],
      ["x1", "done"],
      ["f2", "dropped"],
    ];
    for (const [id, state] of expected) {
      const chip = chipOf(id)!;
      expect(chip).toHaveAttribute("data-state", state);
      expect(chip).toHaveTextContent(`${STATE_LOOK[state].shape}${STATE_LOOK[state].word}`);
    }
    expect(chipOf("d1")).toBeNull();
  });

  it("FLO-FR-KQCE: an item with no answer yet renders no chip, and the canvas is usable before the answer", async () => {
    const b = flowBackend(samplePlan());
    b.on.load_flow_delivery_state = () => new Promise(() => {});
    const { flows } = await renderFlow(b);

    expect(chipOf("t1")).toBeNull();
    selectItem("t1");
    await userEvent.type(within(panel()).getByRole("textbox", { name: "Description" }), "x");
    expect(findItem(planOf(flows), "t1")!.description).toBe("x");
  });
});

describe("the Blocked marker (FLO-FR-IKMM)", () => {
  it("FLO-FR-IKMM: a blocked item carries a Blocked marker whose tooltip names its blockers", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "x1", "t2")]));
    b.delivery = {
      items: [itemState("t2", "drafting", { blocked: true, blockedBy: ["t1", "x1"] })],
    };
    await renderFlow(b);

    const marker = await within(itemEl("t2")).findByRole("button", { name: /^Blocked by / });
    expect(marker).toHaveAttribute("title", "Blocked by T1, X1");
    expect(marker).toHaveTextContent("Blocked");
    expect(within(itemEl("t1")).queryByRole("button", { name: /^Blocked by / })).toBeNull();
  });

  it("FLO-FR-IKMM: activating the marker selects the blocking items and changes nothing", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), blocks("e2", "x1", "t2")]));
    b.delivery = {
      items: [itemState("t2", "drafting", { blocked: true, blockedBy: ["t1", "x1"] })],
    };
    const { flows } = await renderFlow(b);

    await userEvent.click(await within(itemEl("t2")).findByRole("button", { name: /^Blocked by / }));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    expect(itemEl("x1")).toHaveAttribute("data-selected", "true");
    expect(itemEl("t2")).toHaveAttribute("data-selected", "false");
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
    expect(b.calls.some((c) => /^(create|delete|set|start|cancel|save)_/.test(c.cmd))).toBe(false);
  });

  it("FLO-FR-IKMM: a blocked container carries the marker in its header", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "x1", "f2")]));
    b.delivery = { items: [itemState("f2", "not_started", { blocked: true, blockedBy: ["x1"] })] };
    await renderFlow(b);

    const head = itemEl("f2").querySelector(".flow-ctr__head") as HTMLElement;
    expect(await within(head).findByRole("button", { name: /^Blocked by X1/ })).toBeInTheDocument();
  });
});

describe("the Overlap marker (FLO-FR-KWFH)", () => {
  it("FLO-FR-KWFH: a leaf in an overlap carries the marker, naming the other item and the shared paths", async () => {
    const b = flowBackend(samplePlan());
    b.overlaps = { overlaps: [{ items: ["t1", "t3"], paths: ["src/pay.rs", "src/ui/"] }] };
    await renderFlow(b);

    const marker = await within(itemEl("t1")).findByRole("button", { name: /^Overlaps with / });
    expect(marker).toHaveAttribute("title", "Overlaps with T3 on src/pay.rs, src/ui/");
    expect(within(itemEl("t3")).getByRole("button", { name: /^Overlaps with T1/ })).toBeInTheDocument();
    expect(within(itemEl("t2")).queryByRole("button", { name: /^Overlaps/ })).toBeNull();
    expect(screen.getByTestId("flow-status")).toHaveTextContent("1 overlap");
  });

  it("FLO-FR-KWFH: activating the marker selects the other items of the overlap", async () => {
    const b = flowBackend(samplePlan());
    b.overlaps = {
      overlaps: [
        { items: ["t1", "t3"], paths: ["src/a"] },
        { items: ["x1", "t1"], paths: ["src/b"] },
      ],
    };
    await renderFlow(b);

    await userEvent.click(await within(itemEl("t1")).findByRole("button", { name: /^Overlaps with T3, X1/ }));
    expect(itemEl("t3")).toHaveAttribute("data-selected", "true");
    expect(itemEl("x1")).toHaveAttribute("data-selected", "true");
  });
});

describe("when the canvas asks for the state (FLO-FR-STUN)", () => {
  it("FLO-FR-STUN: the tab asks for the delivery state and the overlaps with the serialized plan when it opens", async () => {
    const doc = samplePlan([blocks("e1", "t1", "t2")]);
    const b = flowBackend(doc);
    await renderFlow(b);

    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(1));
    expect(callsOf(b, "load_flow_delivery_state")[0]).toEqual({ body: serializeFlowDocument(doc) });
    expect(callsOf(b, "analyze_flow_overlap")).toEqual([{ body: serializeFlowDocument(doc) }]);
  });

  it("FLO-FR-STUN: the tab asks again a short rest after the last edit, with the edited plan", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);
    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(1));
    vi.useFakeTimers();

    selectItem("t1");
    const field = within(panel()).getByRole("textbox", { name: "Description" });
    fireEvent.change(field, { target: { value: "a" } });
    act(() => vi.advanceTimersByTime(DELIVERY_REST_MS - 50));
    fireEvent.change(field, { target: { value: "ab" } });
    act(() => vi.advanceTimersByTime(DELIVERY_REST_MS - 50));
    expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(1);
    act(() => vi.advanceTimersByTime(60));
    vi.useRealTimers();
    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(2));
    expect(callsOf(b, "load_flow_delivery_state")[1]).toEqual({
      body: serializeFlowDocument(planOf(flows)),
    });
    expect(callsOf(b, "analyze_flow_overlap")).toHaveLength(2);
  });

  it("FLO-FR-STUN: a \"drafts changed\" or a \"graduation run changed\" event asks again", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);
    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(1));

    act(() => emitEvent("drafts-changed"));
    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(2));
    act(() => emitEvent("graduation-run-changed", { runId: "r1" }));
    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state")).toHaveLength(3));
    expect(callsOf(b, "analyze_flow_overlap")).toHaveLength(3);
  });

  it("FLO-FR-STUN: a newer answer replaces an older one, and an older answer that arrives last is discarded", async () => {
    const b = flowBackend(samplePlan());
    const pending: Array<(s: FlowDeliveryState) => void> = [];
    b.on.load_flow_delivery_state = () => new Promise((res) => pending.push(res));
    await renderFlow(b);
    await waitFor(() => expect(pending).toHaveLength(1));
    act(() => emitEvent("drafts-changed"));
    await waitFor(() => expect(pending).toHaveLength(2));

    await act(async () => pending[1]({ items: [itemState("t1", "done")] }));
    expect(chipOf("t1")).toHaveAttribute("data-state", "done");
    await act(async () => pending[0]({ items: [itemState("t1", "not_started")] }));
    expect(chipOf("t1")).toHaveAttribute("data-state", "done");
  });

  it("FLO-FR-STUN: an answer for a body that an edit superseded is discarded", async () => {
    const b = flowBackend(samplePlan());
    const pending: Array<(s: FlowDeliveryState) => void> = [];
    b.on.load_flow_delivery_state = () => new Promise((res) => pending.push(res));
    await renderFlow(b);
    await waitFor(() => expect(pending).toHaveLength(1));

    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "x");
    await act(async () => pending[0]({ items: [itemState("t1", "done")] }));
    expect(chipOf("t1")).toBeNull();
  });
});

describe("milestones, local-only items, and tracker tags", () => {
  const milestones = () =>
    plan([
      item("d1", "deliverable", { size: { width: 1400, height: 500 } }),
      item("m1", "milestone", { parentId: "d1", due: "2000-01-31", position: { x: 20, y: 60 } }),
      item("m2", "milestone", { parentId: "d1", due: "2000-01-31", position: { x: 700, y: 60 } }),
    ]);

  it("FLO-FR-XAPK: a milestone shows its due date, marked as past when it is before today and not done", async () => {
    const b = flowBackend(milestones());
    b.delivery = { items: [itemState("m1", "in_progress"), itemState("m2", "done")] };
    await renderFlow(b);
    await waitFor(() => expect(chipOf("m1")).not.toBeNull());

    const past = within(itemEl("m1")).getByTestId("flow-due");
    expect(past).toHaveAttribute("data-past", "true");
    expect(past).toHaveTextContent("due 2000-01-31 · past");
    const done = within(itemEl("m2")).getByTestId("flow-due");
    expect(done).toHaveAttribute("data-past", "false");
    expect(done).toHaveTextContent("due 2000-01-31");
  });

  it("FLO-FR-XAPK: a selected milestone offers a Due field that stores a date or nothing", async () => {
    const b = flowBackend(milestones());
    const { flows } = await renderFlow(b);

    selectItem("m1");
    const due = within(panel()).getByLabelText("Due");
    fireEvent.change(due, { target: { value: "2031-12-24" } });
    expect(findItem(planOf(flows), "m1")!.due).toBe("2031-12-24");
    fireEvent.change(due, { target: { value: "" } });
    expect(findItem(planOf(flows), "m1")!.due).toBeUndefined();
    selectItem("d1");
    expect(within(panel()).queryByLabelText("Due")).toBeNull();
  });

  it("FLO-FR-TZSU: a selected item offers Local only, and a local-only item carries the mark", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    expect(within(itemEl("t1")).queryByText("local only")).toBeNull();
    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("switch", { name: "Local only" }));
    expect(findItem(planOf(flows), "t1")!.localOnly).toBe(true);
    expect(within(itemEl("t1")).getByText("local only")).toBeInTheDocument();
    await userEvent.click(within(panel()).getByRole("switch", { name: "Local only" }));
    expect(findItem(planOf(flows), "t1")!.localOnly).toBeUndefined();
  });

  it("FLO-FR-ULEM: a linked item carries a tracker tag naming the repository and the number; activating it opens the link", async () => {
    const doc = samplePlan();
    doc.sync = {
      tracker: "github",
      links: [
        { itemId: "t1", ref: issueRef(51), base: issueBase("T1") },
        { itemId: "m1", ref: milestoneRef(3), base: { name: "M1", description: "", state: "open" } },
      ],
    };
    const b = flowBackend(doc);
    b.on.open_tracker_link = () => undefined;
    await renderFlow(b);

    const tag = within(itemEl("t1")).getByRole("button", { name: "Open acme/payments#51 on GitHub" });
    expect(tag).toHaveTextContent("#51");
    const milestoneTag = within(itemEl("m1")).getByRole("button", { name: "Open acme/payments milestone 3 on GitHub" });
    expect(milestoneTag).toHaveTextContent("milestone 3");
    await userEvent.click(tag);
    expect(callsOf(b, "open_tracker_link")).toEqual([{ url: "https://github.com/acme/payments/issues/51" }]);
    expect(within(itemEl("t2")).queryByRole("button", { name: /on GitHub$/ })).toBeNull();
  });
});
