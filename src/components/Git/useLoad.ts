import { useCallback, useEffect, useRef, useState } from "react";

/**
 * One region's read: what it shows now, and whether that is still current.
 *
 * `error` is the raw rejection. The region turns it into words, so the code
 * stays available to a region that answers one code differently.
 */
export interface Load<T> {
  status: "idle" | "loading" | "ready" | "error";
  data: T | null;
  error: unknown;
  /** The data is from an earlier read, and the latest read failed (GIT-FR-LKRX). */
  stale: boolean;
}

const IDLE: Load<never> = { status: "idle", data: null, error: null, stale: false };

/**
 * GIT-FR-TFAU, GIT-FR-EPSV: one region's request slot.
 *
 * Every `run` supersedes the one before it, so a response that arrives after a
 * newer request, after `reset`, or after the panel unmounted — which is what a
 * project or worktree change does — is dropped and changes nothing the author
 * sees. Nothing here blocks any other region.
 */
export function useLoad<T>() {
  const [state, setState] = useState<Load<T>>(IDLE);
  const token = useRef(0);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  /**
   * Start a read. With `keep`, the data of an earlier read stays on show while
   * this one runs, and when this one fails it stays as **stale** data.
   */
  const run = useCallback(
    (read: () => Promise<T>, options: { keep?: boolean } = {}): Promise<void> => {
      const mine = ++token.current;
      setState((s) => ({
        status: "loading",
        data: options.keep ? s.data : null,
        error: null,
        stale: false,
      }));
      const current = () => alive.current && token.current === mine;
      return read().then(
        (data) => {
          if (current()) setState({ status: "ready", data, error: null, stale: false });
        },
        (error: unknown) => {
          if (!current()) return;
          setState((s) => ({
            status: "error",
            data: options.keep ? s.data : null,
            error,
            stale: options.keep === true && s.data !== null,
          }));
        },
      );
    },
    [],
  );

  /** Forget the data and discard whatever response is still on its way. */
  const reset = useCallback(() => {
    token.current += 1;
    setState(IDLE);
  }, []);

  return { state, run, reset };
}
