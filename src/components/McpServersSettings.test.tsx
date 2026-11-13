import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { flushLogs, resetLogBufferForTest } from "../logging";
import { resetSettingsSections, settingsSectionsPending } from "../state/settingsSweep";
import { otherCommand } from "../test/invokeMocks";
import { httpTransport, mcpEntry, mcpRecord, stdioTransport } from "../test/mcpFixtures";
import type { LogInput, McpServer, McpServerEntry, McpServerStatus, McpTestResult } from "../types";

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

/** MCS-FR-NOKX: the backend reports a new status. */
function pushStatus(id: string, status: McpServerStatus) {
  act(() => {
    for (const handler of listeners.get("mcp-server-status-changed") ?? []) {
      handler({ payload: { id, status } });
    }
  });
}

let list: McpServerEntry[];
/** Commands that refuse, by name, with the code they refuse with. */
let refuse: Record<string, string>;
let testResult: McpTestResult;
/** A read of the list that waits until the test releases it. */
let holdList: Promise<void> | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const savedRecords = () => calls("save_mcp_server").map((c) => (c[1] as { server: McpServer }).server);

function logRecords(): LogInput[] {
  flushLogs();
  return calls("append_log_records").flatMap((c) => (c[1] as { records: LogInput[] }).records);
}

function entryAfterSave(server: McpServer): McpServerEntry {
  const held = list.find((e) => e.id === server.id);
  return {
    ...(held ?? mcpEntry(server, { state: "connecting" })),
    record: server,
    status: server.enabled ? (held?.status ?? { state: "connecting" }) : { state: "disabled" },
  };
}

beforeEach(() => {
  resetLogBufferForTest();
  resetSettingsSections();
  listeners.clear();
  invokeMock.mockReset();
  list = [];
  refuse = {};
  holdList = null;
  testResult = { ok: true, tools: [] };
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (refuse[cmd]) throw refuse[cmd];
    const id = args?.id as string;
    switch (cmd) {
      case "list_mcp_servers":
        if (holdList) await holdList;
        return list;
      case "save_mcp_server": {
        const server = args?.server as McpServer;
        const saved = entryAfterSave(server);
        list = list.map((e) => (e.id === server.id ? saved : e));
        return saved;
      }
      case "trust_mcp_server":
      case "sign_out_mcp_server": {
        const held = list.find((e) => e.id === id)!;
        return {
          ...held,
          status: cmd === "trust_mcp_server" ? { state: "connecting" } : { state: "needs_sign_in" },
        };
      }
      case "test_mcp_server":
        return testResult;
      case "remove_mcp_server":
      case "reconnect_mcp_server":
      case "start_mcp_sign_in":
      case "cancel_mcp_sign_in":
        return undefined;
      default:
        return otherCommand(cmd);
    }
  });
});
afterEach(cleanup);

const row = (name: string) => screen.getByRole("listitem", { name });
const findRow = (name: string) => screen.findByRole("listitem", { name });
const chipOf = (item: HTMLElement) => within(item).getByTestId("mcp-status-chip");

const DOCS = mcpRecord();
const TRACKER = mcpRecord({
  id: "tracker",
  name: "Issue tracker",
  transport: httpTransport({ auth: "oauth2" }),
});
const TOOLS = [
  { name: "create_issue", description: "Create an issue.\nIt needs a title.", disabled: false },
  { name: "delete_issue", description: "", disabled: false },
];

