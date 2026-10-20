/**
 * DCR-FR-30: the accelerators that move the review between the proposal's
 * changes.
 *
 * No accelerator decides a change. Enter and Backspace are typing keys, so a
 * decide key on either decides a change while the author writes a message or a
 * paragraph. Accept and Reject are the chip's and the review bar's controls,
 * which the keyboard reaches as it reaches any other button (DCR-FR-28).
 */
import { useEffect } from "react";

import { setFocusedHunk } from "../../state/draftDiscussion";
/**
 * What a move needs to know about a change: which one it is, and where it
 * stands in the order. Anything more would tie this to how a change is
 * rendered, which is not what moving between them is about.
 */
export interface MovableHunk {
  id: string;
}

/**
 * DCR-FR-30: the change a move starts from, and the one it lands on.
 *
 * Exported so the review bar's own controls move by exactly the rule the
 * accelerators move by, rather than by a second one that agrees with it until
 * it does not.
 */
export function stepTo(
  hunks: readonly MovableHunk[],
  focused: string | null,
  step: 1 | -1,
): string | null {
  // A proposal holding one undecided change has nowhere to move: the review is
  // already on it. Null rather than that change's own id, so the accelerators
  // and the bar's controls say the same thing — a bar that offers no move
  // beside a key that quietly re-sets the focus is two rules, not one.
  if (hunks.length <= 1) return null;
  const at = hunks.findIndex((h) => h.id === focused);
  // A review that has not been moved yet is **on** the first change — the bar
  // says so and the chip is drawn there — so a move starts from it. Starting
  // from nowhere would spend the first press arriving where the author already
  // is, which reads as a control that does nothing.
  const from = at < 0 ? 0 : at;
  const next = (from + step + hunks.length) % hunks.length;
  return hunks[next].id;
}

/**
 * DCR-FR-30: a key typed into a text field of the discussion column.
 *
 * The prose is a contenteditable surface, not a field, so the moves stay live
 * there. A field keeps the keys it has: in a composer, Alt+Up and Alt+Down move
 * the caret.
 */
function isTextField(target: EventTarget | null): boolean {
  return target instanceof HTMLTextAreaElement || target instanceof HTMLInputElement;
}

export function useHunkNavigation({
  draftId,
  hunks,
  focused,
  active,
}: {
  draftId: string;
  /** The changes a move visits, in proposal order (DCR-FR-QPRV). */
  hunks: readonly MovableHunk[];
  /** The change the review is on, which a move starts from. */
  focused: string | null;
  /** False while there is nothing to review, which unbinds every key. */
  active: boolean;
}): void {
  useEffect(() => {
    if (!active || hunks.length === 0) return;
    const onKey = (event: KeyboardEvent) => {
      // `Alt` because neither the editing surface nor the shell claims it, so
      // the move is available while the caret is in the prose — which is where
      // it is most needed.
      if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
      if (isTextField(event.target)) return;
      event.preventDefault();
      const next = stepTo(hunks, focused, event.key === "ArrowDown" ? 1 : -1);
      if (next !== null) setFocusedHunk(draftId, next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [draftId, hunks, focused, active]);
}
