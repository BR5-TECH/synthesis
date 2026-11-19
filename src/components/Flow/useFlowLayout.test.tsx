/**
 * The layout a board commit stores while the host hides items
 * (`DBR-delivery-board.md` DBR-FR-NIGZ, DBR-FR-BKWE, DBR-FR-SKSO).
 */
import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { boardOrders, setCollapsed, type StoredLayout } from "../../state/flowDocument";
import type { FlowLayoutStore } from "../../state/flowLayouts";
import { item, plan } from "../../test/flowPlanFixtures";
import { boardCommitLayout, useFlowLayout } from "./useFlowLayout";

/**
 * m1 holds the epic g of one task t, then the cards a, h, and b: the automatic
 * order puts a container before the leaves (FLO-FR-PLUZ).
 */
const DOC = plan([
  item("m1", "milestone"),
  item("a", "task", { parentId: "m1" }),
  item("h", "task", { parentId: "m1" }),
  item("b", "task", { parentId: "m1" }),
  item("g", "epic", { parentId: "m1" }),
  item("t", "task", { parentId: "g" }),
]);
const ORDER = ["g", "a", "h", "b"];

/** A layout store that holds `stored` and keeps each save. */
function fakeStore(stored: StoredLayout) {
  const saves: StoredLayout[] = [];
  const store = {
    subscribe: () => () => {},
    getVersion: () => 0,
    get: () => stored,
    save: (_scope: string, layout: StoredLayout) => {
      saves.push(layout);
      return Promise.resolve();
    },
    reset: () => Promise.resolve(),
  } as unknown as FlowLayoutStore;
  return { store, saves };
}

describe("useFlowLayout on a board with hidden items (DBR-FR-NIGZ)", () => {
  it("DBR-FR-NIGZ: a collapse while an item is hidden keeps the stack order of every item", () => {
    const stored: StoredLayout = { items: {} };
    const { store, saves } = fakeStore(stored);
    const omitted = new Set(["h"]);
    const { result } = renderHook(() => useFlowLayout(DOC, "flow:a.flow", store, undefined, true, omitted));
    const shown = result.current.layout!;
    // The hidden item takes no slot: b follows a at once.
    expect(shown.b!.position.y).toBe(shown.a!.position.y + shown.a!.size!.height + 8);
    result.current.commit(setCollapsed(DOC, shown, "g", true));
    expect(saves).toHaveLength(1);
    const saved = saves[0]!;
    expect(saved.items.g?.collapsed).toBe(true);
    expect(boardOrders(DOC, saved).get("m1")).toEqual(ORDER);
    // Again, from what was stored: the order stays.
    const again = fakeStore(saved);
    const second = renderHook(() => useFlowLayout(DOC, "flow:a.flow", again.store, undefined, true, omitted));
    second.result.current.commit(setCollapsed(DOC, second.result.current.layout!, "g", false));
    expect(boardOrders(DOC, again.saves[0]!).get("m1")).toEqual(ORDER);
  });

  it("DBR-FR-BKWE: with nothing hidden, a collapse stores the positions of the board as it shows", () => {
    const { store, saves } = fakeStore({ items: {} });
    const { result } = renderHook(() => useFlowLayout(DOC, "flow:a.flow", store, undefined, true));
    const shown = result.current.layout!;
    result.current.commit(setCollapsed(DOC, shown, "g", true));
    expect(saves[0]!.items.h?.position).toEqual(shown.h!.position);
    expect(boardOrders(DOC, saves[0]!).get("m1")).toEqual(ORDER);
  });

  it("DBR-FR-NIGZ, DBR-FR-BKWE: a commit keeps the stored collapse of a hidden container", () => {
    const full = {
      g: { position: { x: 0, y: 40 }, collapsed: true },
      a: { position: { x: 0, y: 80 } },
    };
    const next = {
      g: { position: { x: 0, y: 0 } },
      a: { position: { x: 0, y: 0 } },
    };
    const out = boardCommitLayout(full, next, new Set(["g"]));
    expect(out.g).toEqual({ position: { x: 0, y: 40 }, collapsed: true });
    expect(out.a).toEqual({ position: { x: 0, y: 80 } });
  });

  it("DBR-FR-NIGZ: an item that the full layout does not place keeps its position from the commit", () => {
    const full = { a: { position: { x: 1, y: 2 } } };
    const next = {
      a: { position: { x: 0, y: 0 }, collapsed: true },
      n: { position: { x: 7, y: 9 } },
    };
    expect(boardCommitLayout(full, next)).toEqual({
      a: { position: { x: 1, y: 2 }, collapsed: true },
      n: { position: { x: 7, y: 9 } },
    });
  });
});
