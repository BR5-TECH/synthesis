import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Settings } from "../test/settingsHarness";
import { resetAgentRegistry } from "../state/agentRegistry";
import type { Agent, ProjectAgent } from "../types";

// SET-FR-15: the Project Agents section — where a project describes the agents
// it shares with every contributor. The author's Personal agents are described
// in Global settings (AGT-FR-09), so none of them is listed or authored here.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function agent(id: string, nickname: string): Agent {
  return {
    id,
    nickname,
    title: "",
    modelId: "m",
    instructions: "",
    createdAt: "",
    updatedAt: "",
    reasoning: null,
  };
}

const entry = (a: Agent, scope: ProjectAgent["scope"] = "project"): ProjectAgent => ({
  scope,
  agent: a,
  availability: "ready",
});

function backend(roster: ProjectAgent[]) {
  const calls: { cmd: string; args?: Record<string, unknown> }[] = [];
  let current = roster;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    switch (cmd) {
      case "list_project_agents":
        return current;
      case "get_active_ai_api_catalog":
        return {
          resolution: "inherited",
          catalog: {
            provider: "openrouter",
            displayName: "OpenRouter",
            state: "verified",
            models: [{ id: "m", label: "M" }],
          },
        };
      case "delete_agent":
        current = current.filter((p) => p.agent.id !== args?.id);
        return current.filter((p) => p.scope === "project");
      default:
        return undefined;
    }
  });
  return calls;
}

beforeEach(() => {
  invokeMock.mockReset();
  resetAgentRegistry();
});
afterEach(cleanup);

async function openAgents() {
  await userEvent.click(screen.getByText("Project Agents"));
}

describe("Project settings — Project Agents (SET-FR-15)", () => {
  it("SET-FR-03, SET-FR-15, SET-FR-08: is one of the group's sections, writes at once, and holds no dirty state", async () => {
    // Every action applies immediately through its own operation, so the section
    // never participates in the group's dirty state (SET-FR-08) — asserted as
    // the absence of any save control in it, since the group reports dirtiness
    // by rendering one.
    const calls = backend([
      entry(agent("a1", "arch")),
      entry(agent("a2", "mine"), "personal"),
    ]);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openAgents();

    const section = await screen.findByTestId("agents-section");
    await waitFor(() =>
      expect(within(section).getAllByTestId("agent-row")).toHaveLength(1),
    );
    expect(calls.map((c) => c.cmd)).toContain("list_project_agents");
    expect(within(section).queryByRole("button", { name: /save/i })).toBeNull();

    await userEvent.click(within(section).getByTestId("agent-delete"));
    await userEvent.click(await screen.findByTestId("agent-delete-confirm-ok"));
    await waitFor(() =>
      expect(calls.find((c) => c.cmd === "delete_agent")?.args).toEqual({
        scope: "project",
        id: "a1",
      }),
    );
  });

  it("AGT-FR-22: offers an editor of its own for a Project agent", async () => {
    backend([entry(agent("a1", "arch"))]);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openAgents();

    const section = await screen.findByTestId("agents-section");
    expect(await within(section).findByTestId("agent-edit")).toBeInTheDocument();
    expect(within(section).getByTestId("agent-create")).toBeInTheDocument();
    expect(section).toHaveTextContent(/\.synthesis/);
  });

  it("AGT-FR-06: opens on the editor of the Project agent a request names", async () => {
    backend([entry(agent("a1", "arch"))]);
    render(
      <Settings
        contentRoot="/p#0"
        lineEndings="lf"
        onSelectLineEndings={vi.fn()}
        agentEditorRequest={{ agentId: "a1", nonce: 1 }}
      />,
    );
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).getByTestId("agent-nickname")).toHaveValue("arch");
  });
});
