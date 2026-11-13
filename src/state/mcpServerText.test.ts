import { describe, expect, it } from "vitest";

import type { McpServerEntry, McpServerStatus, McpTestResult } from "../types";
import {
  CONFIG_ERROR_TEXT,
  FAILURE_TEXT,
  firstLine,
  refusalText,
  rowProblemText,
  statusChipText,
  statusChipTone,
  testResultText,
} from "./mcpServerText";

function entry(status: McpServerStatus, over: Partial<McpServerEntry> = {}): McpServerEntry {
  return { id: "docs", record: null, invalid: null, status, secrets: [], tools: [], ...over };
}

describe("the words of the MCP servers section", () => {
  it("MSS-FR-PEGO: each status has its chip text", () => {
    const cases: [McpServerStatus, string][] = [
      [{ state: "disabled" }, "Off"],
      [{ state: "invalid" }, "Invalid"],
      [{ state: "untrusted" }, "Not trusted"],
      [{ state: "needs_secret" }, "Needs a secret"],
      [{ state: "needs_sign_in" }, "Sign in needed"],
      [{ state: "signing_in" }, "Signing in…"],
      [{ state: "connecting" }, "Connecting…"],
      [{ state: "ready", toolCount: 3 }, "Connected · 3 tools"],
      [{ state: "ready", toolCount: 0 }, "Connected · 0 tools"],
      [{ state: "failed", failure: "unreachable" }, "Failed"],
    ];
    for (const [status, text] of cases) expect(statusChipText(status)).toBe(text);
  });

  it("MSS-FR-PEGO: the chip is green when ready, red when failed, grey when off, amber when it needs the author", () => {
    expect(statusChipTone("ready")).toBe("ok");
    expect(statusChipTone("failed")).toBe("danger");
    expect(statusChipTone("disabled")).toBe("neutral");
    for (const state of ["untrusted", "needs_secret", "needs_sign_in", "signing_in", "invalid"] as const) {
      expect(statusChipTone(state)).toBe("warn");
    }
  });

  it("MSS-FR-VUWB: the failure class texts are exactly the spec's", () => {
    expect(FAILURE_TEXT).toEqual({
      command_not_found: "The command was not found on this computer.",
      process_exited: "The server process stopped.",
      unreachable: "The server cannot be reached.",
      tls_untrusted: "The server certificate is not trusted.",
      rejected: "The server refused the credentials.",
      protocol_error: "The server did not answer as an MCP server.",
      timed_out: "The server did not answer in time.",
      oauth_failed: "The sign-in did not complete.",
      keychain_unavailable: "The system keychain is not available.",
    });
  });

  it("MSS-FR-YIMR: a failed row shows its failure text, an invalid row its validation text", () => {
    expect(rowProblemText(entry({ state: "failed", failure: "timed_out" }))).toBe(
      "The server did not answer in time.",
    );
    expect(rowProblemText(entry({ state: "invalid" }, { invalid: "invalid_url" }))).toBe(
      CONFIG_ERROR_TEXT.invalid_url,
    );
    expect(rowProblemText(entry({ state: "invalid" }))).toBe("The server record is not valid.");
    expect(rowProblemText(entry({ state: "ready", toolCount: 1 }))).toBeNull();
    expect(rowProblemText(entry({ state: "untrusted" }))).toBeNull();
  });

  it("MSS-FR-VWKQ: a test success counts the tools; a failure names its class or the status that refused it", () => {
    expect(
      testResultText({ ok: true, tools: [{ name: "a", description: "", disabled: false }] }),
    ).toBe("Connected. 1 tools.");
    expect(testResultText({ ok: false, failure: "process_exited", stderrTail: [] })).toBe(
      "The server process stopped.",
    );
    expect(testResultText({ ok: false, failure: "untrusted", stderrTail: [] })).toBe(
      "Trust this server before it runs.",
    );
    expect(testResultText({ ok: false, failure: "needs_secret", stderrTail: [] })).toBe(
      "Add the secrets this server needs on this computer.",
    );
  });

  it("MSS-FR-UWZQ: a refusal code reads as text, and an unknown text passes through", () => {
    expect(refusalText("invalid_url")).toBe(CONFIG_ERROR_TEXT.invalid_url);
    expect(refusalText("keychain_unavailable")).toBe(FAILURE_TEXT.keychain_unavailable);
    expect(refusalText("no_project_open")).toBe("No project is open.");
    expect(refusalText("invalid_secret")).toBe("The value cannot hold a line break.");
    expect(refusalText("something else")).toBe("something else");
  });

  it("MSS-FR-UVWX: the trust refusals and the sign-in refusal of an untrusted server read their texts", () => {
    expect(refusalText("record_changed")).toBe("The record changed. Close this and look at it again.");
    expect(refusalText("no_credentials")).toBe("This server needs no trust step.");
    expect(refusalText("untrusted")).toBe("Trust this server before you sign in.");
  });

  it("MSS-FR-QMTG: a tool shows the first line of its description", () => {
    expect(firstLine("Create an issue.\nSecond line.")).toBe("Create an issue.");
    expect(firstLine("  One line  ")).toBe("One line");
    expect(firstLine("")).toBe("");
  });
  it("MSS-FR-OKDG: each validation code reads its fixed text", () => {
    expect(CONFIG_ERROR_TEXT).toEqual({
      invalid_id: "Use 1 to 32 lowercase letters, digits, and hyphens. Start with a letter or a digit.",
      duplicate_id: "Another server of this project has this ID.",
      invalid_name: "Enter a name of 1 to 80 characters.",
      invalid_transport: "Choose a transport.",
      invalid_command: "Enter a command.",
      invalid_cwd: "Use a relative path to a folder in the project.",
      invalid_url: "Enter an https URL.",
      invalid_env_name: "Use letters, digits, and underscores. Do not start with a digit.",
      invalid_header_name: "Use a valid HTTP header name.",
      invalid_header_value: "A header value cannot hold a line break.",
      duplicate_name: "This name is in the list more than one time.",
      authorization_header_conflict:
        "Remove this header. The authentication sets the Authorization header.",
      invalid_callback_port: "Enter a port from 1024 to 65535.",
      invalid_auth: "Choose an authentication.",
    });
  });

  it("MSS-FR-TMUX: a test refused for a status reads the text of that status", () => {
    const refused = (failure: string) =>
      testResultText({ ok: false, failure, stderrTail: [] } as McpTestResult);
    expect(refused("disabled")).toContain("The server is off.");
    expect(refused("invalid")).toContain("The server record is not valid.");
    expect(refused("untrusted")).toBe("Trust this server before it runs.");
    expect(refused("needs_secret")).toContain("Add the secrets this server needs on this computer.");
    expect(refused("needs_sign_in")).toContain("Sign in to this server first.");
    expect(refused("signing_in")).toContain("The sign-in is not complete.");
    expect(refused("connecting")).toContain("The server is connecting.");
  });
});
