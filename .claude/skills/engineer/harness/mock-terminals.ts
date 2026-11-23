/*
 * Terminal sessions stand-in (TRS-terminal-sessions.md), for UI auditing only.
 *
 * Each session is a fake echo shell: an attach sends a prompt line, and each
 * input is echoed back with a new prompt. `?terminalsSeeded` starts with three
 * sessions in two groups, one of them pinned and one exited. `?terminalsBusy`
 * marks the work stream busy. `?terminalsLimit` starts with 16 sessions.
 *
 * Console hooks for states and output the fake shell cannot reach alone:
 *   window.__terminalOutput("t-1", "text\r\n")       // live output to a session
 *   window.__terminalSet("t-1", { state: "starting" })  // patch a session, fire the event
 *   window.__terminalCalls                              // each terminal command and its args
 */

import { NOT_HANDLED } from "./mock-github-polling";

type Session = Record<string, any> & { id: string; state: string; pinned: boolean };

const flag = (name: string) =>
  typeof window !== "undefined" && new URLSearchParams(window.location.search).has(name);

const ACTIVE = { kind: "worktree", path: "/Users/demo/dev/acme" };
const FEATURE = { kind: "worktree", path: "/Users/demo/dev/acme-feature" };
const STREAM = { kind: "work_stream", streamId: "stream-auth" };

let sessions: Session[] = [];
let nextId = 1;
const channels = new Map<string, { onmessage?: (frame: ArrayBuffer) => void }>();
const history = new Map<string, string>();

function fireChanged() {
  queueMicrotask(() => {
    const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((name: string, payload?: unknown) => void)
      | undefined;
    fire?.("terminals-changed", {});
  });
}

function busy(target: Record<string, unknown>): string | null {
  return flag("terminalsBusy") && target.kind === "work_stream" ? "run-42" : null;
}

function nameOf(target: Record<string, any>): { name: string; branch: string } {
  if (target.kind === "work_stream") return { name: "auth-rework", branch: "synthesis/stream/auth-rework" };
  if (target.path === FEATURE.path) return { name: "acme-feature", branch: "feature/login" };
  return { name: "acme", branch: "main" };
}

function labelFor(kind: string, vendor?: string): string {
  const base =
    kind === "host_shell"
      ? "Host shell"
      : kind === "container_shell"
        ? "Container shell"
        : kind === "microsandbox_shell"
          ? "Microsandbox shell"
          : vendor === "codex"
            ? "Codex"
            : vendor === "opencode"
              ? "OpenCode"
              : "Claude Code";
  let n = 1;
  while (sessions.some((s) => s.label === `${base} ${n}`)) n += 1;
  return `${base} ${n}`;
}

function make(target: Record<string, any>, kind: string, vendor?: string, extra: Partial<Session> = {}): Session {
  const id = `t-${nextId++}`;
  const { name, branch } = nameOf(target);
  const runsIn = kind === "host_shell" ? "host" : kind === "microsandbox_shell" ? "microsandbox" : "docker";
  const session: Session = {
    id,
    label: labelFor(kind, vendor),
    target,
    targetName: name,
    branch,
    kind,
    ...(vendor ? { vendor } : {}),
    runsIn,
    ...(runsIn === "docker" ? { environmentName: `syn-term-${id}` } : {}),
    ...(runsIn === "microsandbox" ? { environmentName: "syn-auth-rework" } : {}),
    state: "running",
    pinned: false,
    busyRunId: busy(target),
    hasHistory: true,
    // TRS-FR-FKVW: a host session runs in tmux in the mock.
    persistent: runsIn === "host",
    createdAt: new Date(Date.now() + nextId).toISOString(),
    ...extra,
  };
  history.set(id, `\x1b[32m${name}\x1b[0m $ `);
  return session;
}

function seed() {
  if (flag("terminalsLimit")) {
    for (let i = 0; i < 16; i += 1) sessions.push(make(ACTIVE, "host_shell"));
    return;
  }
  if (!flag("terminalsSeeded")) return;
  sessions.push(make(ACTIVE, "host_shell", undefined, { pinned: true }));
  sessions.push(make(ACTIVE, "agent_cli", "claude_code"));
  sessions.push(make(STREAM, "microsandbox_shell", undefined, { state: "exited", exitCode: 0, endReason: "process_exited" }));
}
seed();

