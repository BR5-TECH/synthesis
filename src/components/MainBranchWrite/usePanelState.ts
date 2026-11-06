/**
 * The state the confirmation and the inline section share: the guard, a read
 * that failed, a confirmation that failed, and the three actions that change
 * them (MBW-FR-XBJS, MBW-FR-UNAN).
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { logWarn } from "../../logging";
import {
  guardReads,
  logConfirmation,
  rememberConfirmation,
} from "../../state/mainBranchWrite";
import type { BranchWriteGuard, MainBranchWriteKind } from "../../types";

export interface PanelState {
  guard: BranchWriteGuard | null;
  readError: string | null;
  confirmError: string | null;
  busy: boolean;
  /** MBW-FR-UNAN: read the guard again, with `refresh` true. */
  checkAgain: () => Promise<BranchWriteGuard | null>;
  /** MBW-FR-UNAN: read the guard again after a read that failed. */
  tryAgain: () => Promise<BranchWriteGuard | null>;
  /** MBW-FR-XBJS: record the confirmation. Resolves `true` on success. */
  confirm: () => Promise<boolean>;
  /** Take a guard that arrived from elsewhere, a refusal for example. */
  adopt: (guard: BranchWriteGuard | null, readError: string | null) => void;
}

export function usePanelState(options: {
  write: MainBranchWriteKind;
  branch: string | null;
  surface: "dialog" | "inline";
  initialGuard: BranchWriteGuard | null;
  initialError: string | null;
}): PanelState {
  const { write, branch, surface } = options;
  const [guard, setGuard] = useState<BranchWriteGuard | null>(options.initialGuard);
  const [readError, setReadError] = useState<string | null>(options.initialError);
  const [confirmError, setConfirmError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const live = useRef(true);
  /** A guard that arrives by `adopt` replaces any read still under way. */
  const generation = useRef(0);
  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  const read = useCallback(
    async (refresh: boolean) => {
      const mine = ++generation.current;
      setBusy(true);
      try {
        const next = await guardReads().read(branch ?? guard?.branch ?? null, refresh);
        if (!live.current || mine !== generation.current) return next;
        setGuard(next);
        setReadError(null);
        return next;
      } catch (e) {
        logWarn(["frontend"], "branch write guard read failed", { write, surface });
        if (live.current && mine === generation.current) setReadError(String(e));
        return null;
      } finally {
        if (live.current) setBusy(false);
      }
    },
    [branch, guard?.branch, write, surface],
  );

  const confirm = useCallback(async () => {
    if (!guard) return false;
    setBusy(true);
    setConfirmError(null);
    try {
      const next = await guardReads().confirm(guard.branch);
      rememberConfirmation(next);
      logConfirmation("confirmed", write, guard.branch, surface);
      if (live.current) setGuard(next);
      return true;
    } catch (e) {
      // MBW-FR-XBJS: a failed confirmation shows its error and writes nothing.
      logWarn(["frontend"], "main branch write confirmation failed", {
        write,
        surface,
        branch: guard.branch,
      });
      if (live.current) setConfirmError(String(e));
      return false;
    } finally {
      if (live.current) setBusy(false);
    }
  }, [guard, write, surface]);

  const adopt = useCallback((next: BranchWriteGuard | null, error: string | null) => {
    generation.current++;
    setGuard(next);
    setReadError(error);
    setConfirmError(null);
  }, []);

  return {
    guard,
    readError,
    confirmError,
    busy,
    checkAgain: () => read(true),
    tryAgain: () => read(false),
    confirm,
    adopt,
  };
}
