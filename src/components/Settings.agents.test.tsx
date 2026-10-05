import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Settings } from "./Settings";
import type { Agent, ProjectAgent } from "../types";

// SET-FR-15: the Project settings Agents section — where a project decides who
// it can talk to. The personas themselves are described in Global settings
// (AGT-FR-22), so nothing here authors one.
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
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    reasoning: null,
  };
}

const enrolled = (a: Agent): ProjectAgent => ({ agent: a, availability: "ready" });

function backend(all: Agent[], mine: ProjectAgent[]) {
  const calls: string[] = [];
  let current = mine;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    calls.push(cmd);
    switch (cmd) {
      case "list_agents":
        return all;
      case "list_project_agents":
        return current;
      case "enrol_project_agent":
        current = [...current, enrolled(all.find((a) => a.id === args?.agentId)!)];
        return current;
      case "remove_project_agent":
        current = current.filter((p) => p.agent.id !== args?.agentId);
        return current;
      default:
        return undefined;
    }
  });
  return calls;
}

beforeEach(() => invokeMock.mockReset());
afterEach(cleanup);

async function openAgents() {
  await userEvent.click(screen.getByText("Agents"));
}

describe("Project settings — Agents (SET-FR-15)", () => {
  it("SET-FR-03, SET-FR-15, SET-FR-08 is one of the tab's sections, enrols at once, and holds no dirty state", async () => {
    // SET-FR-15 / AGT-FR-22, FR-23. Enrolment and removal apply immediately
    // through their own operations, so the section never participates in the
    // tab's dirty state (SET-FR-08) — asserted as the absence of any save
    // control in it, since this tab reports dirtiness by rendering one.
    const calls = backend(
      [agent("a1", "arch"), agent("a2", "sec")],
      [enrolled(agent("a1", "arch"))],
    );
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openAgents();

    const section = await screen.findByTestId("project-agents-section");
    await waitFor(() =>
      expect(within(section).getAllByTestId("project-agent-row")).toHaveLength(1),
    );
    expect(calls).toContain("list_project_agents");
    expect(within(section).queryByRole("button", { name: /save/i })).toBeNull();

    await userEvent.click(within(section).getByTestId("project-agent-add"));
    await userEvent.click(
      (await screen.findAllByTestId("project-agent-option"))[0],
    );
    await waitFor(() => expect(calls).toContain("enrol_project_agent"));
    await waitFor(() =>
      expect(screen.getAllByTestId("project-agent-row")).toHaveLength(2),
    );

    await userEvent.click(screen.getAllByTestId("project-agent-remove")[0]);
    await waitFor(() => expect(calls).toContain("remove_project_agent"));
  });

  it("names where a persona is described and offers no editor of its own", async () => {
    // AGT-FR-22: this section decides only who is in this project.
    backend([agent("a1", "arch")], [enrolled(agent("a1", "arch"))]);
    render(<Settings contentRoot="/p#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />);
    await openAgents();

    const section = await screen.findByTestId("project-agents-section");
    expect(section).toHaveTextContent(/Global settings/);
    expect(within(section).queryByTestId("agent-edit")).toBeNull();
    expect(within(section).queryByTestId("agent-create")).toBeNull();
  });
});
