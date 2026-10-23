/**
 * The main window's one transfer — a push or a pull: its running state, its
 * transcript, and the branch's standing against its remote
 * (`specifications/ui/GIT-git.md` GIT-FR-QMYB).
 *
 * The top-chrome **Push** control, the top-chrome pull control, the Git panel's
 * **Push** and **Pull**, and the Changes panel's **Push** start one of two
 * transfers. They read one state from here, so a transfer started by any of
 * them makes every other one unavailable, and every output line lands in one
 * transcript that the Git panel renders. The state lives above the panels
 * because a transfer started while the Git panel is closed must still be in its
 * output area when the panel opens.
 *
 * Nothing here writes to the diagnostic Logs panel. A push failure is a
 * transfer outcome, and it belongs in the transcript (GIT-FR-IMSH).
 */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";

import * as api from "../api";
import {
  onChangesUpdated,
  onGitOperationFinished,
  onGitOutputLine,
} from "../events";
import { tokenErrorMessage } from "../components/GithubTokens";
import { logDebug } from "../logging";
import { GITHUB_TOKEN_ERRORS } from "../types";
import type {
  PullOutcome,
  PullReconcileOutcome,
  PullStrategy,
  UpstreamSyncState,
} from "../types";

/** How a push that did not complete ended, as the three controls tell it apart. */
export type PushFailure =
  /** GIT-FR-QMYB: another push was running, so no command was invoked. */
  | { ok: false; cause: "busy" }
  /** GTC-FR-10: tokens are stored but the project names none. */
  | { ok: false; cause: "selection_required"; error: string }
  /** GTC-FR-10: no token is stored at all. */
  | { ok: false; cause: "token_missing"; error: string }
  | { ok: false; cause: "failed"; error: string };

export type PushOutcome = { ok: true } | PushFailure;

/** Which transfer runs (GIT-FR-QMYB). */
export type TransferOperation = "push" | "pull";

/**
 * How a pull call ended. A pull that ran answers its outcome, a refused one
 * included (GTC-FR-QHMT); only a busy transfer or a rejection is a failure.
 */
export type PullCall =
  | { ok: true; outcome: PullOutcome }
  | { ok: false; cause: "busy" }
  | { ok: false; cause: "failed"; error: string };

/** How a reconcile call ended (GTC-FR-NJUY). */
export type ReconcileCall =
  | { ok: true; outcome: PullReconcileOutcome }
  | { ok: false; cause: "busy" }
  | { ok: false; cause: "failed"; error: string };

export interface GitTransfer {
  /** CHG-FR-37: the branch's standing against its upstream, `null` when unread. */
  sync: UpstreamSyncState | null;
  /** GIT-FR-QMYB: a transfer is running, whichever control started it. */
  running: boolean;
  /** GIT-FR-QMYB: which transfer runs, `null` while none does. */
  operation: TransferOperation | null;
  /** GIT-FR-PZIE: the push/pull output area's lines. */
  lines: string[];
  /**
   * GIT-FR-QMYB: invoke `"push current branch"` unless a push is running.
   * Resolves with how it ended; the typed token causes are left to the caller
   * because each control answers them its own way (GIT-FR-10, CHG-FR-45).
   */
  pushBranch: () => Promise<PushOutcome>;
  /** GIT-FR-QMYB / WTS-FR-30: invoke `"pull current branch"` unless a transfer runs. */
  pullBranch: () => Promise<PullCall>;
  /**
   * GIT-FR-RCDL: invoke `"reconcile pull"` at the tips the author was shown,
   * unless a transfer runs.
   */
  reconcilePull: (
    strategy: PullStrategy,
    localTip: string,
    remoteTip: string,
  ) => Promise<ReconcileCall>;
  /** CHG-FR-38: read the standing again. */
  refreshSync: () => void;
}

