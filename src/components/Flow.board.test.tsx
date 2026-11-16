/**
 * The board of the Delivery lens on the canvas (`../../specifications/ui/DBR-delivery-board.md`).
 * The canvas runs with `board` over a Flow file, as the lens hosts it for a
 * `file` source (MDL-FR-VLQJ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import {
  absolutePosition,
  BOARD_CARD_H,
  boardOrders,
  findItem,
  resolveBoardLayout,
  type FlowDocument,
  type StoredLayout,
} from "../state/flowDocument";
import { resetToasts } from "../state/toasts";
import { cancelAllScheduledWrites } from "../state/writeSchedule";
import {
  currentMainBranchWrite,
  rememberConfirmation,
  resetMainBranchWriteForTest,
  settleMainBranchWrite,
} from "../state/mainBranchWrite";
import { mainGuard, scriptedReads } from "../test/mainBranchWriteFixtures";
import { callsOf, clearFlowStores, flowBackend, itemState, planOf, renderFlow } from "../test/flowCanvasHarness";
import { canvas, client, ID, itemEl } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { blocks, item, plan } from "../test/flowPlanFixtures";

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
  cancelAllScheduledWrites();
  cleanup();
  clearFlowStores();
});

const DOC = plan(
  [
    item("m1", "milestone", { due: "2027-03-31" }),
    item("m2", "milestone", { due: "2027-06-30" }),
    item("e1", "epic", { parentId: "m1", description: "An epic with an excerpt." }),
    item("f1", "feature", { parentId: "e1" }),
    item("t1", "task", { parentId: "f1", name: "Tokenize card numbers", draftId: "d-1", artifactIds: ["docs/payments-hld.md"] }),
    item("t2", "task", { parentId: "f1", name: "Card form", description: "Card number, expiry and CVC." }),
    item("t3", "task", { parentId: "m1" }),
    item("t4", "task", { parentId: "m2" }),
    item("x1", "defect"),
  ],
  [blocks("e-1", "t1", "t2")],
);

async function open(doc: FlowDocument = DOC, layout: StoredLayout = { items: {} }) {
  const b = flowBackend(doc, layout);
  b.delivery = {
    items: [
      itemState("t1", "in_progress", { draft: { id: "d-1", name: "tokenize-cards", status: "drafting" } }),
      itemState("t2", "drafting", { blocked: true, blockedBy: ["t1"] }),
      itemState("t3", "not_started"),
      itemState("t4", "done"),
      itemState("x1", "not_started", { draftMissing: true }),
    ],
  } as never;
  const r = await renderFlow(b, { board: true });
  await screen.findByTestId("flow-item-t1");
  await waitFor(() => expect(within(itemEl("t1")).queryByTestId("flow-state-glyph")).not.toBeNull());
  return { b, ...r };
}

/** The middle of an item as the board lays it out now, in canvas units. */
function middleOf(doc: FlowDocument, stored: StoredLayout, id: string, lifted?: Set<string>) {
  const b = resolveBoardLayout(doc, stored, lifted ? { lifted } : {});
  const at = absolutePosition(doc, b.layout, id);
  const size = b.layout[id].size!;
  return { x: at.x + size.width / 2, y: at.y + size.height / 2, top: at.y };
}

function press(id: string, at: { x: number; y: number }) {
  fireEvent.pointerDown(itemEl(id).querySelector(`[data-item-id="${id}"]`) ?? itemEl(id), {
    pointerId: 1,
    buttons: 1,
    ...client(at),
  });
}
const move = (at: { x: number; y: number }) =>
  fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, ...client(at) });
const release = () => fireEvent.pointerUp(canvas(), { pointerId: 1 });

