import { useCallback, useEffect, useRef, useState } from "react";
import { targetLines } from "../../diff/targetEdit";

/**
 * `text`, but only once the author has stopped changing it.
 *
 * The comparison is re-derived from the target (DFV-FR-43), and doing that on
 * every keystroke would re-align and re-word-diff the whole file per character.
 * Settling it is also what keeps the caret still: the rows the author is typing
 * into are not rebuilt under them mid-word.
 *
 * A load adopted into the session (`seedToken`) settles immediately — it is not
 * the author typing, and waiting would show the pre-load comparison for a beat.
 */
export function useSettledText(
  text: string,
  seedToken: number,
  editEpoch: number,
  held: boolean,
): { settled: string; adopt: (next?: string) => void } {
  const [settled, setSettled] = useState(text);
  const textRef = useRef(text);
  textRef.current = text;
  /**
   * Adopt the target, whatever the rest says.
   *
   * `next` is given by a caller that has just replaced the buffer in the same
   * tick — a history traversal does exactly that — because the ref this reads
   * from is only refreshed by a render, and no render has happened yet. Reading
   * the ref there would settle on the document as it stood *before* the undo.
   */
  const adopt = useCallback((next?: string) => {
    setSettled((current) => {
      const value = next ?? textRef.current;
      return current === value ? current : value;
    });
  }, []);
  const seedRef = useRef(seedToken);
  if (seedRef.current !== seedToken) {
    seedRef.current = seedToken;
    if (settled !== text) setSettled(text);
  }
  /**
   * An edit that changed the NUMBER of lines is adopted at once rather than
   * settled.
   *
   * Every editable row addresses the target by the line it occupies
   * (DFV-FR-44), so a split or a join shifts the index of every row below it.
   * Waiting the rest out would leave those rows addressing the positions they
   * held before — and the next edit to any of them would land on the wrong
   * line. Re-marking is what the rest is for; re-indexing cannot wait for it.
   */
  const structural = !held && countLines(text) !== countLines(settled);
  useEffect(() => {
    if (held) return;
    if (settled === text) return;
    if (structural) {
      setSettled(text);
      return;
    }
    const timer = setTimeout(() => setSettled(text), DERIVE_SETTLE_MS);
    return () => clearTimeout(timer);
    // `editEpoch` is a dependency so the rest RESTARTS on each keystroke: a
    // burst of typing then re-marks once at the end of it rather than at a
    // fixed interval through it.
  }, [held, text, settled, structural, editEpoch]);
  return { settled: structural ? text : settled, adopt };
}

/** How many lines a revision has, for the structural check above. */
function countLines(text: string): number {
  return targetLines(text).length;
}

/**
 * How long the target must be still before the comparison is re-derived over it.
 *
 * Short enough that the marking follows the author's thinking rather than their
 * patience, long enough that a burst of typing is one derivation.
 */
export const DERIVE_SETTLE_MS = 250;
