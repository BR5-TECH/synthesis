import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { flushLogs, resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import { httpTransport, mcpEntry, mcpRecord, stdioTransport } from "../test/mcpFixtures";
import type { LogInput, McpServer, McpServerEntry } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { McpServerDialog } from "./McpServerDialog";

/** Commands that refuse, with the code. A slot key refuses the secret step of that slot. */
let refuse: Record<string, string>;
let holdSave: Promise<void> | null;
/** The secret presence that a save answers with: the one of the entry of the dialog. */
let presence: McpServerEntry["secrets"];

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const sequence = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] !== "append_log_records")
    .map((c) => {
      const args = c[1] as { slot?: string };
      return args?.slot ? `${c[0]} ${args.slot}` : (c[0] as string);
    });
function logRecords(): LogInput[] {
  flushLogs();
  return calls("append_log_records").flatMap((c) => (c[1] as { records: LogInput[] }).records);
}
const savedServer = (index = 0) => (calls("save_mcp_server")[index][1] as { server: McpServer }).server;

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  refuse = {};
  holdSave = null;
  presence = [];
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (refuse[cmd]) throw refuse[cmd];
    switch (cmd) {
      case "save_mcp_server": {
        if (holdSave) await holdSave;
        const server = args?.server as McpServer;
        return mcpEntry(server, { state: "connecting" }, { secrets: presence });
      }
      case "set_mcp_server_secret":
      case "clear_mcp_server_secret": {
        const slot = args?.slot as string;
        if (refuse[slot]) throw refuse[slot];
        return mcpEntry(null, { state: "connecting" }, { id: args?.id as string, secrets: [{ slot, present: cmd === "set_mcp_server_secret" }] });
      }
      default:
        return otherCommand(cmd);
    }
  });
});
afterEach(cleanup);

interface Setup {
  entry?: McpServerEntry | null;
  knownIds?: string[];
  focusMissingSecret?: boolean;
}

function setup({ entry = null, knownIds = [], focusMissingSecret = false }: Setup = {}) {
  const onClose = vi.fn();
  const onSaved = vi.fn();
  presence = entry?.secrets ?? [];
  render(
    <McpServerDialog
      entry={entry}
      knownIds={knownIds}
      focusMissingSecret={focusMissingSecret}
      onClose={onClose}
      onSaved={onSaved}
    />,
  );
  const dialog = screen.getByRole("dialog");
  return { dialog, onClose, onSaved, user: userEvent.setup() };
}

const box = (dialog: HTMLElement, name: string) => within(dialog).getByRole("textbox", { name });
const saveButton = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Save" });
/** MSS-FR-UHXN: the ID as text, with `Edit ID` on add. */
const idLine = (dialog: HTMLElement) => within(dialog).getByRole("group", { name: "ID" });
const editId = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Edit ID" });
const noIdInput = (dialog: HTMLElement) => {
  expect(within(dialog).queryByRole("textbox", { name: "ID" })).toBeNull();
};
const ID_HINT = "Agents see it in the names of the tools. You cannot change it later.";
const INVALID_ID = "Use 1 to 32 lowercase letters, digits, and hyphens. Start with a letter or a digit.";
const DUPLICATE_ID = "Another server of this project has this ID.";

const TRACKER = mcpRecord({
  id: "tracker",
  name: "Issue tracker",
  enabled: false,
  disabledTools: ["delete_issue"],
  transport: httpTransport({
    auth: "oauth2",
    headers: { "X-Team": "platform" },
    secretHeaders: ["X-Api-Key"],
    scopes: ["issues:read", "issues:write"],
  }),
});

