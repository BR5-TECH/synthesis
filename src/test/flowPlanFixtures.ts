/**
 * Plans and layouts for the Flow document tests (FLO, FGV). Each builder gives
 * a new object, so a test can change what it gets without effect on the next
 * test.
 */
import {
  FLOW_VERSION,
  resolveLayout,
  type FlowDocument,
  type FlowEdge,
  type FlowItem,
  type FlowItemKind,
  type FlowLayout,
  type PlacedItem,
  type StoredLayout,
} from "../state/flowDocument";

/** An item of `kind`, named by its id. */
export function item(
  id: string,
  kind: FlowItemKind,
  extra: Partial<FlowItem> = {},
): FlowItem {
  return { id, kind, name: id.toUpperCase(), ...extra };
}

export const blocks = (id: string, from: string, to: string): FlowEdge => ({
  id,
  from,
  to,
  type: "blocks",
});

export const relates = (id: string, from: string, to: string): FlowEdge => ({
  id,
  from,
  to,
  type: "relates",
});

export function plan(
  items: FlowItem[],
  edges: FlowEdge[] = [],
  extra: Partial<FlowDocument> = {},
): FlowDocument {
  return { version: FLOW_VERSION, name: "Plan", references: [], items, edges, ...extra };
}

/**
 * The sample plan. `sampleLayout` gives it these canvas positions:
 *
 * ```
 * m1 (20,60) 1000×700                       -> canvas (20,60)
 *   f1 (20,60) 420×300                       -> canvas (40,120)
 *     t1 (20,60)                             -> canvas (60,180)
 *     t2 (220,60)                            -> canvas (260,180)
 *   ep1 (460,60) 500×400                     -> canvas (480,120)
 *     f2 (20,60) 400×220                     -> canvas (500,180)
 *       t3 (20,60)                           -> canvas (520,240)
 *   x1 (20,400)                              -> canvas (40,460)
 * ```
 */
export function samplePlan(edges: FlowEdge[] = []): FlowDocument {
  return plan(
    [
      item("m1", "milestone"),
      item("f1", "feature", { parentId: "m1" }),
      item("t1", "task", { parentId: "f1" }),
      item("t2", "task", { parentId: "f1" }),
      item("ep1", "epic", { parentId: "m1" }),
      item("f2", "feature", { parentId: "ep1" }),
      item("t3", "task", { parentId: "f2" }),
      item("x1", "defect", { parentId: "m1" }),
    ],
    edges,
  );
}

/** The stored layout of the sample plan (see `samplePlan`). */
export function sampleLayout(): StoredLayout {
  return {
    items: {
      // FLO-FR-OGPF: a milestone column has no stored position or size, so the
      // layout puts m1 at the origin, whatever an entry says.
      m1: { position: { x: 20, y: 60 }, size: { width: 1000, height: 700 } },
      f1: { position: { x: 20, y: 60 }, size: { width: 420, height: 300 } },
      t1: { position: { x: 20, y: 60 } },
      t2: { position: { x: 220, y: 60 } },
      ep1: { position: { x: 460, y: 60 }, size: { width: 500, height: 400 } },
      f2: { position: { x: 20, y: 60 }, size: { width: 400, height: 220 } },
      t3: { position: { x: 20, y: 60 } },
      x1: { position: { x: 20, y: 400 } },
    },
  };
}

/** A stored layout from item positions, and sizes for the containers named. */
export function storedLayout(
  entries: Record<string, { x: number; y: number; width?: number; height?: number; collapsed?: boolean }>,
): StoredLayout {
  const items: StoredLayout["items"] = {};
  for (const [id, e] of Object.entries(entries)) {
    items[id] = {
      position: { x: e.x, y: e.y },
      ...(e.width !== undefined && e.height !== undefined
        ? { size: { width: e.width, height: e.height } }
        : {}),
      ...(e.collapsed ? { collapsed: true } : {}),
    };
  }
  return { items };
}

/** The layout the canvas draws for `doc` with `stored` applied. */
export function layoutFor(doc: FlowDocument, stored: StoredLayout = { items: {} }): FlowLayout {
  return resolveLayout(doc, stored);
}

/**
 * A layout that places each item exactly where `stored` says, also a
 * milestone column, for a test of the geometry helpers alone. The automatic
 * layout puts a milestone column at its own place (FLO-FR-SQUS, FLO-FR-OGPF).
 */
export function placedAsStored(stored: StoredLayout): FlowLayout {
  const out: Record<string, PlacedItem> = {};
  for (const [id, entry] of Object.entries(stored.items)) {
    out[id] = {
      position: { x: entry.position?.x ?? 0, y: entry.position?.y ?? 0 },
      ...(entry.size ? { size: { ...entry.size } } : {}),
      ...(entry.collapsed ? { collapsed: true } : {}),
    };
  }
  return out;
}

/** Add items to a plan, after the ones it has. */
export function withItems(doc: FlowDocument, ...items: FlowItem[]): FlowDocument {
  return { ...doc, items: [...doc.items, ...items] };
}

/** Every item id of a plan, in document order. */
export const ids = (doc: FlowDocument) => doc.items.map((i) => i.id);
