/**
 * The links between the two lenses of the Map tab
 * (`../../../specifications/ui/SDL-specification-delivery-links.md`
 * SDL-FR-KNOP .. SDL-FR-NXXQ), and the **unplanned** toggle in the toolbar
 * of `SMP-specification-map.md` SMP-FR-CVIB and SMP-FR-ONSD.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { deliveryLensData } from "../../state/delivery/lensData";
import type { IndexNode, SpecNode, SpecificationIndex } from "../../state/specMap/types";
import { itemEl, panel, selectItem } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import { nodeButton, setView } from "../../test/specMapRender";
import { smallIndex, spec } from "../../test/specMapFixtures";
import {
  clearLensStores,
  collapsedLayout,
  deliveryBackend,
  expectCentred,
  githubAnswer,
  githubSource,
  leafState,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";
import type { GithubDeliveryPlan } from "../../types/delivery";

const A1 = "specifications/ui/A1-a1.md";
const A2 = "specifications/core/A2-a2.md";
const C1 = "specifications/ui/C1-c1.md";

const DOC = plan([
  item("m1", "milestone"),
  item("f1", "feature", { parentId: "m1", artifactIds: [C1] }),
  item("t1", "task", { parentId: "f1", artifactIds: [A1] }),
  item("t2", "task", { footprint: ["specifications/core/"] }),
  item("t3", "task"),
]);

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend(
    "",
    githubAnswer(DOC, { states: [leafState("t1", "in_progress"), leafState("f1", "in_progress"), leafState("m1", "in_progress")] }),
  );
});

afterEach(() => {
  cleanup();
  clearLensStores();
  vi.restoreAllMocks();
});

const lensRadio = (name: string) =>
  within(screen.getByRole("radiogroup", { name: "Lens" })).getByRole("radio", { name });

async function openDelivery(options: { index?: SpecificationIndex | (() => Promise<SpecificationIndex>) } = {}) {
  const r = await renderLens({ source: githubSource(), ...options });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

async function openSpecifications() {
  const r = await openDelivery();
  fireEvent.click(lensRadio("Specifications"));
  await screen.findByTestId("spec-map-canvas");
  return r;
}

describe("the Specs list of a delivery item (SDL-FR-KNOP, SDL-FR-BVNS, SDL-FR-VYIF)", () => {
  it("SDL-FR-KNOP: the panel of a selected item lists each linked spec node with its badge, label, and completeness dot", async () => {
    await openDelivery();
    selectItem("f1");
    const specs = await within(panel()).findByTestId("delivery-specs");
    const rows = within(specs).getAllByRole("button");
    expect(rows.map((r) => r.querySelector(".smap-code")?.textContent)).toEqual(["A1", "C1"]);
    expect(rows[0]).toHaveTextContent("a1 label");
    expect(rows[0].querySelector(".smap-dot")).toHaveAttribute("data-state", "verified");
  });

  it("SDL-FR-KNOP: the list is absent where the item has no spec link", async () => {
    await openDelivery();
    selectItem("t3");
    expect(within(panel()).queryByTestId("delivery-specs")).toBeNull();
  });

  it("SDL-FR-BVNS: a row switches to the Specifications lens, selects that spec node, shows the last level, and centres the node", async () => {
    const r = await openDelivery();
    selectItem("t1");
    fireEvent.click(within(await within(panel()).findByTestId("delivery-specs")).getByRole("button"));
    await screen.findByTestId("spec-map-canvas");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "true");
    expect(r.store.snapshot().selection).toEqual({ kind: "spec", code: "A1" });
    // SMI-FR-PRSL: level 3 is the last level; jsdom measures the canvas 0 wide.
    // The chip of A1 sits at (-150, -222), 96 by 22, so its centre is at
    // (-102, -211), and at the scale 0.85 the view moves it to the origin.
    await waitFor(() =>
      expect(r.store.snapshot().view).toEqual({
        level: 3,
        scale: 0.85,
        tx: expect.closeTo(86.7, 6),
        ty: expect.closeTo(179.35, 6),
        touched: true,
      }),
    );
  });

  it("SDL-FR-VYIF: the Delivery lens loads the index, and the Specs list shows a loading line until it answers", async () => {
    let answer!: (i: SpecificationIndex) => void;
    const r = await openDelivery({ index: () => new Promise((res) => (answer = res)) });
    expect(r.ops.load).toHaveBeenCalledTimes(1);
    selectItem("t1");
    expect(within(panel()).getByTestId("delivery-specs")).toHaveTextContent("Loading the specification index…");
    await act(async () => answer(smallIndex()));
    expect(within(within(panel()).getByTestId("delivery-specs")).getAllByRole("button")).toHaveLength(1);
  });
});

describe("the Delivered by list (SDL-FR-VAGU, SDL-FR-MCGI, SDL-FR-VYIF)", () => {
  it("SDL-FR-VAGU: a spec node lists each item that links it, in plan order, with its glyph, name, and status chip", async () => {
    const r = await openSpecifications();
    act(() => r.store.select({ kind: "spec", code: "C1" }));
    const list = await screen.findByTestId("spec-map-delivered-by");
    const rows = within(list).getAllByRole("button");
    expect(rows.map((row) => row.querySelector(".smap-row__label")?.textContent)).toEqual(["M1", "F1"]);
    expect(rows[0].querySelector(".flow-glyph")).toHaveTextContent("⚑");
    expect(within(rows[1]).getByTestId("flow-status-chip")).toHaveTextContent("In progress");
  });

  it("SDL-FR-VAGU: a leaf group, a branch, and a root list the items that link a spec node under them", async () => {
    const r = await openSpecifications();
    act(() => r.store.select({ kind: "index", id: "g1" }));
    const labels = () =>
      within(screen.getByTestId("spec-map-delivered-by"))
        .getAllByRole("button")
        .map((row) => row.querySelector(".smap-row__label")?.textContent);
    expect(labels()).toEqual(["M1", "F1", "T1", "T2"]);
    act(() => r.store.select({ kind: "index", id: "d2" }));
    expect(labels()).toEqual(["M1", "F1"]);
  });

  it("SDL-FR-VAGU: a subject that no item links says so", async () => {
    const r = await openSpecifications();
    act(() => r.store.select({ kind: "spec", code: "B1" }));
    expect(await screen.findByTestId("spec-map-delivered-by")).toHaveTextContent("No plan item delivers it yet.");
  });

  it("SDL-FR-MCGI: a row switches to the Delivery lens, selects that item, expands the containers that hold it, and centres it", async () => {
    b.layout = collapsedLayout(["m1", "f1"]);
    const r = await renderLens({ source: githubSource() });
    await screen.findByTestId("flow-item-m1");
    await settle();
    await waitFor(() => expect(itemEl("m1")).toHaveAttribute("data-collapsed", "true"));
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    act(() => r.store.select({ kind: "spec", code: "A1" }));
    fireEvent.click(within(await screen.findByTestId("spec-map-delivered-by")).getByRole("button", { name: /task T1/ }));
    expect(await screen.findByTestId("flow-item-t1")).toHaveAttribute("data-selected", "true");
    expect(lensRadio("Delivery")).toHaveAttribute("aria-checked", "true");
    expect(itemEl("m1")).toHaveAttribute("data-collapsed", "false");
    expect(itemEl("f1")).toHaveAttribute("data-collapsed", "false");
    expectCentred("t1");
  });

  it("SDL-FR-VYIF: the Specifications lens loads the plan when the session holds none, with a loading line meanwhile", async () => {
    let answer!: (a: GithubDeliveryPlan) => void;
    b.plan = () => new Promise((res) => (answer = res));
    const r = await renderLens({ source: githubSource() });
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    act(() => r.store.select({ kind: "spec", code: "A1" }));
    expect(await screen.findByTestId("spec-map-delivered-by")).toHaveTextContent("Loading the delivery plan…");
    await act(async () => answer(githubAnswer(DOC)));
    await waitFor(() => expect(within(screen.getByTestId("spec-map-delivered-by")).getAllByRole("button")).toHaveLength(3));
  });
});

describe("Plan work (SDL-FR-CFHH)", () => {
  it("SDL-FR-CFHH: a spec node offers Plan work, which adds a task named after it with its file, and shows it on the Delivery lens", async () => {
    b.apply = (edit) => {
      if (edit.kind === "create_item") {
        return githubAnswer({ ...DOC, items: [...DOC.items, { id: "new-1", kind: "task", name: String(edit.name) }] });
      }
      return githubAnswer({ ...DOC, items: [...DOC.items, { id: "new-1", kind: "task", name: "b1 label", artifactIds: ["specifications/ai/B1-b1.md"] }] });
    };
    const r = await openSpecifications();
    act(() => r.store.select({ kind: "spec", code: "B1" }));
    await userEvent.click(screen.getByRole("button", { name: "Plan work" }));
    expect(await screen.findByTestId("flow-item-new-1")).toBeInTheDocument();
    await waitFor(() => expect(itemEl("new-1")).toHaveAttribute("data-selected", "true"));
    const edits = b.calls.filter((c) => c.cmd === "apply_github_delivery_edit").map((c) => c.args.edit);
    expect(edits).toEqual([
      { kind: "create_item", itemKind: "task", name: "b1 label", containerId: null },
      { kind: "set_references", itemId: "new-1", artifactIds: ["specifications/ai/B1-b1.md"] },
    ]);
  });

  it("SDL-FR-CFHH: a leaf group asks for its label and the file of each spec node; a branch and a root offer no Plan work", async () => {
    const r = await openSpecifications();
    const request = vi.spyOn(r.store, "requestPlanWork");
    act(() => r.store.select({ kind: "index", id: "f1" }));
    expect(screen.queryByRole("button", { name: "Plan work" })).toBeNull();
    act(() => r.store.select({ kind: "index", id: "d1" }));
    expect(screen.queryByRole("button", { name: "Plan work" })).toBeNull();
    act(() => r.store.select({ kind: "index", id: "g1" }));
    fireEvent.click(screen.getByRole("button", { name: "Plan work" }));
    expect(request).toHaveBeenCalledWith("g1 label", [A1, A2]);
  });
});

describe("the planned mark and the unplanned toggle (SDL-FR-SGQM .. SDL-FR-VUQH, SMP-FR-CVIB)", () => {
  it("SDL-FR-SGQM: a linked spec chip carries the planned mark, and its hover card ends with the count of plan items", async () => {
    const r = await openSpecifications();
    setView(r.store, { level: 3 });
    expect(nodeButton("C1 c1 label")).toHaveAttribute("data-plan-linked", "true");
    expect(nodeButton("B1 b1 label")).not.toHaveAttribute("data-plan-linked");
    fireEvent.mouseOver(nodeButton("C1 c1 label"));
    expect(screen.getByRole("tooltip").querySelector(".smap-hover__meta")?.textContent).toMatch(/ · 2 plan items$/);
    fireEvent.mouseOver(nodeButton("A2 a2 label"));
    expect(screen.getByRole("tooltip").querySelector(".smap-hover__meta")?.textContent).toMatch(/ · 1 plan item$/);
  });

  it("SDL-FR-HZLB, SMP-FR-CVIB: the unplanned toggle follows gaps only, is off by default, and takes the token treatment of gaps only", async () => {
    await openSpecifications();
    const toolbar = screen.getByRole("toolbar", { name: "Map controls" });
    const filters = [...toolbar.querySelectorAll(".smap-filter")].map((f) => f.getAttribute("data-filter"));
    expect(filters.slice(-3)).toEqual(["gaps", "unplanned", "discussion"]);
    const toggle = screen.getByRole("button", { name: "Unplanned" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(toggle).toHaveClass("smap-filter");
  });

  it("SDL-FR-KXCY: unplanned keeps gap and drafted chips no item links at full opacity, and dims every other chip and node", async () => {
    // A4 is a gap that no item links, beside the built A3 in g2.
    const index = smallIndex();
    const g2 = (index.roots[0].children[0] as IndexNode).children[1] as IndexNode;
    (g2.children as SpecNode[]).push(spec("A4", "gap", 3));
    const r = await openDelivery({ index });
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    fireEvent.click(screen.getByRole("button", { name: "Unplanned" }));
    expect(nodeButton("d2 label, domain")).toHaveAttribute("data-dim", "unplanned");
    expect(nodeButton("d1 label, domain")).not.toHaveAttribute("data-dim");
    setView(r.store, { level: 3 });
    expect(nodeButton("B1 b1 label")).not.toHaveAttribute("data-dim");
    // A2 is a gap that t2's footprint covers, so it is planned.
    expect(nodeButton("A2 a2 label")).toHaveAttribute("data-dim", "unplanned");
    expect(nodeButton("A3 a3 label")).toHaveAttribute("data-dim", "unplanned");
    expect(nodeButton("g3 label, group")).not.toHaveAttribute("data-dim");
    expect(nodeButton("g1 label, group")).toHaveAttribute("data-dim", "unplanned");
    expect(nodeButton("A4 a4 label")).not.toHaveAttribute("data-dim");
    expect(nodeButton("g2 label, group")).not.toHaveAttribute("data-dim");
    expect(nodeButton("g4 label, group")).toHaveAttribute("data-dim", "unplanned");
  });

  it("SDL-FR-VUQH: turning unplanned on turns gaps only off, and the other way round", async () => {
    await openSpecifications();
    const gaps = screen.getByRole("button", { name: "Gaps only" });
    const unplanned = screen.getByRole("button", { name: "Unplanned" });
    fireEvent.click(gaps);
    fireEvent.click(unplanned);
    expect(gaps).toHaveAttribute("aria-pressed", "false");
    expect(unplanned).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(gaps);
    expect(gaps).toHaveAttribute("aria-pressed", "true");
    expect(unplanned).toHaveAttribute("aria-pressed", "false");
  });

  it("SMP-FR-CVIB, SMP-FR-ONSD: the unplanned toggle is disabled while the map load is in its error state", async () => {
    const r = await renderLens({ source: githubSource(), index: () => Promise.reject(new Error("boom")) });
    await screen.findByTestId("flow-item-t1");
    fireEvent.click(lensRadio("Specifications"));
    await waitFor(() => expect(r.store.snapshot().status).toBe("error"));
    expect(screen.getByRole("button", { name: "Unplanned" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });
});

describe("no delivery and new derivations (SDL-FR-CKAA, SDL-FR-NXXQ)", () => {
  it("SDL-FR-CKAA: with the source none there is no Delivered by, no planned mark, no unplanned toggle, and no Plan work", async () => {
    const r = await renderLens({ source: { kind: "none", reason: "no_source" } });
    await screen.findByTestId("spec-map-canvas");
    act(() => r.store.select({ kind: "spec", code: "A1" }));
    expect(screen.queryByTestId("spec-map-delivered-by")).toBeNull();
    expect(screen.queryByRole("button", { name: "Plan work" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Unplanned" })).toBeNull();
    setView(r.store, { level: 3 });
    expect(document.querySelector("[data-plan-linked]")).toBeNull();
    expect(b.calls.some((c) => c.cmd === "load_github_delivery_plan")).toBe(false);
  });

  it("SDL-FR-CKAA: a source that becomes none while unplanned is on removes the toggle and dims no chip", async () => {
    const r = await openSpecifications();
    fireEvent.click(screen.getByRole("button", { name: "Unplanned" }));
    setView(r.store, { level: 3 });
    expect(nodeButton("A3 a3 label")).toHaveAttribute("data-dim", "unplanned");
    act(() => r.store.setDeliverySource({ kind: "none", reason: "no_source" }));
    expect(screen.queryByRole("button", { name: "Unplanned" })).toBeNull();
    expect(document.querySelector('[data-dim="unplanned"]')).toBeNull();
    expect(document.querySelector("[data-plan-linked]")).toBeNull();
    act(() => r.store.select({ kind: "spec", code: "A1" }));
    expect(screen.queryByTestId("spec-map-delivered-by")).toBeNull();
    expect(screen.queryByRole("button", { name: "Plan work" })).toBeNull();
  });

  it("SDL-FR-NXXQ: a new plan derives the links again and changes no selection and no view", async () => {
    const r = await openSpecifications();
    setView(r.store, { level: 3, tx: 12, ty: 7 });
    act(() => r.store.select({ kind: "spec", code: "B1" }));
    expect(nodeButton("B1 b1 label")).not.toHaveAttribute("data-plan-linked");
    const view = r.store.snapshot().view;
    b.plan = () => githubAnswer({ ...DOC, items: [...DOC.items, { id: "t9", kind: "task", name: "T9", artifactIds: ["specifications/ai/B1-b1.md"] }] });
    await act(async () => {
      await deliveryLensData(r.store).github!.load(true);
    });
    await waitFor(() => expect(nodeButton("B1 b1 label")).toHaveAttribute("data-plan-linked", "true"));
    expect(r.store.snapshot().selection).toEqual({ kind: "spec", code: "B1" });
    expect(r.store.snapshot().view).toEqual(view);
  });
});
