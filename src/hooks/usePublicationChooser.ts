/**
 * The publication chooser's lists while it is open (`NAW-new-artifact.md`
 * NAW-FR-HNVR, NAW-FR-NDDO, NAW-FR-HRXE, served by
 * `../../specifications/core/GHP-github-publication.md` GHP-FR-JTYL,
 * GHP-FR-SIMK, GHP-FR-JIAU).
 *
 * The chooser opens on the backend's metadata cache where it holds lists for
 * the chooser's remote, and requests one background refresh. Where the cache
 * holds none, it reads the lists itself. While it is open, it follows
 * `"publication metadata changed"` for its remote.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  getPublicationMetadataCache,
  loadPublicationMetadata,
  refreshPublicationMetadata,
} from "../api";
import { onPublicationMetadataChanged } from "../events";
import { logDebug, logError, logWarn } from "../logging";
import { githubPollingErrorMessage } from "../state/githubPolling";
import type { PublicationMetadata } from "../types";

/**
 * NAW-FR-HNVR: the publication chooser while it is open. The metadata comes
 * from the cache or from one read (`ready`), or its read failed outright
 * (`failed`), in which case the chooser still offers root publication
 * (NAW-FR-TCQB). `refreshing` is true while the refresh of NAW-FR-NDDO runs.
 */
export interface PublicationChooserState {
  remoteName: string;
  persist: boolean;
  status: "loading" | "ready" | "failed";
  metadata: PublicationMetadata | null;
  error: string | null;
  refreshing: boolean;
}

export interface UsePublicationChooserResult {
  chooser: PublicationChooserState | null;
  openChooser: (remoteName: string, persist: boolean) => Promise<void>;
  /** NAW-FR-VUCK: close it, so no answer that is still due reopens it. */
  closeChooser: () => void;
}

/** The cached lists of `remoteName`, or null where the read fails. */
async function cachedMetadata(
  draftId: string,
  remoteName: string,
): Promise<PublicationMetadata | null> {
  try {
    return await getPublicationMetadataCache(remoteName);
  } catch (e) {
    logWarn(["frontend", "remote"], "publication metadata cache could not be read", {
      draftId,
      error: String(e),
    });
    return null;
  }
}

export function usePublicationChooser(draftId: string | null): UsePublicationChooserResult {
  const [chooser, setChooser] = useState<PublicationChooserState | null>(null);
  // A chooser closed before an answer lands must not be reopened by it.
  const request = useRef(0);

  /** Apply `change` only while the chooser of `ticket` is still open. */
  const update = useCallback(
    (ticket: number, change: (state: PublicationChooserState) => PublicationChooserState) => {
      if (ticket !== request.current) return;
      setChooser((state) => (state ? change(state) : state));
    },
    [],
  );

  /** NAW-FR-NDDO: one background refresh; a failure keeps the lists. */
  const refresh = useCallback(
    async (ticket: number, remoteName: string) => {
      try {
        const answer = await refreshPublicationMetadata();
        logDebug(["frontend", "remote"], "publication metadata refresh ended", {
          outcome: answer.outcome,
          errorCode: answer.errorCode,
        });
        const fresh =
          answer.remoteName === remoteName && answer.metadata ? answer.metadata : null;
        update(ticket, (state) => ({
          ...state,
          refreshing: false,
          metadata: fresh ?? state.metadata,
        }));
      } catch (e) {
        logWarn(["frontend", "remote"], "publication metadata refresh failed", {
          error: String(e),
        });
        update(ticket, (state) => ({ ...state, refreshing: false }));
      }
    },
    [update],
  );

  const openChooser = useCallback(
    async (remoteName: string, persist: boolean) => {
      if (!draftId) return;
      const ticket = ++request.current;
      setChooser({
        remoteName,
        persist,
        status: "loading",
        metadata: null,
        error: null,
        refreshing: false,
      });
      const cached = await cachedMetadata(draftId, remoteName);
      if (ticket !== request.current) return;
      if (cached) {
        // NAW-FR-HNVR / NAW-FR-WMEP: the cached lists render at once.
        setChooser({
          remoteName,
          persist,
          status: "ready",
          metadata: cached,
          error: null,
          refreshing: true,
        });
        await refresh(ticket, remoteName);
        return;
      }
      try {
        const metadata = await loadPublicationMetadata(draftId, remoteName);
        update(ticket, (state) => ({ ...state, status: "ready", metadata, error: null }));
      } catch (e) {
        logError(["frontend", "remote"], "publication metadata could not be read", {
          draftId,
          error: String(e),
        });
        update(ticket, (state) => ({
          ...state,
          status: "failed",
          metadata: null,
          error: githubPollingErrorMessage(e),
        }));
      }
    },
    [draftId, refresh, update],
  );

  const closeChooser = useCallback(() => {
    request.current += 1;
    setChooser(null);
  }, []);

  // NAW-FR-HRXE: follow the cache for the chooser's remote while it is open.
  const open = chooser !== null;
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    const subscription = onPublicationMetadataChanged((payload) => {
      if (cancelled || !payload.metadata) return;
      const metadata = payload.metadata;
      setChooser((state) =>
        state && state.remoteName === payload.remoteName && state.status !== "loading"
          ? { ...state, status: "ready", metadata, error: null }
          : state,
      );
    });
    return () => {
      cancelled = true;
      void subscription.then((unlisten) => unlisten());
    };
  }, [open]);

  return { chooser, openChooser, closeChooser };
}