describe("MCP server dialog: fields", () => {
  it("MSS-FR-MWAV, MSS-FR-UHXN: Add opens empty, with Name, the ID line, and the two transports", () => {
    const { dialog } = setup();
    expect(dialog).toHaveAccessibleName("Add MCP server");
    expect(box(dialog, "Name")).toHaveValue("");
    noIdInput(dialog);
    expect(idLine(dialog)).toHaveTextContent("Made from the name");
    expect(editId(dialog)).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: "Local command" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "Remote (HTTP)" })).not.toBeChecked();
    // The Modal puts the focus on the first field.
    expect(box(dialog, "Name")).toHaveFocus();
  });

  it("MSS-FR-MWAV: Edit opens with the record of the row", () => {
    const { dialog } = setup({ entry: mcpEntry(TRACKER, { state: "needs_sign_in" }) });
    expect(dialog).toHaveAccessibleName("Edit MCP server");
    expect(box(dialog, "Name")).toHaveValue("Issue tracker");
    expect(box(dialog, "URL")).toHaveValue("https://mcp.tracker.example/mcp");
    expect(box(dialog, "Scopes")).toHaveValue("issues:read issues:write");
    expect(within(dialog).getByRole("radio", { name: "OAuth 2" })).toBeChecked();
  });

  it("MSS-FR-DVDX: on add, ID follows Name as a slug until the author edits ID", async () => {
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Issue Tracker");
    expect(idLine(dialog)).toHaveTextContent("issue-tracker");
    await user.click(editId(dialog));
    expect(box(dialog, "ID")).toHaveValue("issue-tracker");
    await user.clear(box(dialog, "ID"));
    await user.type(box(dialog, "ID"), "issues");
    await user.type(box(dialog, "Name"), " 2");
    expect(box(dialog, "ID")).toHaveValue("issues");
    // Cleared by the author, it does not follow again.
    await user.clear(box(dialog, "ID"));
    await user.type(box(dialog, "Name"), "x");
    expect(box(dialog, "ID")).toHaveValue("");
  });

  it("MSS-FR-DVDX: on edit, the ID line shows the ID as text, with no Edit ID and no input", async () => {
    const { dialog, user } = setup({ entry: mcpEntry(TRACKER, { state: "ready", toolCount: 0 }) });
    noIdInput(dialog);
    expect(within(dialog).queryByRole("button", { name: "Edit ID" })).toBeNull();
    expect(idLine(dialog)).toHaveTextContent("tracker");
    await user.type(box(dialog, "Name"), " new");
    expect(idLine(dialog)).toHaveTextContent("tracker");
  });

  it("MSS-FR-XMNV: Local command has Command, Arguments, Working folder, and the environment list", async () => {
    const { dialog, user } = setup();
    expect(box(dialog, "Command")).toBeInTheDocument();
    expect(box(dialog, "Arguments").tagName).toBe("TEXTAREA");
    expect(within(dialog).getByText("One argument on each line.")).toBeInTheDocument();
    expect(box(dialog, "Working folder")).toBeInTheDocument();
    expect(within(dialog).getByRole("group", { name: "Environment" })).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "+ Add variable" }));
    expect(within(dialog).getAllByTestId("mcp-env-row")).toHaveLength(1);
    expect(within(dialog).queryByRole("textbox", { name: "URL" })).toBeNull();
  });

  it("MSS-FR-HIWH: Remote has URL, Authentication, and headers; each auth shows its fields", async () => {
    const { dialog, user } = setup();
    await user.click(within(dialog).getByRole("radio", { name: "Remote (HTTP)" }));
    expect(box(dialog, "URL")).toBeInTheDocument();
    expect(within(dialog).getByRole("group", { name: "Headers" })).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: "None" })).toBeChecked();
    expect(within(dialog).queryByLabelText("Token")).toBeNull();

    await user.click(within(dialog).getByRole("radio", { name: "Bearer token" }));
    expect(within(dialog).getByLabelText("Token")).toHaveAttribute("type", "password");
    expect(within(dialog).queryByRole("textbox", { name: "Client ID" })).toBeNull();

    await user.click(within(dialog).getByRole("radio", { name: "OAuth 2" }));
    expect(within(dialog).queryByLabelText("Token")).toBeNull();
    expect(box(dialog, "Scopes")).toBeInTheDocument();
    expect(box(dialog, "Callback port")).toBeInTheDocument();
    // The client secret shows only with a client ID.
    expect(within(dialog).queryByLabelText("Client secret")).toBeNull();
    await user.type(box(dialog, "Client ID"), "app");
    expect(within(dialog).getByLabelText("Client secret")).toHaveAttribute("type", "password");
  });

  it("MSS-FR-BHNZ: a row has a name, a value, and a Secret check box; a secret row saves its name, not its value", async () => {
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(within(dialog).getByRole("button", { name: "+ Add variable" }));
    await user.click(within(dialog).getByRole("button", { name: "+ Add variable" }));
    const [plain, secret] = within(dialog).getAllByTestId("mcp-env-row");
    await user.type(within(plain).getByRole("textbox", { name: "Name of variable 1" }), "MODE");
    await user.type(within(plain).getByRole("textbox", { name: "Value of MODE" }), "fast");
    await user.type(within(secret).getByRole("textbox", { name: "Name of variable 2" }), "API_KEY");
    await user.click(within(secret).getByRole("checkbox", { name: "Secret API_KEY" }));
    const value = within(secret).getByLabelText("Value of API_KEY");
    expect(value).toHaveAttribute("type", "password");
    await user.type(value, "s3cret");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("set_mcp_server_secret")).toHaveLength(1));
    const transport = savedServer().transport;
    expect(transport).toMatchObject({ env: { MODE: "fast" }, secretEnv: ["API_KEY"] });
    expect(JSON.stringify(savedServer())).not.toContain("s3cret");
    expect(calls("set_mcp_server_secret")[0][1]).toEqual({ id: "docs", slot: "env:API_KEY", value: "s3cret" });
  });

  it("MSS-FR-BHNZ: the Secret check box of a row is named for its row", async () => {
    const { dialog, user } = setup();
    await user.click(within(dialog).getByRole("button", { name: "+ Add variable" }));
    const row = within(dialog).getByTestId("mcp-env-row");
    // A row with no name yet is named by its place in the list.
    expect(within(row).getByRole("checkbox", { name: "Secret variable 1" })).not.toBeChecked();
    await user.type(within(row).getByRole("textbox", { name: "Name of variable 1" }), "TOKEN");
    expect(within(row).getByRole("checkbox", { name: "Secret TOKEN" })).toBeInTheDocument();
  });

  it("MSS-FR-BHNZ: a row can be removed", async () => {
    const entry = mcpEntry(TRACKER, { state: "ready", toolCount: 0 });
    const { dialog, user } = setup({ entry });
    await user.click(within(dialog).getByRole("button", { name: "Remove header X-Team" }));
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(savedServer().transport).toMatchObject({ headers: {}, secretHeaders: ["X-Api-Key"] });
    // MSS-FR-MWAV: enabled and disabledTools stay as the row had them.
    expect(savedServer()).toMatchObject({ enabled: false, disabledTools: ["delete_issue"] });
  });
});

