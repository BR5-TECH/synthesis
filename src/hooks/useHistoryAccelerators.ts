import { useEffect, type RefObject } from "react";

/**
 * Claim undo and redo for a surface that edits a document through a history it
 * does not own (`EDT-editor.md` EDT-FR-15, EDT-FR-22).
 *
 * Two routes reach a webview and both have to be taken, in the **capture**
 * phase, or a `contenteditable` or a ProseMirror instance underneath runs its
 * own surface-local stack first and that stack immediately disagrees with the
 * document's:
 *
 * - the accelerators (⌘Z / ⇧⌘Z / ⌘Y) as `keydown`, and
 * - the native Edit menu's Undo and Redo (`SNV-shell-navigation.md` SNV-FR-14),
 *   which arrive as a `beforeinput` carrying `historyUndo` / `historyRedo`.
 *
 * The default is always prevented — including while the surface is blocked — so
 * a surface-local stack can never diverge from the one the document keeps.
 *
 * Shared rather than written twice because the Diff tab's target
 * (`DFV-diff-viewer.md` DFV-FR-50) and the draft-change review's candidate
 * (`DCR-draft-change-review.md` DCR-FR-24) make the same promise about the same
 * two routes, and a second copy is a second place for one of them to be
 * forgotten — which is exactly how a surface ends up with undo that appears to
 * work and reverses the wrong thing.
 */
export function useHistoryAccelerators(
  ref: RefObject<HTMLElement | null>,
  traverse: (direction: "undo" | "redo") => void,
): void {
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (!e.metaKey && !e.ctrlKey) return;
      const key = e.key.toLowerCase();
      const isRedo = key === "y" || (key === "z" && e.shiftKey);
      const isUndo = key === "z" && !e.shiftKey;
      if (!isUndo && !isRedo) return;
      e.preventDefault();
      e.stopPropagation();
      traverse(isRedo ? "redo" : "undo");
    };
    const onBeforeInput = (e: Event) => {
      const type = (e as InputEvent).inputType;
      if (type !== "historyUndo" && type !== "historyRedo") return;
      e.preventDefault();
      e.stopPropagation();
      traverse(type === "historyRedo" ? "redo" : "undo");
    };
    el.addEventListener("keydown", onKeyDown, true);
    el.addEventListener("beforeinput", onBeforeInput, true);
    return () => {
      el.removeEventListener("keydown", onKeyDown, true);
      el.removeEventListener("beforeinput", onBeforeInput, true);
    };
  }, [ref, traverse]);
}
