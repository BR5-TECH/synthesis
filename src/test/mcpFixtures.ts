/**
 * Entries and records of the MCP servers section (`MSS-mcp-servers-settings.md`)
 * for its tests.
 */
import type {
  McpHttpTransport,
  McpServer,
  McpServerEntry,
  McpServerStatus,
  McpStdioTransport,
} from "../types";

export function stdioTransport(over: Partial<McpStdioTransport> = {}): McpStdioTransport {
  return {
    kind: "stdio",
    command: "npx",
    args: ["-y", "docs-mcp-server"],
    cwd: null,
    env: {},
    secretEnv: [],
    ...over,
  };
}

export function httpTransport(over: Partial<McpHttpTransport> = {}): McpHttpTransport {
  return {
    kind: "http",
    url: "https://mcp.tracker.example/mcp",
    headers: {},
    secretHeaders: [],
    auth: "none",
    clientId: null,
    scopes: [],
    callbackPort: null,
    ...over,
  };
}

export function mcpRecord(over: Partial<McpServer> = {}): McpServer {
  return {
    id: "docs",
    name: "Docs index",
    enabled: true,
    disabledTools: [],
    transport: stdioTransport(),
    ...over,
  };
}

export function mcpEntry(
  record: McpServer | null,
  status: McpServerStatus,
  over: Partial<McpServerEntry> = {},
): McpServerEntry {
  return {
    id: record?.id ?? "broken",
    scope: "team",
    enabledHere: record?.enabled ?? false,
    record,
    invalid: null,
    status,
    secrets: [],
    tools: [],
    ...over,
  };
}