describe("MCP servers section: the list", () => {
  it("MSS-FR-RYEQ: shows a loading line until the list arrives", async () => {
    let release = () => {};
    holdList = new Promise<void>((resolve) => (release = resolve));
    list = [mcpEntry(DOCS, { state: "untrusted" })];
    render(<McpServersSettings />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading MCP servers…");
    expect(screen.queryByRole("button", { name: "+ Add server" })).toBeNull();
    release();
    expect(await findRow("Docs index")).toBeInTheDocument();
    expect(screen.queryByText("Loading MCP servers…")).toBeNull();
  });

  it("MSS-FR-RYEQ: a failed load shows its error and Try again reads the list again", async () => {
    refuse.list_mcp_servers = "malformed_project_config";
    render(<McpServersSettings />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("The project settings file cannot be read.");
    delete refuse.list_mcp_servers;
    list = [mcpEntry(DOCS, { state: "untrusted" })];
    await userEvent.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await findRow("Docs index")).toBeInTheDocument();
    expect(calls("list_mcp_servers")).toHaveLength(2);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("MSS-FR-CSHM: with no server, the section says so and offers Add server", async () => {
    render(<McpServersSettings />);
    expect(
      await screen.findByText(
        "No MCP servers yet. Add one to give the agents of this project its tools.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "+ Add server" })).toBeInTheDocument();
  });

  it("MSS-FR-XALV / MSS-FR-PEGO: each row shows name, id, transport badge, and chip, in list order", async () => {
    list = [
      mcpEntry(TRACKER, { state: "ready", toolCount: 3 }),
      mcpEntry(DOCS, { state: "untrusted" }),
    ];
    render(<McpServersSettings />);
    await findRow("Issue tracker");
    const rows = screen.getAllByTestId("mcp-server-row");
    expect(rows.map((r) => r.getAttribute("data-server-id"))).toEqual(["tracker", "docs"]);
    expect(within(rows[0]).getByText("tracker")).toBeInTheDocument();
    expect(within(rows[0]).getByTestId("mcp-transport-badge")).toHaveTextContent("Remote");
    expect(chipOf(rows[0])).toHaveTextContent("Connected · 3 tools");
    expect(chipOf(rows[0]).className).toContain("badge--ok");
    expect(within(rows[1]).getByTestId("mcp-transport-badge")).toHaveTextContent("Local");
    expect(chipOf(rows[1])).toHaveTextContent("Not trusted");
    expect(chipOf(rows[1]).className).toContain("badge--warn");
  });

  it("MSS-FR-YIMR: a failed row shows its failure line, an invalid row its validation text", async () => {
    list = [
      mcpEntry(DOCS, { state: "failed", failure: "command_not_found" }),
      mcpEntry(null, { state: "invalid" }, { id: "broken", invalid: "invalid_url" }),
    ];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(within(docs).getByTestId("mcp-server-problem")).toHaveTextContent(
      "The command was not found on this computer.",
    );
    expect(chipOf(docs).className).toContain("badge--danger");
    const broken = row("broken");
    expect(chipOf(broken)).toHaveTextContent("Invalid");
    expect(within(broken).getByTestId("mcp-server-problem")).toHaveTextContent("Enter an https URL.");
    // An invalid entry has no record, so its switch cannot save one.
    expect(within(broken).getByRole("switch")).toBeDisabled();
  });

  it("MSS-FR-CLQR: a status event updates the chip and the failure line at once, with no reload", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" }), mcpEntry(TRACKER, { state: "needs_sign_in" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(chipOf(docs)).toHaveTextContent("Connecting…");

    pushStatus("docs", { state: "failed", failure: "process_exited" });
    expect(chipOf(row("Docs index"))).toHaveTextContent("Failed");
    expect(within(row("Docs index")).getByTestId("mcp-server-problem")).toHaveTextContent(
      "The server process stopped.",
    );
    // The other row is untouched.
    expect(chipOf(row("Issue tracker"))).toHaveTextContent("Sign in needed");

    pushStatus("docs", { state: "connecting" });
    expect(chipOf(row("Docs index"))).toHaveTextContent("Connecting…");
    expect(within(row("Docs index")).queryByTestId("mcp-server-problem")).toBeNull();
    // An event for a server the section does not hold changes nothing.
    pushStatus("unknown", { state: "disabled" });
    expect(screen.getAllByTestId("mcp-server-row")).toHaveLength(2);
    expect(calls("list_mcp_servers")).toHaveLength(1);
  });

  it("MSS-FR-CLQR: a server that becomes ready with other tools reads its tools again", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" })];
    render(<McpServersSettings />);
    await findRow("Docs index");
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 }, { tools: TOOLS })];
    pushStatus("docs", { state: "ready", toolCount: 2 });
    expect(chipOf(row("Docs index"))).toHaveTextContent("Connected · 2 tools");
    await waitFor(() => expect(calls("list_mcp_servers")).toHaveLength(2));
    await userEvent.click(within(row("Docs index")).getByRole("button", { name: /Tools \(2\)/ }));
    expect(await screen.findByText("create_issue")).toBeInTheDocument();
  });

  it("MSS-FR-CLQR: the section stops listening when it closes", async () => {
    const { unmount } = render(<McpServersSettings />);
    await screen.findByTestId("mcp-servers-empty");
    await waitFor(() => expect(listeners.get("mcp-server-status-changed")).toHaveLength(1));
    unmount();
    expect(listeners.get("mcp-server-status-changed")).toHaveLength(0);
  });
});

