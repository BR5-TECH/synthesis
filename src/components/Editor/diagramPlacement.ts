/**
 * DCR-FR-JKVA / DCR-FR-BYSX: finding the diagram fence that a diagram hunk
 * changes or removes in the document the author is looking at.
 *
 * A diagram hunk names the fence it alters by the source of that fence, which
 * the surface reads from the fence text of the hunk. It is found by that source
 * and not by a position, as any hunk is (DCP-FR-HRQN). An addition names no
 * fence, so it is placed as any insertion is, by its lead (DCR-FR-TSNW).
 */
import type { Node as PMNode } from "@tiptap/pm/model";

import { fenceStarts, sourceAt } from "../DiagramEditor/fence";
import type { DocText, PosRange } from "../findHighlight";

/**
 * The range of the fence whose source is `oldSource`, or null where the
 * document holds none. Sources are compared after trimming, because the fence
 * text of a hunk and the document can differ in the blank lines at the edges.
 * Where several fences hold it, the one nearest the hint wins.
 */
export function placeDiagramFence(
  doc: PMNode,
  index: DocText,
  oldSource: string | null | undefined,
  hint: number,
): PosRange | null {
  if (oldSource === null || oldSource === undefined) return null;
  const wanted = oldSource.trim();
  let best: PosRange | null = null;
  let bestDistance = Infinity;
  for (const pos of fenceStarts(doc)) {
    if (sourceAt(doc, pos).trim() !== wanted) continue;
    const node = doc.nodeAt(pos);
    if (node === null) continue;
    // The text offset of the fence, which is where its source starts.
    const segment = index.segments.find((s) => s.from > pos);
    const offset = segment?.offset ?? index.text.length;
    const distance = Math.abs(offset - hint);
    if (distance < bestDistance) {
      best = { from: pos, to: pos + node.nodeSize };
      bestDistance = distance;
    }
  }
  return best;
}
