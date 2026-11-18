/**
 * The Flow canvas with a plan binding: a plan the host holds, the GitHub plan
 * of the Delivery lens (`../../specifications/ui/MDL-map-delivery-lens.md`
 * MDL-FR-XJBV, MDL-FR-PUCM, MDL-FR-XQGD, MDL-FR-KKYH, MDL-FR-NTVL, and
 * `FLO-flow.md` FLO-FR-ULEM, FLO-FR-21).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { resetLogBufferForTest } from "../logging";
import type { FlowDocument, StoredLayout } from "../state/flowDocument";
import { FlowLayoutStore } from "../state/flowLayouts";
import { resetToasts, visibleToasts } from "../state/toasts";
import type { TrackerFacts } from "../types";
import {
  FlowCanvas,
  type FlowPlanAnswer,
  type FlowPlanBinding,
  type FlowPlanChange,
  type FlowRemovalRequest,
} from "./Flow";
import { callsOf, flowBackend, itemState, type FlowBackend } from "../test/flowCanvasHarness";
import {
  clickItem,
  connectItems,
  dragItem,
  HLD_ID,
  itemEl,
  descriptionSource,
  panel,
  queryIssueView,
  selectItem,
} from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { blocks, item, plan, sampleLayout, samplePlan } from "../test/flowPlanFixtures";

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
  cleanup();
});

const SCOPE = "github:PVT_1";

function facts(itemId: string, number: number, kind: "issue" | "milestone" = "issue"): TrackerFacts {
  return {
    itemId,
    ref: {
      kind,
      number,
      url: `https://github.com/acme/payments/${kind === "issue" ? "issues" : "milestone"}/${number}`,
    },
    state: "open",
    closeReason: null,
    updatedAt: `at-${itemId}`,
    status: null,
    claimEligible: false,
    externalBlockers: [],
  };
}

const TRACKER = [facts("m1", 3, "milestone"), facts("t1", 51), facts("t2", 53), facts("f1", 42)];

interface Host {
  changes: FlowPlanChange[];
  removals: FlowRemovalRequest[];
  /** Answer the oldest edit that waits, with the plan it gives or a refusal. */
  answer: (a?: FlowPlanAnswer) => void;
  backend: FlowBackend;
}

/**
 * Render the canvas as the Delivery lens hosts it: the host holds the plan,
 * renders each edit at once, and answers it later.
 */
function renderPlan(
  doc: FlowDocument,
  opts: {
    layout?: StoredLayout;
    pending?: Set<string>;
    itemErrors?: Record<string, string>;
    extra?: Partial<React.ComponentProps<typeof FlowCanvas>>;
    binding?: Partial<FlowPlanBinding>;
  } = {},
): Host {
  const backend = flowBackend("", opts.layout ?? sampleLayout());
  backend.on.open_tracker_link = () => undefined;
  const waiting: Array<{ change: FlowPlanChange; done: (a: FlowPlanAnswer) => void }> = [];
  const host: Host = {
    changes: [],
    removals: [],
    answer: (a) => {
      const next = waiting.shift();
      next?.done(a ?? { ok: true, document: next.change.next });
    },
    backend,
  };
  function Lens() {
    const [plan, setPlan] = useState(doc);
    const binding: FlowPlanBinding = {
      kind: "plan",
      scope: SCOPE,
      document: plan,
      states: [itemState("t1", "in_progress")],
      overlaps: [],
      tracker: TRACKER,
      repository: "acme/payments",
      pending: opts.pending,
      itemErrors: opts.itemErrors,
      onEdit: (change) => {
        host.changes.push(change);
        setPlan(change.next);
        return new Promise((done) => waiting.push({ change, done }));
      },
      onRemoveRequest: (r) => host.removals.push(r),
      ...opts.binding,
    };
    return <FlowCanvas binding={binding} {...opts.extra} />;
  }
  render(<Lens />);
  return host;
}

const settle = () => waitFor(() => expect(screen.getByTestId("flow-status")).toBeInTheDocument());

