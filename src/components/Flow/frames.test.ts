/** The geometry of the canvas with items the host hides (`FLO-flow.md` FLO-FR-OHZB, `DBR-delivery-board.md` DBR-FR-POOT). */
import { describe, expect, it } from "vitest";

import { item, plan } from "../../test/flowPlanFixtures";
import { createFrames } from "./frames";

const DOC = plan([
  item("m1", "milestone"),
  item("g", "epic", { parentId: "m1" }),
  item("t1", "task", { parentId: "g" }),
  item("t2", "task", { parentId: "m1" }),
  item("t3", "task", { parentId: "m1" }),
]);

const POSITIONS: Record<string, { x: number; y: number }> = {
  m1: { x: 0, y: 0 },
  g: { x: 10, y: 50 },
  t1: { x: 20, y: 140 },
  t2: { x: 10, y: 300 },
  t3: { x: 10, y: 400 },
};

function frames(omitted: ReadonlySet<string>, collapsed: ReadonlySet<string> = new Set()) {
  return createFrames({
    doc: DOC,
    heights: {},
    collapsed,
    absOf: (id) => POSITIONS[id] ?? { x: 0, y: 0 },
    sizeOf: (id) => (id === "m1" ? { width: 300, height: 600 } : { width: 270, height: 80 }),
    board: true,
    omitted,
  });
}

describe("frames with hidden items (FLO-FR-OHZB, DBR-FR-POOT)", () => {
  it("FLO-FR-OHZB: a hidden item has no rectangle, and an item that shows has one", () => {
    const f = frames(new Set(["t2"]));
    expect(f.rectOf("t2")).toBeNull();
    expect(f.rectOf("t3")).toEqual({ x: 10, y: 400, width: 270, height: 80 });
    expect(f.distanceTo("t2", { x: 10, y: 300 })).toBe(Infinity);
  });

  it("DBR-FR-POOT: an edge with a hidden end is not drawn, and an edge between two shown items is", () => {
    const f = frames(new Set(["t2"]));
    expect(f.route("t2", "t3")).toBeNull();
    expect(f.route("t3", "t2")).toBeNull();
    expect(f.route("t1", "t3")).not.toBeNull();
  });

  it("DBR-FR-POOT: an edge whose hidden end is in a collapsed group that shows is not drawn", () => {
    const f = frames(new Set(["t1"]), new Set(["g"]));
    expect(f.rectOf("g")).not.toBeNull();
    expect(f.route("t1", "t3")).toBeNull();
    // With nothing hidden, the same edge lands on the collapsed group.
    expect(frames(new Set(), new Set(["g"])).route("t1", "t3")).toMatchObject({ from: "g", to: "t3" });
  });
});
