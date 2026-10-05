import { useEffect, useState } from "react";
import * as api from "../api";
import { onOperationProgress } from "../events";
import type { Operation } from "../types";

/**
 * STB-FR-15: the set of operations currently in flight.
 *
 * Established once from `"list in-flight operations"` on mount and kept current
 * from `"operation progress"` events thereafter, so the status bar shows work
 * that was already running when the window opened without waiting for that work
 * to emit anything (STB-FR-15).
 *
 * The subscription is attached **before** the initial read is applied, and the
 * read is merged into whatever events have already arrived rather than replacing
 * it. Applying the snapshot wholesale would resurrect an operation that started
 * and terminated in the gap between the two — the "never left rendered" case
 * STB-FR-15 rules out.
 */
export function useInFlightOperations(): Operation[] {
  const [operations, setOperations] = useState<Operation[]>([]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    // Ids the event stream has already reported as terminated. The initial
    // snapshot is filtered through this, so a race cannot leave a finished
    // operation stuck on the bar forever.
    const terminated = new Set<string>();

    const apply = (operation: Operation) => {
      if (!operation?.id) return;
      if (operation.state !== "running") terminated.add(operation.id);
      setOperations((current) => {
        const rest = current.filter((o) => o.id !== operation.id);
        // PRG-FR-08: a terminated operation leaves the in-flight set.
        if (operation.state !== "running") return rest;
        // PRG-FR-02: most-recently-started first.
        return [operation, ...rest].sort((a, b) => b.sequence - a.sequence);
      });
    };

    void onOperationProgress(apply).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    void api
      .listInFlightOperations()
      .then((initial) => {
        // PRG-FR-02 promises a list. Guarding is not paranoia about the
        // contract but about the boundary: a build whose backend predates the
        // command answers with nothing, and a status bar is the last surface
        // that should take the window down with it.
        if (cancelled || !Array.isArray(initial)) return;
        setOperations((current) => {
          const known = new Set(current.map((o) => o.id));
          const fresh = initial.filter(
            (o) => !known.has(o.id) && !terminated.has(o.id),
          );
          return [...current, ...fresh].sort((a, b) => b.sequence - a.sequence);
        });
      })
      .catch(() => {
        // PRG-FR-02 says the command never errors; if the backend is
        // nonetheless unreachable the bar simply shows nothing, which is the
        // same thing an idle application looks like (STB-FR-07).
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return operations;
}
