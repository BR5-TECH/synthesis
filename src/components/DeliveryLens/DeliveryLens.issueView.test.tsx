/**
 * When the issue view of the Delivery lens opens and closes
 * (`../../../specifications/ui/MIV-map-issue-view.md` MIV-FR-SVXH ..
 * MIV-FR-HGGS).
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

/** MIV-FR-ITSA: each review store the Map tab makes, so a test can open a review. */
const reviews = vi.hoisted(() => [] as unknown[]);
vi.mock("../../state/mapPlan/reviewStore", async (original) => {
  const mod = await original<typeof import("../../state/mapPlan/reviewStore")>();
  class Tracked extends mod.MapPlanReviewStore {
    constructor(...args: ConstructorParameters<typeof mod.MapPlanReviewStore>) {
      super(...args);
      reviews.push(this);
    }
  }
  return { ...mod, MapPlanReviewStore: Tracked };
});

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import type { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { callsOf } from "../../test/flowCanvasHarness";
import {
  canvas,
  clickItem,
  client,
  grip,
  issueView,
  itemEl,
  queryIssueView,
} from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  deliveryBackend,
  fileSource,
  githubAnswer,
  githubSource,
  issueFacts,
  leafState,
  milestoneFacts,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";

const FLOW = "plans/release.flow";
const DOC = plan([
  item("m1", "milestone"),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
  item("t2", "task", { parentId: "f1" }),
  item("t3", "task"),
]);
const TRACKER = [
  milestoneFacts("m1", 3),
  issueFacts("f1", 42),
  issueFacts("t1", 51),
  issueFacts("t2", 53),
  issueFacts("t3", 54),
];
const STATES = [
  ...DOC.items.filter((i) => i.id !== "t2").map((i) => leafState(i.id, "not_started")),
  leafState("t2", "not_started", { blocked: true, blockedBy: ["t1"] }),
];

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  reviews.length = 0;
  b = deliveryBackend(DOC, githubAnswer(DOC, { tracker: TRACKER, states: STATES }));
  b.plan = () => githubAnswer(DOC, { tracker: TRACKER, states: STATES });
  b.apply = () => githubAnswer(DOC, { tracker: TRACKER, states: STATES });
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openGithub() {
  const r = await renderLens({ source: async () => githubSource() });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

async function openFile() {
  const r = await renderLens({ source: async () => fileSource(FLOW), host: { claims: claimsOf() } });
  await screen.findByTestId("flow-item-t1");
  return r;
}

/** The review store of the Map tab that the test rendered last. */
const reviewStore = () => reviews[reviews.length - 1] as MapPlanReviewStore;

const edits = () => callsOf(b, "apply_github_delivery_edit").map((c) => c.edit as Record<string, unknown>);

describe("opening the issue view (MIV-FR-SVXH, MIV-FR-TTZU)", () => {
  it("MIV-FR-SVXH, MIV-FR-HGGS: a click on a card, a group header, or a milestone header opens its issue view in place of the panel", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByRole("dialog", { name: "Issue #51 T1" });
    expect(view).toHaveAttribute("aria-modal", "false");
    expect(view).toHaveAttribute("data-item", "t1");
    expect(screen.queryByTestId("flow-selection-panel")).toBeNull();
    clickItem("f1");
    expect(await screen.findByRole("dialog", { name: "Issue #42 F1" })).toBeInTheDocument();
    clickItem("m1");
    expect(await screen.findByRole("dialog", { name: "Milestone M1" })).toBeInTheDocument();
    expect(screen.getAllByTestId("map-issue-view")).toHaveLength(1);
  });

  it("MIV-FR-SVXH, MIV-FR-HGGS: an item of a file source opens its issue view too, named by its kind", async () => {
    await openFile();
    clickItem("t1");
    expect(await screen.findByRole("dialog", { name: "Task T1" })).toBeInTheDocument();
  });

  it("MIV-FR-SVXH: Enter on a focused card opens its issue view; Enter on a control in it does not", async () => {
    await openGithub();
    const tag = within(itemEl("t3")).getByTestId("flow-tracker-tag");
    fireEvent.keyDown(tag, { key: "Enter" });
    expect(queryIssueView()).toBeNull();
    itemEl("t3").focus();
    fireEvent.keyDown(itemEl("t3"), { key: "Enter" });
    expect(await screen.findByRole("dialog", { name: "Issue #54 T3" })).toBeInTheDocument();
  });

  it("MIV-FR-SVXH, DBR-FR-MYVI: a press that moves more than 4 pixels is a drag and opens no view; one that moves 3 pixels is a click", async () => {
    await openGithub();
    fireEvent.pointerDown(grip("t3"), { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 6, y: 0 }) });
    fireEvent.pointerUp(canvas(), { pointerId: 1, ...client({ x: 6, y: 0 }) });
    expect(queryIssueView()).toBeNull();
    fireEvent.pointerDown(grip("t3"), { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
    fireEvent.pointerUp(canvas(), { pointerId: 1, ...client({ x: 3, y: 0 }) });
    expect(await screen.findByTestId("map-issue-view")).toHaveAttribute("data-item", "t3");
  });

  it("MIV-FR-TTZU: the Overlap marker selects both items and opens no view", async () => {
    b.plan = () =>
      githubAnswer(DOC, { tracker: TRACKER, states: STATES, overlaps: [{ items: ["t1", "t3"], paths: ["src/pay.ts"] }] });
    await openGithub();
    fireEvent.click(within(itemEl("t3")).getByTestId("flow-marker-overlap"));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    expect(queryIssueView()).toBeNull();
  });

  it("MIV-FR-TTZU: the Blocked marker, a request to show an item, and a press on the address select and open no view", async () => {
    const r = await openGithub();
    fireEvent.click(within(itemEl("t2")).getByTestId("flow-marker-blocked"));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    expect(queryIssueView()).toBeNull();
    act(() => r.store.showDeliveryItem("t3"));
    await waitFor(() => expect(itemEl("t3")).toHaveAttribute("data-selected", "true"));
    expect(queryIssueView()).toBeNull();
    const tag = within(itemEl("t1")).getByTestId("flow-tracker-tag");
    fireEvent.pointerDown(tag, { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
    fireEvent.pointerUp(canvas(), { pointerId: 1, ...client({ x: 0, y: 0 }) });
    expect(queryIssueView()).toBeNull();
  });
});

describe("closing the issue view (MIV-FR-HULC .. MIV-FR-HOAE)", () => {
  it("MIV-FR-HULC: a press on empty canvas closes the view; a click on another card moves it there; a press inside it keeps it", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    fireEvent.pointerDown(within(view).getByTestId("miv-title"), { pointerId: 1, buttons: 1 });
    fireEvent.pointerUp(within(view).getByTestId("miv-title"), { pointerId: 1 });
    expect(queryIssueView()).toHaveAttribute("data-item", "t1");
    clickItem("t3");
    await waitFor(() => expect(issueView()).toHaveAttribute("data-item", "t3"));
    expect(screen.getAllByTestId("map-issue-view")).toHaveLength(1);
    fireEvent.pointerDown(canvas(), { pointerId: 1, buttons: 1, clientX: 900, clientY: 900 });
    fireEvent.pointerUp(canvas(), { pointerId: 1, clientX: 900, clientY: 900 });
    await waitFor(() => expect(queryIssueView()).toBeNull());
  });

  it("MIV-FR-HULC: a drag that starts closes the view, and it stays closed after the drop", async () => {
    await openGithub();
    clickItem("t3");
    await screen.findByTestId("map-issue-view");
    fireEvent.pointerDown(grip("t3"), { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client({ x: 40, y: 0 }) });
    await waitFor(() => expect(queryIssueView()).toBeNull());
    fireEvent.pointerUp(canvas(), { pointerId: 1, ...client({ x: 40, y: 0 }) });
    expect(queryIssueView()).toBeNull();
  });

  it("MIV-FR-HGGS: a pinch over the view does not zoom the canvas behind it; a plain wheel does not pan it", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    const zoom = screen.getByTestId("flow-zoom").textContent;
    const transform = (document.querySelector(".flow-viewport") as HTMLElement).style.transform;
    fireEvent.wheel(view, { deltaY: -40, ctrlKey: true });
    fireEvent.wheel(within(view).getByTestId("miv-main"), { deltaY: 120 });
    expect(screen.getByTestId("flow-zoom").textContent).toBe(zoom);
    expect((document.querySelector(".flow-viewport") as HTMLElement).style.transform).toBe(transform);
  });

  it("MIV-FR-LENY, MIV-FR-SWZY: Edit puts the focus in the description source, so Escape leaves the source and not the view", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    await userEvent.click(within(view).getByRole("button", { name: "Edit the description" }));
    expect(within(view).getByRole("textbox", { name: "Description" })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(issueView()).toBeInTheDocument();
    expect(within(issueView()).queryByRole("textbox", { name: "Description" })).toBeNull();
  });

  it("MIV-FR-YPWK, MIV-FR-HOAE: the view opened by a click takes the focus; Close and Escape close it, keep the selection, and focus the card", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    expect(view).toHaveFocus();
    const close = within(view).getByRole("button", { name: "Close" });
    expect(close).toHaveAttribute("title", "Close");
    await userEvent.click(close);
    expect(queryIssueView()).toBeNull();
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    expect(itemEl("t1")).toHaveFocus();
    clickItem("f1");
    fireEvent.keyDown(await screen.findByTestId("map-issue-view"), { key: "Escape" });
    expect(queryIssueView()).toBeNull();
    expect(itemEl("f1")).toHaveAttribute("data-selected", "true");
    expect(itemEl("f1")).toHaveFocus();
  });

  it("MIV-FR-SWZY, MIV-FR-DYXQ: Escape in the rename field cancels the rename alone; the view stays open", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    await userEvent.click(within(view).getByRole("button", { name: "Rename" }));
    const field = within(view).getByRole("textbox", { name: "Item name" });
    await userEvent.clear(field);
    await userEvent.type(field, "Renamed");
    fireEvent.keyDown(field, { key: "Escape" });
    expect(issueView()).toBeInTheDocument();
    expect(within(issueView()).getByTestId("miv-title")).toHaveTextContent("T1");
    await settle();
    expect(edits()).toHaveLength(0);
  });

  it("MIV-FR-SWZY, MIV-FR-BEZE: Escape in the description source returns to the rich text and makes its one edit; the view stays open", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    await userEvent.click(within(view).getByRole("button", { name: "Edit the description" }));
    const field = within(view).getByRole("textbox", { name: "Description" });
    await userEvent.type(field, "New body");
    fireEvent.keyDown(field, { key: "Escape" });
    expect(issueView()).toBeInTheDocument();
    expect(within(issueView()).queryByRole("textbox", { name: "Description" })).toBeNull();
    await waitFor(() => expect(edits().filter((e) => e.kind === "set_description")).toHaveLength(1));
    expect(edits()[0]).toMatchObject({ kind: "set_description", itemId: "t1", description: "New body" });
  });

  it.each([
    ["the close control", () => fireEvent.click(within(issueView()).getByRole("button", { name: "Close" }))],
    ["a click on another card", () => clickItem("t3")],
  ])("MIV-FR-BEZE: a rename in progress is committed once when %s closes the view", async (_, closeIt) => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    await userEvent.click(within(view).getByRole("button", { name: "Rename" }));
    const field = within(view).getByRole("textbox", { name: "Item name" });
    await userEvent.clear(field);
    await userEvent.type(field, "Renamed");
    act(() => closeIt());
    await waitFor(() => expect(edits().filter((e) => e.kind === "rename")).toHaveLength(1));
    await settle();
    expect(edits().filter((e) => e.kind === "rename")).toHaveLength(1);
    expect(edits().find((e) => e.kind === "rename")).toMatchObject({ itemId: "t1", name: "Renamed" });
  });

  it("MIV-FR-BEZE: a description edit in progress is committed once when the view closes", async () => {
    await openGithub();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    await userEvent.click(within(view).getByRole("button", { name: "Edit the description" }));
    await userEvent.type(within(view).getByRole("textbox", { name: "Description" }), "Body");
    fireEvent.click(within(view).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(edits().filter((e) => e.kind === "set_description")).toHaveLength(1));
    await settle();
    expect(edits().filter((e) => e.kind === "set_description")).toHaveLength(1);
  });
});

