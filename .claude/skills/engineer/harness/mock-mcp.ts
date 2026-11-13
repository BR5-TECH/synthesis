/*
 * MCP server connections stand-in (MCS-mcp-server-connections.md), for UI
 * auditing only.
 *
 * The MCP servers section of the Project settings (MSS-mcp-servers-settings.md).
 * The seeded list holds one entry of each status the section draws differently:
 * ready with tools (oauth2, so `Sign out` shows), untrusted stdio, needs_secret,
 * needs_sign_in oauth2, failed with a failure class, signing_in, disabled, and an
 * invalid record. `?mcp=empty` starts with no server. `?mcp=loadFail` refuses
 * each load until a script sets `window.__mcpLoadOk = true` (a mount can load
 * twice, so a count of refusals is not enough). A test of the `failed` server fails with standard error lines.
 * Sign-in, reconnect, and trust emit `"mcp-server-status-changed"`.
 */

import { NOT_HANDLED } from "./mock-github-polling";

type Tool = { name: string; description: string; disabled: boolean };
type Entry = {
  id: string;
  record: any | null;
  invalid: string | null;
  status: any;
  secrets: { slot: string; present: boolean }[];
  tools: Tool[];
};

const query = new URLSearchParams(location.search);
const mode = query.get("mcp");

const http = (over: Record<string, unknown> = {}) => ({
  kind: "http",
  url: "https://mcp.tracker.example/mcp",
  headers: {},
  secretHeaders: [],
  auth: "none",
  clientId: null,
  scopes: [],
  callbackPort: null,
  ...over,
});
const stdio = (over: Record<string, unknown> = {}) => ({
  kind: "stdio",
  command: "npx",
  args: ["-y", "docs-mcp-server"],
  cwd: null,
  env: {},
  secretEnv: [],
  ...over,
});
const record = (id: string, name: string, transport: unknown, over: Record<string, unknown> = {}) => ({
  id,
  name,
  enabled: true,
  disabledTools: [],
  transport,
  ...over,
});

const TRACKER_TOOLS: Tool[] = [
  { name: "create_issue", description: "Create an issue in the tracker.\nIt needs a title and a project.", disabled: false },
  { name: "search_issues", description: "Search the issues with a query string.", disabled: false },
  { name: "comment_on_issue", description: "Add a comment to one issue.", disabled: false },
  {
    name: "transition_issue_to_another_workflow_state_with_a_very_long_tool_name",
    description:
      "Move an issue to another state of its workflow, and record the person who moved it and the reason for the move, which can be long.",
    disabled: true,
  },
];

function seed(): Entry[] {
  if (mode === "empty") return [];
  return [
    {
      id: "tracker",
      record: record(
        "tracker",
        "Issue tracker",
        http({ auth: "oauth2", scopes: ["issues:read", "issues:write"], headers: { "X-Team": "platform" }, secretHeaders: ["X-Api-Key"] }),
        { disabledTools: ["transition_issue_to_another_workflow_state_with_a_very_long_tool_name"] },
      ),
      invalid: null,
      status: { state: "ready", toolCount: TRACKER_TOOLS.length },
      secrets: [{ slot: "header:X-Api-Key", present: true }],
      tools: TRACKER_TOOLS.map((t) => ({ ...t })),
    },
    {
      id: "docs",
      record: record(
        "docs",
        "Docs index",
        stdio({ cwd: "tools/docs", env: { DOCS_REGION: "eu" }, secretEnv: ["DOCS_API_KEY"] }),
      ),
      invalid: null,
      status: { state: "untrusted" },
      secrets: [{ slot: "env:DOCS_API_KEY", present: true }],
      tools: [],
    },
    {
      id: "search",
      record: record(
        "search",
        "Code search",
        stdio({ command: "uvx", args: ["code-search-mcp"], secretEnv: ["SEARCH_TOKEN", "SEARCH_ORG"] }),
      ),
      invalid: null,
      status: { state: "needs_secret" },
      secrets: [
        { slot: "env:SEARCH_TOKEN", present: true },
        { slot: "env:SEARCH_ORG", present: false },
      ],
      tools: [],
    },
    {
      id: "wiki",
      record: record("wiki", "Team wiki", http({ url: "https://wiki.example.com/mcp", auth: "oauth2", clientId: "synthesis-app" })),
      invalid: null,
      status: { state: "needs_sign_in" },
      secrets: [{ slot: "oauth_client_secret", present: false }],
      tools: [],
    },
    {
      id: "metrics",
      record: record("metrics", "Metrics", http({ url: "https://metrics.internal.example/mcp", auth: "bearer" })),
      invalid: null,
      status: { state: "failed", failure: "unreachable" },
      secrets: [{ slot: "bearer", present: true }],
      tools: [],
    },
    {
      id: "files",
      record: record("files", "Local files", stdio({ command: "files-mcp", args: ["--root", "."] })),
      invalid: null,
      status: { state: "failed", failure: "process_exited" },
      secrets: [],
      tools: [],
    },
    {
      id: "calendar",
      record: record("calendar", "Calendar", http({ url: "https://cal.example.com/mcp", auth: "oauth2" })),
      invalid: null,
      status: { state: "signing_in" },
      secrets: [],
      tools: [],
    },
    {
      id: "archive",
      record: record("archive", "Old archive", http({ url: "https://archive.example.com/mcp" }), { enabled: false }),
      invalid: null,
      status: { state: "disabled" },
      secrets: [],
      tools: [],
    },
    {
      id: "Broken_Server",
      record: null,
      invalid: "invalid_id",
      status: { state: "invalid" },
      secrets: [],
      tools: [],
    },
  ];
}

