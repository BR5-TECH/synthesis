import { useEffect, useMemo, useRef, useState } from "react";

import * as api from "../../api";
import { onDraftsChanged, onGraduationRunChanged } from "../../events";
import { errorLogCode } from "../../errorLogCode";
import { logWarn } from "../../logging";
import { serializeFlowDocument, type FlowDocument } from "../../state/flowDocument";
import type { FlowOverlap, ItemDeliveryState } from "../../types";

/** FLO-FR-STUN: the short rest after an edit before the state is asked again. */
export const DELIVERY_REST_MS = 400;

export interface DeliveryView {
  /** Each item's delivery state, or null before the first answer. */
  states: Map<string, ItemDeliveryState> | null;
  /** The overlaps among the leaf items, or null before the first answer. */
  overlaps: FlowOverlap[] | null;
}

/**
 * FLO-FR-STUN: the delivery state (FDL) and the overlaps (FOA) of the plan.
 *
 * The canvas asks for both with the serialized plan when the tab opens, a short
 * rest after each edit, and on each `"drafts changed"` and `"graduation run
 * changed"` event. A newer answer replaces an older one, and an answer for a
 * body that is no longer the plan is discarded. The canvas does not wait for
 * either answer (FLO NFR).
 */
export function useDeliveryState(doc: FlowDocument | null): DeliveryView {
  const [states, setStates] = useState<Map<string, ItemDeliveryState> | null>(null);
  const [overlaps, setOverlaps] = useState<FlowOverlap[] | null>(null);
  /** The body the canvas holds now; an answer for another body is stale. */
  const bodyRef = useRef<string | null>(null);
  const seq = useRef({ issued: 0, stateApplied: 0, overlapApplied: 0 });
  const alive = useRef(true);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const body = useMemo(() => (doc ? serializeFlowDocument(doc) : null), [doc]);

  const ask = useRef((asked: string) => {
    const n = ++seq.current.issued;
    const current = () => alive.current && bodyRef.current === asked;
    void Promise.resolve()
      .then(() => api.loadFlowDeliveryState(asked))
      .then((answer) => {
        if (!current() || n < seq.current.stateApplied || !answer?.items) return;
        seq.current.stateApplied = n;
        setStates(new Map(answer.items.map((s) => [s.itemId, s])));
      })
      .catch((e) => {
        logWarn(["frontend"], "flow delivery state failed", { code: errorLogCode(e) });
      });
    void Promise.resolve()
      .then(() => api.analyzeFlowOverlap(asked))
      .then((answer) => {
        if (!current() || n < seq.current.overlapApplied || !answer?.overlaps) return;
        seq.current.overlapApplied = n;
        setOverlaps(answer.overlaps);
      })
      .catch((e) => {
        logWarn(["frontend"], "flow overlap analysis failed", { code: errorLogCode(e) });
      });
  });

  useEffect(() => {
    if (body === null) return;
    const first = bodyRef.current === null;
    if (body === bodyRef.current) return;
    bodyRef.current = body;
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    if (first) {
      ask.current(body);
      return;
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      if (bodyRef.current !== null) ask.current(bodyRef.current);
    }, DELIVERY_REST_MS);
  }, [body]);

  useEffect(() => {
    alive.current = true;
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const again = () => {
      if (bodyRef.current !== null) ask.current(bodyRef.current);
    };
    const keep = (p: Promise<() => void>) => {
      void p.then((fn) => {
        if (cancelled) fn();
        else unlisten.push(fn);
      });
    };
    keep(onDraftsChanged(again));
    keep(onGraduationRunChanged(again));
    return () => {
      cancelled = true;
      alive.current = false;
      unlisten.forEach((fn) => fn());
      if (timer.current) clearTimeout(timer.current);
      timer.current = null;
    };
  }, []);

  return { states, overlaps };
}
