/**
 * Where focus goes when a change workspace opens
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DMGL;
 * `SMI-specification-map-inspector.md` SMI-FR-UKIV).
 *
 * The viewport mounts the active tab alone, so the control that opened the
 * tab (a map chip's Open change, for example) goes away with its surface, and
 * focus falls to the document body. The tab then takes focus, so the keyboard
 * stays in the content the author opened. Focus that is on a control still
 * stays there: an open from a panel that stays on the screen keeps it.
 */
import { useEffect, useRef } from "react";

export function useFocusOnOpen<T extends HTMLElement>(openSeq: number) {
  const ref = useRef<T | null>(null);
  useEffect(() => {
    const active = document.activeElement;
    if (active !== null && active !== document.body) return;
    ref.current?.focus({ preventScroll: true });
  }, [openSeq]);
  return ref;
}