describe("the board in the canvas (DBR-FR-ERHW, DBR-FR-LNZI .. DBR-FR-ZRBD)", () => {
  it("DBR-FR-ERHW: with the board the canvas lays the plan out as a board; a Flow tab has no board", async () => {
    await open();
    expect(canvas()).toHaveAttribute("data-board", "true");
    expect(itemEl("t1")).toHaveClass("flow-bcard");
    cleanup();
    clearFlowStores();
    const b = flowBackend(DOC, { items: {} });
    await renderFlow(b);
    await screen.findByTestId("flow-item-t1");
    expect(canvas()).toHaveAttribute("data-board", "false");
    expect(itemEl("t1")).not.toHaveClass("flow-bcard");
  });

  it("DBR-FR-LNZI, DBR-FR-XOOU, DBR-FR-LVBY, DBR-FR-HWHO: a card has three lines: the kind, the name, and the state name of a Flow file", async () => {
    await open();
    const card = itemEl("t2");
    const lines = card.querySelectorAll(".flow-bcard__line");
    expect(lines).toHaveLength(3);
    expect(lines[0]).toHaveTextContent("Task");
    expect(within(card).queryByTestId("flow-tracker-tag")).toBeNull();
    expect(lines[1]).toHaveTextContent("Card form");
    expect(card).toHaveAttribute("title", "Card form");
    expect(within(card).getByTestId("flow-card-status")).toHaveTextContent("Drafting");
    expect(card.style.height).toBe(`${BOARD_CARD_H}px`);
    expect(itemEl("x1").querySelectorAll(".flow-bcard__line")[0]).toHaveTextContent("Defect");
  });

  it("DBR-FR-LNZI: every card has the same height, whatever its name and its markers", async () => {
    await open();
    const heights = ["t1", "t2", "t3", "t4", "x1"].map((id) => itemEl(id).style.height);
    expect(new Set(heights)).toEqual(new Set([`${BOARD_CARD_H}px`]));
  });

  it("DBR-FR-QZPA: the bottom line starts with the state glyph, named in its tooltip, and the card carries the state for its stripe", async () => {
    await open();
    const glyph = within(itemEl("t4")).getByTestId("flow-state-glyph");
    expect(glyph).toHaveAttribute("title", "Done");
    expect(glyph).toHaveAccessibleName("Done");
    expect(itemEl("t4")).toHaveAttribute("data-state", "done");
    expect(itemEl("t1").querySelector(".flow-bcard__status")!.firstElementChild).toBe(
      within(itemEl("t1")).getByTestId("flow-state-glyph"),
    );
  });

  it("DBR-FR-GFGG: the markers are glyphs at the end of the bottom line; Blocked keeps its activation and the draft glyph names the draft", async () => {
    await open();
    const blocked = within(itemEl("t2")).getByTestId("flow-marker-blocked");
    expect(blocked).toHaveAttribute("title", "Blocked by Tokenize card numbers");
    fireEvent.click(blocked);
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    expect(within(itemEl("t1")).getByTestId("flow-draft-glyph")).toHaveAttribute("title", "Draft tokenize-cards");
    expect(within(itemEl("x1")).queryByTestId("flow-draft-glyph")).toBeNull();
  });

  it("DBR-FR-GFGG: a missing draft shows a warning glyph in place of the draft glyph", async () => {
    const doc = plan([...DOC.items.map((i) => (i.id === "x1" ? { ...i, draftId: "gone" } : i))], DOC.edges);
    await open(doc);
    const warn = within(itemEl("x1")).getByTestId("flow-draft-missing");
    expect(warn).toHaveAttribute("title", "Draft gone is missing");
  });

  it("DBR-FR-ZRBD: a card shows no chip, no reference, no excerpt, and no draft row; the panel shows the draft row with its open action", async () => {
    await open();
    const card = itemEl("t1");
    expect(card.querySelector(".flow-chip")).toBeNull();
    expect(card.querySelector(".flow-refs, .flow-ref")).toBeNull();
    expect(card.querySelector(".flow-excerpt")).toBeNull();
    expect(card.querySelector(".flow-draft")).toBeNull();
    expect(within(itemEl("t2")).queryByTestId("flow-item-excerpt-t2")).toBeNull();
    press("t1", { x: 0, y: 0 });
    release();
    const panel = await screen.findByTestId("flow-selection-panel");
    expect(within(panel).getByTestId("flow-panel-draft")).toHaveTextContent("tokenize-cards");
    expect(within(panel).getByRole("button", { name: "Unlink draft" })).toBeInTheDocument();
  });

  it("DBR-FR-SBTR, DBR-FR-BKWE: a group header is one line with the state glyph; a group shows no excerpt and offers no resize", async () => {
    await open();
    const epic = itemEl("e1");
    expect(epic).toHaveAttribute("data-board", "true");
    expect(epic.querySelector(".flow-ctr__about")).toBeNull();
    expect(screen.queryByTestId("flow-resize-e1-se")).toBeNull();
    expect(screen.queryByTestId("flow-resize-f1-se")).toBeNull();
  });

  it("DBR-FR-BYQX: a column header keeps its order number, due date, and state chip", async () => {
    await open();
    const head = itemEl("m1").querySelector(".flow-ctr__head")!;
    expect(within(head as HTMLElement).getByTestId("flow-column-order-m1")).toHaveTextContent("1");
    expect(within(head as HTMLElement).getByTestId("flow-due")).toHaveTextContent("2027-03-31");
  });

  it("DBR-FR-HTCE: the stored positions give the stack order", async () => {
    const stored: StoredLayout = { items: { t2: { position: { x: 0, y: 0 } }, t1: { position: { x: 0, y: 900 } } } };
    await open(DOC, stored);
    expect(parseFloat(itemEl("t2").style.top)).toBeLessThan(parseFloat(itemEl("t1").style.top));
  });

  it("DBR-FR-CCAS: the edges of the plan render between the cards of the board", async () => {
    await open();
    expect(document.querySelectorAll("[data-edge-id]").length).toBeGreaterThan(0);
  });
});