describe("the plan binding (MDL-FR-XJBV)", () => {
  it("MDL-FR-XJBV, FLO-FR-STUN: the canvas renders the host's plan, its states, and asks the backend for no plan", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    expect(itemEl("t1")).toBeInTheDocument();
    expect(within(itemEl("t1")).getByTestId("flow-status-chip")).toHaveTextContent("In progress");
    for (const cmd of [
      "load_artifact_contents_by_id",
      "validate_flow_document",
      "load_flow_delivery_state",
      "analyze_flow_overlap",
    ]) {
      expect(callsOf(host.backend, cmd), cmd).toEqual([]);
    }
  });

  it("MDL-FR-XJBV: a connection goes to the host as an add_edge with the blocked end's updated_at, and writes nothing", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    connectItems("t1", "t2");
    expect(host.changes).toHaveLength(1);
    expect(host.changes[0].edits).toEqual([
      { kind: "add_edge", from: "t1", to: "t2", type: "blocks", updatedAt: "at-t2" },
    ]);
    expect(host.changes[0].itemIds.sort()).toEqual(["t1", "t2"]);
    expect(callsOf(host.backend, "save_artifact_contents")).toEqual([]);
  });

  it("MDL-FR-XJBV: a drop into another container is a move edit, and the place is stored in the plan's scope", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    dragItem("t1", { x: 70, y: 190 }, { x: 620, y: 300 });
    expect(host.changes.map((c) => c.edits)).toEqual([
      [{ kind: "move", itemId: "t1", containerId: "f2", updatedAt: "at-t1" }],
    ]);
    const saves = callsOf(host.backend, "save_delivery_layout");
    expect(saves).toHaveLength(1);
    expect(saves[0].scope).toBe(SCOPE);
  });

  it("FLO-FR-21, FLO-FR-TDHR: a drag inside a container is no edit, and stores the layout once", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    dragItem("t1", { x: 70, y: 190 }, { x: 80, y: 220 }, 4);
    expect(host.changes).toEqual([]);
    expect(callsOf(host.backend, "save_delivery_layout")).toHaveLength(1);
  });

  it("MDL-FR-XWGP: an edit made before the host answers the last one starts from the plan that edit gave", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    connectItems("t1", "t2");
    connectItems("t2", "t3");
    expect(host.changes.map((c) => c.edits.map((e) => e.kind))).toEqual([["add_edge"], ["add_edge"]]);
    expect(host.changes[1].next.edges).toHaveLength(2);
  });

  it("MDL-FR-XJBV: a rename is one edit when the field closes, not one per key", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t1"));
    const field = screen.getByRole("textbox", { name: "Item name" });
    await userEvent.clear(field);
    await userEvent.type(field, "Tokenize");
    expect(host.changes).toEqual([]);
    fireEvent.keyDown(field, { key: "Enter" });
    expect(host.changes.map((c) => c.edits)).toEqual([
      [{ kind: "rename", itemId: "t1", name: "Tokenize", updatedAt: "at-t1" }],
    ]);
  });

  it("MDL-FR-XJBV: Escape ends a rename with no edit", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t1"));
    const field = screen.getByRole("textbox", { name: "Item name" });
    await userEvent.type(field, "x");
    fireEvent.keyDown(field, { key: "Escape" });
    expect(host.changes).toEqual([]);
    expect(screen.getByTestId("flow-item-name-t1")).toHaveTextContent("T1");
  });

  it("MDL-FR-XJBV: a description is one edit when its field loses focus", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    selectItem("t1");
    const source = descriptionSource();
    await userEvent.type(source, "Use **Visa**");
    expect(host.changes).toEqual([]);
    fireEvent.blur(source);
    expect(host.changes.map((c) => c.edits)).toEqual([
      [{ kind: "set_description", itemId: "t1", description: "Use **Visa**", updatedAt: "at-t1" }],
    ]);
  });

  it("MDL-FR-XJBV, FLO-FR-13: Done makes the one edit of the typed text and shows it as rich text", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    selectItem("t1");
    await userEvent.type(descriptionSource(), "Use **Visa**");
    await userEvent.click(within(panel()).getByRole("button", { name: "Done editing the description" }));
    expect(host.changes.map((c) => c.edits)).toEqual([
      [{ kind: "set_description", itemId: "t1", description: "Use **Visa**", updatedAt: "at-t1" }],
    ]);
    expect(within(panel()).queryByRole("textbox", { name: "Description" })).toBeNull();
  });

  it("GDP-FR-EJTS: the kind choice offers no change to or from a milestone", async () => {
    renderPlan(samplePlan());
    await settle();

    selectItem("x1");
    const kinds = within(within(panel()).getByRole("combobox", { name: "Kind" }))
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    expect(kinds).not.toContain("milestone");
    selectItem("m1");
    const own = within(within(panel()).getByRole("combobox", { name: "Kind" }))
      .getAllByRole("option")
      .map((o) => (o as HTMLOptionElement).value);
    expect(own).toEqual(["milestone"]);
  });
});

