/**
 * The vertical panel's pending reveal-and-select request (SNV-FR-64 ..
 * SNV-FR-68), and the two ways one is minted.
 */
import { useCallback, useRef, useState } from "react";
import type { MutableRefObject } from "react";
import type { PanelRevealRequest, PanelSurface } from "../../types";

export interface PanelRevealState {
  panelReveal: PanelRevealRequest | null;
  setPanelReveal: (request: PanelRevealRequest | null) => void;
  /**
   * The standing request, readable from the follow effect without making it a
   * dependency — which would re-run the follow every time a reveal was issued.
   */
  pendingReveal: MutableRefObject<PanelRevealRequest | null>;
  requestPanelReveal: (
    panel: PanelSurface,
    id: string,
    kind: { optimistic: boolean; focus: boolean },
  ) => void;
  setRevealArtifactId: (id: string | null) => void;
}

export function usePanelReveal(): PanelRevealState {
  /**
   * A pending reveal-and-select: which vertical panel is being asked, which item
   * it names, and a nonce making each request distinct.
   *
   * The nonce is what makes this a *request* rather than a standing value. Two
   * consecutive reveals of the same item — an Editor tab activated, a Dashboard
   * activated, that Editor tab activated again — carry the same id, and a
   * consumer keyed on the id alone would treat the second as nothing to do,
   * leaving SNV-FR-68's "the next selection happens at the next qualifying
   * activation" unmet. Keying on the nonce also keeps the property the id-keyed
   * version had: it does not change when a panel reloads, so a long-finished
   * reveal is never re-asserted over what the author has since selected.
   *
   * Produced by two unrelated routes — a creation reveal (NAW-FR-10 / NFI-FR-12
   * / NFW-FR-11 / NTS-FR-11), and the active tab being followed (SNV-FR-64).
   */
  const [panelReveal, setPanelReveal] = useState<PanelRevealRequest | null>(
    null,
  );
  const revealNonce = useRef(0);
  const pendingReveal = useRef<PanelRevealRequest | null>(null);
  /**
   * Ask a vertical panel to reveal and select an item. Which surface is on
   * screen is decided by the caller of this hook, which is the only place that
   * can open a hidden panel (SNV-FR-45); this records what to reveal once it is.
   */
  const requestPanelReveal = useCallback(
    (
      panel: PanelSurface,
      id: string,
      kind: { optimistic: boolean; focus: boolean },
    ) => {
      const next = { panel, id, nonce: ++revealNonce.current, ...kind };
      pendingReveal.current = next;
      setPanelReveal(next);
    },
    [],
  );
  /**
   * The creation reveals name a Project-panel node that may still be arriving,
   * and are the author asking to be taken to it — so they are optimistic and
   * they take focus (NFI-FR-12 / NFW-FR-11 / NAW-FR-10 / NTS-FR-11).
   */
  const setRevealArtifactId = useCallback(
    (id: string | null) => {
      if (id === null) {
        pendingReveal.current = null;
        setPanelReveal(null);
      }
      else requestPanelReveal("library", id, { optimistic: true, focus: true });
    },
    [requestPanelReveal],
  );

  return {
    panelReveal,
    setPanelReveal,
    pendingReveal,
    requestPanelReveal,
    setRevealArtifactId,
  };
}
