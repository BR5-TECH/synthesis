/**
 * MCP server connections (`MCS-mcp-server-connections.md`): the records, their
 * secrets, trust, sign-in, and the test of one server.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under. Every
 * command refuses with `no_project_open` and `malformed_project_config`.
 */
import { invoke } from "@tauri-apps/api/core";
import type { McpServer, McpServerEntry, McpTestResult } from "../types";

/** MCS-FR-FIEE: every record of the project in file order, with its status. */
export const listMcpServers = () => invoke<McpServerEntry[]>("list_mcp_servers");

/** MCS-FR-XCOS: validate and store one record. Rejects with a `McpConfigError`. */
export const saveMcpServer = (server: McpServer) =>
  invoke<McpServerEntry>("save_mcp_server", { server });

/** MCS-FR-UOIO: remove a record, its secrets, and its trust. Rejects with `not_found`. */
export const removeMcpServer = (id: string) => invoke<void>("remove_mcp_server", { id });

/**
 * MCS-FR-IXHD: store the value of one secret slot. Rejects with `unknown_slot`,
 * `invalid_secret`, `not_found`, or `keychain_unavailable`.
 */
export const setMcpServerSecret = (id: string, slot: string, value: string) =>
  invoke<McpServerEntry>("set_mcp_server_secret", { id, slot, value });

/** MCS-FR-WBDV: remove the value of one secret slot. */
export const clearMcpServerSecret = (id: string, slot: string) =>
  invoke<McpServerEntry>("clear_mcp_server_secret", { id, slot });

/**
 * MCS-FR-EVRS: trust the record that the trust dialog showed (`shown`). Rejects
 * with `record_changed` when the record in the file is not that record now,
 * with `no_credentials` for a remote server that needs no trust, and with
 * `not_found`.
 */
export const trustMcpServer = (id: string, shown: McpServer) =>
  invoke<McpServerEntry>("trust_mcp_server", { id, shown });

/** MCS-FR-ETIS: open a new connection, list the tools, and close it. */
export const testMcpServer = (id: string) => invoke<McpTestResult>("test_mcp_server", { id });

/** MCS-FR-KCIH: reread the record and connect again. */
export const reconnectMcpServer = (id: string) => invoke<void>("reconnect_mcp_server", { id });

/**
 * MCS-FR-YWVC: start the OAuth sign-in. The result arrives as a status change.
 * Rejects with `not_oauth2` or `sign_in_in_progress`.
 */
export const startMcpSignIn = (id: string) => invoke<void>("start_mcp_sign_in", { id });

/** MCS-FR-CMWV: end a sign-in at once. */
export const cancelMcpSignIn = (id: string) => invoke<void>("cancel_mcp_sign_in", { id });

/** MCS-FR-QRIB: remove the tokens and keep the registered client. */
export const signOutMcpServer = (id: string) =>
  invoke<McpServerEntry>("sign_out_mcp_server", { id });