function send(id: string, text: string) {
  history.set(id, (history.get(id) ?? "") + text);
  const channel = channels.get(id);
  channel?.onmessage?.(new TextEncoder().encode(text).buffer as ArrayBuffer);
}

const calls: { cmd: string; args: Record<string, unknown> }[] = [];
if (typeof window !== "undefined") {
  const hooks = window as unknown as Record<string, unknown>;
  hooks.__terminalCalls = calls;
  hooks.__terminalOutput = (id: string, text: string) => send(id, text);
  hooks.__terminalSet = (id: string, patch: Partial<Session>) => {
    const s = sessions.find((x) => x.id === id);
    if (!s) return false;
    Object.assign(s, patch);
    fireChanged();
    return true;
  };
}

function kinds(target: Record<string, any>) {
  const stream = target.kind === "work_stream";
  return [
    { kind: "host_shell", runsIn: "host", available: true },
    { kind: "container_shell", runsIn: "docker", available: true },
    stream
      ? { kind: "microsandbox_shell", runsIn: "microsandbox", available: true }
      : { kind: "microsandbox_shell", runsIn: "microsandbox", available: false, reason: "no_vm_running" },
    { kind: "agent_cli", vendor: "claude_code", runsIn: "docker", available: true },
    { kind: "agent_cli", vendor: "codex", runsIn: "host", available: true },
    { kind: "agent_cli", vendor: "opencode", runsIn: "host", available: false, reason: "vendor_unverified" },
  ];
}

export function terminalsInvoke(cmd: string, a: Record<string, any>): unknown {
  if (cmd.includes("terminal")) {
    const { output: _output, ...args } = a ?? {};
    calls.push({ cmd, args });
  }
  switch (cmd) {
    case "list_terminal_targets":
      return [ACTIVE, FEATURE, STREAM].map((target) => ({
        target,
        ...nameOf(target),
        isActiveWorktree: target === ACTIVE,
        busyRunId: busy(target),
        kinds: kinds(target),
      }));
    case "list_terminals":
      return sessions.map((s) => ({ ...s }));
    case "create_terminal": {
      if (sessions.length >= 16) return Promise.reject({ code: "terminal_limit_reached" });
      const session = make(a.target, a.kind, a.vendor ?? undefined);
      sessions.push(session);
      fireChanged();
      return { ...session };
    }
    case "attach_terminal": {
      channels.set(a.terminalId, a.output);
      const text = history.get(a.terminalId) ?? "";
      queueMicrotask(() => a.output?.onmessage?.(new TextEncoder().encode(text).buffer));
      const s = sessions.find((x) => x.id === a.terminalId);
      return { state: s?.state ?? "exited", historyBytes: text.length };
    }
    case "detach_terminal":
      channels.delete(a.terminalId);
      return null;
    case "write_terminal_input": {
      const data = String(a.data ?? "");
      if (data === "\r") send(a.terminalId, "\r\n$ ");
      else send(a.terminalId, data);
      return null;
    }
    case "resize_terminal":
    case "acknowledge_terminal_output":
      return null;
    case "set_terminal_pinned": {
      const s = sessions.find((x) => x.id === a.terminalId);
      if (!s) return Promise.reject({ code: "unknown_terminal" });
      s.pinned = Boolean(a.pinned);
      fireChanged();
      return { ...s };
    }
    case "set_terminal_label": {
      const s = sessions.find((x) => x.id === a.terminalId);
      if (!s) return Promise.reject({ code: "unknown_terminal" });
      const label = String(a.label ?? "").trim();
      if (label === "" || [...label].length > 64) return Promise.reject({ code: "invalid_label" });
      s.label = label;
      fireChanged();
      return { ...s };
    }
    case "set_terminal_theme": {
      const s = sessions.find((x) => x.id === a.terminalId);
      if (!s) return Promise.reject({ code: "unknown_terminal" });
      const theme = a.theme ?? null;
      if (theme !== null && !["paper", "sand", "mist", "night", "ember"].includes(String(theme)))
        return Promise.reject({ code: "invalid_theme" });
      (s as { theme?: unknown }).theme = theme;
      fireChanged();
      return { ...s };
    }
    case "close_terminal":
      sessions = sessions.filter((x) => x.id !== a.terminalId);
      channels.delete(a.terminalId);
      history.delete(a.terminalId);
      fireChanged();
      return null;
    default:
      return NOT_HANDLED;
  }
}