describe("MCP servers section: row actions", () => {
  it("MSS-FR-GBOM: each row has the Enabled switch, Test, Edit, and Remove", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(within(docs).getByRole("switch", { name: "Enabled" })).toBeChecked();
    for (const name of ["Test", "Edit", "Remove"]) {
      expect(within(docs).getByRole("button", { name })).toBeInTheDocument();
    }
  });

  it("MSS-FR-GBOM: the switch saves the record with the new enabled value", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("switch", { name: "Enabled" }));
    await waitFor(() => expect(savedRecords()).toEqual([{ ...DOCS, enabled: false }]));
    await waitFor(() => expect(chipOf(row("Docs index"))).toHaveTextContent("Off"));
    expect(within(row("Docs index")).getByRole("switch", { name: "Enabled" })).not.toBeChecked();
  });

  it("MSS-FR-GBOM: a refused save puts the switch back and shows the refusal in the row", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" })];
    refuse.save_mcp_server = "malformed_project_config";
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    const toggle = within(docs).getByRole("switch", { name: "Enabled" });
    await userEvent.click(toggle);
    expect(await within(docs).findByTestId("mcp-row-error")).toHaveTextContent(
      "The project settings file cannot be read.",
    );
    expect(toggle).toBeChecked();
  });

  it("MSS-FR-GBOM: quick changes are saved one after the other, and each is kept", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    const toggle = within(docs).getByRole("switch", { name: "Enabled" });
    await userEvent.click(toggle);
    await userEvent.click(toggle);
    await waitFor(() => expect(savedRecords()).toHaveLength(2));
    expect(savedRecords().map((r) => r.enabled)).toEqual([false, true]);
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it("MSS-FR-YFPL / MSS-FR-GJVD: Trust… opens the trust dialog, and Trust and start trusts and closes it", async () => {
    list = [mcpEntry(DOCS, { state: "untrusted" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(within(docs).queryByRole("button", { name: "Sign in" })).toBeNull();
    await userEvent.click(within(docs).getByRole("button", { name: "Trust…" }));
    const dialog = await screen.findByRole("dialog", { name: "Trust this server?" });
    expect(dialog).toHaveTextContent("This project runs this command on your computer:");
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("trust_mcp_server")[0][1]).toEqual({ id: "docs", shown: DOCS });
    expect(chipOf(row("Docs index"))).toHaveTextContent("Connecting…");
  });

  it("MSS-FR-YFPL / MSS-FR-SCFN: an untrusted Remote row has Trust… too, and its dialog names where credentials go", async () => {
    list = [mcpEntry(TRACKER, { state: "untrusted" })];
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    await userEvent.click(within(tracker).getByRole("button", { name: "Trust…" }));
    const dialog = await screen.findByRole("dialog", { name: "Trust this server?" });
    expect(dialog).toHaveTextContent("This project sends your credentials for this server to:");
    expect(dialog).toHaveTextContent("https://mcp.tracker.example/mcp");
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    await waitFor(() => expect(calls("trust_mcp_server")).toHaveLength(1));
    expect(calls("trust_mcp_server")[0][1]).toEqual({ id: "tracker", shown: TRACKER });
  });

  it("MSS-FR-TBQY: after Trust and start, focus goes to the row of the server", async () => {
    list = [mcpEntry(DOCS, { state: "untrusted" }), mcpEntry(TRACKER, { state: "needs_sign_in" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Trust…" }));
    const dialog = await screen.findByRole("dialog", { name: "Trust this server?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // The Trust… button that opened the dialog is gone with the untrusted status.
    expect(within(row("Docs index")).queryByRole("button", { name: "Trust…" })).toBeNull();
    await waitFor(() => expect(row("Docs index")).toHaveFocus());
  });

  it("MSS-FR-UVWX: a record_changed refusal keeps the trust dialog open with its text", async () => {
    list = [mcpEntry(DOCS, { state: "untrusted" })];
    refuse.trust_mcp_server = "record_changed";
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Trust…" }));
    const dialog = await screen.findByRole("dialog", { name: "Trust this server?" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "The record changed. Close this and look at it again.",
    );
    expect(screen.getByRole("dialog", { name: "Trust this server?" })).toBeInTheDocument();
    expect(chipOf(row("Docs index"))).toHaveTextContent("Not trusted");
  });

  it("MSS-FR-YFPL: only an untrusted row has Trust…", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(within(docs).queryByRole("button", { name: "Trust…" })).toBeNull();
  });

  it("MSS-FR-HXVP: needs_sign_in has Sign in, which starts the sign-in", async () => {
    list = [mcpEntry(TRACKER, { state: "needs_sign_in" })];
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    await userEvent.click(within(tracker).getByRole("button", { name: "Sign in" }));
    await waitFor(() => expect(calls("start_mcp_sign_in")).toHaveLength(1));
    expect(calls("start_mcp_sign_in")[0][1]).toEqual({ id: "tracker" });
  });

  it("MSS-FR-HXVP: a failed oauth2 row with oauth_failed or rejected has Sign in; a bearer row does not", async () => {
    const bearer = mcpRecord({
      id: "metrics",
      name: "Metrics",
      transport: httpTransport({ auth: "bearer" }),
    });
    list = [
      mcpEntry(TRACKER, { state: "failed", failure: "rejected" }),
      mcpEntry(bearer, { state: "failed", failure: "rejected" }),
    ];
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    expect(within(tracker).getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(within(row("Metrics")).queryByRole("button", { name: "Sign in" })).toBeNull();
    pushStatus("tracker", { state: "failed", failure: "oauth_failed" });
    expect(within(row("Issue tracker")).getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    pushStatus("tracker", { state: "failed", failure: "unreachable" });
    expect(within(row("Issue tracker")).queryByRole("button", { name: "Sign in" })).toBeNull();
  });

  it("MSS-FR-HXVP: signing_in shows Waiting for the browser… and Cancel cancels the sign-in", async () => {
    list = [mcpEntry(TRACKER, { state: "signing_in" })];
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    expect(within(tracker).getByText("Waiting for the browser…")).toBeInTheDocument();
    await userEvent.click(within(tracker).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(calls("cancel_mcp_sign_in")).toHaveLength(1));
    expect(calls("cancel_mcp_sign_in")[0][1]).toEqual({ id: "tracker" });
  });

  it("MSS-FR-HXVP: a refused sign-in shows its text in the row", async () => {
    list = [mcpEntry(TRACKER, { state: "needs_sign_in" })];
    refuse.start_mcp_sign_in = "sign_in_in_progress";
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    await userEvent.click(within(tracker).getByRole("button", { name: "Sign in" }));
    expect(await within(tracker).findByTestId("mcp-row-error")).toHaveTextContent(
      "A sign-in is already in progress.",
    );
  });

  it("MSS-FR-HXVP: a sign-in refused for an untrusted record asks for trust first", async () => {
    list = [mcpEntry(TRACKER, { state: "needs_sign_in" })];
    refuse.start_mcp_sign_in = "untrusted";
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    await userEvent.click(within(tracker).getByRole("button", { name: "Sign in" }));
    expect(await within(tracker).findByTestId("mcp-row-error")).toHaveTextContent(
      "Trust this server before you sign in.",
    );
  });

  it("MSS-FR-ANUU: a ready oauth2 row has Sign out; a ready row of another auth does not", async () => {
    list = [
      mcpEntry(TRACKER, { state: "ready", toolCount: 1 }),
      mcpEntry(DOCS, { state: "ready", toolCount: 1 }),
    ];
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    expect(within(row("Docs index")).queryByRole("button", { name: "Sign out" })).toBeNull();
    await userEvent.click(within(tracker).getByRole("button", { name: "Sign out" }));
    await waitFor(() => expect(chipOf(row("Issue tracker"))).toHaveTextContent("Sign in needed"));
    expect(calls("sign_out_mcp_server")[0][1]).toEqual({ id: "tracker" });
  });

  it("MSS-FR-PDVK: needs_secret says so, and Set secrets opens the dialog on the first secret with no value", async () => {
    const record = mcpRecord({
      transport: stdioTransport({ secretEnv: ["FIRST", "SECOND"] }),
    });
    list = [
      mcpEntry(record, { state: "needs_secret" }, {
        secrets: [
          { slot: "env:FIRST", present: true },
          { slot: "env:SECOND", present: false },
        ],
      }),
    ];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(
      within(docs).getByText("Add the secrets this server needs on this computer."),
    ).toBeInTheDocument();
    await userEvent.click(within(docs).getByRole("button", { name: "Set secrets" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit MCP server" });
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("Docs index");
    expect(within(dialog).getByLabelText("Value of SECOND")).toHaveFocus();
  });

  it("MSS-FR-WHXM: a failed row has Reconnect, which reconnects the server", async () => {
    list = [mcpEntry(DOCS, { state: "failed", failure: "timed_out" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Reconnect" }));
    await waitFor(() => expect(calls("reconnect_mcp_server")).toHaveLength(1));
    expect(calls("reconnect_mcp_server")[0][1]).toEqual({ id: "docs" });
    pushStatus("docs", { state: "connecting" });
    expect(within(row("Docs index")).queryByRole("button", { name: "Reconnect" })).toBeNull();
  });

  it("MSS-FR-VWKQ: Test shows the tool count of a success under the row", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 })];
    testResult = { ok: true, tools: TOOLS };
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    expect(await within(docs).findByText("Connected. 2 tools.")).toBeInTheDocument();
    expect(calls("test_mcp_server")[0][1]).toEqual({ id: "docs" });
  });

  it("MSS-FR-VWKQ: a failed test shows the failure text and the standard error lines; the next test replaces it", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 })];
    testResult = { ok: false, failure: "process_exited", stderrTail: ["line one", "line two"] };
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    expect(await within(docs).findByText("The server process stopped.")).toBeInTheDocument();
    expect(within(docs).getByTestId("mcp-test-stderr").tagName).toBe("PRE");
    expect(within(docs).getByTestId("mcp-test-stderr")).toHaveTextContent("line one line two");

    testResult = { ok: false, failure: "unreachable", stderrTail: [] };
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    expect(await within(docs).findByText("The server cannot be reached.")).toBeInTheDocument();
    expect(within(docs).queryByText("The server process stopped.")).toBeNull();
    expect(within(docs).queryByTestId("mcp-test-stderr")).toBeNull();
  });

  it("MSS-FR-VWKQ: a test refused for the status names it; a refused command shows its text", async () => {
    list = [mcpEntry(DOCS, { state: "untrusted" })];
    testResult = { ok: false, failure: "untrusted", stderrTail: [] };
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    expect(await within(docs).findByText("Trust this server before it runs.")).toBeInTheDocument();
    refuse.test_mcp_server = "not_found";
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    expect(
      await within(docs).findByText("This server is not in the project settings now."),
    ).toBeInTheDocument();
  });

  it("MSS-FR-VWKQ: the test result goes when the section closes", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 })];
    const first = render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    await within(docs).findByText("Connected. 0 tools.");
    first.unmount();
    render(<McpServersSettings />);
    await findRow("Docs index");
    expect(screen.queryByTestId("mcp-test-result")).toBeNull();
  });

  it("MSS-FR-KQNF: Remove asks first; Cancel removes nothing", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    expect(dialog).toHaveTextContent(
      "Remove Docs index? Its secrets and its sign-in on this computer are removed too.",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("remove_mcp_server")).toHaveLength(0);
    expect(row("Docs index")).toBeInTheDocument();
  });

  it("MSS-FR-KQNF: Remove removes the server and its row", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 }), mcpEntry(TRACKER, { state: "needs_sign_in" })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "Docs index" })).toBeNull());
    expect(calls("remove_mcp_server")[0][1]).toEqual({ id: "docs" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(row("Issue tracker")).toBeInTheDocument();
  });

  it("MSS-FR-TBQY: after a confirmed Remove, focus goes to the Add server button", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(screen.getByRole("button", { name: "+ Add server" })).toHaveFocus());
  });

  it("MSS-FR-KQNF: a refused remove keeps the row and shows the refusal in the confirmation", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 })];
    refuse.remove_mcp_server = "not_found";
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: "Remove" }));
    const dialog = await screen.findByRole("dialog", { name: "Remove MCP server" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "This server is not in the project settings now.",
    );
    expect(row("Docs index")).toBeInTheDocument();
  });
});