describe("MCP server dialog: secrets", () => {
  const withKey = mcpEntry(TRACKER, { state: "ready", toolCount: 0 }, {
    secrets: [{ slot: "header:X-Api-Key", present: true }],
  });

  it("MSS-FR-IUMU: a secret field is empty and says whether this computer has its value", async () => {
    const { dialog, user } = setup({ entry: withKey });
    const row = within(dialog).getAllByTestId("mcp-header-row")[1];
    expect(within(row).getByLabelText("Value of X-Api-Key")).toHaveValue("");
    expect(row).toHaveTextContent("Set on this computer");
    await user.click(within(row).getByRole("checkbox", { name: "Secret X-Api-Key" }));
    await user.click(within(row).getByRole("checkbox", { name: "Secret X-Api-Key" }));
    await user.type(within(row).getByRole("textbox", { name: "Name of header 2" }), "2");
    // A renamed secret is a new slot, which has no value yet.
    expect(row).toHaveTextContent("Not set");
  });

  it("MSS-FR-IUMU: an empty secret field keeps the stored value", async () => {
    const { dialog, user, onClose } = setup({ entry: withKey });
    await user.click(saveButton(dialog));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(calls("set_mcp_server_secret")).toHaveLength(0);
    expect(calls("clear_mcp_server_secret")).toHaveLength(0);
  });

  it("MSS-FR-IUMU: Clear clears the value when the dialog saves, and Keep takes the Clear back", async () => {
    const { dialog, user, onClose } = setup({ entry: withKey });
    const row = within(dialog).getAllByTestId("mcp-header-row")[1];
    await user.click(within(row).getByRole("button", { name: "Clear X-Api-Key" }));
    expect(row).toHaveTextContent("Cleared when you save");
    await user.click(within(row).getByRole("button", { name: "Keep X-Api-Key" }));
    expect(row).toHaveTextContent("Set on this computer");
    await user.click(within(row).getByRole("button", { name: "Clear X-Api-Key" }));
    expect(calls("clear_mcp_server_secret")).toHaveLength(0);
    await user.click(saveButton(dialog));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(calls("clear_mcp_server_secret")[0][1]).toEqual({ id: "tracker", slot: "header:X-Api-Key" });
  });

  it("MSS-FR-IUMU: a field that is not set has no Clear", () => {
    const { dialog } = setup({ entry: mcpEntry(TRACKER, { state: "needs_secret" }) });
    const row = within(dialog).getAllByTestId("mcp-header-row")[1];
    expect(row).toHaveTextContent("Not set");
    expect(within(row).queryByRole("button", { name: /Clear/ })).toBeNull();
  });

  it("MSS-FR-UWZQ: Save writes the record, then each typed secret, then each cleared secret", async () => {
    const record = mcpRecord({
      transport: httpTransport({ auth: "bearer", secretHeaders: ["X-Old", "X-New"] }),
    });
    const entry = mcpEntry(record, { state: "ready", toolCount: 0 }, {
      secrets: [
        { slot: "header:X-Old", present: true },
        { slot: "header:X-New", present: false },
        { slot: "bearer", present: true },
      ],
    });
    const { dialog, user, onClose, onSaved } = setup({ entry });
    await user.click(within(dialog).getByRole("button", { name: "Clear X-Old" }));
    await user.type(within(dialog).getByLabelText("Value of X-New"), "new-value");
    await user.type(within(dialog).getByLabelText("Token"), "token-value");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(sequence()).toEqual([
      "save_mcp_server",
      "set_mcp_server_secret header:X-New",
      "set_mcp_server_secret bearer",
      "clear_mcp_server_secret header:X-Old",
    ]);
    expect(onSaved).toHaveBeenCalledTimes(1);
  });

  it("MSS-FR-PDVK: Set secrets puts the focus on the first secret that has no value", () => {
    const record = mcpRecord({
      transport: httpTransport({ auth: "bearer", secretHeaders: ["X-Set"] }),
    });
    const entry = mcpEntry(record, { state: "needs_secret" }, {
      secrets: [
        { slot: "header:X-Set", present: true },
        { slot: "bearer", present: false },
      ],
    });
    const { dialog } = setup({ entry, focusMissingSecret: true });
    expect(within(dialog).getByLabelText("Token")).toHaveFocus();
  });
});

