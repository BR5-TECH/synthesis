// MCP server connections (MCS-mcp-server-connections.md)
//
// The shapes of the MCP server commands and of `"mcp-server-status-changed"`.
// Re-exported from `./index`, so `from "../types"` resolves them.

/** MCS-FR-KALQ: a server that the application starts as a local command. */
export interface McpStdioTransport {
  kind: "stdio";
  command: string;
  args: string[];
  /** MCS-FR-KLQS: relative to the active worktree. */
  cwd: string | null;
  /** Plain values. They are committed. */
  env: Record<string, string>;
  /** Names only. The values are in the vault (MCS-FR-QBFA). */
  secretEnv: string[];
}

/** MCS-FR-DINZ: the authentication of a remote server. */
export type McpAuthMode = "none" | "bearer" | "oauth2";

/** MCS-FR-DINZ: a server that the application reaches over HTTP. */
export interface McpHttpTransport {
  kind: "http";
  url: string;
  headers: Record<string, string>;
  secretHeaders: string[];
  auth: McpAuthMode;
  /** MCS-FR-UAAN: oauth2 only. */
  clientId: string | null;
  scopes: string[];
  callbackPort: number | null;
}

export type McpTransport = McpStdioTransport | McpHttpTransport;

/** MCS-FR-XOSW: the record of one server, as `.synthesis/project.toml` holds it. */
export interface McpServer {
  id: string;
  name: string;
  enabled: boolean;
  /** MCS-FR-HBYA */
  disabledTools: string[];
  transport: McpTransport;
}

/** MCS-FR-JNXV: why a connection failed. */
export type McpFailure =
  | "command_not_found"
  | "process_exited"
  | "unreachable"
  | "tls_untrusted"
  | "rejected"
  | "protocol_error"
  | "timed_out"
  | "oauth_failed"
  | "keychain_unavailable";

/** MCS-FR-RXZG: the status of a server. */
export type McpServerStatus =
  | {
      state:
        | "disabled"
        | "invalid"
        | "untrusted"
        | "needs_secret"
        | "needs_sign_in"
        | "signing_in"
        | "connecting";
    }
  | { state: "ready"; toolCount: number }
  | { state: "failed"; failure: McpFailure };

export type McpServerState = McpServerStatus["state"];

/** The contract surface: a validation code of a record. */
export type McpConfigError =
  | "invalid_id"
  | "duplicate_id"
  | "invalid_name"
  | "invalid_transport"
  | "invalid_command"
  | "invalid_cwd"
  | "invalid_url"
  | "invalid_env_name"
  | "invalid_header_name"
  | "invalid_header_value"
  | "duplicate_name"
  | "authorization_header_conflict"
  | "invalid_callback_port"
  | "invalid_auth";

/** One tool that a server offers now. */
export interface McpToolInfo {
  name: string;
  description: string;
  disabled: boolean;
}

/** MCS-FR-QBFA: one settable secret slot, and whether this machine has its value. */
export interface McpSecretPresence {
  slot: string;
  present: boolean;
}

/** MCS-FR-FIEE: one server of `"list MCP servers"`. */
export interface McpServerEntry {
  id: string;
  /** Null when the record is invalid (MCS-FR-HBOD). */
  record: McpServer | null;
  invalid: McpConfigError | null;
  status: McpServerStatus;
  /** Settable slots only. No value. */
  secrets: McpSecretPresence[];
  /** Empty unless the server is ready. */
  tools: McpToolInfo[];
}

/** MCS-FR-ETIS, MCS-FR-ERQP: the result of `"test MCP server"`. */
export type McpTestResult =
  | { ok: true; tools: McpToolInfo[] }
  | { ok: false; failure: McpFailure | McpServerState; stderrTail: string[] };

/** MCS-FR-NOKX: the payload of `"mcp-server-status-changed"`. */
export interface McpServerStatusChanged {
  id: string;
  status: McpServerStatus;
}
