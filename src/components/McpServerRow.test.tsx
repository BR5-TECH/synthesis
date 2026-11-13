import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { httpTransport, mcpEntry, mcpRecord } from "../test/mcpFixtures";
import type { McpServerEntry } from "../types";
import { McpServerRow, offersSignIn, offersSignOut, type McpTestView } from "./McpServerRow";

afterEach(cleanup);

function renderRow(entry: McpServerEntry, extra: { test?: McpTestView; busy?: boolean } = {}) {
  const actions = {
    onToggleEnabled: vi.fn(),
    onToggleTool: vi.fn(),
    onTest: vi.fn(),
    onEdit: vi.fn(),
    onSetSecrets: vi.fn(),
    onRemove: vi.fn(),
    onTrust: vi.fn(),
    onSignIn: vi.fn(),
    onCancelSignIn: vi.fn(),
    onSignOut: vi.fn(),
    onReconnect: vi.fn(),
  };
  render(
    <ul>
      <McpServerRow entry={entry} {...actions} {...extra} />
    </ul>,
  );
  return { row: screen.getByRole("listitem"), actions };
}

const buttons = (row: HTMLElement) => within(row).getAllByRole("button").map((b) => b.textContent);
const OAUTH = mcpRecord({ id: "tracker", name: "Tracker", transport: httpTransport({ auth: "oauth2" }) });
const BEARER = mcpRecord({ id: "metrics", name: "Metrics", transport: httpTransport({ auth: "bearer" }) });

describe("MCP server row", () => {
  it("MSS-FR-XALV: an invalid entry uses its id as the name and shows no transport badge", () => {
    const { row } = renderRow(mcpEntry(null, { state: "invalid" }, { id: "broken", invalid: "invalid_id" }));
    expect(row).toHaveAccessibleName("broken");
    expect(within(row).queryByTestId("mcp-transport-badge")).toBeNull();
  });

  it("MSS-FR-GBOM / MSS-FR-YFPL / MSS-FR-WHXM / MSS-FR-ANUU: each status shows its own actions, in the wireframe order", () => {
    cleanup();
    expect(buttons(renderRow(mcpEntry(mcpRecord(), { state: "untrusted" })).row)).toEqual([
      "Trust…",
      "Test",
      "Edit",
      "Remove",
    ]);
    cleanup();
    expect(buttons(renderRow(mcpEntry(mcpRecord(), { state: "failed", failure: "unreachable" })).row)).toEqual([
      "Reconnect",
      "Test",
      "Edit",
      "Remove",
    ]);
    cleanup();
    expect(buttons(renderRow(mcpEntry(OAUTH, { state: "ready", toolCount: 0 })).row)).toEqual([
      "Test",
      "Edit",
      "Sign out",
      "Remove",
      "▸ Tools (0)",
    ]);
    cleanup();
    expect(buttons(renderRow(mcpEntry(mcpRecord(), { state: "needs_secret" })).row)).toEqual([
      "Set secrets",
      "Test",
      "Edit",
      "Remove",
    ]);
  });

  it("MSS-FR-HXVP / MSS-FR-ANUU: Sign in and Sign out follow the status and the auth mode", () => {
    expect(offersSignIn(mcpEntry(OAUTH, { state: "needs_sign_in" }))).toBe(true);
    expect(offersSignIn(mcpEntry(OAUTH, { state: "failed", failure: "oauth_failed" }))).toBe(true);
    expect(offersSignIn(mcpEntry(OAUTH, { state: "failed", failure: "rejected" }))).toBe(true);
    expect(offersSignIn(mcpEntry(OAUTH, { state: "failed", failure: "unreachable" }))).toBe(false);
    expect(offersSignIn(mcpEntry(BEARER, { state: "failed", failure: "rejected" }))).toBe(false);
    expect(offersSignIn(mcpEntry(OAUTH, { state: "ready", toolCount: 1 }))).toBe(false);
    expect(offersSignOut(mcpEntry(OAUTH, { state: "ready", toolCount: 1 }))).toBe(true);
    expect(offersSignOut(mcpEntry(BEARER, { state: "ready", toolCount: 1 }))).toBe(false);
    expect(offersSignOut(mcpEntry(OAUTH, { state: "needs_sign_in" }))).toBe(false);
  });

  it("MSS-FR-HXVP: while a command of the row runs, its action buttons are disabled", () => {
    const { row } = renderRow(mcpEntry(OAUTH, { state: "needs_sign_in" }), { busy: true });
    expect(within(row).getByRole("button", { name: "Sign in" })).toBeDisabled();
  });

  it("MSS-FR-VWKQ: a running test disables Test; a failure with no standard error shows no block", () => {
    cleanup();
    const running = renderRow(mcpEntry(mcpRecord(), { state: "ready", toolCount: 0 }), {
      test: { phase: "running" },
    });
    expect(within(running.row).getByRole("button", { name: "Test" })).toBeDisabled();
    expect(within(running.row).getByTestId("mcp-test-result")).toHaveTextContent("Testing…");
    cleanup();
    const failed = renderRow(mcpEntry(mcpRecord(), { state: "ready", toolCount: 0 }), {
      test: { phase: "done", result: { ok: false, failure: "timed_out", stderrTail: [] } },
    });
    expect(within(failed.row).getByTestId("mcp-test-result")).toHaveTextContent(
      "The server did not answer in time.",
    );
    expect(within(failed.row).queryByTestId("mcp-test-stderr")).toBeNull();
  });

  it("MSS-FR-QMTG: the switch of a tool follows disabledTools of the record", async () => {
    const record = mcpRecord({ disabledTools: ["b"] });
    const { row } = renderRow(
      mcpEntry(record, { state: "ready", toolCount: 2 }, {
        tools: [
          { name: "a", description: "\nSecond line first.", disabled: false },
          { name: "b", description: "", disabled: true },
        ],
      }),
    );
    await userEvent.click(within(row).getByRole("button", { name: /Tools \(2\)/ }));
    expect(await within(row).findByRole("switch", { name: "a" })).toBeChecked();
    expect(within(row).getByRole("switch", { name: "b" })).not.toBeChecked();
    expect(within(row).getByText("Second line first.")).toBeInTheDocument();
  });
});
