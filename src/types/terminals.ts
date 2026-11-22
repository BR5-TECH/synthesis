// The shapes of the terminal session contract
// (`../../specifications/core/TRS-terminal-sessions.md`, Contract surface).

/** TRS-FR-BGDL: what a session is bound to. */
export type TargetRef = { kind: "worktree"; path: string } | { kind: "work_stream"; streamId: string };

export type TerminalKind = "host_shell" | "container_shell" | "microsandbox_shell" | "agent_cli";

export type CliVendor = "claude_code" | "codex" | "opencode";

export type RunsIn = "host" | "docker" | "devcontainer" | "microsandbox";

/** TRS-FR-XXPE. */
export type UnavailableReason =
  | "docker_unverified"
  | "vendor_image_unconfigured"
  | "devcontainer_not_ready"
  | "devcontainer_project"
  | "isolation_not_microsandbox"
  | "no_vm_running"
  | "vendor_unverified";

export interface KindAvailability {
  kind: TerminalKind;
  vendor?: CliVendor;
  runsIn: RunsIn;
  available: boolean;
  reason?: UnavailableReason;
}

export interface TerminalTarget {
  target: TargetRef;
  name: string;
  branch?: string;
  isActiveWorktree: boolean;
  busyRunId: string | null;
  kinds: KindAvailability[];
}

export type TerminalState = "starting" | "running" | "exited" | "target_gone";

export type EndReason = "process_exited" | "start_failed" | "vm_stopped" | "target_gone";

export interface TerminalSummary {
  id: string;
  label: string;
  target: TargetRef;
  targetName: string;
  branch?: string;
  kind: TerminalKind;
  vendor?: CliVendor;
  runsIn: RunsIn;
  environmentName?: string;
  state: TerminalState;
  exitCode?: number;
  endReason?: EndReason;
  endDetail?: string;
  pinned: boolean;
  busyRunId: string | null;
  hasHistory: boolean;
  /** TRS-FR-FKVW: the session runs in the application's tmux server and survives an IDE restart. */
  persistent: boolean;
  createdAt: string;
}

export interface TerminalAttachment {
  state: TerminalState;
  historyBytes: number;
}

/** The typed error a terminal command rejects with. */
export interface TerminalError {
  code: string;
  reason?: UnavailableReason;
  message?: string;
}