describe("pending edits and refusals (MDL-FR-PUCM)", () => {
  it("MDL-FR-PUCM: an item with a pending edit carries the pending mark", async () => {
    renderPlan(samplePlan(), { pending: new Set(["t1", "f1"]) });
    await settle();

    expect(itemEl("t1")).toHaveAttribute("data-pending", "true");
    expect(within(itemEl("t1")).getByTestId("flow-pending")).toBeInTheDocument();
    expect(itemEl("f1")).toHaveAttribute("data-pending", "true");
    expect(itemEl("t2")).toHaveAttribute("data-pending", "false");
  });

  it("MDL-FR-PUCM, MDL-FR-HNLP: the host's error text shows on the item", async () => {
    renderPlan(samplePlan(), { itemErrors: { t2: "This item changed on GitHub." } });
    await settle();

    expect(screen.getByTestId("flow-item-error-t2")).toHaveTextContent("This item changed on GitHub.");
  });

  it("MDL-FR-PUCM: an edit renders at once, before the host answers", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    connectItems("t1", "t2");
    expect(screen.getByTestId("flow-status")).toHaveTextContent("1 edge");
    await act(async () => host.answer());
  });
});

describe("removal in a plan binding (MDL-FR-XQGD)", () => {
  it("MDL-FR-XQGD: a removal is handed to the host with the count of items under it, and the canvas asks nothing", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    selectItem("m1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove" }));
    expect(screen.queryByTestId("flow-remove-confirm")).toBeNull();
    expect(host.removals).toEqual([{ itemId: "m1", name: "M1", kind: "milestone", count: 7 }]);
    expect(host.changes).toEqual([]);
  });
});

describe("tracker tags (FLO-FR-ULEM)", () => {
  it("FLO-FR-ULEM: each item carries a tag naming the repository and the number; activating it opens the link", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    const tag = within(itemEl("t1")).getByRole("button", { name: "Open acme/payments#51 on GitHub" });
    expect(tag).toHaveTextContent("#51");
    const milestoneTag = within(itemEl("m1")).getByRole("button", {
      name: "Open acme/payments milestone 3 on GitHub",
    });
    expect(milestoneTag).toHaveTextContent("milestone 3");
    await userEvent.click(tag);
    expect(callsOf(host.backend, "open_tracker_link")).toEqual([
      { url: "https://github.com/acme/payments/issues/51" },
    ]);
  });

  it("FLO-FR-ULEM: a tracker link that does not open says why", async () => {
    const host = renderPlan(samplePlan());
    host.backend.on.open_tracker_link = () =>
      Promise.reject({ code: "tracker_url_refused", message: "The link is not a page of the tracker." });
    await settle();

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: /#51 on GitHub/ }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(callsOf(host.backend, "open_tracker_link")).toHaveLength(1);
    expect(visibleToasts()[0]).toMatchObject({
      level: "Error",
      title: "Link not opened",
      body: "The link is not a page of the tracker.",
    });
  });
});