let entries: Entry[] = seed();

function fire(id: string, status: unknown, delay = 400) {
  setTimeout(() => {
    const e = entries.find((x) => x.id === id);
    if (e) e.status = status;
    const f = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((n: string, p?: unknown) => void)
      | undefined;
    f?.("mcp-server-status-changed", { id, status });
  }, delay);
}

function slotsOf(rec: any): string[] {
  const t = rec.transport;
  if (t.kind === "stdio") return t.secretEnv.map((n: string) => `env:${n}`);
  const slots = t.secretHeaders.map((n: string) => `header:${n}`);
  if (t.auth === "bearer") slots.push("bearer");
  if (t.clientId) slots.push("oauth_client_secret");
  return slots;
}

function statusOf(e: Entry): unknown {
  const rec = e.record;
  if (!rec.enabled) return { state: "disabled" };
  const missing = e.secrets.some((s) => !s.present && s.slot !== "oauth_client_secret");
  if (missing) return { state: "needs_secret" };
  if (rec.transport.kind === "http" && rec.transport.auth === "oauth2" && e.status.state !== "ready") {
    return { state: "needs_sign_in" };
  }
  if (e.status.state === "ready") return e.status;
  return { state: "connecting" };
}

function find(id: string): Entry {
  const e = entries.find((x) => x.id === id);
  if (!e) throw "not_found";
  return e;
}

const copy = (e: Entry): Entry => JSON.parse(JSON.stringify(e));

export function mcpInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_mcp_servers":
      if (mode === "loadFail" && !(globalThis as Record<string, unknown>).__mcpLoadOk) {
        return Promise.reject("malformed_project_config");
      }
      return entries.map(copy);
    case "save_mcp_server": {
      const rec = a.server;
      const held = entries.find((x) => x.id === rec.id);
      const present = new Set((held?.secrets ?? []).filter((s) => s.present).map((s) => s.slot));
      const entry: Entry = {
        id: rec.id,
        record: rec,
        invalid: null,
        status: held?.status ?? { state: "connecting" },
        secrets: slotsOf(rec).map((slot) => ({ slot, present: present.has(slot) })),
        tools: held?.tools ?? [],
      };
      entry.tools = entry.tools.map((t) => ({ ...t, disabled: rec.disabledTools.includes(t.name) }));
      entry.status = statusOf(entry);
      if (held) entries = entries.map((x) => (x.id === rec.id ? entry : x));
      else entries.push(entry);
      if (entry.status && (entry.status as any).state === "connecting") {
        fire(rec.id, { state: "ready", toolCount: entry.tools.length });
      }
      return copy(entry);
    }
    case "remove_mcp_server":
      find(a.id);
      entries = entries.filter((x) => x.id !== a.id);
      return undefined;
    case "set_mcp_server_secret":
    case "clear_mcp_server_secret": {
      const e = find(a.id);
      const s = e.secrets.find((x) => x.slot === a.slot);
      if (!s) return Promise.reject("unknown_slot");
      s.present = cmd === "set_mcp_server_secret";
      if (e.record) e.status = statusOf(e);
      if (e.status.state === "connecting") fire(a.id, { state: "ready", toolCount: e.tools.length });
      return copy(e);
    }
    case "trust_mcp_server": {
      const e = find(a.id);
      if (e.record?.transport.kind !== "stdio") return Promise.reject("not_stdio");
      e.status = { state: "connecting" };
      e.tools = [
        { name: "search_docs", description: "Search the project documentation.", disabled: false },
        { name: "read_page", description: "Read one documentation page.", disabled: false },
      ];
      fire(a.id, { state: "ready", toolCount: e.tools.length });
      return copy(e);
    }
    case "test_mcp_server": {
      const e = find(a.id);
      const st = e.status.state;
      if (st === "failed") {
        return {
          ok: false,
          failure: e.status.failure,
          stderrTail: [
            "Error: connect ECONNREFUSED 10.0.4.12:443",
            "    at TCPConnectWrap.afterConnect [as oncomplete] (node:net:1555:16)",
            "    at a very long line of standard error output that goes on and on and on past the width of the dialog to test overflow",
          ],
        };
      }
      if (st === "ready") return { ok: true, tools: e.tools };
      return { ok: false, failure: st, stderrTail: [] };
    }
    case "reconnect_mcp_server": {
      const e = find(a.id);
      e.status = { state: "connecting" };
      fire(a.id, { state: "connecting" }, 0);
      fire(a.id, { state: "failed", failure: "unreachable" }, 900);
      return undefined;
    }
    case "start_mcp_sign_in": {
      const e = find(a.id);
      if (e.record?.transport.auth !== "oauth2") return Promise.reject("not_oauth2");
      fire(a.id, { state: "signing_in" }, 0);
      return undefined;
    }
    case "cancel_mcp_sign_in":
      find(a.id);
      fire(a.id, { state: "needs_sign_in" }, 0);
      return undefined;
    case "sign_out_mcp_server": {
      const e = find(a.id);
      e.status = { state: "needs_sign_in" };
      return copy(e);
    }
    default:
      return NOT_HANDLED;
  }
}
