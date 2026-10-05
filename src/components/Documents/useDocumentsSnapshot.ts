/**
 * The panel's copy of the collection's snapshot
 * (`../../../specifications/ui/DPN-documents-panel.md` DPN-FR-FAOR).
 *
 * The first `"list documents"` call fills it. A failed call leaves an error that
 * **Retry** clears by calling again. The panel follows `"documents changed"` from
 * its mount, and a payload replaces the snapshot whole without another call. A
 * payload that arrives while the first call is still pending wins, because it
 * is newer than what that call will return.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../api";
import { onDocumentsChanged } from "../../events";
import { logError } from "../../logging";
import { documentErrorCode } from "../../state/documentErrors";
import type { DocumentsSnapshot } from "../../types";

export type SnapshotStatus = "loading" | "ready" | "error";

export interface DocumentsSnapshotState {
  status: SnapshotStatus;
  snapshot: DocumentsSnapshot | null;
  /** The newest snapshot, readable between two renders. */
  latest: () => DocumentsSnapshot | null;
  /** Replace the snapshot with the answer of a command. */
  replace: (next: DocumentsSnapshot) => void;
  retry: () => void;
}

export function useDocumentsSnapshot(): DocumentsSnapshotState {
  const [state, setState] = useState<{
    status: SnapshotStatus;
    snapshot: DocumentsSnapshot | null;
  }>({ status: "loading", snapshot: null });
  const latestRef = useRef<DocumentsSnapshot | null>(null);
  // Counts the writes that came from anywhere but the list call, so a stale
  // list answer can tell that something newer has already landed.
  const writes = useRef(0);

  const replace = useCallback((next: DocumentsSnapshot) => {
    writes.current += 1;
    latestRef.current = next;
    setState({ status: "ready", snapshot: next });
  }, []);

  const list = useCallback(() => {
    const before = writes.current;
    api
      .listDocuments()
      .then((next) => {
        if (writes.current !== before) return;
        latestRef.current = next;
        setState({ status: "ready", snapshot: next });
      })
      .catch((error: unknown) => {
        if (writes.current !== before) return;
        // Only the typed code is logged. A free-text refusal could carry a path.
        logError(["frontend"], "documents could not be listed", {
          reason: documentErrorCode(error) ?? "unknown",
        });
        setState({ status: "error", snapshot: null });
      });
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onDocumentsChanged((next) => {
      if (!cancelled) replace(next);
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    list();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [list, replace]);

  const retry = useCallback(() => {
    setState({ status: "loading", snapshot: null });
    list();
  }, [list]);

  return {
    status: state.status,
    snapshot: state.snapshot,
    latest: () => latestRef.current,
    replace,
    retry,
  };
}
