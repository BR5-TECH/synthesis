import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { FlowCanvas } from "./Flow";
import { FlowSessionStore } from "../state/flowSessions";
import { parseFlowDocument } from "../state/flowDocument";
import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import {
  callsOf,
  clearFlowStores,
  flowBackend,
  planOf,
  renderFlow,
} from "../test/flowCanvasHarness";
import { canvas, grip, ID, itemEl, panCanvas, panel, queryItem, selectItem } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { blocks, item, plan, relates, samplePlan } from "../test/flowPlanFixtures";

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
  clearFlowStores();
});

describe("opening a Flow", () => {
  it("FLO-FR-01, FLO-FR-03, FLO-FR-46: renders the items and edges of a body that the validation calls valid", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2")]));
    await renderFlow(b);

    const order = b.calls.map((c) => c.cmd);
    expect(order.indexOf("load_artifact_contents_by_id")).toBeLessThan(
      order.indexOf("validate_flow_document"),
    );
    for (const id of ["d1", "m1", "f1", "t1", "t2", "ep1", "f2", "t3", "x1"]) {
      expect(itemEl(id)).toBeInTheDocument();
    }
    expect(screen.getByTestId("flow-edge-path-e1")).toBeInTheDocument();
    expect(screen.getByTestId("flow-status")).toHaveTextContent("9 items · 1 edge");
  });

  it("FLO-FR-04: an empty body opens as an empty plan, ready to edit", async () => {
    const b = flowBackend("");
    await renderFlow(b);

    expect(canvas()).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByTestId("flow-status")).toHaveTextContent("0 items · 0 edges");
    expect(screen.getByRole("button", { name: "Add to plan" })).toBeEnabled();
  });

  it("FLO-FR-05: an invalid body shows its violations with their items, renders no plan, and writes nothing", async () => {
    const b = flowBackend(samplePlan());
    b.validation = {
      valid: false,
      violations: [
        { code: "invalid_parent_kind", message: "a task must sit in a feature", elementId: "t1" },
        { code: "dangling_edge_endpoint", message: "edge names no item", edgeId: "e9" },
      ],
    };
    const { flows } = await renderFlow(b);

    const alert = screen.getByRole("alert");
    expect(within(alert).getByText(/could not be read as a Flow/)).toBeInTheDocument();
    const list = screen.getByTestId("flow-violations");
    expect(list).toHaveTextContent("t1");
    expect(list).toHaveTextContent("a task must sit in a feature");
    expect(list).toHaveTextContent("e9");
    expect(canvas()).toBeNull();
    expect(screen.queryByRole("button", { name: "Add to plan" })).toBeNull();
    await act(async () => {
      await flows.flush(ID, { force: true });
    });
    expect(b.writes).toEqual([]);
  });

  it("FLO-FR-05: a version 1 document shows the error state with its unknown_version violation", async () => {
    const b = flowBackend(JSON.stringify({ version: 1, nodes: [], edges: [] }));
    b.validation = {
      valid: false,
      violations: [{ code: "unknown_version", message: "unknown Flow version 1" }],
    };
    await renderFlow(b);

    expect(screen.getByTestId("flow-violations")).toHaveTextContent("unknown Flow version 1");
    expect(canvas()).toBeNull();
  });

  it("FLO-FR-06, FLO-FR-14: an item with only a kind and a name renders and takes a connection", async () => {
    const b = flowBackend(plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", position: { x: 20, y: 60 } }), item("t2", "task", { parentId: "f1", position: { x: 220, y: 60 } })]));
    const { flows } = await renderFlow(b);

    expect(itemEl("t1")).toHaveAccessibleName("Task T1");
    fireEvent.pointerDown(within(itemEl("t1")).getByLabelText("Connect from T1"), { pointerId: 1, buttons: 1 });
    fireEvent.pointerUp(itemEl("t2"), { pointerId: 1 });
    expect(planOf(flows).edges).toEqual([{ id: "e1", from: "t1", to: "t2", type: "blocks" }]);
  });
});