describe("the issue view with the lens and the review sheet (MIV-FR-RPAU, MIV-FR-ITSA, MIV-FR-HGAQ)", () => {
  it("MIV-FR-RPAU, MIV-FR-HOAE: a lens switch and a return show the view again, and it takes no focus", async () => {
    await openGithub();
    clickItem("t1");
    await screen.findByTestId("map-issue-view");
    const lens = () => within(screen.getByRole("radiogroup", { name: "Lens" }));
    fireEvent.click(lens().getByRole("radio", { name: "Specifications" }));
    await waitFor(() => expect(queryIssueView()).toBeNull());
    fireEvent.click(lens().getByRole("radio", { name: "Delivery" }));
    const again = await screen.findByTestId("map-issue-view");
    expect(again).toHaveAttribute("data-item", "t1");
    expect(again).not.toHaveFocus();
  });

  it("MIV-FR-RPAU: a view closed before a lens switch does not show after the return", async () => {
    await openGithub();
    clickItem("t1");
    fireEvent.click(within(await screen.findByTestId("map-issue-view")).getByRole("button", { name: "Close" }));
    const lens = () => within(screen.getByRole("radiogroup", { name: "Lens" }));
    fireEvent.click(lens().getByRole("radio", { name: "Specifications" }));
    fireEvent.click(lens().getByRole("radio", { name: "Delivery" }));
    await screen.findByTestId("flow-item-t1");
    await settle();
    expect(queryIssueView()).toBeNull();
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
  });

  it("MIV-FR-ITSA: the review sheet closes the view and keeps the selection; while it shows, a click opens no view", async () => {
    await openGithub();
    const review = reviewStore();
    clickItem("t1");
    await screen.findByTestId("map-issue-view");
    act(() => review.openReview("d1", "p1"));
    await waitFor(() => expect(queryIssueView()).toBeNull());
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    clickItem("t3");
    expect(itemEl("t3")).toHaveAttribute("data-selected", "true");
    expect(queryIssueView()).toBeNull();
    act(() => review.closeReview());
    await settle();
    expect(queryIssueView()).toBeNull();
  });

  it("MIV-FR-HGAQ: Remove opens the remove dialog over the view; the view is no overlay and stays open", async () => {
    const r = await openGithub();
    clickItem("f1");
    const view = await screen.findByTestId("map-issue-view");
    expect(r.onOverlayOpening).not.toHaveBeenCalled();
    await userEvent.click(within(view).getByRole("button", { name: "Remove" }));
    const dialog = screen.getByTestId("delivery-remove-dialog");
    expect(issueView()).toBeInTheDocument();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByTestId("delivery-remove-dialog")).toBeNull());
    expect(issueView()).toHaveAttribute("data-item", "f1");
  });
});
