import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import { httpTransport, mcpEntry, mcpRecord, stdioTransport } from "../test/mcpFixtures";
import type { McpServer, McpServerEntry } from "../types";

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
  it("MSS-FR-MWAV: Add opens empty, with Name, ID, and the two transports", () => {
    const { dialog } = setup();
    expect(dialog).toHaveAccessibleName("Add MCP server");
    expect(box(dialog, "Name")).toHaveValue("");
    expect(box(dialog, "ID")).toHaveValue("");
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

  it("MSS-FR-DVDX: on edit, ID is read-only", async () => {
    const { dialog, user } = setup({ entry: mcpEntry(TRACKER, { state: "ready", toolCount: 0 }) });
    expect(box(dialog, "ID")).toHaveAttribute("readonly");
    await user.type(box(dialog, "Name"), " new");
    expect(box(dialog, "ID")).toHaveValue("tracker");
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

  it("MSS-FR-BEQD: an ID from the name that is not valid or not unique shows its problem with the name", async () => {
    const { dialog, user } = setup({ knownIds: ["docs"] });
    await user.type(box(dialog, "Name"), "Docs");
    expect(within(dialog).getByText("Another server of this project has this ID.")).toBeInTheDocument();
    await user.clear(box(dialog, "Name"));
    await user.type(box(dialog, "Name"), "日本");
    expect(
      within(dialog).getByText(
        "Use 1 to 32 lowercase letters, digits, and hyphens. Start with a letter or a digit.",
      ),
    ).toBeInTheDocument();
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

  it("MSS-FR-STXZ: after a first save of a new server, the ID is fixed", async () => {
    refuse.bearer = "invalid_secret";
    const { dialog, user } = setup();
    await user.type(box(dialog, "Name"), "Tracker");
    await user.click(within(dialog).getByRole("radio", { name: "Remote (HTTP)" }));
    await user.type(box(dialog, "URL"), "https://mcp.example.com");
    await user.click(within(dialog).getByRole("radio", { name: "Bearer token" }));
    await user.type(within(dialog).getByLabelText("Token"), "tok");
    await user.click(saveButton(dialog));
    expect(await within(dialog).findByText(/This secret was not stored/)).toBeInTheDocument();
    expect(box(dialog, "ID")).toHaveAttribute("readonly");
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
    expect(box(dialog, "ID")).toHaveValue("broken");
    expect(box(dialog, "ID")).toHaveAttribute("readonly");
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
});