describe("the band (FLO-FR-25)", () => {
  it("FLO-FR-25: the name and the description are edited in place and are part of the plan", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    const name = screen.getByRole("textbox", { name: "Flow name" });
    await userEvent.clear(name);
    await userEvent.type(name, "Payments v2");
    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "EU launch");
    expect(planOf(flows).name).toBe("Payments v2");
    expect(planOf(flows).description).toBe("EU launch");
  });

  it("FLO-FR-25: a missing name is marked, and the Flow still writes", async () => {
    const b = flowBackend(plan([], [], { name: "" }));
    const { flows } = await renderFlow(b);

    const name = screen.getByRole("textbox", { name: "Flow name" });
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByTestId("flow-name-required")).toHaveTextContent("Name required");
    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "x");
    await act(async () => {
      await flows.flush(ID);
    });
    expect(b.writes).toHaveLength(1);
  });

  it("FLO-FR-25: an empty description is absent from the document", async () => {
    const b = flowBackend(plan([], [], { description: "Old" }));
    const { flows } = await renderFlow(b);

    await userEvent.clear(screen.getByRole("textbox", { name: "Flow description" }));
    await act(async () => {
      await flows.flush(ID);
    });
    expect(JSON.parse(b.writes[0])).not.toHaveProperty("description");
  });
});

describe("unsaved changes and writes (FLO-FR-26, FLO-FR-27, FLO-FR-47)", () => {
  it("FLO-FR-26, FLO-FR-27: an edit makes the Flow dirty and writes it a short rest later", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "Q3");
    expect(flows.get(ID)!.dirty).toBe(true);
    await waitFor(() => expect(b.writes).toHaveLength(1), { timeout: 3000 });
    expect(flows.get(ID)!.dirty).toBe(false);
    const written = parseFlowDocument(b.writes[0]);
    expect(written.ok && written.doc.description).toBe("Q3");
  });

  it("FLO-FR-26, FLO-FR-22: selecting, collapsing, panning, and zooming are not edits", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    panCanvas(40, 30);
    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    await userEvent.click(screen.getByRole("button", { name: "Fit" }));
    expect(flows.get(ID)!.dirty).toBe(false);
    expect(flows.hasPendingWrite(ID)).toBe(false);
  });

  it("FLO-FR-47: a write the validation refuses lists every violation, keeps the plan dirty, and writes nothing", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);
    b.validation = {
      valid: false,
      violations: [{ code: "edge_within_lineage", message: "edge joins a lineage", edgeId: "e1" }],
    };

    await userEvent.type(screen.getByRole("textbox", { name: "Flow description" }), "x");
    let result: { ok: boolean } | undefined;
    await act(async () => {
      result = await flows.flush(ID);
    });
    expect(result?.ok).toBe(false);
    expect(b.writes).toEqual([]);
    expect(flows.get(ID)!.dirty).toBe(true);
    expect(screen.getByText("This Flow was not written.")).toBeInTheDocument();
    expect(screen.getByTestId("flow-violations")).toHaveTextContent("edge joins a lineage");
    expect(flows.hasPendingWrite(ID)).toBe(false);
  });
});

