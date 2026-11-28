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
  BOARD_FACE_H,
  BOARD_HEAD_H,
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
import { canvas, clickItem, client, ID, itemEl } from "../test/flowFixtures";
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
  await waitFor(() => expect(within(itemEl("t1")).queryByTestId("flow-status-dot")).not.toBeNull());
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
    expect(itemEl("t1")).toHaveClass("dm-card");
    cleanup();
    clearFlowStores();
    const b = flowBackend(DOC, { items: {} });
    await renderFlow(b);
    await screen.findByTestId("flow-item-t1");
    expect(canvas()).toHaveAttribute("data-board", "false");
    expect(itemEl("t1")).not.toHaveClass("dm-card");
  });

  it("DBR-FR-LNZI, DBR-FR-XOOU, DBR-FR-DZSU, DBR-FR-LVBY, DBR-FR-HWHO: a card has three lines: the kind chip, the name, and the state name of a Flow file in lowercase", async () => {
    await open();
    const card = itemEl("t2");
    const face = card.querySelector(".dm-card__face")!;
    expect([...face.children].map((el) => el.className)).toEqual(["dm-card__id", "dm-card__title", "dm-card__meta"]);
    const chip = within(card).getByTestId("flow-kind-chip");
    expect(chip).toHaveTextContent("TSK");
    expect(chip).toHaveAccessibleName("Task");
    expect(within(card).queryByTestId("flow-tracker-tag")).toBeNull();
    expect(within(card).getByTestId("flow-item-name-t2")).toHaveTextContent("Card form");
    expect(card).toHaveAttribute("title", "Card form");
    expect(within(card).getByTestId("flow-card-status")).toHaveTextContent("drafting");
    expect(within(itemEl("x1")).getByTestId("flow-kind-chip")).toHaveTextContent("DEF");
  });

  it("DBR-FR-LNZI: a card that the canvas has not measured takes the default height of three lines", async () => {
    await open();
    const heights = ["t1", "t2", "t3", "t4", "x1"].map((id) => itemEl(id).style.height);
    expect(new Set(heights)).toEqual(new Set([`${BOARD_CARD_H}px`]));
  });

  it("DBR-FR-LNZI: a card takes the height its lines measure, and the members below it move down", async () => {
    const proto = HTMLElement.prototype;
    const own = Object.getOwnPropertyDescriptor(proto, "offsetHeight")!;
    Object.defineProperty(proto, "offsetHeight", {
      configurable: true,
      get(this: HTMLElement) {
        return this.classList.contains("dm-card__face") && this.closest('[data-item-id="t1"]') ? 102 : 0;
      },
    });
    try {
      await open();
      // The face and its 1px border on each side.
      await waitFor(() => expect(itemEl("t1").style.height).toBe("104px"));
      expect(parseFloat(itemEl("t2").style.top)).toBe(parseFloat(itemEl("t1").style.top) + 104 + 8);
      expect(itemEl("t2").style.height).toBe(`${BOARD_CARD_H}px`);
    } finally {
      Object.defineProperty(proto, "offsetHeight", own);
    }
  });

  it("MIV-FR-SVXH, MIV-FR-HGGS: on the board a click opens the issue view in place of the panel; a Flow tab keeps its panel below the item", async () => {
    await open();
    clickItem("t3");
    const view = await screen.findByTestId("map-issue-view");
    expect(view).toHaveAttribute("role", "dialog");
    expect(view).toHaveAttribute("aria-modal", "false");
    expect(screen.queryByTestId("flow-selection-panel")).toBeNull();
    cleanup();
    clearFlowStores();
    await renderFlow(flowBackend(DOC, { items: {} }));
    await screen.findByTestId("flow-item-t3");
    fireEvent.pointerDown(itemEl("t3"), { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
    release();
    const panel = await screen.findByTestId("flow-selection-panel");
    expect(panel.getAttribute("data-placement")).not.toBe("beside");
    expect(screen.queryByTestId("map-issue-view")).toBeNull();
  });

  it("DBR-FR-QZPA, DBR-FR-GHDZ: the bottom line starts with the status dot, and the status word is beside it", async () => {
    await open();
    const status = within(itemEl("t4")).getByTestId("flow-card-status");
    expect(status).toHaveTextContent("done");
    expect(status.firstElementChild).toBe(within(itemEl("t4")).getByTestId("flow-status-dot"));
    expect(within(itemEl("t4")).getByTestId("flow-status-dot")).toHaveAttribute("data-s", "done");
    expect(itemEl("t4").querySelector(".flow-sglyph, .flow-chip")).toBeNull();
  });

  it("DBR-FR-GFGG, DBR-FR-ZBVZ: the markers are lowercase badges at the end of the bottom line; Blocked keeps its activation and the draft badge names the draft", async () => {
    await open();
    const blocked = within(itemEl("t2")).getByTestId("flow-marker-blocked");
    expect(blocked).toHaveTextContent("blocked");
    expect(blocked).toHaveClass("badge", "badge--danger");
    expect(blocked).toHaveAttribute("title", "Blocked by Tokenize card numbers");
    fireEvent.click(blocked);
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
    const draft = within(itemEl("t1")).getByTestId("flow-draft-glyph");
    expect(draft).toHaveTextContent("change");
    expect(draft).toHaveAttribute("title", "Change tokenize-cards");
    expect(within(itemEl("x1")).queryByTestId("flow-draft-glyph")).toBeNull();
  });

  it("DBR-FR-GFGG: a missing draft shows the draft missing badge in place of the draft badge", async () => {
    const doc = plan([...DOC.items.map((i) => (i.id === "x1" ? { ...i, draftId: "gone" } : i))], DOC.edges);
    await open(doc);
    const warn = within(itemEl("x1")).getByTestId("flow-draft-missing");
    expect(warn).toHaveTextContent("change missing");
    expect(warn).toHaveClass("badge--warn");
    expect(warn).toHaveAttribute("title", "Change gone is missing");
  });

  it("DBR-FR-ZRBD, MIV-FR-VHDP: a card shows no status chip, no reference, no excerpt, and no draft row; the issue view shows the draft row with its open action", async () => {
    await open();
    const card = itemEl("t1");
    expect(card.querySelector(".flow-chip")).toBeNull();
    expect(card.querySelector(".flow-refs, .flow-ref")).toBeNull();
    expect(card.querySelector(".flow-excerpt")).toBeNull();
    expect(card.querySelector(".flow-draft")).toBeNull();
    expect(within(itemEl("t2")).queryByTestId("flow-item-excerpt-t2")).toBeNull();
    clickItem("t1");
    const panel = await screen.findByTestId("map-issue-view");
    expect(within(panel).getByTestId("flow-panel-draft")).toHaveTextContent("tokenize-cards");
    expect(within(panel).getByRole("button", { name: "Unlink change" })).toBeInTheDocument();
  });

  it("DBR-FR-SBTR, DBR-FR-SBLK, DBR-FR-VNSJ, DBR-FR-BKWE: a group has the face of a card with its count and its toggle; it shows no excerpt and offers no resize", async () => {
    await open();
    const epic = itemEl("e1");
    expect(epic).toHaveAttribute("data-board", "true");
    expect(epic).toHaveClass("dm-group");
    const face = epic.querySelector(".dm-group__head")!;
    expect(within(face as HTMLElement).getByTestId("flow-kind-chip")).toHaveTextContent("EPC");
    expect(within(face as HTMLElement).getByTestId("flow-item-name-e1")).toHaveTextContent("E1");
    // The leaves under e1 are t1 (in progress) and t2 (drafting): none is done.
    expect(within(face as HTMLElement).getByTestId("flow-leaf-counts-e1")).toHaveTextContent("0/2");
    expect(within(face as HTMLElement).getByRole("button", { name: "Collapse E1" })).toBeInTheDocument();
    expect(epic.querySelector(".dm-group__body")).not.toBeNull();
    expect(epic.querySelector(".flow-ctr__about")).toBeNull();
    expect(screen.queryByTestId("flow-resize-e1-se")).toBeNull();
    expect(screen.queryByTestId("flow-resize-f1-se")).toBeNull();
  });

  it("DBR-FR-SBLK: a group that holds no item shows no stack, no count, and no toggle", async () => {
    const doc = plan([...DOC.items, item("f9", "feature", { parentId: "m2", name: "Empty" })], DOC.edges);
    await open(doc);
    const empty = itemEl("f9");
    expect(empty.querySelector(".dm-group__body")).toBeNull();
    expect(within(empty).queryByTestId("flow-leaf-counts-f9")).toBeNull();
    expect(within(empty).queryByRole("button", { name: /Collapse/ })).toBeNull();
  });

  it("DBR-FR-BYQX, DBR-FR-VNSJ: a column header shows its flag, name, state badge, due date, and done count, and no order number", async () => {
    const b = flowBackend(DOC, { items: {} });
    b.delivery = {
      items: [itemState("m1", "in_progress"), itemState("t1", "in_progress"), itemState("t4", "done")],
    } as never;
    await renderFlow(b, { board: true });
    await waitFor(() => expect(within(itemEl("m1")).queryByTestId("flow-status-chip")).not.toBeNull());
    const head = itemEl("m1").querySelector(".dm-lane__head") as HTMLElement;
    const row = head.querySelector(".dm-lane__row") as HTMLElement;
    expect(row.querySelector(".dm-lane__flag")).not.toBeNull();
    expect(within(row).getByTestId("flow-item-name-m1")).toHaveTextContent("M1");
    const badge = within(row).getByTestId("flow-status-chip");
    expect(badge).toHaveTextContent("in progress");
    expect(badge).toHaveClass("badge", "badge--live");
    expect(row.lastElementChild).toBe(within(row).getByTestId("flow-leaf-counts-m1"));
    expect(within(head).queryByTestId("flow-column-order-m1")).toBeNull();
    expect(within(head).getByTestId("flow-due")).toHaveTextContent("2027-03-31");
    // m2 holds t4, which is done.
    expect(within(itemEl("m2")).getByTestId("flow-leaf-counts-m2")).toHaveTextContent("1/1");
    expect(within(head).getByRole("button", { name: "Collapse M1" })).toBeInTheDocument();
  });

  it("DBR-FR-BYQX: a column with a description shows it below the header row", async () => {
    const doc = plan(DOC.items.map((i) => (i.id === "m1" ? { ...i, description: "Accept **Visa** first." } : i)), DOC.edges);
    await open(doc);
    expect(screen.getByTestId("flow-column-description-m1")).toHaveTextContent("Accept Visa first.");
    expect(screen.queryByTestId("flow-column-description-m2")).toBeNull();
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
  it("DBR-FR-GHDZ: each delivery state of a Flow file gives its dot, and an item with no state answer has no dot and no status", async () => {
    const doc = plan([
      item("m1", "milestone"),
      ...(["a", "b", "c", "d", "e", "z"] as const).map((id) => item(id, "task", { parentId: "m1" })),
    ]);
    const b = flowBackend(doc, { items: {} });
    const states = ["not_started", "drafting", "in_progress", "done", "dropped"] as const;
    b.delivery = { items: states.map((st, k) => itemState("abcde"[k], st)) } as never;
    await renderFlow(b, { board: true });
    await waitFor(() => expect(within(itemEl("a")).queryByTestId("flow-status-dot")).not.toBeNull());
    const dots = states.map((st, k) => {
      expect(itemEl("abcde"[k])).toHaveAttribute("data-state", st);
      return within(itemEl("abcde"[k])).getByTestId("flow-status-dot").getAttribute("data-s");
    });
    expect(dots).toEqual(["backlog", "ready", "in progress", "done", "backlog"]);
    expect(within(itemEl("e")).getByTestId("flow-card-status")).toHaveTextContent("dropped");
    expect(within(itemEl("z")).queryByTestId("flow-status-dot")).toBeNull();
    expect(within(itemEl("z")).queryByTestId("flow-card-status")).toBeNull();
  });

  it("DBR-FR-GFGG: the bottom line ends with Blocked, Overlap, and the draft badge, in that order", async () => {
    const b = flowBackend(DOC, { items: {} });
    b.delivery = {
      items: [itemState("t1", "in_progress", { blocked: true, blockedBy: ["t3"], draft: { id: "d-1", name: "tokenize-cards", status: "drafting" } })],
    } as never;
    b.overlaps = { overlaps: [{ items: ["t1", "t3"], paths: ["src/pay.rs"] }] };
    await renderFlow(b, { board: true });
    await waitFor(() => expect(within(itemEl("t1")).queryByTestId("flow-marker-overlap")).not.toBeNull());
    const marks = itemEl("t1").querySelector(".dm-card__meta .dm-card__markers")!;
    expect(marks.parentElement).toHaveClass("dm-card__meta");
    expect([...marks.children].map((el) => el.getAttribute("data-testid"))).toEqual([
      "flow-marker-blocked",
      "flow-marker-overlap",
      "flow-draft-glyph",
    ]);
    fireEvent.click(within(itemEl("t1")).getByTestId("flow-marker-overlap"));
    expect(itemEl("t3")).toHaveAttribute("data-selected", "true");
  });

  it("DBR-FR-SBLK: a collapsed group shows its face alone, with its count in place of the counts line", async () => {
    const b = flowBackend(DOC, { items: { f1: { collapsed: true } } });
    await renderFlow(b, { board: true });
    const count = await screen.findByTestId("flow-leaf-counts-f1");
    expect(count.closest(".dm-group__head")).not.toBeNull();
    expect(itemEl("f1").querySelector(".dm-group__body")).toBeNull();
    expect(within(itemEl("f1")).getByRole("button", { name: "Expand F1" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("flow-item-t1")).toBeNull();
  });

  it("DBR-FR-SBLK, DBR-FR-FISH: a collapsed group, also one that holds no item, and a collapsed column are as high as their slot", async () => {
    const doc = plan([...DOC.items, item("f9", "feature", { parentId: "m1", name: "Empty" })], DOC.edges);
    const stored: StoredLayout = { items: { f1: { collapsed: true }, f9: { collapsed: true }, m2: { collapsed: true } } };
    await renderFlow(flowBackend(doc, stored), { board: true });
    await screen.findByTestId("flow-leaf-counts-f1");
    const slots = resolveBoardLayout(doc, stored).layout;
    expect(slots.f1.size!.height).toBe(BOARD_FACE_H + 1);
    for (const id of ["f1", "f9", "m2"]) {
      expect(itemEl(id).style.height).toBe(`${slots[id].size!.height}px`);
    }
    expect(slots.m2.size!.height).toBe(BOARD_HEAD_H + 1);
  });

  it("DBR-FR-WQAN: an item of a Flow file shows no assignee", async () => {
    await open();
    expect(screen.queryByTestId("flow-card-assignee")).toBeNull();
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