describe("MCP servers section: tools", () => {
  it("MSS-FR-QMTG: a ready row has a Tools disclosure that lists name, first description line, and a switch", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 }, { tools: TOOLS })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    const disclosure = within(docs).getByRole("button", { name: /Tools \(2\)/ });
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(within(docs).queryByTestId("mcp-tool-list")).toBeNull();
    await userEvent.click(disclosure);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    const tools = within(docs).getByTestId("mcp-tool-list");
    expect(within(tools).getByText("create_issue")).toBeInTheDocument();
    expect(within(tools).getByText("Create an issue.")).toBeInTheDocument();
    expect(within(tools).queryByText(/It needs a title/)).toBeNull();
    expect(within(tools).getByRole("switch", { name: "create_issue" })).toBeChecked();
  });

  it("MSS-FR-QMTG: a row that is not ready has no Tools disclosure", async () => {
    list = [mcpEntry(DOCS, { state: "connecting" }, { tools: TOOLS })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    expect(within(docs).queryByRole("button", { name: /Tools/ })).toBeNull();
  });

  it("MSS-FR-MMLA: a tool switch turned off adds it to disabledTools, and turned on removes it", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 }, { tools: TOOLS })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: /Tools \(2\)/ }));
    const toggle = within(docs).getByRole("switch", { name: "delete_issue" });
    await userEvent.click(toggle);
    await waitFor(() => expect(savedRecords()).toHaveLength(1));
    expect(savedRecords()[0].disabledTools).toEqual(["delete_issue"]);
    await waitFor(() => expect(toggle).not.toBeChecked());
    await userEvent.click(toggle);
    await waitFor(() => expect(savedRecords()).toHaveLength(2));
    expect(savedRecords()[1].disabledTools).toEqual([]);
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it("MSS-FR-MMLA: a refused save puts the tool switch back", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 }, { tools: TOOLS })];
    refuse.save_mcp_server = "no_project_open";
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("button", { name: /Tools \(2\)/ }));
    const toggle = within(docs).getByRole("switch", { name: "create_issue" });
    await userEvent.click(toggle);
    expect(await within(docs).findByTestId("mcp-row-error")).toHaveTextContent("No project is open.");
    expect(toggle).toBeChecked();
  });
});

