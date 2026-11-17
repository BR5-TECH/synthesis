/**
 * The two bindings of the Flow canvas (`FLO-flow.md`, `MDL-map-delivery-lens.md`).
 *
 * - The **file binding** is the Flow file of a Flow tab, or the delivery's Flow
 *   file in the Delivery lens (MDL-FR-VLQJ). The canvas reads the body,
 *   validates it, writes itself, and asks for the delivery state and the
 *   overlaps (FLO-FR-27, FLO-FR-46, FLO-FR-STUN).
 * - The **plan binding** is a plan that the host holds, a GitHub plan in the
 *   Delivery lens (MDL-FR-XJBV). The host gives the document, the states, the
 *   overlaps, and the tracker facts of each item, and takes each edit as the
 *   semantic edits of `"apply github delivery edit (edit)"` in place of a
 *   write. The canvas writes nothing and asks the backend for nothing.
 *
 * In both bindings the layout is per machine, in the scope of the plan
 * (FLO-FR-21, FLO-FR-HBBF).
 */
import type { FlowDocument, FlowItemKind } from "../../state/flowDocument";
import type { FlowLayoutStore } from "../../state/flowLayouts";
import type { FlowSessionStore } from "../../state/flowSessions";
import type {
  FlowOverlap,
  GithubDeliveryEdit,
  ItemDeliveryState,
  TrackerFacts,
} from "../../types";

/** FLO-FR-03: the canvas over a Flow file, through the shell's store. */
export interface FlowFileBinding {
  kind: "file";
  /** The Flow file's artifact id (ASC-FR-13); absent when no Flow is open. */
  flowId?: string;
  /** FLO-FR-30: the shell-owned store the plan and its dirty state live in. */
  flows: FlowSessionStore;
}

/**
 * MDL-FR-PUCM: one edit of a plan binding. `edits` are the semantic edits in
 * the order they apply; `next` is the plan with the edit applied, which the
 * host renders at once; `itemIds` are the items that carry the pending mark.
 */
export interface FlowPlanChange {
  edits: GithubDeliveryEdit[];
  next: FlowDocument;
  itemIds: string[];
}

/**
 * What the host answers to a change: the plan of the answer, or a refusal. The
 * host restores the plan of the last answer and shows the error on the item
 * itself (MDL-FR-PUCM); the canvas reads the answer only to go on with a
 * request that takes more than one edit (MDL-FR-EEDO).
 */
export type FlowPlanAnswer =
  | { ok: true; document: FlowDocument }
  | { ok: false; error: string };

/** MDL-FR-XQGD: a removal that the host decides on. */
export interface FlowRemovalRequest {
  itemId: string;
  name: string;
  kind: FlowItemKind;
  /** The items under it, which go with it. */
  count: number;
}

/** DBR-FR-AGIN .. DBR-FR-PMGS: the plan proposals as the board shows them. */
export interface FlowProposalView {
  /** DBR-FR-AGIN: the proposal mark of each item id. */
  marks: ReadonlyMap<string, "proposed" | "withdrawn">;
  /** DBR-FR-WKNI: the ghost cards by item id, and whether the plan names the kind of each. */
  ghosts: ReadonlyMap<string, { kindKnown: boolean }>;
  /** DBR-FR-PMGS: the item id of the row the review sheet selects, or null. */
  reviewed: string | null;
}

/** MDL-FR-XJBV: the canvas over a plan the host holds. */
export interface FlowPlanBinding {
  kind: "plan";
  /** DMS-FR-DMWD: the layout scope, `github:<project node id>`. */
  scope: string;
  /**
   * The plan to render: the last answer with the pending edits applied, or
   * null before the first answer. Its name and description are read-only on
   * the canvas (MDL-FR-KKYH).
   */
  document: FlowDocument | null;
  /** FLO-FR-KQCE / FLO-FR-IKMM: each item's state, or null before an answer. */
  states: readonly ItemDeliveryState[] | null;
  /** FLO-FR-KWFH: the overlaps among the leaves, or null before an answer. */
  overlaps: readonly FlowOverlap[] | null;
  /** FLO-FR-ULEM: the tracker facts of each item. */
  tracker: readonly TrackerFacts[];
  /** FLO-FR-ULEM: the repository the tracker tags name, as `owner/name`. */
  repository: string;
  /** MDL-FR-PUCM: the items with an edit pending. */
  pending?: ReadonlySet<string>;
  /** MDL-FR-PUCM / MDL-FR-HNLP: the error text to show on an item. */
  itemErrors?: Readonly<Record<string, string>>;
  /**
   * The store the layout is kept in. Give one that outlives the canvas, so a
   * canvas that mounts again shows the layout at once; without it, the canvas
   * keeps one of its own.
   */
  layouts?: FlowLayoutStore;
  /** DBR-FR-AGIN .. DBR-FR-PMGS: what the open plan proposals show on the board. */
  proposal?: FlowProposalView;
  /** MDL-FR-XJBV / MDL-FR-XWGP: each edit, in the order the author made it. */
  onEdit: (change: FlowPlanChange) => Promise<FlowPlanAnswer>;
  /** MDL-FR-XQGD: a removal, which the host asks about and applies. */
  onRemoveRequest: (request: FlowRemovalRequest) => void;
}

export type FlowBinding = FlowFileBinding | FlowPlanBinding;

/** The file binding of a Flow tab. */
export function fileBinding(flows: FlowSessionStore, flowId?: string): FlowFileBinding {
  return { kind: "file", flows, ...(flowId !== undefined ? { flowId } : {}) };
}
