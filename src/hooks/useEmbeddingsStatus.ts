/**
 * The status of the embeddings index (`EMB-embeddings-index.md` EMB-FR-ZFGT),
 * read once on mount and then followed through `"embeddings status changed"`.
 *
 * Both Indexing sections show it (GLS-FR-RQFK, SET-FR-UNRY). The event carries
 * the whole status, so the latest one replaces what the hook holds.
 */
import { useEffect, useState } from "react";
import { getEmbeddingsStatus } from "../api";
import { onEmbeddingsStatusChanged } from "../events";
import { logWarn } from "../logging";
import type { EmbeddingsStatus } from "../types";

export function useEmbeddingsStatus(): EmbeddingsStatus | null {
  const [status, setStatus] = useState<EmbeddingsStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    // An event that arrives before the read answers is newer than the read, so
    // the read must not overwrite it.
    let fromEvent = false;
    const unlisten = onEmbeddingsStatusChanged((next) => {
      if (cancelled) return;
      fromEvent = true;
      setStatus(next);
    });
    getEmbeddingsStatus()
      .then((first) => {
        if (!cancelled && !fromEvent) setStatus(first);
      })
      .catch((e) => {
        logWarn(["frontend"], "embeddings status could not be read", {
          error: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn()).catch(() => undefined);
    };
  }, []);

  return status;
}