describe("a drag of the board (DBR-FR-MYVI .. DBR-FR-BBDZ)", () => {
  it("DBR-FR-MYVI: a press that moves 4 pixels or less is a click: it selects and drags nothing", async () => {
    const { b } = await open();
    const at = middleOf(DOC, { items: {} }, "t3");
    press("t3", at);
    move({ x: at.x + 3, y: at.y + 2 });
    expect(itemEl("t3")).toHaveAttribute("data-lifted", "false");
    release();
    expect(itemEl("t3")).toHaveAttribute("data-selected", "true");
    expect(b.layoutWrites).toHaveLength(0);
  });

  it("DBR-FR-MYVI: a press on the marker glyph starts no drag", async () => {
    await open();
    const blocked = within(itemEl("t2")).getByTestId("flow-marker-blocked");
    fireEvent.pointerDown(blocked, { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
    move({ x: 200, y: 200 });
    expect(itemEl("t2")).toHaveAttribute("data-lifted", "false");
    release();
  });

  it("DBR-FR-QCAJ, DBR-FR-CZEU: a drag lifts the item, closes its slot, and opens a placeholder in the target stack, which takes the drop-target emphasis", async () => {
    await open();
    const stored = { items: {} };
    const from = middleOf(DOC, stored, "t3");
    const lifted = new Set(["t3"]);
    const over = middleOf(DOC, stored, "t4", lifted);
    press("t3", from);
    move({ x: from.x + 20, y: from.y });
    move({ x: over.x, y: over.top + 2 });
    expect(itemEl("t3")).toHaveAttribute("data-lifted", "true");
    expect(itemEl("m2")).toHaveAttribute("data-drop-target", "true");
    const slot = screen.getByTestId("flow-board-placeholder");
    const expected = resolveBoardLayout(DOC, stored, { lifted, gap: { holder: "m2", index: 0, height: BOARD_CARD_H } }).gapRect!;
    expect(parseFloat(slot.style.top)).toBe(expected.y);
    expect(parseFloat(slot.style.left)).toBe(expected.x);
    // The member after the placeholder moves down to make room.
    expect(parseFloat(itemEl("t4").style.top)).toBeGreaterThan(over.top);
    expect(screen.queryByTestId("flow-selection-panel")).toBeNull();
    release();
  });

  it("DBR-FR-KTJS: a drop into another stack moves the item there and writes the layout once with the slot positions", async () => {
    const { b, flows } = await open();
    const from = middleOf(DOC, { items: {} }, "t3");
    const over = middleOf(DOC, { items: {} }, "t4", new Set(["t3"]));
    press("t3", from);
    move({ x: from.x + 20, y: from.y });
    move({ x: over.x, y: over.top + 2 });
    release();
    expect(findItem(planOf(flows), "t3")!.parentId).toBe("m2");
    expect(b.layoutWrites).toHaveLength(1);
    const written = b.layoutWrites[0].layout;
    expect(boardOrders(planOf(flows), written).get("m2")).toEqual(["t3", "t4"]);
    expect(flows.get(ID)!.dirty).toBe(true);
    await waitFor(() => expect(screen.queryByTestId("flow-board-placeholder")).toBeNull());
  });

  it("DBR-FR-KTJS: a drop in the same stack changes the order alone and is no edit", async () => {
    const { b, flows } = await open();
    const stored = { items: {} };
    const from = middleOf(DOC, stored, "t2");
    const over = middleOf(DOC, stored, "t1", new Set(["t2"]));
    press("t2", from);
    move({ x: from.x, y: from.y - 20 });
    move({ x: over.x, y: over.top + 2 });
    release();
    expect(flows.get(ID)!.dirty).toBe(false);
    expect(b.layoutWrites).toHaveLength(1);
    expect(boardOrders(planOf(flows), b.layoutWrites[0].layout).get("f1")).toEqual(["t2", "t1"]);
    expect(parseFloat(itemEl("t2").style.top)).toBeLessThan(parseFloat(itemEl("t1").style.top));
  });

  it("DBR-FR-BBDZ: Escape during a drag puts the item back and changes no plan and no layout", async () => {
    const { b, flows } = await open();
    const from = middleOf(DOC, { items: {} }, "t3");
    const over = middleOf(DOC, { items: {} }, "t4", new Set(["t3"]));
    const before = itemEl("t3").style.top;
    press("t3", from);
    move({ x: over.x, y: over.top + 2 });
    expect(itemEl("t3")).toHaveAttribute("data-lifted", "true");
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    expect(itemEl("t3")).toHaveAttribute("data-lifted", "false");
    release();
    expect(itemEl("t3").style.top).toBe(before);
    expect(findItem(planOf(flows), "t3")!.parentId).toBe("m1");
    expect(b.layoutWrites).toHaveLength(0);
  });

  it("DBR-FR-BBDZ: a release where no placeholder shows changes nothing", async () => {
    const { b, flows } = await open();
    const from = middleOf(DOC, { items: {} }, "t3");
    press("t3", from);
    move({ x: -400, y: -400 });
    expect(screen.queryByTestId("flow-board-placeholder")).toBeNull();
    expect(canvas()).toHaveAttribute("data-refuse", "true");
    release();
    expect(findItem(planOf(flows), "t3")!.parentId).toBe("m1");
    expect(b.layoutWrites).toHaveLength(0);
  });

  it("DBR-FR-BBDZ: a release outside the canvas puts the item back, also where the point behind it is a column", async () => {
    const { b, flows } = await open();
    const rect = { left: 0, top: 0, right: 1000, bottom: 800, width: 1000, height: 800, x: 0, y: 0, toJSON: () => ({}) };
    const spy = vi.spyOn(canvas(), "getBoundingClientRect").mockReturnValue(rect as DOMRect);
    try {
      const from = middleOf(DOC, { items: {} }, "t3");
      const over = middleOf(DOC, { items: {} }, "t4", new Set(["t3"]));
      press("t3", from);
      move({ x: over.x, y: over.top + 2 });
      expect(screen.getByTestId("flow-board-placeholder")).toBeInTheDocument();
      // The same canvas point, with the pointer outside the canvas: a view panned to the left.
      fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: -30, clientY: client(over).clientY });
      expect(screen.queryByTestId("flow-board-placeholder")).toBeNull();
      release();
      expect(findItem(planOf(flows), "t3")!.parentId).toBe("m1");
      expect(b.layoutWrites).toHaveLength(0);
    } finally {
      spy.mockRestore();
    }
  });

  it("DBR-FR-BBDZ: a drop that the plan rules refuse puts the item back and says why", async () => {
    // f2 blocks t3: t3 cannot go into f2, because the edge would join it to its container.
    const doc = plan([...DOC.items, item("f2", "feature", { parentId: "m2" })], [blocks("e-2", "f2", "t3")]);
    const { b, flows } = await open(doc);
    const from = middleOf(doc, { items: {} }, "t3");
    const over = middleOf(doc, { items: {} }, "f2", new Set(["t3"]));
    press("t3", from);
    move({ x: from.x + 20, y: from.y });
    move({ x: over.x, y: over.y });
    release();
    expect(await screen.findByTestId("flow-refusal")).toBeInTheDocument();
    expect(findItem(planOf(flows), "t3")!.parentId).toBe("m1");
    expect(b.layoutWrites).toHaveLength(0);
  });

  it("DBR-FR-EEKD, DBR-FR-CZEU: over a stack that cannot hold the kind, the hint names where the kind can sit, and the column around it takes the drop", async () => {
    const doc = plan([...DOC.items, item("f9", "feature")], DOC.edges);
    await open(doc);
    const lifted = new Set(["e1", "f1", "t1", "t2"]);
    const from = middleOf(doc, { items: {} }, "e1");
    const over = middleOf(doc, { items: {} }, "f9", lifted);
    press("e1", { x: from.x, y: from.top + 10 });
    move({ x: from.x + 20, y: from.top + 10 });
    move({ x: over.x, y: over.y });
    expect(screen.getByTestId("flow-board-hint")).toHaveTextContent(
      "An epic can sit only at the top level or in a milestone.",
    );
    // The No milestone column holds an epic, so it shows the placeholder.
    expect(screen.getByTestId("flow-no-milestone")).toHaveAttribute("data-drop-target", "true");
    expect(screen.getByTestId("flow-board-placeholder")).toBeInTheDocument();
    expect(canvas()).toHaveAttribute("data-refuse", "false");
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    release();
  });
});