describe("the band and the chrome of a plan binding (MDL-FR-KKYH, MDL-FR-NTVL)", () => {
  it("MDL-FR-KKYH: the plan name and description are read-only, and no name is marked as missing", async () => {
    const doc = { ...samplePlan(), name: "", description: "From GitHub" };
    const host = renderPlan(doc);
    await settle();

    const band = within(screen.getByTestId("flow-band"));
    const name = band.getByRole("textbox", { name: "Flow name" });
    expect(name).toHaveAttribute("readonly");
    expect(band.getByRole("textbox", { name: "Flow description" })).toHaveAttribute("readonly");
    expect(screen.queryByTestId("flow-name-required")).toBeNull();
    fireEvent.change(name, { target: { value: "Renamed" } });
    expect(host.changes).toEqual([]);
  });

  it("MDL-FR-KKYH, FLO-FR-YDWT: the plan's references are one set_plan_references edit", async () => {
    const host = renderPlan(samplePlan());
    await settle();

    await waitFor(() =>
      expect(within(screen.getByTestId("flow-band")).getByRole("option", { name: /payments-hld/ })).toBeInTheDocument(),
    );
    fireEvent.change(within(screen.getByTestId("flow-band")).getByRole("combobox", { name: "Add HLD" }), {
      target: { value: HLD_ID },
    });
    expect(host.changes.map((c) => c.edits)).toEqual([
      [{ kind: "set_plan_references", references: [HLD_ID] }],
    ]);
  });

  it("MDL-FR-NTVL: a plan binding carries no action control and no discussions control", async () => {
    renderPlan(samplePlan());
    await settle();

    expect(screen.queryByRole("button", { name: "Actions" })).toBeNull();
    expect(document.querySelector(".flow-actionhost")).toBeNull();
  });

  it("MDL-FR-KKYH, SDL-FR-KNOP, MDL-FR-PUKL, MIV-FR-VHDP: the host's band, issue view, and add-menu rows render in place", async () => {
    renderPlan(samplePlan(), {
      extra: {
        board: true,
        bandExtras: <button type="button">Refresh</button>,
        issueView: {
          parts: (item) => ({ specs: <div data-testid="host-row">Specs of {item.name}</div> }),
        },
        addMenuExtras: (close) => (
          <button type="button" role="menuitem" onClick={close}>
            Add existing issue
          </button>
        ),
      },
    });
    await settle();

    expect(within(screen.getByTestId("flow-band")).getByRole("button", { name: "Refresh" })).toBeInTheDocument();
    selectItem("t1");
    expect(within(panel()).getByTestId("host-row")).toHaveTextContent("Specs of T1");
    await userEvent.click(screen.getByRole("button", { name: "Add to plan" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Add existing issue" }));
    expect(screen.queryByTestId("flow-add-menu")).toBeNull();
  });

  it("MIV-FR-TTZU, DBR-FR-WKNI: a click on a ghost card selects it and opens no issue view; a live card opens one", async () => {
    renderPlan(samplePlan(), {
      extra: { board: true },
      binding: { proposal: { marks: new Map(), ghosts: new Map([["t1", { kindKnown: true }]]), reviewed: null } },
    });
    await settle();
    clickItem("t1");
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    expect(queryIssueView()).toBeNull();
    clickItem("t2");
    expect(await screen.findByTestId("map-issue-view")).toHaveAttribute("data-item", "t2");
  });

  it("MIV-FR-HHQR: while the host keeps the canvas inert, the issue view opens and every edit in it is disabled", async () => {
    renderPlan(samplePlan(), { extra: { board: true, inert: true } });
    await settle();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    for (const name of ["Rename", "Edit the description", "Remove"]) {
      expect(within(view).getByRole("button", { name })).toBeDisabled();
    }
    expect(within(view).getByRole("combobox", { name: "Kind" })).toBeDisabled();
    expect(within(view).getByRole("combobox", { name: "Add path" })).toBeDisabled();
    expect(within(view).getByRole("button", { name: "Close" })).toBeEnabled();
  });

  it("FLO-FR-21: a host that gives a layout store keeps the layout in it", async () => {
    const layouts = new FlowLayoutStore();
    const host = renderPlan(samplePlan([blocks("e1", "t1", "t2")]), { binding: { layouts } });
    await settle();

    dragItem("t1", { x: 70, y: 190 }, { x: 80, y: 220 }, 4);
    expect(layouts.get(SCOPE)?.items.t1.position).toEqual({ x: 30, y: 90 });
    expect(host.changes).toEqual([]);
  });
});

describe("a host that has not rendered an edit yet (MDL-FR-XWGP)", () => {
  it("MDL-FR-XWGP: the next edit starts from the plan the last edit gave, and sends only its own edits", async () => {
    const changes: FlowPlanChange[] = [];
    flowBackend("", sampleLayout());
    const binding: FlowPlanBinding = {
      kind: "plan",
      scope: SCOPE,
      document: samplePlan(),
      states: null,
      overlaps: null,
      tracker: TRACKER,
      repository: "acme/payments",
      onEdit: (change) => {
        changes.push(change);
        return new Promise(() => {});
      },
      onRemoveRequest: () => {},
    };
    render(<FlowCanvas binding={binding} />);
    await settle();

    connectItems("t1", "t2");
    connectItems("t2", "t3");
    expect(changes).toHaveLength(2);
    expect(changes[1].edits).toEqual([
      { kind: "add_edge", from: "t2", to: "t3", type: "blocks", updatedAt: "" },
    ]);
    expect(changes[1].next.edges.map((e) => `${e.from}>${e.to}`)).toEqual(["t1>t2", "t2>t3"]);
  });

  it("MDL-FR-PUCM: an onEdit that throws is a refused edit: it is rolled back, and the next edit starts from the plan before it", async () => {
    flowBackend("", sampleLayout());
    const changes: FlowPlanChange[] = [];
    const onEdit = vi.fn<FlowPlanBinding["onEdit"]>((change) => {
      changes.push(change);
      if (changes.length === 1) throw new Error("lens gone");
      return new Promise(() => {});
    });
    const binding: FlowPlanBinding = {
      kind: "plan",
      scope: SCOPE,
      document: samplePlan(),
      states: null,
      overlaps: null,
      tracker: [],
      repository: "acme/payments",
      onEdit,
      onRemoveRequest: () => {},
    };
    render(<FlowCanvas binding={binding} />);
    await settle();

    const edgesBefore = samplePlan().edges.map((e) => `${e.from}>${e.to}`);
    connectItems("t1", "t2");
    expect(onEdit).toHaveBeenCalledTimes(1);
    await settle();
    // The refused edge is not on the canvas.
    expect(document.querySelectorAll('[data-testid^="flow-edge-path-"]')).toHaveLength(edgesBefore.length);
    // The next edit starts from the plan before the refused one.
    connectItems("t2", "t3");
    expect(onEdit).toHaveBeenCalledTimes(2);
    expect(changes[1].edits).toEqual([{ kind: "add_edge", from: "t2", to: "t3", type: "blocks", updatedAt: "" }]);
    expect(changes[1].next.edges.map((e) => `${e.from}>${e.to}`)).toEqual([...edgesBefore, "t2>t3"]);
  });
});

describe("a refused move (MDL-FR-PUCM, FLO-FR-21)", () => {
  it("MDL-FR-PUCM: a move the host refuses puts the item back where it was in its old container", async () => {
    const original = samplePlan();
    let restore!: () => void;
    function Lens() {
      const [plan, setPlan] = useState(original);
      restore = () => setPlan(original);
      const binding: FlowPlanBinding = {
        kind: "plan",
        scope: SCOPE,
        document: plan,
        states: null,
        overlaps: null,
        tracker: TRACKER,
        repository: "acme/payments",
        onEdit: async (change) => {
          setPlan(change.next);
          return { ok: false, error: "changed_on_tracker" };
        },
        onRemoveRequest: () => {},
      };
      return <FlowCanvas binding={binding} />;
    }
    flowBackend("", sampleLayout());
    render(<Lens />);
    await settle();

    // The milestone column m1 is at the origin (FLO-FR-SQUS); t1 is at (40, 120).
    dragItem("t1", { x: 50, y: 130 }, { x: 600, y: 240 });
    await act(async () => restore());
    await waitFor(() => expect(itemEl("t1").style.left).toBe("40px"));
    expect(itemEl("t1").style.top).toBe("120px");
  });
});

describe("a milestone column moved in a GitHub plan (FLO-FR-OGPF)", () => {
  it("FLO-FR-OGPF, MDL-FR-XJBV: a column dragged before another sends set_milestone_order with the new order, first to last", async () => {
    const doc = plan([item("m1", "milestone"), item("m2", "milestone"), item("m3", "milestone")]);
    const host = renderPlan(doc, { layout: { items: {} } });
    await settle();
    const left = (id: string) => Number.parseFloat(itemEl(id).style.left);
    dragItem("m3", { x: left("m3") + 30, y: 10 }, { x: left("m2") - 5, y: 10 });
    expect(host.changes).toHaveLength(1);
    expect(host.changes[0].edits).toEqual([{ kind: "set_milestone_order", milestoneIds: ["m1", "m3", "m2"] }]);
    expect(callsOf(host.backend, "save_delivery_layout")).toEqual([]);
  });
});
