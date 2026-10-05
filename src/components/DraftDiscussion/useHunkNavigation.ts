/**
 * DCR-FR-30: moving between the proposal's changes, and the accelerators that
 * decide one.
 *
 * The move keys are always live; the decide keys are live **only** while the
 * hunk review holds focus. The editing surface binds Enter and Backspace
 * already, so an unguarded accelerator would decide a change every time the
 * author started a new paragraph — which is the one failure this guard exists
 * to prevent.
 */
import { useEffect } from "react";

import {
  draftDiscussionState,
  setFocusedHunk,
} from "../../state/draftDiscussion";
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

export function useHunkNavigation({
  draftId,
  hunks,
  focused,
  active,
  onAccept,
  onReject,
}: {
  draftId: string;
  /** The undecided changes, in proposal order. */
  hunks: readonly MovableHunk[];
  /** The change the review is on, which a move starts from. */
  focused: string | null;
  /** False while there is nothing to review, which unbinds every key. */
  active: boolean;
  onAccept: (hunkId: string) => void;
  onReject: (hunkId: string) => void;
}): void {
  useEffect(() => {
    if (!active || hunks.length === 0) return;
    const onKey = (event: KeyboardEvent) => {
      const view = draftDiscussionState(draftId);

      // DCR-FR-30: the two move keys. `Alt` because neither the editing surface
      // nor the shell claims it, so the move is available while the caret is in
      // the prose — which is where it is most needed.
      if (event.altKey && (event.key === "ArrowUp" || event.key === "ArrowDown")) {
        event.preventDefault();
        const next = stepTo(hunks, focused, event.key === "ArrowDown" ? 1 : -1);
        if (next !== null) setFocusedHunk(draftId, next);
        return;
      }

      // The decide keys, and the guard that is the whole of DCR-FR-30's second
      // sentence: they act only while hunk navigation holds focus.
      if (!view.hunkFocusHeld || view.focusedHunkId === null) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.key === "Enter") {
        event.preventDefault();
        onAccept(view.focusedHunkId);
      } else if (event.key === "Backspace") {
        event.preventDefault();
        onReject(view.focusedHunkId);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [draftId, hunks, focused, active, onAccept, onReject]);
}