/** The text of a rejected invocation. */
function rejection(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

/** The two typed causes of GTC-FR-10 end a push that made no network request. */
function isTokenCause(error: string): boolean {
  return (
    error.includes(GITHUB_TOKEN_ERRORS.selectionRequired) ||
    error.includes(GITHUB_TOKEN_ERRORS.tokenMissing)
  );
}

interface TransferStateOptions {
  /** False makes the hook inert, so a consumer under a provider owns no state. */
  enabled: boolean;
  /** GIT-FR-XXLE: no read is made outside a Git repository. */
  inRepository?: boolean;
  /** GIT-FR-QMYB: a change discards the transcript and the running state. */
  resetKey?: string;
}

/** The state itself. The provider calls it once for the whole window. */
export function useGitTransferState({
  enabled,
  inRepository = true,
  resetKey = "",
}: TransferStateOptions): GitTransfer {
  // The standing is kept with the key it was read under, so the render that
  // follows a worktree change never offers a push on the previous branch's state.
  const [read, setRead] = useState<{
    key: string;
    state: UpstreamSyncState | null;
  }>({ key: resetKey, state: null });
  const sync = read.key === resetKey ? read.state : null;
  const setSync = useCallback(
    (state: UpstreamSyncState | null) => setRead({ key: resetKey, state }),
    [resetKey],
  );
  const [syncEpoch, setSyncEpoch] = useState(0);
  const [operation, setOperation] = useState<TransferOperation | null>(null);
  const running = operation !== null;
  const [lines, setLines] = useState<string[]>([]);
  // The ref is the gate and the state is what renders: two activations in one
  // tick both see the state as idle, and only the ref tells the second one.
  const runningRef = useRef(false);

  const setRunningNow = useCallback(
    (next: boolean, which: TransferOperation = "push") => {
      runningRef.current = next;
      setOperation(next ? which : null);
    },
    [],
  );

  const refreshSync = useCallback(() => setSyncEpoch((n) => n + 1), []);

  // GIT-FR-QMYB: a project or worktree change discards the transcript. The
  // running state stays: a push already started keeps running in the backend,
  // and no control may start a second one before it ends.
  const firstKey = useRef(resetKey);
  useEffect(() => {
    if (!enabled || firstKey.current === resetKey) return;
    firstKey.current = resetKey;
    setLines([]);
    refreshSync();
  }, [enabled, resetKey, refreshSync]);

  // CHG-FR-38 / GTC-FR-21: read from local refs alone, so this reaches no remote.
  useEffect(() => {
    if (!enabled) return;
    if (!inRepository) {
      setSync(null);
      return;
    }
    let cancelled = false;
    void Promise.resolve()
      .then(() => api.getUpstreamSyncState())
      .then((s) => {
        if (!cancelled) setSync(s ?? null);
      })
      .catch(() => {
        // No branch, or not a repository. Push stays unavailable rather than
        // being offered on a state no control can describe.
        logDebug(["frontend"], "upstream sync state unavailable");
        if (!cancelled) setSync(null);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, inRepository, syncEpoch, setSync]);

  // CHG-FR-38: a commit landing makes the branch pushable.
  useEffect(() => {
    if (!enabled) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onChangesUpdated(refreshSync).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [enabled, refreshSync]);

  // GTC-FR-04 / GTC-FR-22: both channels are surface-agnostic, so a push begun
  // by any control — or by the graduation driver — arrives here the same way.
  useEffect(() => {
    if (!enabled) return;
    const unlisteners: (() => void)[] = [];
    let cancelled = false;
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisteners.push(fn);
    };
    void onGitOutputLine((payload) => {
      if (payload.operation === "push" || payload.operation === "pull")
        setRunningNow(true, payload.operation);
      setLines((prev) => [...prev, payload.line]);
    }).then(keep);
    void onGitOperationFinished((payload) => {
      setRunningNow(false);
      // A completed transfer moves the branch's standing against its remote.
      refreshSync();
      const cause = payload.error ?? "unknown error";
      const failure = isTokenCause(cause) ? tokenErrorMessage(cause) : cause;
      setLines((prev) => [
        ...prev,
        payload.ok
          ? `${payload.operation} finished`
          : `${payload.operation} failed: ${failure}`,
      ]);
    }).then(keep);
    return () => {
      cancelled = true;
      for (const fn of unlisteners) fn();
    };
  }, [enabled, refreshSync, setRunningNow]);

  const pushBranch = useCallback(async (): Promise<PushOutcome> => {
    if (runningRef.current) return { ok: false, cause: "busy" };
    setRunningNow(true);
    try {
      await api.pushCurrentBranch();
      return { ok: true };
    } catch (e) {
      const raw = rejection(e);
      if (raw.includes(GITHUB_TOKEN_ERRORS.selectionRequired))
        return { ok: false, cause: "selection_required", error: raw };
      if (raw.includes(GITHUB_TOKEN_ERRORS.tokenMissing))
        return { ok: false, cause: "token_missing", error: raw };
      return { ok: false, cause: "failed", error: raw };
    } finally {
      // The terminal event clears this too. Clearing here as well covers an
      // invocation rejected before the backend emitted anything.
      setRunningNow(false);
      refreshSync();
    }
  }, [refreshSync, setRunningNow]);

  const pullBranch = useCallback(async (): Promise<PullCall> => {
    if (runningRef.current) return { ok: false, cause: "busy" };
    setRunningNow(true, "pull");
    try {
      return { ok: true, outcome: await api.pullCurrentBranch() };
    } catch (e) {
      return { ok: false, cause: "failed", error: rejection(e) };
    } finally {
      setRunningNow(false);
      refreshSync();
    }
  }, [refreshSync, setRunningNow]);

  const reconcilePull = useCallback(
    async (
      strategy: PullStrategy,
      localTip: string,
      remoteTip: string,
    ): Promise<ReconcileCall> => {
      if (runningRef.current) return { ok: false, cause: "busy" };
      setRunningNow(true, "pull");
      try {
        const outcome = await api.reconcilePull(strategy, localTip, remoteTip);
        return { ok: true, outcome };
      } catch (e) {
        return { ok: false, cause: "failed", error: rejection(e) };
      } finally {
        setRunningNow(false);
        refreshSync();
      }
    },
    [refreshSync, setRunningNow],
  );

  return {
    sync,
    running,
    operation,
    lines,
    pushBranch,
    pullBranch,
    reconcilePull,
    refreshSync,
  };
}

const GitTransferContext = createContext<GitTransfer | null>(null);

export function GitTransferProvider({
  value,
  children,
}: {
  value: GitTransfer;
  children: ReactNode;
}) {
  return (
    <GitTransferContext.Provider value={value}>
      {children}
    </GitTransferContext.Provider>
  );
}

/**
 * The window's push state. Under a provider every consumer shares the
 * provider's state; with none — a panel rendered alone — the consumer owns a
 * state of its own, which is the one behaviour a standalone panel always had.
 */
export function useGitTransfer(): GitTransfer {
  const shared = useContext(GitTransferContext);
  const own = useGitTransferState({ enabled: shared === null });
  return shared ?? own;
}
