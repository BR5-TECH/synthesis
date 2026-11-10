/**
 * `FLO-flow.md`: the Flow document, version 2 — a delivery plan. This module
 * holds its serialization, its deserialization, and every plan rule that the
 * canvas answers locally while the author draws (FLO-FR-46).
 *
 * The document's shape is not owned here: `FGV-flow-graph-validation.md`
 * defines it and the backend validator decides whether a body is a Flow
 * (FGV-FR-01). This module writes to that schema and enforces the same rules,
 * so a refused edit costs no backend call (FLO NFR).
 *
 * The module has no React and no `api`, so its rules are pure functions. Each
 * edit returns a new document, the same document when it changes nothing, or a
 * typed refusal. None changes its input.
 */

export * from "./types";
export {
  absolutePosition,
  ancestorsOf,
  childrenOf,
  containerBounds,
  findItem,
  holdsAnything,
  inLineage,
  innermostContainerAt,
  isAncestor,
  itemBounds,
  itemExtent,
  leavesUnder,
  parentOf,
  permitsParent,
  subtreeOf,
  type FlowRect,
} from "./containment";
export { parseFlowDocument, type FlowParseResult } from "./parse";
export { serializeFlowDocument } from "./serialize";
export {
  docOrUnchanged,
  type FlowEditResult,
  type FlowRefusal,
  type FlowRefusalCode,
} from "./result";
export {
  addFlowReference,
  addFootprintEntry,
  addItem,
  addItemArtifact,
  allowedKinds,
  isCalendarDate,
  isFootprintPath,
  itemLinkingDraft,
  kindChangeDrops,
  removalScope,
  removeFlowReference,
  removeFootprintEntry,
  removeItem,
  removeItemArtifact,
  renameItem,
  setDue,
  setFlowDescription,
  setFlowName,
  setItemDescription,
  setItemDraft,
  setItemKind,
  setLocalOnly,
  type KindDroppedField,
} from "./items";
export { MIN_CONTAINER_SIZE, minContainerSize, moveItem, resizeItem } from "./layout";
export {
  connect,
  connectRefusal,
  edgeBetween,
  removeEdge,
  reverseEdge,
  setEdgeType,
} from "./edges";
export { connectSync, sameTrackerRef, setSyncLink, syncLinkOf } from "./sync";
export { cycleClosedBy, dependencyCycleEdges, waitsFor } from "./waitGraph";