describe("the card in detail (DBR-FR-QZPA, DBR-FR-GFGG, DBR-FR-SBTR)", () => {
  it("DBR-FR-QZPA: each delivery state has its own glyph and state on the card, and an item with no state answer has neither", async () => {
    const doc = plan([
      item("m1", "milestone"),
      ...(["a", "b", "c", "d", "e", "z"] as const).map((id) => item(id, "task", { parentId: "m1" })),
    ]);
    const b = flowBackend(doc, { items: {} });
    const states = ["not_started", "drafting", "in_progress", "done", "dropped"] as const;
    b.delivery = { items: states.map((st, k) => itemState("abcde"[k], st)) } as never;
    await renderFlow(b, { board: true });
    await waitFor(() => expect(within(itemEl("a")).queryByTestId("flow-state-glyph")).not.toBeNull());
    const glyphs = states.map((st, k) => {
      expect(itemEl("abcde"[k])).toHaveAttribute("data-state", st);
      return within(itemEl("abcde"[k])).getByTestId("flow-state-glyph").textContent;
    });
    expect(new Set(glyphs).size).toBe(5);
    expect(within(itemEl("z")).queryByTestId("flow-state-glyph")).toBeNull();
    expect(itemEl("z")).not.toHaveAttribute("data-state");
  });

  it("DBR-FR-GFGG: the bottom line ends with Blocked, Overlap, and the draft glyph, in that order", async () => {
    const b = flowBackend(DOC, { items: {} });
    b.delivery = {
      items: [itemState("t1", "in_progress", { blocked: true, blockedBy: ["t3"], draft: { id: "d-1", name: "tokenize-cards", status: "drafting" } })],
    } as never;
    b.overlaps = { overlaps: [{ items: ["t1", "t3"], paths: ["src/pay.rs"] }] };
    await renderFlow(b, { board: true });
    await waitFor(() => expect(within(itemEl("t1")).queryByTestId("flow-marker-overlap")).not.toBeNull());
    const marks = itemEl("t1").querySelector(".flow-bcard__status .flow-bmarks")!;
    expect([...marks.children].map((el) => el.getAttribute("data-testid"))).toEqual([
      "flow-marker-blocked",
      "flow-marker-overlap",
      "flow-draft-glyph",
    ]);
    fireEvent.click(within(itemEl("t1")).getByTestId("flow-marker-overlap"));
    expect(itemEl("t3")).toHaveAttribute("data-selected", "true");
  });

  it("DBR-FR-SBTR: a collapsed group shows its header and the line of counts", async () => {
    const b = flowBackend(DOC, { items: { f1: { collapsed: true } } });
    await renderFlow(b, { board: true });
    expect(await screen.findByTestId("flow-leaf-counts-f1")).toBeInTheDocument();
    expect(screen.queryByTestId("flow-item-t1")).toBeNull();
  });
});

