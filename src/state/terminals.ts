/**
 * The terminal sessions of this project window, as the frontend knows them
 * (`../../specifications/ui/TRM-terminals.md` TRM-FR-LAZH).
 *
 * The backend owns every session (`TRS-terminal-sessions.md`). This store holds
 * the last list it returned, reads it again on each `"terminals-changed"`, and
 * tells its subscribers. It is a module store rather than React state because
 * the panel, the tab strip, and the terminal tab all read it, and the panel
 * unmounts on a surface switch.
 */
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "../api";
import { onTerminalsChanged } from "../events";
import { logWarn } from "../logging";
import type { TargetRef, TerminalSummary } from "../types";

/** TRS-FR-AYCY: the most sessions one project window holds. */
export const MAX_TERMINALS = 16;

export interface TerminalsSnapshot {
  /** Every session, in the order the backend lists them (creation order). */
  readonly sessions: readonly TerminalSummary[];
  /** Whether the list was read at least once. Before that, no tab is closed for a missing session. */
  readonly loaded: boolean;
}

const EMPTY: TerminalsSnapshot = { sessions: [], loaded: false };

let snapshot: TerminalsSnapshot = EMPTY;
const listeners = new Set<() => void>();
let unlisten: Promise<UnlistenFn> | null = null;
let generation = 0;
/** Counts the local changes, so that a list read before one does not undo it. */
let localChanges = 0;

function publish(next: TerminalsSnapshot) {
  snapshot = next;
  for (const listener of listeners) listener();
}

export function terminalsSnapshot(): TerminalsSnapshot {
  return snapshot;
}

export function subscribeTerminals(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** TRM-FR-LAZH: reads the list from the backend. A failed read keeps the last list. */
export async function refreshTerminals(): Promise<void> {
  const asked = generation;
  const changesAtStart = localChanges;
  try {
    const sessions = await api.listTerminals();
    if (asked !== generation) return;
    // A create or a close answered while the read ran: this list can be older
    // than that answer, so read again.
    if (changesAtStart !== localChanges) return refreshTerminals();
    publish({ sessions: Array.isArray(sessions) ? sessions : [], loaded: true });
  } catch (err) {
    logWarn(["frontend"], "terminal list read failed", { code: errorCode(err) });
  }
}

/** Starts to follow `"terminals-changed"` once, and reads the list. */
export function ensureTerminalsFollowed(): void {
  if (!unlisten) {
    unlisten = onTerminalsChanged(() => void refreshTerminals());
    unlisten.catch(() => {
      unlisten = null;
    });
  }
  void refreshTerminals();
}

/** Puts one summary into the list, as the answer of a create or a pin change gives it. */
export function upsertTerminal(summary: TerminalSummary): void {
  localChanges += 1;
  const index = snapshot.sessions.findIndex((s) => s.id === summary.id);
  const sessions =
    index < 0
      ? [...snapshot.sessions, summary]
      : snapshot.sessions.map((s, i) => (i === index ? summary : s));
  publish({ sessions, loaded: snapshot.loaded });
}

/** Takes one session out of the list, after a close. */
export function removeTerminal(id: string): void {
  if (!snapshot.sessions.some((s) => s.id === id)) return;
  localChanges += 1;
  publish({ sessions: snapshot.sessions.filter((s) => s.id !== id), loaded: snapshot.loaded });
}

export function findTerminal(id: string): TerminalSummary | undefined {
  return snapshot.sessions.find((s) => s.id === id);
}

/** Whether a session can still end a process: the confirmation of TRM-FR-JJEY counts these. */
export function isLiveTerminal(summary: TerminalSummary): boolean {
  return summary.state === "starting" || summary.state === "running";
}

export function sameTarget(a: TargetRef, b: TargetRef): boolean {
  if (a.kind === "worktree" && b.kind === "worktree") return a.path === b.path;
  if (a.kind === "work_stream" && b.kind === "work_stream") return a.streamId === b.streamId;
  return false;
}

export function targetKey(target: TargetRef): string {
  return target.kind === "worktree" ? `worktree:${target.path}` : `work_stream:${target.streamId}`;
}

/** The code of a rejected terminal command, for a log record or a message. */
export function errorCode(err: unknown): string {
  if (err && typeof err === "object" && "code" in err && typeof (err as { code: unknown }).code === "string")
    return (err as { code: string }).code;
  return "unknown";
}

/** Resets the store between tests. */
export function resetTerminalsForTest(): void {
  generation += 1;
  snapshot = EMPTY;
  listeners.clear();
  if (unlisten) void unlisten.then((stop) => stop()).catch(() => {});
  unlisten = null;
}
