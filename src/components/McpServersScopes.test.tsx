import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetSettingsSections } from "../state/settingsSweep";
import { otherCommand } from "../test/invokeMocks";
import { mcpEntry, mcpRecord } from "../test/mcpFixtures";
import type { McpServer, McpServerEntry, McpServerStatus } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Handler[]>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: Handler) => {
    listeners.set(name, [...(listeners.get(name) ?? []), handler]);
    return Promise.resolve(() => {
      listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== handler));
    });
  },
  emit: () => Promise.resolve(),
}));

import { McpServersSettings } from "./McpServersSettings";

function pushStatus(id: string, status: McpServerStatus) {
  act(() => {
    for (const handler of listeners.get("mcp-server-status-changed") ?? []) {
      handler({ payload: { id, status } });
    }
  });
}

let list: McpServerEntry[];
/** The switch answers keep the status of the row, as a server that is still stopping. */
let keepStatus: boolean;
/** Commands that refuse, by name, with the code they refuse with. */
let refuse: Record<string, string>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetSettingsSections();
  listeners.clear();
  invokeMock.mockReset();
  list = [];
  keepStatus = false;
  refuse = {};
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (refuse[cmd]) throw refuse[cmd];
    const id = args?.id as string;
    switch (cmd) {
      case "list_mcp_servers":
        return list;
      case "save_mcp_server": {
        const server = args?.server as McpServer;
        const held = list.find((e) => e.id === server.id && e.record);
        return { ...held!, record: server, enabledHere: server.enabled };
      }
      case "set_mcp_server_enabled": {
        const enabled = args?.enabled as boolean;
        const held = list.find((e) => e.id === id && e.record)!;
        return {
          ...held,
          record: held.scope === "team" ? held.record : { ...held.record!, enabled },
          enabledHere: enabled,
          status: keepStatus ? held.status : enabled ? { state: "connecting" } : { state: "disabled" },
        };
      }
      case "remove_mcp_server":
        return undefined;
      default:
        return otherCommand(cmd);
    }
  });
});
afterEach(cleanup);

const TEAM_DOCS = mcpRecord();
const MINE = mcpRecord({ id: "notes", name: "My notes" });
const EVERYWHERE = mcpRecord({ id: "scratch", name: "Scratch DB" });
const TOOLS = [{ name: "search", description: "Search.", disabled: false }];

const group = (key: "team" | "personal") => screen.getByTestId(`mcp-group-${key}`);