describe("more of a drag of the board (DBR-FR-MYVI, DBR-FR-QCAJ, DBR-FR-CCAS, DBR-FR-BYQX, DBR-FR-BKWE, DBR-FR-IQOM)", () => {
  it("DBR-FR-MYVI: a move of exactly 4 pixels is a click, and 5 pixels starts the drag", async () => {
    await open();
    const at = middleOf(DOC, { items: {} }, "t3");
    press("t3", at);
    move({ x: at.x + 4, y: at.y });
    expect(itemEl("t3")).toHaveAttribute("data-lifted", "false");
    move({ x: at.x + 5, y: at.y });
    expect(itemEl("t3")).toHaveAttribute("data-lifted", "true");
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    release();
  });

  it("DBR-FR-MYVI: a press on the collapse toggle or on the connection handle starts no drag", async () => {
    await open();
    for (const target of [
      within(itemEl("f1")).getByRole("button", { name: /^Collapse / }),
      within(itemEl("t3")).getByLabelText(/^Connect from /),
    ]) {
      fireEvent.pointerDown(target, { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
      move({ x: 300, y: 300 });
      expect(document.querySelectorAll('[data-lifted="true"]')).toHaveLength(0);
      release();
    }
  });

  it("DBR-FR-QCAJ: a drag of a group takes the group with every item in it", async () => {
    await open();
    const from = middleOf(DOC, { items: {} }, "f1");
    press("f1", { x: from.x, y: from.top + 10 });
    move({ x: from.x + 40, y: from.top + 10 });
    for (const id of ["f1", "t1", "t2"]) expect(itemEl(id)).toHaveAttribute("data-lifted", "true");
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    release();
  });

  it("DBR-FR-CCAS: while a drag runs, the edges of the dragged item follow it", async () => {
    await open();
    const path = () => document.querySelector('[data-edge-id="e-1"] path')?.getAttribute("d") ?? document.querySelector('[data-edge-id="e-1"]')?.getAttribute("d");
    const before = path();
    expect(before).toBeTruthy();
    const at = middleOf(DOC, { items: {} }, "t1");
    press("t1", at);
    move({ x: at.x + 200, y: at.y + 50 });
    expect(path()).not.toBe(before);
    act(() => {
      fireEvent.keyDown(window, { key: "Escape" });
    });
    release();
  });

  it("DBR-FR-BYQX: a drag of a column header on the board changes the milestone order", async () => {
    // Undated milestones keep the order the author gives them (FLO-FR-MCFR).
    const doc = plan(DOC.items.map((i) => (i.kind === "milestone" ? { ...i, due: undefined } : i)), DOC.edges);
    const { flows } = await open(doc);
    const lay = resolveBoardLayout(doc, { items: {} });
    const m2 = lay.layout.m2.position;
    press("m2", { x: m2.x + 30, y: 10 });
    move({ x: m2.x, y: 10 });
    move({ x: -40, y: 10 });
    release();
    expect(planOf(flows).items.filter((i) => i.kind === "milestone").map((i) => i.id)).toEqual(["m2", "m1"]);
  });

  it("DBR-FR-BKWE: a collapse stores its flag with no size, and Reset layout gives every stack its automatic order and expands every group", async () => {
    const stored: StoredLayout = { items: { t2: { position: { x: 0, y: 0 } }, t1: { position: { x: 0, y: 900 } } } };
    const ref = { current: null as import("./Flow").FlowCanvasHandle | null };
    const b = flowBackend(DOC, stored);
    await renderFlow(b, { board: true, ref: (h: import("./Flow").FlowCanvasHandle | null) => {
      ref.current = h;
    } });
    await screen.findByTestId("flow-item-t1");
    fireEvent.click(within(itemEl("f1")).getByRole("button", { name: /^Collapse / }));
    const collapsed = b.layoutWrites[b.layoutWrites.length - 1].layout;
    expect(collapsed.items.f1).toEqual({ position: expect.any(Object), collapsed: true });
    expect(Object.values(collapsed.items).some((e) => "size" in e)).toBe(false);
    act(() => ref.current!.resetLayout());
    await waitFor(() => expect(b.layoutWrites[b.layoutWrites.length - 1].layout).toEqual({ items: {} }));
    expect(await screen.findByTestId("flow-item-t1")).toBeInTheDocument();
    expect(parseFloat(itemEl("t1").style.top)).toBeLessThan(parseFloat(itemEl("t2").style.top));
  });

  it("DBR-FR-IQOM: a drag near an edge of the canvas pans the view, and the placeholder follows the point under the pointer", async () => {
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame"] });
    try {
      await open();
      const rect = { left: 0, top: 0, right: 1000, bottom: 800, width: 1000, height: 800, x: 0, y: 0, toJSON: () => ({}) };
      vi.spyOn(canvas(), "getBoundingClientRect").mockReturnValue(rect as DOMRect);
      const viewport = () => (document.querySelector(".flow-viewport") as HTMLElement).style.transform;
      const at = middleOf(DOC, { items: {} }, "t3");
      press("t3", at);
      move({ x: at.x + 20, y: at.y });
      const before = viewport();
      fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 995, clientY: 400 });
      act(() => {
        vi.advanceTimersToNextFrame();
        vi.advanceTimersToNextFrame();
      });
      expect(viewport()).not.toBe(before);
      expect(/translate\((-?[\d.]+)px/.exec(viewport())![1]).not.toBe(/translate\((-?[\d.]+)px/.exec(before)![1]);
      act(() => {
        fireEvent.keyDown(window, { key: "Escape" });
      });
      release();
    } finally {
      vi.useRealTimers();
      vi.restoreAllMocks();
    }
  });
});

describe("a drop on a main branch (DBR-FR-KTJS, DBR-FR-BBDZ, EDT-FR-UPNU)", () => {
  afterEach(() => resetMainBranchWriteForTest());

  /** Drag t3 from Beta to GA while the gate holds the first change. */
  async function dropWhileHeld() {
    scriptedReads(mainGuard());
    const r = await open();
    const from = middleOf(DOC, { items: {} }, "t3");
    const over = middleOf(DOC, { items: {} }, "t4", new Set(["t3"]));
    press("t3", from);
    move({ x: from.x + 20, y: from.y });
    move({ x: over.x, y: over.top + 2 });
    act(() => release());
    await waitFor(() => expect(currentMainBranchWrite()).not.toBeNull());
    expect(findItem(planOf(r.flows), "t3")!.parentId).toBe("m1");
    return r;
  }

  it("DBR-FR-KTJS, EDT-FR-UPNU: a held drop writes no layout until the author confirms, and then lands the move and its order", async () => {
    const { b, flows } = await dropWhileHeld();
    expect(b.layoutWrites).toHaveLength(0);
    await act(async () => {
      rememberConfirmation(mainGuard({ confirmed: true }));
      settleMainBranchWrite(true);
    });
    await waitFor(() => expect(findItem(planOf(flows), "t3")!.parentId).toBe("m2"));
    expect(b.layoutWrites).toHaveLength(1);
    expect(boardOrders(planOf(flows), b.layoutWrites[0].layout).get("m2")).toEqual(["t3", "t4"]);
  });

  it("DBR-FR-BBDZ, EDT-FR-UPNU: a cancel of the confirmation changes no plan and no layout", async () => {
    const { b, flows } = await dropWhileHeld();
    await act(async () => settleMainBranchWrite(false));
    await act(async () => {});
    expect(findItem(planOf(flows), "t3")!.parentId).toBe("m1");
    expect(b.layoutWrites).toHaveLength(0);
  });
});
