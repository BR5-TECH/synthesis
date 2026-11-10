/** Small helpers that the edit operations share. */

import type { FlowDocument, FlowItem, FlowItemKind } from "./types";

/**
 * Make an id with `prefix` that `taken` does not hold. The search starts at
 * `from` and is checked against the ids the document holds, not its size, so
 * a removal followed by an add cannot make an id that is in use.
 */
export function mintId(prefix: string, taken: ReadonlySet<string>, from: number): string {
  let n = Math.max(1, from);
  while (taken.has(`${prefix}${n}`)) n += 1;
  return `${prefix}${n}`;
}

/** The id prefix of each kind, as in the FGV-FR-01 example. */
export const ITEM_ID_PREFIX: Readonly<Record<FlowItemKind, string>> = {
  milestone: "m",
  epic: "ep",
  feature: "f",
  task: "t",
  defect: "x",
};

/**
 * Replace one item with what `patch` returns. Gives back `doc` itself when the
 * item is not there or the patch returns the same object.
 */
export function patchItem(
  doc: FlowDocument,
  id: string,
  patch: (item: FlowItem) => FlowItem,
): FlowDocument {
  let changed = false;
  const items = doc.items.map((i) => {
    if (i.id !== id) return i;
    const next = patch(i);
    if (next !== i) changed = true;
    return next;
  });
  return changed ? { ...doc, items } : doc;
}

/** A copy of `item` without the named fields. */
export function without<K extends keyof FlowItem>(
  item: FlowItem,
  ...keys: K[]
): FlowItem {
  const out: Partial<FlowItem> = { ...item };
  for (const k of keys) delete out[k];
  return out as FlowItem;
}
