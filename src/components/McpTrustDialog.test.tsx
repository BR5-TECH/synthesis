import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import { httpTransport, mcpEntry, mcpRecord, stdioTransport } from "../test/mcpFixtures";
import type { McpServer } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { commandLine, McpTrustDialog } from "./McpTrustDialog";

let refuseTrust: string | null;

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  refuseTrust = null;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "trust_mcp_server") {
      if (refuseTrust) throw refuseTrust;
      return mcpEntry(args?.shown as McpServer, { state: "connecting" });
    }
    return otherCommand(cmd);
  });
});
afterEach(cleanup);

function setup(record: McpServer) {
  const onClose = vi.fn();
  const onTrusted = vi.fn();
  const view = render(<McpTrustDialog record={record} onClose={onClose} onTrusted={onTrusted} />);
  return {
    dialog: screen.getByRole("dialog", { name: "Trust this server?" }),
    onClose,
    onTrusted,
    view,
  };
}

const trustCalls = () => invokeMock.mock.calls.filter((c) => c[0] === "trust_mcp_server");

const LOCAL = mcpRecord({
  transport: stdioTransport({
    command: "npx",
    args: ["-y", "docs-mcp-server"],
    cwd: "tools/docs",
    env: { DOCS_REGION: "eu", MODE: "fast" },
    secretEnv: ["DOCS_API_KEY"],
  }),
});

const REMOTE = mcpRecord({
  id: "tracker",
  name: "Issue tracker",
  transport: httpTransport({
    url: "https://mcp.tracker.example/mcp",
    auth: "oauth2",
    headers: { "X-Team": "platform" },
    secretHeaders: ["X-Api-Key"],
  }),
});

describe("MCP trust dialog: a Local command", () => {
  it("MSS-FR-VJPZ: shows the command as one line, the working folder, each plain variable with its value, and the secret names", () => {
    const { dialog } = setup(LOCAL);
    expect(dialog).toHaveTextContent("This project runs this command on your computer:");
    expect(within(dialog).getByTestId("mcp-trust-command")).toHaveTextContent("npx -y docs-mcp-server");
    expect(within(dialog).getByTestId("mcp-trust-cwd")).toHaveTextContent("Working folder: tools/docs");
    expect(within(dialog).getByTestId("mcp-trust-env")).toHaveTextContent(
      "Environment: DOCS_REGION=eu, MODE=fast",
    );
    expect(within(dialog).getByTestId("mcp-trust-secret-env")).toHaveTextContent(
      "Secret environment: DOCS_API_KEY",
    );
    // A secret variable is a name alone: it is not shown as a NAME=value pair.
    expect(dialog).not.toHaveTextContent("DOCS_API_KEY=");
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Trust and start" })).toBeInTheDocument();
  });

  it("MSS-FR-VJPZ: no working folder reads as the project root, and no variable as none", () => {
    const { dialog } = setup(mcpRecord({ transport: stdioTransport() }));
    expect(within(dialog).getByTestId("mcp-trust-cwd")).toHaveTextContent("Working folder: (project root)");
    expect(within(dialog).getByTestId("mcp-trust-env")).toHaveTextContent("Environment: (none)");
    expect(within(dialog).getByTestId("mcp-trust-secret-env")).toHaveTextContent(
      "Secret environment: (none)",
    );
  });

  it("MSS-FR-VJPZ: an argument with a space, a quote, or no text stays visible as one argument", () => {
    expect(commandLine(stdioTransport({ command: "uvx", args: ["--from", "a b", "", 'say"hi'] }))).toBe(
      'uvx --from "a b" "" "say\\"hi"',
    );
  });
});

describe("MCP trust dialog: a Remote server", () => {
  it("MSS-FR-SCFN: shows the URL, the authentication, and the names of the secret headers", () => {
    const { dialog } = setup(REMOTE);
    expect(dialog).toHaveTextContent("This project sends your credentials for this server to:");
    expect(within(dialog).getByTestId("mcp-trust-url")).toHaveTextContent("https://mcp.tracker.example/mcp");
    expect(within(dialog).getByTestId("mcp-trust-auth")).toHaveTextContent("Authentication: OAuth 2");
    expect(within(dialog).getByTestId("mcp-trust-secret-headers")).toHaveTextContent(
      "Secret headers: X-Api-Key",
    );
    expect(dialog).not.toHaveTextContent("This project runs this command");
    expect(within(dialog).getByRole("button", { name: "Trust and start" })).toBeInTheDocument();
  });

  it("MSS-FR-SCFN: the authentication reads in the words of the dialog, and no secret header reads none", () => {
    const bearer = setup(mcpRecord({ transport: httpTransport({ auth: "bearer" }) }));
    expect(within(bearer.dialog).getByTestId("mcp-trust-auth")).toHaveTextContent(
      "Authentication: Bearer token",
    );
    expect(within(bearer.dialog).getByTestId("mcp-trust-secret-headers")).toHaveTextContent(
      "Secret headers: (none)",
    );
  });
});

describe("MCP trust dialog: stored credentials", () => {
  it("MSS-FR-VNIK: a Remote server's dialog says that its stored credentials are removed", () => {
    const { dialog } = setup(REMOTE);
    expect(within(dialog).getByTestId("mcp-trust-credentials-removed")).toHaveTextContent(
      "Stored credentials for this server are removed. Enter them again after you trust it.",
    );
  });

  it("MSS-FR-SCFN: an oauth2 client id, part of what is trusted, shows with the authentication", () => {
    const { dialog } = setup(
      mcpRecord({ transport: httpTransport({ auth: "oauth2", clientId: "pre-1" }) }),
    );
    expect(within(dialog).getByTestId("mcp-trust-auth")).toHaveTextContent(
      "Authentication: OAuth 2 (client pre-1)",
    );
  });
});

describe("MCP trust dialog: Trust and start", () => {
  it("MSS-FR-GJVD: sends the record the dialog shows, then closes", async () => {
    const { dialog, onClose, onTrusted } = setup(REMOTE);
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(trustCalls()[0][1]).toEqual({ id: "tracker", shown: REMOTE });
    expect(onTrusted).toHaveBeenCalledWith(expect.objectContaining({ id: "tracker" }));
  });

  it("MSS-FR-GJVD: a newer record of the row does not change what the dialog shows and sends", async () => {
    const { dialog, view, onClose } = setup(LOCAL);
    const changed = mcpRecord({ transport: stdioTransport({ command: "rm" }) });
    view.rerender(<McpTrustDialog record={changed} onClose={onClose} onTrusted={vi.fn()} />);
    expect(within(dialog).getByTestId("mcp-trust-command")).toHaveTextContent("npx -y docs-mcp-server");
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    await waitFor(() => expect(trustCalls()).toHaveLength(1));
    expect(trustCalls()[0][1]).toEqual({ id: "docs", shown: LOCAL });
  });

  it("MSS-FR-UVWX: a record_changed refusal says so and the dialog stays open", async () => {
    refuseTrust = "record_changed";
    const { dialog, onClose, onTrusted } = setup(LOCAL);
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "The record changed. Close this and look at it again.",
    );
    expect(within(dialog).getByRole("button", { name: "Trust and start" })).toBeEnabled();
    expect(onTrusted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("MSS-FR-GJVD: a no_credentials refusal reads its text in the dialog", async () => {
    refuseTrust = "no_credentials";
    const { dialog } = setup(mcpRecord({ transport: httpTransport({ auth: "none" }) }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Trust and start" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("This server needs no trust step.");
  });
});