describe("MCP servers section: dialogs, state, and logs", () => {
  it("MSS-FR-MWAV: Add server opens the empty dialog, and a save adds the row at the end", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 0 })];
    render(<McpServersSettings />);
    await findRow("Docs index");
    await userEvent.click(screen.getByRole("button", { name: "+ Add server" }));
    const dialog = await screen.findByRole("dialog", { name: "Add MCP server" });
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("");
    await userEvent.type(within(dialog).getByRole("textbox", { name: "Name" }), "Wiki");
    await userEvent.type(within(dialog).getByRole("textbox", { name: "Command" }), "wiki-mcp");
    await userEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const rows = screen.getAllByTestId("mcp-server-row");
    expect(rows.map((r) => r.getAttribute("data-server-id"))).toEqual(["docs", "wiki"]);
  });

  it("MSS-FR-MWAV: Edit opens the dialog with the record of the row", async () => {
    list = [mcpEntry(TRACKER, { state: "needs_sign_in" })];
    render(<McpServersSettings />);
    const tracker = await findRow("Issue tracker");
    await userEvent.click(within(tracker).getByRole("button", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit MCP server" });
    expect(within(dialog).getByRole("textbox", { name: "ID" })).toHaveValue("tracker");
    expect(within(dialog).getByRole("radio", { name: "Remote (HTTP)" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "OAuth 2" })).toBeChecked();
  });

  it("MSS-FR-GMHC: no action makes the section a pending change of the window", async () => {
    list = [mcpEntry(DOCS, { state: "ready", toolCount: 2 }, { tools: TOOLS })];
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("switch", { name: "Enabled" }));
    await waitFor(() => expect(savedRecords()).toHaveLength(1));
    await userEvent.click(within(docs).getByRole("button", { name: "Edit" }));
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), " changed");
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("MSS-FR-VSMZ: each action logs its start at INFO and each refusal at WARN, with the id and the code", async () => {
    const secretRecord = mcpRecord({
      transport: stdioTransport({
        command: "secret-command",
        env: { PLAIN: "plain-value" },
      }),
    });
    list = [mcpEntry(secretRecord, { state: "ready", toolCount: 0 })];
    refuse.save_mcp_server = "invalid_cwd";
    render(<McpServersSettings />);
    const docs = await findRow("Docs index");
    await userEvent.click(within(docs).getByRole("switch", { name: "Enabled" }));
    await within(docs).findByTestId("mcp-row-error");
    await userEvent.click(within(docs).getByRole("button", { name: "Test" }));
    await within(docs).findByTestId("mcp-test-result");

    const records = logRecords();
    const started = records.find((r) => r.message === "mcp server save started");
    expect(started).toMatchObject({ level: "INFO", fields: { id: "docs" } });
    const refused = records.find((r) => r.message === "mcp server save refused");
    expect(refused).toMatchObject({ level: "WARN", fields: { id: "docs", error: "invalid_cwd" } });
    expect(records.find((r) => r.message === "mcp server test started")).toMatchObject({
      level: "INFO",
      fields: { id: "docs" },
    });
    // No record holds a field value.
    const text = JSON.stringify(records);
    expect(text).not.toContain("secret-command");
    expect(text).not.toContain("plain-value");
  });
});
