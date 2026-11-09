/**
 * Plans for the Flow document tests (FLO, FGV). Each builder gives a new
 * object, so a test can change what it gets without effect on the next test.
 */
import {
  DEFAULT_CONTAINER_SIZES,
  FLOW_VERSION,
  isContainerKind,
  type FlowDocument,
  type FlowEdge,
  type FlowItem,
  type FlowItemKind,
  type SyncBase,
  type TrackerRef,
} from "../state/flowDocument";

/** An item of `kind`, named by its id, at the origin of its container. */
export function item(
  id: string,
  kind: FlowItemKind,
  extra: Partial<FlowItem> = {},
): FlowItem {
  return {
    id,
    kind,
    name: id.toUpperCase(),
    position: { x: 0, y: 0 },
    ...(isContainerKind(kind)
      ? { size: { ...DEFAULT_CONTAINER_SIZES[kind as keyof typeof DEFAULT_CONTAINER_SIZES] } }
      : {}),
    ...extra,
  };
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
 * The sample plan, with these canvas positions:
 *
 * ```
 * d1 (0,0) 1200×800
 *   m1 (20,60) 1000×700                     -> canvas (20,60)
 *     f1 (20,60) 420×300                     -> canvas (40,120)
 *       t1 (20,60)                           -> canvas (60,180)
 *       t2 (220,60)                          -> canvas (260,180)
 *     ep1 (460,60) 500×400                   -> canvas (480,120)
 *       f2 (20,60) 400×220                   -> canvas (500,180)
 *         t3 (20,60)                         -> canvas (520,240)
 *     x1 (20,400)                            -> canvas (40,460)
 * ```
 */
export function samplePlan(edges: FlowEdge[] = []): FlowDocument {
  return plan(
    [
      item("d1", "deliverable", { size: { width: 1200, height: 800 } }),
      item("m1", "milestone", {
        parentId: "d1",
        position: { x: 20, y: 60 },
        size: { width: 1000, height: 700 },
      }),
      item("f1", "feature", {
        parentId: "m1",
        position: { x: 20, y: 60 },
        size: { width: 420, height: 300 },
      }),
      item("t1", "task", { parentId: "f1", position: { x: 20, y: 60 } }),
      item("t2", "task", { parentId: "f1", position: { x: 220, y: 60 } }),
      item("ep1", "epic", {
        parentId: "m1",
        position: { x: 460, y: 60 },
        size: { width: 500, height: 400 },
      }),
      item("f2", "feature", {
        parentId: "ep1",
        position: { x: 20, y: 60 },
        size: { width: 400, height: 220 },
      }),
      item("t3", "task", { parentId: "f2", position: { x: 20, y: 60 } }),
      item("x1", "defect", { parentId: "m1", position: { x: 20, y: 400 } }),
    ],
    edges,
  );
}

/** Add items to a plan, after the ones it has. */
export function withItems(doc: FlowDocument, ...items: FlowItem[]): FlowDocument {
  return { ...doc, items: [...doc.items, ...items] };
}

export function issueRef(number: number, repo = "payments"): TrackerRef {
  return {
    kind: "issue",
    host: "github.com",
    owner: "acme",
    repo,
    number,
    url: `https://github.com/acme/${repo}/issues/${number}`,
  };
}

export function milestoneRef(number: number): TrackerRef {
  return {
    kind: "milestone",
    host: "github.com",
    owner: "acme",
    repo: "payments",
    number,
    url: `https://github.com/acme/payments/milestone/${number}`,
  };
}

export function issueBase(name: string, description = ""): SyncBase {
  return {
    name,
    description,
    state: "open",
    type: null,
    parent: null,
    milestone: null,
    blockedBy: [],
  };
}

/** Every item id of a plan, in document order. */
export const ids = (doc: FlowDocument) => doc.items.map((i) => i.id);
