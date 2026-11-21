/**
 * Shared parts of the Terminals tests. The backend is mocked at `invoke`, which
 * routes by command name, answers the background commands, and rejects every
 * command it does not expect (see `src/test/invokeMocks.ts`).
 */
import { vi } from "vitest";
import { otherCommand } from "../../test/invokeMocks";
import type { TerminalSummary, TerminalTarget } from "../../types";

export interface Call {
  cmd: string;
  args: Record<string, unknown>;
}

export const calls: Call[] = [];

type Handler = (args: Record<string, unknown>) => unknown;
const handlers = new Map<string, Handler>();

/** Sets the answer of one command for the running test. */
export function serve(cmd: string, handler: Handler): void;
export function serve(cmd: string, value: unknown): void;
export function serve(cmd: string, handler: unknown): void {
  handlers.set(cmd, typeof handler === "function" ? (handler as Handler) : () => handler);
}

export const invokeMock = vi.fn((cmd: string, args: Record<string, unknown> = {}) => {
  calls.push({ cmd, args });
  const handler = handlers.get(cmd);
  if (!handler) return otherCommand(cmd);
  try {
    return Promise.resolve(handler(args));
  } catch (err) {
    return Promise.reject(err);
  }
});

/** A stand-in for the Tauri `Channel`, which a test pushes frames into. */
export class FakeChannel<T> {
  static last: FakeChannel<unknown> | null = null;
  onmessage: ((message: T) => void) | null = null;
  constructor() {
    FakeChannel.last = this as FakeChannel<unknown>;
  }
  push(message: T) {
    this.onmessage?.(message);
  }
}

export const listenMock = vi.fn(async () => () => {});

export function commands(name: string): Call[] {
  return calls.filter((c) => c.cmd === name);
}

export function resetTerminalFixtures(): void {
  calls.length = 0;
  handlers.clear();
  invokeMock.mockClear();
  listenMock.mockClear();
  FakeChannel.last = null;
}

let nextId = 1;

export function session(overrides: Partial<TerminalSummary> = {}): TerminalSummary {
  const id = overrides.id ?? `t${nextId++}`;
  return {
    id,
    label: "Host shell 1",
    target: { kind: "worktree", path: "/repo" },
    targetName: "repo",
    branch: "main",
    kind: "host_shell",
    runsIn: "host",
    state: "running",
    pinned: false,
    busyRunId: null,
    hasHistory: true,
    createdAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

export function target(overrides: Partial<TerminalTarget> = {}): TerminalTarget {
  return {
    target: { kind: "worktree", path: "/repo" },
    name: "repo",
    branch: "main",
    isActiveWorktree: true,
    busyRunId: null,
    kinds: [
      { kind: "host_shell", runsIn: "host", available: true },
      { kind: "container_shell", runsIn: "docker", available: true },
      { kind: "microsandbox_shell", runsIn: "microsandbox", available: false, reason: "no_vm_running" },
      { kind: "agent_cli", vendor: "codex", runsIn: "host", available: true },
    ],
    ...overrides,
  };
}
