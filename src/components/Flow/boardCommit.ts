/**
 * DBR-FR-KTJS / DBR-FR-BBDZ / DBR-FR-CVJY: land a drop of the board. A drop
 * into another container is an edit; the new stack order is the layout,
 * stored once. A refused drop puts the item back and says why.
 */
import { logDebug, logInfo, logWarn } from "../../logging";
import {
  boardOrders,
  docOrUnchanged,
  reparentItem,
  type FlowDocument,
  type FlowItem,
  type FlowRefusal,
  type StoredLayout,
} from "../../state/flowDocument";
import { isEditHeld, waitEditGate } from "../../state/mainBranchWrite";
import type { FlowPlanAnswer } from "./binding";
import { boardDropResult, type BoardDrop } from "./boardDrop";

export interface BoardCommitContext {
  doc: FlowDocument;
  stored: StoredLayout;
  item: FlowItem;
  drop: BoardDrop;
  /** The binding of the canvas: a Flow file applies an edit, a plan answers it. */
  mode: "file" | "plan";
  edit: (fn: (d: FlowDocument) => FlowDocument) => boolean;
  editAndWait: (fn: (d: FlowDocument) => FlowDocument) => Promise<FlowPlanAnswer> | null;
  commitStored: (next: StoredLayout) => void;
  /** The stored layout now, for a restore after an answer. */
  latestStored: () => StoredLayout | null;
  refuse: (refusal: FlowRefusal | null) => void;
}

export function commitBoardDrop(ctx: BoardCommitContext): void {
  const { doc, stored, item, drop } = ctx;
  const parentId = drop.holder ?? undefined;
  let after = doc;
  if (parentId !== item.parentId) {
    const first = reparentItem(doc, item.id, parentId);
    if (!first.ok) {
      logInfo(["frontend"], "board drop refused by the plan rules", { kind: item.kind, code: first.refusal.code });
      ctx.refuse(first.refusal);
      return;
    }
    after = first.doc;
  }
  const result = boardDropResult(doc, after, stored, boardOrders(doc, stored), item.id, drop);
  ctx.refuse(null);
  logDebug(["frontend"], "board drop", {
    kind: item.kind,
    moves: result.moves,
    reordered: result.stored !== null,
    touched: result.touched.length,
  });
  if (!result.moves) {
    if (result.stored) ctx.commitStored(result.stored);
    return;
  }
  const move = (d: FlowDocument) => docOrUnchanged(d, reparentItem(d, item.id, parentId));
  /** The move and its layout, or false where the edit did not apply. */
  const land = (): boolean => {
    if (ctx.mode === "file") {
      if (!ctx.edit(move)) return false;
      if (result.stored) ctx.commitStored(result.stored);
      return true;
    }
    const answer = ctx.editAndWait(move);
    if (!answer) return false;
    if (result.stored) ctx.commitStored(result.stored);
    // DBR-FR-CVJY: a refused move restores the plan of the last answer, and the
    // item takes its old slot again.
    void answer.then((a) => {
      const latest = ctx.latestStored();
      if (a.ok || !latest) return;
      logWarn(["frontend"], "board move refused; the item takes its old slot", { kind: item.kind });
      const items = { ...latest.items };
      for (const id of result.touched) {
        if (stored.items[id]) items[id] = stored.items[id];
        else delete items[id];
      }
      ctx.commitStored({ items });
    });
    return true;
  };
  // EDT-FR-UPNU: the gate can hold the edit while the branch is read or while
  // the author confirms a write to a main branch. The move and its layout go in
  // once the gate lets them, and not at all after a cancel.
  if (!land() && isEditHeld()) {
    logDebug(["frontend"], "board drop waits for the write gate", { kind: item.kind });
    void waitEditGate().then((confirmed) => {
      if (confirmed) land();
    });
  }
}