describe("MCP servers section: team servers and your servers", () => {
  it("MSS-FR-POYD, MSS-FR-SAKG: the team rows and the personal rows are in two groups, with scope badges", async () => {
    list = [
      mcpEntry(TEAM_DOCS, { state: "connecting" }),
      mcpEntry(EVERYWHERE, { state: "connecting" }, { scope: "personal_all" }),
      mcpEntry(MINE, { state: "connecting" }, { scope: "personal_project" }),
    ];
    render(<McpServersSettings />);
    await screen.findByRole("listitem", { name: "Docs index" });
    const team = group("team");
    expect(within(team).getByRole("heading", { name: "Team servers" })).toBeInTheDocument();
    expect(team).toHaveTextContent(
      "Saved in the project and shared with everyone who works on it. You can turn each one off on this computer.",
    );
    expect(within(team).getAllByRole("listitem").map((li) => li.getAttribute("aria-label"))).toEqual([
      "Docs index",
    ]);
    expect(within(team).queryByTestId("mcp-scope-badge")).toBeNull();
    // MSS-FR-POYD: the team group comes first.
    expect(team.compareDocumentPosition(group("personal")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const personal = group("personal");
    expect(within(personal).getByRole("heading", { name: "Your servers" })).toBeInTheDocument();
    expect(personal).toHaveTextContent("Saved on this computer only. Nobody else sees them.");
    const rows = within(personal).getAllByRole("listitem");
    // MCS-FR-FIEE: this project's records before those of all projects.
    expect(rows.map((li) => li.getAttribute("aria-label"))).toEqual(["My notes", "Scratch DB"]);
    expect(within(rows[0]).getByTestId("mcp-scope-badge")).toHaveTextContent("This project");
    expect(within(rows[1]).getByTestId("mcp-scope-badge")).toHaveTextContent("All projects");
  });

  it("MSS-FR-POYD: a group with no entry says so", async () => {
    list = [mcpEntry(MINE, { state: "connecting" }, { scope: "personal_project" })];
    render(<McpServersSettings />);
    await screen.findByRole("listitem", { name: "My notes" });
    expect(screen.getByTestId("mcp-group-team-empty")).toHaveTextContent("No team servers.");
    list = [mcpEntry(TEAM_DOCS, { state: "connecting" })];
    cleanup();
    render(<McpServersSettings />);
    await screen.findByRole("listitem", { name: "Docs index" });
    expect(screen.getByTestId("mcp-group-personal-empty")).toHaveTextContent("No servers of your own.");
  });

  it("MSS-FR-CSHM: with no server of any scope there is no group", async () => {
    render(<McpServersSettings />);
    expect(await screen.findByTestId("mcp-servers-empty")).toBeInTheDocument();
    expect(screen.queryByTestId("mcp-group-team")).toBeNull();
    expect(screen.queryByTestId("mcp-group-personal")).toBeNull();
  });

  it("MSS-FR-RAYC, MSS-FR-GBOM: a personal row that shares its ID with a team row can only be removed", async () => {
    list = [
      mcpEntry(TEAM_DOCS, { state: "connecting" }),
      mcpEntry(null, { state: "invalid" }, { id: "docs", scope: "personal_project", invalid: "duplicate_id" }),
    ];
    render(<McpServersSettings />);
    await screen.findByRole("listitem", { name: "Docs index" });
    const shadowed = within(group("personal")).getByRole("listitem", { name: "docs" });
    expect(within(shadowed).getByTestId("mcp-server-problem")).toHaveTextContent(
      "A team server, or another server of yours, has this ID. Remove this one and add it again with another ID.",
    );
    expect(within(shadowed).getAllByRole("button").map((b) => b.textContent)).toEqual(["Remove"]);
    expect(within(shadowed).queryByRole("switch")).toBeNull();

    // MCS-FR-PQHI: the status event of the ID is of the team row.
    pushStatus("docs", { state: "ready", toolCount: 0 });
    const teamRow = within(group("team")).getByRole("listitem", { name: "Docs index" });
    expect(within(teamRow).getByTestId("mcp-status-chip")).toHaveTextContent("Connected · 0 tools");
    expect(within(shadowed).getByTestId("mcp-status-chip")).toHaveTextContent("Invalid");

    // MSS-FR-KQNF, MSS-FR-UYFX: the remove names the scope of the row.
    await userEvent.click(within(shadowed).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    expect(dialog).toHaveTextContent(
      "Remove docs? Its secrets and its sign-in on this computer are removed too.",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(calls("remove_mcp_server")).toHaveLength(1));
    expect(calls("remove_mcp_server")[0][1]).toEqual({ id: "docs", scope: "personal_project" });
    await waitFor(() => expect(screen.getByTestId("mcp-group-personal-empty")).toBeInTheDocument());
    expect(within(group("team")).getByRole("listitem", { name: "Docs index" })).toBeInTheDocument();
  });

  it("MCS-FR-PQHI: after the remove of the team row, the personal row of its ID is read again", async () => {
    list = [
      mcpEntry(TEAM_DOCS, { state: "connecting" }),
      mcpEntry(null, { state: "invalid" }, { id: "docs", scope: "personal_project", invalid: "duplicate_id" }),
    ];
    render(<McpServersSettings />);
    const teamRow = await screen.findByRole("listitem", { name: "Docs index" });
    await userEvent.click(within(teamRow).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    list = [mcpEntry({ ...TEAM_DOCS, name: "My docs" }, { state: "connecting" }, { scope: "personal_project" })];
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(await screen.findByRole("listitem", { name: "My docs" })).toBeInTheDocument();
    expect(calls("remove_mcp_server")[0][1]).toEqual({ id: "docs", scope: "team" });
    expect(screen.getByTestId("mcp-group-team-empty")).toBeInTheDocument();
  });

  it("MSS-FR-UYFX: the remove of a server of all projects says it goes from all of them", async () => {
    list = [mcpEntry(EVERYWHERE, { state: "connecting" }, { scope: "personal_all" })];
    render(<McpServersSettings />);
    const row = await screen.findByRole("listitem", { name: "Scratch DB" });
    await userEvent.click(within(row).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    expect(dialog).toHaveTextContent(
      "Remove Scratch DB from all your projects? Its secrets and its sign-in on this computer are removed too.",
    );
    // MSS-FR-KQNF: the remove names the scope of the row.
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(calls("remove_mcp_server")).toHaveLength(1));
    expect(calls("remove_mcp_server")[0][1]).toEqual({ id: "scratch", scope: "personal_all" });
  });

  it("MSS-FR-UYFX: the remove of a server of this project names it", async () => {
    list = [mcpEntry(MINE, { state: "connecting" }, { scope: "personal_project" })];
    render(<McpServersSettings />);
    const row = await screen.findByRole("listitem", { name: "My notes" });
    await userEvent.click(within(row).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    expect(dialog).toHaveTextContent(
      "Remove My notes? Its secrets and its sign-in on this computer are removed too.",
    );
  });

  it("MSS-FR-GBOM: a refused personal switch goes back and shows the refusal", async () => {
    list = [mcpEntry(MINE, { state: "connecting" }, { scope: "personal_project" })];
    refuse.set_mcp_server_enabled = "keychain_unavailable";
    render(<McpServersSettings />);
    const row = await screen.findByRole("listitem", { name: "My notes" });
    const toggle = within(row).getByRole("switch", { name: "On on this computer" });
    await userEvent.click(toggle);
    expect(await within(row).findByTestId("mcp-row-error")).toHaveTextContent(
      "The system keychain is not available.",
    );
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it("MCS-FR-LLQD: a new server takes no ID of a personal server", async () => {
    list = [mcpEntry(MINE, { state: "connecting" }, { scope: "personal_project" })];
    render(<McpServersSettings />);
    await screen.findByRole("listitem", { name: "My notes" });
    await userEvent.click(screen.getByRole("button", { name: "+ Add server" }));
    const dialog = await screen.findByRole("dialog", { name: "Add MCP server" });
    await userEvent.type(within(dialog).getByRole("textbox", { name: "Name" }), "Notes");
    expect(dialog).toHaveTextContent("Another server of this project has this ID.");
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
    expect(calls("save_mcp_server")).toHaveLength(0);
  });

  it("MSS-FR-GBOM, MSS-FR-OOUI: a personal switch is its enabled, and a later tool change keeps it", async () => {
    list = [
      mcpEntry(MINE, { state: "ready", toolCount: 1 }, { scope: "personal_project", tools: TOOLS }),
    ];
    keepStatus = true;
    render(<McpServersSettings />);
    const row = await screen.findByRole("listitem", { name: "My notes" });
    await userEvent.click(within(row).getByRole("switch", { name: "On on this computer" }));
    expect(within(row).getByRole("switch", { name: "On on this computer" })).not.toBeChecked();
    // A personal row has no team value to differ from.
    expect(within(row).queryByTestId("mcp-switch-note")).toBeNull();
    await userEvent.click(within(row).getByRole("button", { name: /Tools \(1\)/ }));
    await userEvent.click(within(row).getByRole("switch", { name: "search" }));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(calls("set_mcp_server_enabled")[0][1]).toEqual({ id: "notes", enabled: false });
    const saved = calls("save_mcp_server")[0][1] as { server: McpServer; scope: string };
    expect(saved.scope).toBe("personal_project");
    expect(saved.server.enabled).toBe(false);
    expect(saved.server.disabledTools).toEqual(["search"]);
  });
});
