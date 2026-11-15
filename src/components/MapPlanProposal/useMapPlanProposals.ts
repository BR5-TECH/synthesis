/**
 * The proposals of one Map discussion, read with `"list map plan proposals
 * (discussion id)"` and kept current by `"map plan proposals changed"`
 * (`MPV-map-proposal-review.md` MPV-FR-IBSF, MPV-FR-CZDO).
 *
 * Every card of a thread reads the same list, so reads of one discussion that
 * overlap are one call.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { listMapPlanProposals } from "../../api";
import { onMapPlanProposalsChanged } from "../../events";
import { logWarn } from "../../logging";
import type { MapPlanProposal } from "../../types";

const inFlight = new Map<string, Promise<MapPlanProposal[]>>();

function read(discussionId: string, fresh: boolean): Promise<MapPlanProposal[]> {
  // A read that an event asked for is never joined to one that began before it.
  if (fresh) return listMapPlanProposals(discussionId);
  const pending = inFlight.get(discussionId);
  if (pending) return pending;
  const call = listMapPlanProposals(discussionId).finally(() => {
    if (inFlight.get(discussionId) === call) inFlight.delete(discussionId);
  });
  inFlight.set(discussionId, call);
  return call;
}

export interface MapPlanProposalsView {
  /** Null until the first read lands. */
  proposals: MapPlanProposal[] | null;
  /** True when the last read failed and no proposal is held. */
  unavailable: boolean;
  /** Put the proposal that a decision answered in place of the held one. */
  replace: (proposal: MapPlanProposal) => void;
  /** Read the proposals again. */
  reload: (fresh?: boolean) => void;
}

export function useMapPlanProposals(discussionId: string): MapPlanProposalsView {
  const [proposals, setProposals] = useState<MapPlanProposal[] | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  const alive = useRef(true);
  /** Counts the reads, so an older answer never replaces a newer one. */
  const sequence = useRef(0);

  const reload = useCallback(
    (fresh = false) => {
    const mine = ++sequence.current;
    read(discussionId, fresh).then(
      (list) => {
        if (!alive.current || mine !== sequence.current) return;
        setProposals(Array.isArray(list) ? list : []);
        setUnavailable(false);
      },
      () => {
        if (!alive.current || mine !== sequence.current) return;
        logWarn(["frontend"], "map plan proposals could not be read", {});
        setUnavailable(true);
      },
    );
    },
    [discussionId],
  );

  useEffect(() => {
    alive.current = true;
    setProposals(null);
    setUnavailable(false);
    reload(false);
    return () => {
      alive.current = false;
    };
  }, [reload]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onMapPlanProposalsChanged((payload) => {
      if (payload.discussionId === discussionId) reload(true);
    }).then(
      (fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      },
      () => {},
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [discussionId, reload]);

  const replace = useCallback((proposal: MapPlanProposal) => {
    sequence.current += 1;
    setProposals((held) =>
      held === null ? [proposal] : held.map((p) => (p.id === proposal.id ? proposal : p)),
    );
  }, []);

  return { proposals, unavailable, replace, reload };
}
