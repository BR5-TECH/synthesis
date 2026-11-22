/**
 * The pure rules of the Terminals panel: groups, order, and the sets the close
 * actions end (`../../specifications/ui/TRM-terminals.md`).
 */
import type { CliVendor, RunsIn, TargetRef, TerminalKind, TerminalSummary, UnavailableReason } from "../types";
import { isLiveTerminal, targetKey } from "./terminals";

export type TargetRole = "active" | "worktree" | "stream";

export interface TerminalGroup {
  key: string;
  target: TargetRef;
  name: string;
  branch?: string;
  role: TargetRole;
  sessions: TerminalSummary[];
}

/** TRM-FR-ZQPC: pinned rows first, then the others, each in creation order. */
export function sortRows(sessions: readonly TerminalSummary[]): TerminalSummary[] {
  return [...sessions.filter((s) => s.pinned), ...sessions.filter((s) => !s.pinned)];
}

/**
 * TRM-FR-EAUY: one group per target. The group of the active worktree is first;
 * the others follow in the order of their earliest session. `sessions` is in
 * creation order, as the backend lists them.
 */
export function groupTerminals(
  sessions: readonly TerminalSummary[],
  activeWorktreePath: string | null,
): TerminalGroup[] {
  const groups = new Map<string, TerminalGroup>();
  for (const session of sessions) {
    const key = targetKey(session.target);
    let group = groups.get(key);
    if (!group) {
      const role: TargetRole =
        session.target.kind === "work_stream"
          ? "stream"
          : session.target.path === activeWorktreePath
            ? "active"
            : "worktree";
      group = {
        key,
        target: session.target,
        name: session.targetName,
        branch: session.branch,
        role,
        sessions: [],
      };
      groups.set(key, group);
    }
    group.sessions.push(session);
  }
  const ordered = [...groups.values()];
  const active = ordered.filter((g) => g.role === "active");
  const rest = ordered.filter((g) => g.role !== "active");
  return [...active, ...rest].map((g) => ({ ...g, sessions: sortRows(g.sessions) }));
}

/** The rows in the order the panel shows them, for keyboard movement (TRM-FR-UIWG). */
export function rowOrder(groups: readonly TerminalGroup[]): TerminalSummary[] {
  return groups.flatMap((g) => g.sessions);
}

export type CloseScope = "one" | "others" | "all";

/**
 * TRM-FR-MANC, TRM-FR-BHYF: the sessions a close ends. **Close terminal** ends
 * the clicked one, pinned or not. The two mass closes end every unpinned
 * session of the window, in all groups, and **Close other terminals** skips
 * the clicked one.
 */
export function closeSet(
  sessions: readonly TerminalSummary[],
  scope: CloseScope,
  clickedId: string,
): TerminalSummary[] {
  if (scope === "one") return sessions.filter((s) => s.id === clickedId);
  return sessions.filter((s) => !s.pinned && (scope === "all" || s.id !== clickedId));
}

/** TRM-FR-JJEY: how many sessions of a close set still run. */
export function runningCount(set: readonly TerminalSummary[]): number {
  return set.filter(isLiveTerminal).length;
}

/** TRM-FR-BEYN: the kind as the row names it. */
export function kindName(kind: TerminalKind, vendor?: CliVendor): string {
  switch (kind) {
    case "host_shell":
      return "Host shell";
    case "container_shell":
      return "Container shell";
    case "microsandbox_shell":
      return "Microsandbox shell";
    case "agent_cli":
      return `Agent CLI · ${vendor ? vendorName(vendor) : "unknown"}`;
  }
}

export function vendorName(vendor: CliVendor): string {
  switch (vendor) {
    case "claude_code":
      return "Claude Code";
    case "codex":
      return "Codex";
    case "opencode":
      return "OpenCode";
  }
}

/** TRM-FR-BEYN: where a session runs, as its row names it. */
export function placeName(runsIn: RunsIn, environmentName?: string): string {
  const place =
    runsIn === "host"
      ? "host"
      : runsIn === "docker"
        ? "docker"
        : runsIn === "devcontainer"
          ? "dev container"
          : "vm";
  return environmentName ? `${place} · ${environmentName}` : place;
}

/** TRM-FR-BEYN: the state as the row names it. */
export function stateName(session: TerminalSummary): string {
  switch (session.state) {
    case "starting":
      return "starting";
    case "running":
      return "running";
    case "exited":
      return session.exitCode === undefined
          ? "exited"
          : `exited ${session.exitCode}`;
    case "target_gone":
      return "target gone";
  }
}

/** TRM-FR-HJUD: the reason text of a greyed kind entry. */
export function reasonText(reason: UnavailableReason | undefined): string {
  switch (reason) {
    case "docker_unverified":
      return "Docker is not verified";
    case "vendor_image_unconfigured":
      return "No image is configured for this vendor";
    case "devcontainer_not_ready":
      return "The dev container is not ready";
    case "devcontainer_project":
      return "Not available in a dev container project";
    case "isolation_not_microsandbox":
      return "Isolation is not Microsandbox";
    case "no_vm_running":
      return "No VM runs for this target";
    case "vendor_unverified":
      return "The CLI is not verified";
    default:
      return "Not available";
  }
}

/** TRM-FR-AASW: the role text of a target. */
export function roleName(role: TargetRole): string {
  return role === "active" ? "active worktree" : role === "stream" ? "work stream" : "worktree";
}