describe("MCP server dialog: checks and refusals", () => {
  it("MSS-FR-BEQD: a problem shows under its field once the author works on it, and Save is disabled", async () => {
    const { dialog, user } = setup();
    expect(saveButton(dialog)).toBeDisabled();
    expect(within(dialog).queryByTestId("mcp-field-problem")).toBeNull();
    await user.type(box(dialog, "Name"), "Docs");
    await user.click(box(dialog, "Command"));
    await user.tab();
    expect(within(dialog).getByText("Enter a command.")).toBeInTheDocument();
    expect(box(dialog, "Command")).toHaveAttribute("aria-invalid", "true");
    expect(saveButton(dialog)).toBeDisabled();
    await user.type(box(dialog, "Command"), "npx");
    expect(within(dialog).queryByText("Enter a command.")).toBeNull();
    expect(saveButton(dialog)).toBeEnabled();
  });

  it("MSS-FR-WQRL, MSS-FR-BEQD: an ID from the name that is not valid or not unique shows its problem under the closed ID line", async () => {
    const { dialog, user } = setup({ knownIds: ["docs"] });
    await user.type(box(dialog, "Name"), "Docs");
    expect(within(idLine(dialog)).getByText(DUPLICATE_ID)).toBeInTheDocument();
    expect(idLine(dialog)).toHaveAccessibleDescription(DUPLICATE_ID);
    await user.clear(box(dialog, "Name"));
    await user.type(box(dialog, "Name"), "日本");
    expect(idLine(dialog)).toHaveTextContent("Made from the name");
    expect(within(idLine(dialog)).getByText(INVALID_ID)).toBeInTheDocument();
    expect(saveButton(dialog)).toBeDisabled();
    // The same problem shows under the open input.
    await user.click(editId(dialog));
    expect(box(dialog, "ID")).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getByText(INVALID_ID)).toBeInTheDocument();
    expect(saveButton(dialog)).toBeDisabled();
  });

  it("MSS-FR-BEQD: the header list shows the Authorization conflict under its row", async () => {
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Tracker");
    await user.click(within(dialog).getByRole("radio", { name: "Remote (HTTP)" }));
    await user.type(box(dialog, "URL"), "https://mcp.example.com");
    await user.click(within(dialog).getByRole("radio", { name: "Bearer token" }));
    await user.click(within(dialog).getByRole("button", { name: "+ Add header" }));
    const row = within(dialog).getByTestId("mcp-header-row");
    await user.type(within(row).getByRole("textbox", { name: "Name of header 1" }), "authorization");
    expect(row).toHaveTextContent(
      "Remove this header. The authentication sets the Authorization header.",
    );
    expect(saveButton(dialog)).toBeDisabled();
  });

  it("MSS-FR-GHDO: a plain http URL to another computer says to use https", async () => {
    const { dialog, user } = setup();
    await user.click(within(dialog).getByRole("radio", { name: "Remote (HTTP)" }));
    await user.type(box(dialog, "URL"), "http://mcp.example.com/mcp");
    expect(within(dialog).getByText("Use https for a server on another computer.")).toBeInTheDocument();
    await user.clear(box(dialog, "URL"));
    await user.type(box(dialog, "URL"), "http://localhost:8080/mcp");
    expect(within(dialog).queryByText("Use https for a server on another computer.")).toBeNull();
  });

  it("MSS-FR-UWZQ / MSS-FR-STXZ: a refused save shows under its field and keeps every field as typed", async () => {
    refuse.save_mcp_server = "invalid_cwd";
    const { dialog, user, onClose, onSaved } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.type(box(dialog, "Working folder"), "tools");
    await user.click(within(dialog).getByRole("button", { name: "+ Add variable" }));
    const row = within(dialog).getByTestId("mcp-env-row");
    await user.type(within(row).getByRole("textbox", { name: "Name of variable 1" }), "KEY");
    await user.click(within(row).getByRole("checkbox", { name: "Secret KEY" }));
    await user.type(within(row).getByLabelText("Value of KEY"), "typed-secret");
    await user.click(saveButton(dialog));
    expect(await within(dialog).findByText("Use a relative path to a folder in the project.")).toBeInTheDocument();
    expect(box(dialog, "Name")).toHaveValue("Docs");
    expect(box(dialog, "Working folder")).toHaveValue("tools");
    expect(within(row).getByLabelText("Value of KEY")).toHaveValue("typed-secret");
    expect(onClose).not.toHaveBeenCalled();
    expect(onSaved).not.toHaveBeenCalled();
    expect(calls("set_mcp_server_secret")).toHaveLength(0);
    // The refusal goes when the author changes its field.
    await user.type(box(dialog, "Working folder"), "/sub");
    expect(within(dialog).queryByText("Use a relative path to a folder in the project.")).toBeNull();
  });

  it("MSS-FR-UWZQ: a refusal that names no field shows at the top of the dialog", async () => {
    refuse.save_mcp_server = "no_project_open";
    const { dialog, user } = setup({ entry: mcpEntry(TRACKER, { state: "ready", toolCount: 0 }) });
    await user.click(saveButton(dialog));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("No project is open.");
  });

  it("MSS-FR-STXZ: a secret that is not stored after a save keeps the dialog open and is named", async () => {
    refuse["header:X-B"] = "keychain_unavailable";
    const record = mcpRecord({
      id: "tracker",
      name: "Tracker",
      transport: httpTransport({ secretHeaders: ["X-A", "X-B"] }),
    });
    const { dialog, user, onClose, onSaved } = setup({
      entry: mcpEntry(record, { state: "needs_secret" }),
    });
    await user.type(within(dialog).getByLabelText("Value of X-A"), "a-value");
    await user.type(within(dialog).getByLabelText("Value of X-B"), "b-value");
    await user.click(saveButton(dialog));
    const [rowA, rowB] = within(dialog).getAllByTestId("mcp-header-row");
    expect(await within(rowB).findByText(/This secret was not stored: The system keychain is not available\./)).toBeInTheDocument();
    expect(within(rowA).queryByText(/not stored/)).toBeNull();
    expect(within(dialog).getByRole("alert")).toHaveTextContent("The server is saved.");
    expect(onClose).not.toHaveBeenCalled();
    expect(onSaved).toHaveBeenCalledTimes(1);
    // The value that was stored is not sent again; the other one is.
    expect(within(rowA).getByLabelText("Value of X-A")).toHaveValue("");
    expect(within(rowB).getByLabelText("Value of X-B")).toHaveValue("b-value");
    delete refuse["header:X-B"];
    await user.click(saveButton(dialog));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(sequence()).toEqual([
      "save_mcp_server",
      "set_mcp_server_secret header:X-A",
      "set_mcp_server_secret header:X-B",
      "save_mcp_server",
      "set_mcp_server_secret header:X-B",
    ]);
  });

  it("MSS-FR-STXZ, MSS-FR-DVDX: after a first save of a new server, the ID is fixed text", async () => {
    refuse.bearer = "invalid_secret";
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Tracker");
    // The open input also turns back into text.
    await user.click(editId(dialog));
    await user.click(within(dialog).getByRole("radio", { name: "Remote (HTTP)" }));
    await user.type(box(dialog, "URL"), "https://mcp.example.com");
    await user.click(within(dialog).getByRole("radio", { name: "Bearer token" }));
    await user.type(within(dialog).getByLabelText("Token"), "tok");
    await user.click(saveButton(dialog));
    expect(await within(dialog).findByText(/This secret was not stored/)).toBeInTheDocument();
    noIdInput(dialog);
    expect(within(dialog).queryByRole("button", { name: "Edit ID" })).toBeNull();
    expect(idLine(dialog)).toHaveTextContent("tracker");
  });

  it("MSS-FR-GMHC: Cancel and Escape close the dialog and save nothing", async () => {
    const first = setup({ entry: mcpEntry(TRACKER, { state: "ready", toolCount: 0 }) });
    await first.user.type(box(first.dialog, "Name"), " edited");
    await first.user.click(within(first.dialog).getByRole("button", { name: "Cancel" }));
    expect(first.onClose).toHaveBeenCalledTimes(1);
    cleanup();
    const second = setup();
    await second.user.type(box(second.dialog, "Name"), "Docs");
    await second.user.keyboard("{Escape}");
    expect(second.onClose).toHaveBeenCalledTimes(1);
    expect(calls("save_mcp_server")).toHaveLength(0);
  });

  it("MSS-FR-UWZQ: a dialog closed during a save still hands the saved entry to the section", async () => {
    let release = () => {};
    holdSave = new Promise<void>((resolve) => (release = resolve));
    const { dialog, user, onSaved } = setup({ entry: mcpEntry(TRACKER, { state: "ready", toolCount: 0 }) });
    await user.click(saveButton(dialog));
    expect(within(dialog).getByRole("button", { name: "Saving…" })).toBeDisabled();
    cleanup();
    release();
    await waitFor(() => expect(onSaved).toHaveBeenCalledTimes(1));
  });

  it("MSS-FR-MWAV: an invalid row opens Edit with its ID and empty fields", () => {
    const { dialog } = setup({
      entry: mcpEntry(null, { state: "invalid" }, { id: "broken", invalid: "invalid_url" }),
    });
    noIdInput(dialog);
    expect(within(dialog).queryByRole("button", { name: "Edit ID" })).toBeNull();
    expect(idLine(dialog)).toHaveTextContent("broken");
    expect(box(dialog, "Name")).toHaveValue("");
  });

  it("MSS-FR-XMNV: the stdio record of an edit fills the command fields", () => {
    const record = mcpRecord({
      transport: stdioTransport({ args: ["-y", "pkg"], cwd: "tools", env: { MODE: "fast" } }),
    });
    const { dialog } = setup({ entry: mcpEntry(record, { state: "ready", toolCount: 0 }) });
    expect(box(dialog, "Command")).toHaveValue("npx");
    expect(box(dialog, "Arguments")).toHaveValue("-y\npkg");
    expect(box(dialog, "Working folder")).toHaveValue("tools");
    expect(within(dialog).getByRole("textbox", { name: "Value of MODE" })).toHaveValue("fast");
  });

  it("MSS-FR-SCDK, MSS-FR-ENMX: on add, the team is chosen first and the team default is on", async () => {
    const { dialog, user } = setup();
    const group = within(dialog).getByRole("group", { name: "Who uses it" });
    const choices = within(group).getAllByRole("radio").map((r) => r.closest("label")?.textContent);
    expect(choices).toEqual([
      "The team (saved in the project)",
      "Only me, in this project",
      "Only me, in all my projects",
    ]);
    expect(within(group).getByRole("radio", { name: "The team (saved in the project)" })).toBeChecked();
    const teamDefault = within(dialog).getByRole("checkbox", { name: "On for the team by default" });
    expect(teamDefault).toBeChecked();
    await user.click(teamDefault);
    await user.type(box(dialog, "Name"), "Docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(calls("save_mcp_server")[0][1]).toMatchObject({ scope: "team" });
    expect(savedServer().enabled).toBe(false);
  });

  it("MSS-FR-SCDK, MSS-FR-ENMX: a personal server has no team default and is saved on, in its scope", async () => {
    const { dialog, user } = setup();
    await user.click(within(dialog).getByRole("radio", { name: "Only me, in all my projects" }));
    expect(within(dialog).queryByRole("checkbox", { name: "On for the team by default" })).toBeNull();
    await user.type(box(dialog, "Name"), "Notes");
    await user.type(box(dialog, "Command"), "notes-mcp");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(calls("save_mcp_server")[0][1]).toMatchObject({ scope: "personal_all" });
    expect(savedServer().enabled).toBe(true);
  });

  it("MSS-FR-SCDK, MSS-FR-ENMX: on edit, the scope shows as text and a personal record keeps its enabled", async () => {
    const mine = mcpRecord({ enabled: false });
    const entry = mcpEntry(mine, { state: "disabled" }, { scope: "personal_project" });
    const { dialog, user } = setup({ entry });
    expect(within(dialog).queryByRole("group", { name: "Who uses it" })).toBeNull();
    expect(within(dialog).getByTestId("mcp-dialog-scope")).toHaveTextContent("Only me, in this project");
    expect(within(dialog).queryByRole("checkbox", { name: "On for the team by default" })).toBeNull();
    await user.type(box(dialog, "Name"), " 2");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(calls("save_mcp_server")[0][1]).toMatchObject({ scope: "personal_project" });
    expect(savedServer().enabled).toBe(false);
  });

  it("MSS-FR-SCDK, MSS-FR-ENMX: a server of this project is saved on, also after the team default was off", async () => {
    const { dialog, user } = setup();
    await user.click(within(dialog).getByRole("checkbox", { name: "On for the team by default" }));
    await user.click(within(dialog).getByRole("radio", { name: "Only me, in this project" }));
    await user.type(box(dialog, "Name"), "Mine");
    await user.type(box(dialog, "Command"), "mine-mcp");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(calls("save_mcp_server")[0][1]).toMatchObject({ scope: "personal_project" });
    expect(savedServer().enabled).toBe(true);
  });

  it("MSS-FR-ENMX: on edit of a team record, the check box is its enabled", async () => {
    const { dialog, user } = setup({ entry: mcpEntry(TRACKER, { state: "disabled" }) });
    const teamDefault = within(dialog).getByRole("checkbox", { name: "On for the team by default" });
    expect(teamDefault).not.toBeChecked();
    await user.click(teamDefault);
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(savedServer().enabled).toBe(true);
  });
});

