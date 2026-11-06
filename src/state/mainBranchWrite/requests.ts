/**
 * The standalone confirmation, as a request that the shell mounts
 * (MBW-FR-GYVD, `SNV-shell-navigation.md` SNV-FR-56).
 *
 * A surface asks for the confirmation and awaits the answer. The shell mounts
 * the dialog for the request and closes every other overlay when it opens. An
 * overlay that opens after it settles the request as cancelled.
 */
import type { BranchWriteGuard, MainBranchWriteKind } from "../../types";

export interface MainBranchWriteRequest {
  /** A counter, so two requests never share a key. */
  id: number;
  write: MainBranchWriteKind;
  /** The branch the write lands on; `null` is the active branch. */
  branch: string | null;
  /** The guard the request opens with, when one was read. */
  guard: BranchWriteGuard | null;
  /** MBW-FR-UNAN: why the guard could not be read. */
  readError: string | null;
}

interface Pending extends MainBranchWriteRequest {
  settle: (confirmed: boolean) => void;
}

let pending: Pending | null = null;
let seq = 0;
const listeners = new Set<() => void>();
let opening: (() => void) | null = null;

function notify() {
  for (const listener of listeners) listener();
}

export function subscribeMainBranchWrite(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The request the shell mounts, or `null`. */
export function currentMainBranchWrite(): MainBranchWriteRequest | null {
  return pending;
}

/** SNV-FR-56: what the shell closes when the confirmation opens. */
export function setMainBranchWriteOpening(next: (() => void) | null): void {
  opening = next;
}

/** Open the confirmation and resolve with the author's answer. */
export function requestMainBranchWrite(
  request: Omit<MainBranchWriteRequest, "id">,
): Promise<boolean> {
  // A request that is still open is answered first, so no caller waits forever.
  settleMainBranchWrite(false);
  opening?.();
  return new Promise<boolean>((resolve) => {
    pending = { ...request, id: ++seq, settle: resolve };
    notify();
  });
}

/** Answer the open request, if there is one. */
export function settleMainBranchWrite(confirmed: boolean): void {
  const open = pending;
  if (!open) return;
  pending = null;
  notify();
  open.settle(confirmed);
}

/** SNV-FR-56: another overlay opened, so the confirmation closes. */
export function dismissMainBranchWrite(): void {
  settleMainBranchWrite(false);
}

export function resetMainBranchWriteRequestsForTest(): void {
  const open = pending;
  pending = null;
  open?.settle(false);
  opening = null;
  listeners.clear();
}
