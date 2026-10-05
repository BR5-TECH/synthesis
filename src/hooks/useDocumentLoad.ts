/**
 * Reads one document for a viewer tab and follows the collection while the tab
 * is open (DTV-FR-SQNK, DTV-FR-SAIG, DTV-FR-BEQL, DTV-FR-EKFD, PDV-FR-SLWS,
 * PDV-FR-YOQS).
 *
 * The two viewers share this state machine and differ only in the read they
 * make. The hook owns the one rule that decides what a viewer shows:
 *
 * - The first read runs on mount, and the viewer shows its loading text until
 *   it answers.
 * - `"documents changed"` carries the whole snapshot. When the viewer's own
 *   entry is missing or `unavailable`, the viewer shows its unavailable state at
 *   once and holds no stale content. When the entry is available with a revision
 *   other than the one on show, the hook reads again. The content on show stays
 *   in place while that read runs, so a refresh does not flash.
 * - A read that fails as unavailable or unknown shows the unavailable state. Any
 *   other failure shows the failed state and is logged by the viewer.
 *
 * The hook calls `read` and nothing else. In particular it never lists the
 * collection: the event is the only way it learns what the collection holds.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { onDocumentsChanged } from "../events";
import { isUnavailableRefusal } from "../state/documentErrors";
import type { DocumentEntry } from "../types";

export type DocumentLoadPhase = "loading" | "ready" | "unavailable" | "failed";

export interface DocumentLoad<T> {
  phase: DocumentLoadPhase;
  /** The content on show. Present only while `phase` is `ready`. */
  data: T | null;
}

export interface DocumentLoadCallbacks<T> {
  /** A read finished. */
  onLoaded?: (data: T) => void;
  /**
   * A read failed. `unavailable` is true for a refusal the viewer shows as its
   * unavailable state, and false for a failure it shows as a failed read.
   */
  onFailed?: (error: unknown, unavailable: boolean) => void;
}

/** What the entry of the document looks like to the follow rule. */
function entryKey(entry: DocumentEntry | null): string {
  return entry === null ? "absent" : `${entry.status}:${entry.revision ?? ""}`;
}

export function useDocumentLoad<T extends { revision: string }>(
  documentId: string,
  read: (id: string) => Promise<T>,
  callbacks: DocumentLoadCallbacks<T> = {},
): DocumentLoad<T> {
  const [state, setState] = useState<DocumentLoad<T>>({
    phase: "loading",
    data: null,
  });
  const [key, setKey] = useState<string | null>(null);
  const latest = useRef<{ seen: boolean; entry: DocumentEntry | null }>({
    seen: false,
    entry: null,
  });
  const shownRevision = useRef<string | null>(null);
  const sequence = useRef(0);
  const readRef = useRef(read);
  readRef.current = read;
  const callbacksRef = useRef(callbacks);
  callbacksRef.current = callbacks;

  const load = useCallback(() => {
    const mine = ++sequence.current;
    setState((current) =>
      current.phase === "ready" ? current : { phase: "loading", data: null },
    );
    readRef
      .current(documentId)
      .then((data) => {
        if (mine !== sequence.current) return;
        shownRevision.current = data.revision;
        setState({ phase: "ready", data });
        callbacksRef.current.onLoaded?.(data);
      })
      .catch((error: unknown) => {
        if (mine !== sequence.current) return;
        const unavailable = isUnavailableRefusal(error);
        shownRevision.current = null;
        setState({ phase: unavailable ? "unavailable" : "failed", data: null });
        callbacksRef.current.onFailed?.(error, unavailable);
      });
  }, [documentId]);

  // The follow starts before the first read, so a change that lands while the
  // read is pending is not missed.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onDocumentsChanged((snapshot) => {
      const entry = snapshot.documents.find((d) => d.id === documentId) ?? null;
      latest.current = { seen: true, entry };
      setKey(entryKey(entry));
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [documentId]);

  useEffect(() => {
    load();
    return () => {
      sequence.current += 1;
    };
  }, [load]);

  useEffect(() => {
    const { seen, entry } = latest.current;
    if (!seen) return;
    if (entry === null || entry.status === "unavailable") {
      sequence.current += 1;
      shownRevision.current = null;
      setState({ phase: "unavailable", data: null });
      return;
    }
    if (entry.revision !== shownRevision.current) load();
  }, [key, load]);

  return state;
}