describe("the view (FLO-FR-22, FLO-FR-23)", () => {
  it("FLO-FR-22: the view controls zoom and fit, and a press on them neither pans nor clears the selection", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    selectItem("t1");
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    const viewport = document.querySelector(".flow-viewport") as HTMLElement;
    const before = viewport.style.transform;
    fireEvent.pointerDown(screen.getByRole("button", { name: "Zoom in" }), { buttons: 1 });
    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("110%");
    expect(viewport.style.transform).not.toBe(before);
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    await userEvent.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("100%");
  });

  it("FLO-FR-22: a press on empty canvas pans the view and clears the selection", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    selectItem("t1");
    const viewport = document.querySelector(".flow-viewport") as HTMLElement;
    panCanvas(50, 20);
    expect(viewport.style.transform).toContain("translate(74px, 44px)");
    expect(itemEl("t1")).toHaveAttribute("data-selected", "false");
  });

  it("FLO-FR-22: a reopened Flow shows the default view with every container expanded", async () => {
    const b = flowBackend(samplePlan());
    const first = await renderFlow(b);
    await userEvent.click(screen.getByRole("button", { name: "Collapse F1" }));
    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    first.view.unmount();

    render(<FlowCanvas flowId={ID} flows={first.flows} />);
    expect(screen.getByRole("button", { name: "Collapse F1" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("100%");
  });

  it("FLO-FR-23: a plan of several hundred items renders with no cap", async () => {
    const tasks = Array.from({ length: 300 }, (_, i) =>
      item(`t${i}`, "task", { parentId: "f1", position: { x: 20 + (i % 20) * 180, y: 60 + Math.floor(i / 20) * 120 } }),
    );
    const b = flowBackend(plan([item("f1", "feature", { size: { width: 3700, height: 1900 } }), ...tasks]));
    await renderFlow(b);

    expect(screen.getByTestId("flow-status")).toHaveTextContent("301 items");
    expect(itemEl("t299")).toBeInTheDocument();
  });
});

describe("the action control (FLO-FR-34, FLO-FR-35, FLO-FR-36)", () => {
  it("FLO-FR-34, FLO-FR-36: the tab carries the action control, bound to the Flow file", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    expect(within(canvas()).getByRole("button", { name: "Actions" })).toBeInTheDocument();
    await waitFor(() =>
      expect(callsOf(b, "list_discussions").some((a) => JSON.stringify(a).includes(ID))).toBe(true),
    );
  });

  it("FLO-FR-35, FLO-FR-07: the add menu and the action control share one transient surface", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    await userEvent.click(screen.getByRole("button", { name: "Add to plan" }));
    expect(screen.getByTestId("flow-add-menu")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Actions" }));
    expect(screen.queryByTestId("flow-add-menu")).toBeNull();
    expect(flows.get(ID)!.dirty).toBe(false);
  });
});

describe("description and name (FLO-FR-09, FLO-FR-13)", () => {
  it("FLO-FR-13: an unselected item renders its description as rich text, and the panel edits its Markdown source", async () => {
    const b = flowBackend(plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", description: "Use **bold** text", position: { x: 20, y: 60 } })]));
    const { flows } = await renderFlow(b);

    const rendered = screen.getByTestId("flow-item-description-t1");
    expect(within(rendered).getByText("bold").tagName).toBe("STRONG");
    selectItem("t1");
    const source = within(panel()).getByRole("textbox", { name: "Description" });
    expect(source).toHaveValue("Use **bold** text");
    expect(within(panel()).queryByRole("toolbar")).toBeNull();
    await userEvent.clear(source);
    await userEvent.type(source, "Plain");
    expect(planOf(flows).items[1].description).toBe("Plain");
  });

  it("FLO-FR-09: an item is renamed in place, and two items may share a name", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t1"));
    const field = within(itemEl("t1")).getByRole("textbox", { name: "Item name" });
    await userEvent.clear(field);
    await userEvent.type(field, "T2{Enter}");
    expect(planOf(flows).items.find((i) => i.id === "t1")!.name).toBe("T2");
    expect(within(itemEl("t1")).queryByRole("textbox", { name: "Item name" })).toBeNull();
    expect(screen.getAllByText("T2")).toHaveLength(2);
  });

  it("FLO-FR-09: the panel's Rename turns a container's name into a field", async () => {
    const b = flowBackend(samplePlan());
    await renderFlow(b);

    selectItem("f1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Rename" }));
    expect(within(grip("f1")).getByRole("textbox", { name: "Item name" })).toHaveValue("F1");
  });

  it("FLO-FR-44: selecting an item opens its panel and changes no item's size", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);
    const before = itemEl("f1").getAttribute("style");

    selectItem("f1");
    expect(panel()).toBeInTheDocument();
    expect(itemEl("f1").getAttribute("style")).toBe(before);
    expect(planOf(flows).items.find((i) => i.id === "f1")!.size).toEqual({ width: 420, height: 300 });
  });
});

describe("a plan with relates edges", () => {
  it("FLO-FR-15: a relates edge and a blocks edge both render with their own type", async () => {
    const b = flowBackend(samplePlan([blocks("e1", "t1", "t2"), relates("e2", "t1", "x1")]));
    await renderFlow(b);

    expect(screen.getByTestId("flow-edge-path-e1")).toHaveAttribute("data-type", "blocks");
    expect(screen.getByTestId("flow-edge-path-e2")).toHaveAttribute("data-type", "relates");
    expect(queryItem("e1")).toBeNull();
  });
});