describe("MCP server dialog: the ID line", () => {
  it("MSS-FR-UHXN, MSS-FR-WQRL: Edit ID opens a focused input with the ID, and the hint shows only when it is open", async () => {
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    expect(within(dialog).queryByText(ID_HINT)).toBeNull();
    await user.click(editId(dialog));
    expect(box(dialog, "ID")).toHaveValue("docs");
    expect(box(dialog, "ID")).toHaveFocus();
    expect(within(dialog).queryByRole("button", { name: "Edit ID" })).toBeNull();
    expect(within(dialog).getByText(ID_HINT)).toBeInTheDocument();
  });

  it("MSS-FR-DVDX: the open input follows Name until the author types in it", async () => {
    const { dialog, user } = setup();
    await user.click(editId(dialog));
    await user.type(box(dialog, "Name"), "Wiki Pages");
    expect(box(dialog, "ID")).toHaveValue("wiki-pages");
    await user.type(box(dialog, "ID"), "x");
    await user.type(box(dialog, "Name"), " 2");
    expect(box(dialog, "ID")).toHaveValue("wiki-pagesx");
  });

  it("MSS-FR-UHXN: the input stays open while the author works on other fields", async () => {
    const { dialog, user } = setup();
    await user.click(editId(dialog));
    await user.type(box(dialog, "Name"), "Docs");
    await user.clear(box(dialog, "ID"));
    await user.click(within(dialog).getByRole("radio", { name: "Remote (HTTP)" }));
    expect(box(dialog, "ID")).toHaveValue("");
  });

  it("MSS-FR-UHXN: a save after Edit ID sends the ID that the author typed", async () => {
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.click(editId(dialog));
    await user.clear(box(dialog, "ID"));
    await user.type(box(dialog, "ID"), "my-docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(saveButton(dialog));
    await waitFor(() => expect(calls("save_mcp_server")).toHaveLength(1));
    expect(savedServer().id).toBe("my-docs");
  });

  it("MSS-FR-WQRL, MSS-FR-BEQD: a duplicate typed in the open input shows its problem until it is unique", async () => {
    const { dialog, user } = setup({ knownIds: ["docs"] });
    await user.type(box(dialog, "Name"), "Wiki");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(editId(dialog));
    await user.clear(box(dialog, "ID"));
    await user.type(box(dialog, "ID"), "docs");
    expect(within(dialog).getByText(DUPLICATE_ID)).toBeInTheDocument();
    expect(box(dialog, "ID")).toHaveAttribute("aria-invalid", "true");
    expect(saveButton(dialog)).toBeDisabled();
    await user.type(box(dialog, "ID"), "-2");
    expect(within(dialog).queryByText(DUPLICATE_ID)).toBeNull();
    expect(saveButton(dialog)).toBeEnabled();
  });

  it("MSS-FR-HHEJ: a duplicate_id refusal opens the ID input and moves the focus to it", async () => {
    refuse.save_mcp_server = "duplicate_id";
    const { dialog, user, onSaved } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(saveButton(dialog));
    const input = await within(dialog).findByRole("textbox", { name: "ID" });
    expect(input).toHaveValue("docs");
    await waitFor(() => expect(input).toHaveFocus());
    expect(within(dialog).getByText(DUPLICATE_ID)).toBeInTheDocument();
    expect(box(dialog, "Name")).toHaveValue("Docs");
    expect(onSaved).not.toHaveBeenCalled();
    await user.type(input, "-2");
    expect(within(dialog).queryByText(DUPLICATE_ID)).toBeNull();
  });

  it("MSS-FR-HHEJ: an invalid_id refusal moves the focus back to an input that is already open", async () => {
    refuse.save_mcp_server = "invalid_id";
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.click(editId(dialog));
    await user.type(box(dialog, "Command"), "npx");
    expect(box(dialog, "Command")).toHaveFocus();
    await user.click(saveButton(dialog));
    expect(await within(dialog).findByText(INVALID_ID)).toBeInTheDocument();
    await waitFor(() => expect(box(dialog, "ID")).toHaveFocus());
  });

  it("MSS-FR-HHEJ, MSS-FR-DVDX: on edit, a duplicate_id refusal shows under the ID text and opens no input", async () => {
    refuse.save_mcp_server = "duplicate_id";
    const { dialog, user } = setup({ entry: mcpEntry(TRACKER, { state: "ready", toolCount: 0 }) });
    await user.click(saveButton(dialog));
    expect(await within(idLine(dialog)).findByText(DUPLICATE_ID)).toBeInTheDocument();
    noIdInput(dialog);
    expect(within(dialog).queryByRole("button", { name: "Edit ID" })).toBeNull();
  });

  it("MSS-FR-HHEJ: an invalid_id refusal opens the closed ID input and moves the focus to it", async () => {
    refuse.save_mcp_server = "invalid_id";
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(saveButton(dialog));
    const input = await within(dialog).findByRole("textbox", { name: "ID" });
    expect(within(dialog).getByText(INVALID_ID)).toBeInTheDocument();
    await waitFor(() => expect(input).toHaveFocus());
  });

  it("MSS-FR-HHEJ: a refusal of another field does not open the ID input", async () => {
    refuse.save_mcp_server = "invalid_command";
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Docs");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(saveButton(dialog));
    expect(await within(dialog).findByText("Enter a command.")).toBeInTheDocument();
    noIdInput(dialog);
    expect(idLine(dialog)).toHaveTextContent("docs");
  });

  it("MSS-FR-UHXN: Edit ID with an empty Name opens an empty input that has the focus", async () => {
    const { dialog, user } = setup();
    await user.click(editId(dialog));
    expect(box(dialog, "ID")).toHaveValue("");
    expect(box(dialog, "ID")).toHaveFocus();
  });

  it("MSS-FR-UHXN, MSS-FR-HHEJ: each way of opening the ID input logs it, with no ID or name", async () => {
    refuse.save_mcp_server = "duplicate_id";
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Secret Name");
    await user.type(box(dialog, "Command"), "npx");
    await user.click(saveButton(dialog));
    await within(dialog).findByRole("textbox", { name: "ID" });
    cleanup();
    const second = setup();
    await second.user.click(editId(second.dialog));
    const opened = logRecords().filter((r) => r.message === "mcp server id input opened");
    expect(opened.map((r) => r.fields)).toEqual([{ reason: "refusal" }, { reason: "edit_id" }]);
    expect(opened.every((r) => r.level === "INFO")).toBe(true);
    expect(JSON.stringify(opened)).not.toContain("secret");
  });
});
