import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import type { AgentGroupSettings } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { ProjectAgentGroupSettings, parseJoinTimeout } from "./ProjectAgentGroupSettings";

let stored: AgentGroupSettings;
let refuse: string | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  stored = { enabled: false, joinTimeoutSecs: 60 };
  refuse = null;
  invokeMock.mockImplementation((cmd: string, args?: { settings?: AgentGroupSettings }) => {
    if (cmd === "load_agent_group_settings") return Promise.resolve(stored);
    if (cmd === "save_agent_group_settings") {
      if (refuse) return Promise.reject(refuse);
      stored = args?.settings as AgentGroupSettings;
      return Promise.resolve(stored);
    }
    return otherCommand(cmd);
  });
});

afterEach(() => {
  cleanup();
});

describe("Project Agents: multi-persona conversations", () => {
  it("SET-FR-WOSJ, SET-FR-AGRZ, SET-FR-VUHW: reads the switch and the timeout when the section mounts, off and 60 by default", async () => {
    render(<ProjectAgentGroupSettings />);
    const toggle = await screen.findByTestId("agent-group-switch");
    await waitFor(() => expect(toggle).not.toBeDisabled());
    expect(toggle).not.toBeChecked();
    expect(screen.getByTestId("agent-group-timeout")).toHaveValue("60");
    expect(calls("load_agent_group_settings")).toHaveLength(1);
    expect(screen.getByText(/never committed/i)).toBeInTheDocument();
  });

  it("SET-FR-WOSJ: turning the switch on saves at once with the stored timeout", async () => {
    const user = userEvent.setup();
    render(<ProjectAgentGroupSettings />);
    const toggle = await screen.findByTestId("agent-group-switch");
    await waitFor(() => expect(toggle).not.toBeDisabled());
    await user.click(toggle);
    await waitFor(() => expect(stored).toEqual({ enabled: true, joinTimeoutSecs: 60 }));
    expect(toggle).toBeChecked();
  });

  it("SET-FR-AGRZ: a timeout in range is saved when the field loses focus", async () => {
    const user = userEvent.setup();
    render(<ProjectAgentGroupSettings />);
    const field = await screen.findByTestId("agent-group-timeout");
    await waitFor(() => expect(field).not.toBeDisabled());
    await user.clear(field);
    await user.type(field, "90");
    await user.tab();
    await waitFor(() => expect(stored.joinTimeoutSecs).toBe(90));
    expect(screen.queryByTestId("agent-group-timeout-error")).toBeNull();
  });

  it("SET-FR-AGRZ: a timeout out of range is refused inline and is not saved", async () => {
    const user = userEvent.setup();
    render(<ProjectAgentGroupSettings />);
    const field = await screen.findByTestId("agent-group-timeout");
    await waitFor(() => expect(field).not.toBeDisabled());
    for (const text of ["4", "601", "abc", ""]) {
      await user.clear(field);
      if (text) await user.type(field, text);
      await user.tab();
      expect(await screen.findByTestId("agent-group-timeout-error")).toBeInTheDocument();
    }
    expect(calls("save_agent_group_settings")).toHaveLength(0);
    expect(stored.joinTimeoutSecs).toBe(60);
  });

  it("SET-FR-WOSJ: a refused save restores what the backend stored and says why", async () => {
    refuse = "join_timeout_out_of_range";
    const user = userEvent.setup();
    render(<ProjectAgentGroupSettings />);
    const toggle = await screen.findByTestId("agent-group-switch");
    await waitFor(() => expect(toggle).not.toBeDisabled());
    await user.click(toggle);
    expect(await screen.findByRole("alert")).toHaveTextContent("join_timeout_out_of_range");
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it("SET-FR-AGRZ: the timeout parser takes whole numbers from 5 to 600 only", () => {
    expect(parseJoinTimeout("5")).toBe(5);
    expect(parseJoinTimeout(" 600 ")).toBe(600);
    for (const text of ["4", "601", "1.5", "-5", "", "60s"]) {
      expect(parseJoinTimeout(text)).toBeNull();
    }
  });
});
