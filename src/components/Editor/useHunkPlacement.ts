/**
 * Placing a proposal's changes in the document the author is looking at
 * (`../../../specifications/ui/DCR-draft-change-review.md` DCR-FR-05,
 * DCR-FR-06).
 *
 * A change names the text it alters rather than a position (DCP-FR-HRQN), so it
 * is **found** here rather than mapped from an offset — the same
 * nearest-occurrence rule an anchored comment is placed by. That is what makes
 * the decorations follow the author's own edits around them: they type above a
 * change and it stays on the words it is about, instead of sliding onto
 * whatever now occupies the position the agent recorded.
 *
 * Its own module because it is a lookup in text and has nothing to do with the
 * editing surface it runs inside.
 */
import { useEffect, useLayoutEffect, useState } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";

import { docText } from "../findHighlight";
import { placeHunk, reduce } from "./hunkMatch";
import {
  hunkDecorationKey,
  HUNK_ATTR,
  type DecoratedHunk,
} from "../DraftDiscussion";
import type { EditorReview } from "./props";

export function useHunkPlacement(
  editor: TiptapEditor | null,
  review: EditorReview | undefined,
  /** Stands in for the mutable document, which is not a React value. */
  docVersion: number,
  mode: string,
): {
  placedHunks: DecoratedHunk[];
  /** DCR-FR-12: the placed change the action chip is on, if it could be placed. */
  focusedPlaced: DecoratedHunk | null;
  /** DCR-FR-12: a press on a change is what puts it under review. */
  focusHunkFromBody: (event: React.MouseEvent) => void;
} {
  /**
   * DCR-FR-05 / DCR-FR-06: place the proposal's changes in the document as it
   * stands, and push them into the decoration plugin.
   *
   * A change names the text it alters (DCP-FR-HRQN), so it is *found* here
   * rather than mapped from an offset — the same nearest-occurrence rule an
   * anchored comment is placed by. This is what makes the decorations follow
   * the author's own edits around them: they type above a change and it stays
   * on the words it is about, rather than sliding onto whatever now occupies
   * the position the agent recorded.
   *
   * A change the surface cannot place is dropped from the decorations and not
   * drawn somewhere it does not belong. It is still in the review — the review
   * bar counts it and the backend decides it — so nothing is lost by not
   * finding it here.
   */
  const [placedHunks, setPlacedHunks] = useState<DecoratedHunk[]>([]);
  useLayoutEffect(() => {
    if (!editor || editor.isDestroyed || !review) {
      setPlacedHunks((prev) => (prev.length === 0 ? prev : []));
      return;
    }
    const index = docText(editor.state.doc);
    // DCR-FR-05: the change's text was copied from the **source**, and this is
    // the **rendered** document — the backticks, the hashes and the bullets
    // that carried it are gone, and a paragraph break has become one newline.
    // Reduced once per recompute rather than per change, both sides meeting on
    // the words they share (`./hunkMatch`).
    const reducedDoc = reduce(index.text);
    const placed: DecoratedHunk[] = [];
    for (const hunk of review.hunks) {
      if (hunk.state === "accepted" || hunk.state === "rejected") continue;
      // DCR-FR-05: placed by its kind, in `./hunkMatch`.
      //
      // DCR-FR-31: a change this surface cannot place stays in the review,
      // counted and decidable, and the review bar says it is not drawn
      // (`onPlaced` below).
      const range = placeHunk(index, reducedDoc, hunk);
      if (range === null) continue;
      placed.push({
        id: hunk.id,
        kind: hunk.kind,
        state: hunk.state,
        from: range.from,
        to: range.to,
        after: hunk.after,
        lost: hunk.lost,
        position: hunk.position,
        total: hunk.total,
        agent: hunk.agent,
        draft: hunk.draft,
        editable: hunk.editable,
      });
    }
    setPlacedHunks((prev) =>
      prev.length === placed.length &&
      prev.every(
        (h, i) =>
          h.id === placed[i].id &&
          h.from === placed[i].from &&
          h.to === placed[i].to &&
          h.state === placed[i].state &&
          h.after === placed[i].after &&
          h.lost === placed[i].lost,
      )
        ? prev
        : placed,
    );
    // `docVersion` stands in for the mutable document.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, docVersion, review, mode]);

  /**
   * DCR-FR-05: push the placed changes and the focused one into the plugin.
   *
   * A meta-only transaction changes no document, so this adds no history step
   * (EDT-FR-23) and does not re-enter `onUpdate` — the prompt is byte-for-byte
   * what it was while a proposal stands (DCR-FR-02).
   */
  // DCR-FR-31: what the document could draw, told to whoever states it.
  const placedIds = placedHunks.map((h) => h.id).join("\u0000");
  useEffect(() => {
    review?.onPlaced(placedIds === "" ? [] : placedIds.split("\u0000"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [placedIds]);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const tr = editor.state.tr.setMeta(hunkDecorationKey, {
      hunks: placedHunks,
      focused: review?.focused ?? null,
      onEdit: review?.onEdit,
    });
    tr.setMeta("addToHistory", false);
    editor.view.dispatch(tr);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor, placedHunks, review?.focused]);

  /**
   * DCR-FR-12: a press on a change is what puts it under review.
   *
   * On the decorated element rather than on a control of its own: the change is
   * the thing the author is looking at, and needing to find a handle for it
   * would put a control between them and the text.
   */
  /** DCR-FR-12: the placed change the chip is on, if it could be placed. */
  const focusedPlaced =
    placedHunks.find((h) => h.id === review?.focused) ?? null;

  const focusHunkFromBody = (event: React.MouseEvent) => {
    if (!review) return;
    const el = (event.target as Element | null)?.closest?.(`[${HUNK_ATTR}]`);
    const id = el?.getAttribute(HUNK_ATTR);
    if (id) review.onFocus(id);
  };


  return { placedHunks, focusedPlaced, focusHunkFromBody };
}
