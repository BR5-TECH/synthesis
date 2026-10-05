// The agent surfaces against the active AI API provider -
// `specifications/ui/AGT-agents.md` AGT-FR-01, AGT-FR-10, AGT-FR-12,
// AGT-FR-14, AGT-FR-15, AGT-FR-17, AGT-FR-PRVQ, AGT-FR-MDQW and AGT-FR-RFSH.
//
// An agent stores no provider. The editor and the Global settings section read
// the catalog of the provider that is active for the open project from
// "get active ai api catalog", and they read it again when the agent registry
// revision changes. The backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { notifyAgentRegistryChanged, resetAgentRegistry } from "../state/agentRegistry";
import { AgentsChromeControl, GlobalAgents, ProjectAgents } from "./Agents";
import type {
  ActiveAiApiCatalog,
  Agent,
  AiApiCatalog,
  ModelOption,
  ProjectAgent,
} from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

function agent(over: Partial<Agent> & { id: string; nickname: string }): Agent {
  return {
    title: "",
    modelId: "claude-opus-5",
    instructions: "",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    reasoning: null,
    ...over,
  };
}

const ANTHROPIC_MODELS: ModelOption[] = [
  { id: "claude-opus-5", label: "Claude Opus 5" },
  {
    id: "claude-thinker",
    label: "Claude Thinker",
    reasoning: { mandatory: false, supportedEfforts: ["high", "low"] },
  },
];

const GATEWAY_MODELS: ModelOption[] = [
  { id: "gw-both", label: "Gateway Both", mode: null },
  { id: "gw-absent", label: "Gateway Absent" },
  { id: "gw-chat", label: "Gateway Chat", mode: "chat" },
  { id: "gw-resp", label: "Gateway Resp", mode: "responses" },
];

function catalog(
  provider: AiApiCatalog["provider"],
  models: ModelOption[],
  displayName: string = provider,
): AiApiCatalog {
  return { provider, displayName, state: "verified", models };
}

const anthropic = (): ActiveAiApiCatalog => ({
  resolution: "inherited",
  catalog: catalog("anthropic", ANTHROPIC_MODELS, "Anthropic"),
});
const custom = (): ActiveAiApiCatalog => ({
  resolution: "overridden",
  catalog: catalog("custom", GATEWAY_MODELS, "Custom"),
});
const none = (resolution: ActiveAiApiCatalog["resolution"]): ActiveAiApiCatalog => ({
  resolution,
  catalog: null,
});

interface Backend {
  active: ActiveAiApiCatalog;
  agents: Agent[];
  projectAgents: ProjectAgent[];
  calls: { cmd: string; args: unknown }[];
}

function backend(over: Partial<Backend> = {}): Backend {
  const b: Backend = {
    active: anthropic(),
    agents: [],
    projectAgents: [],
    calls: [],
    ...over,
  };
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "get_active_ai_api_catalog":
        return b.active;
      case "list_agents":
        return b.agents;
      case "list_project_agents":
        return b.projectAgents;
      case "list_agent_turns":
        return [];
      case "create_agent": {
        const { draft } = args as { draft: Omit<Agent, "id"> };
        return agent({ ...draft, id: "new" });
      }
      case "update_agent": {
        const { id, draft } = args as { id: string; draft: Omit<Agent, "id"> };
        return agent({ ...draft, id });
      }
      default:
        throw new Error(`unexpected invoke ${cmd}`);
    }
  });
  return b;
}

const calls = (b: Backend, cmd: string) => b.calls.filter((c) => c.cmd === cmd);

beforeEach(() => {
  invokeMock.mockReset();
  resetAgentRegistry();
});
afterEach(cleanup);

async function openSelector(editor: HTMLElement, testId: string) {
  await userEvent.click(within(editor).getByTestId(testId));
  return within(await screen.findByRole("listbox"));
}

async function optionTexts(editor: HTMLElement, testId = "agent-model") {
  const list = await openSelector(editor, testId);
  return list.getAllByRole("option").map((o) => o.textContent);
}

describe("the persona editor against the active provider", () => {
  it("AGT-FR-12, AGT-FR-14, AGT-FR-15: has no provider selector and offers the active catalog's models only", async () => {
    backend();
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");

    expect(screen.queryByTestId("agent-provider")).not.toBeInTheDocument();
    expect(within(editor).queryByText("Provider")).not.toBeInTheDocument();
    expect(await optionTexts(editor)).toEqual(["Claude Opus 5", "Claude Thinker"]);
  });

  it("AGT-FR-14, AGT-FR-01: sends a draft that carries no provider on create", async () => {
    const b = backend();
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    await userEvent.type(within(editor).getByTestId("agent-nickname"), "arch");
    const list = await openSelector(editor, "agent-model");
    await userEvent.click(list.getByText("Claude Opus 5"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));

    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: Record<string, unknown> };
    expect(draft).not.toHaveProperty("provider");
    expect(draft.modelId).toBe("claude-opus-5");
  });

  it("AGT-FR-17: seeds the model from the agent when the active provider offers it", async () => {
    const b = backend({ agents: [agent({ id: "a1", nickname: "arch", modelId: "claude-thinker" })] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");

    expect(within(editor).getByTestId("agent-model")).toHaveTextContent("Claude Thinker");
    expect(screen.queryByTestId("agent-model-unavailable")).not.toBeInTheDocument();
    expect(within(editor).getByTestId("agent-editor-confirm")).toBeEnabled();
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: Record<string, unknown> };
    expect(draft).not.toHaveProperty("provider");
  });

  it("AGT-FR-17, AGT-FR-MDQW: shows no model and a note when the stored model is unavailable, and keeps Confirm disabled until one is chosen", async () => {
    const b = backend({
      agents: [agent({ id: "a1", nickname: "arch", modelId: "retired-model" })],
    });
    render(<GlobalAgents />);
    // The row stays listed with the reason (AGT-FR-10, AGT-FR-MDQW).
    expect(await screen.findByTestId("agent-row-warning")).toHaveTextContent(
      /does not offer this model/,
    );
    await userEvent.click(screen.getByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");

    expect(within(editor).getByTestId("agent-model-unavailable")).toHaveTextContent(
      /stored model.*not offered/i,
    );
    expect(within(editor).getByTestId("agent-model")).not.toHaveTextContent("retired-model");
    const confirm = within(editor).getByTestId("agent-editor-confirm");
    expect(confirm).toBeDisabled();
    await userEvent.click(confirm);
    expect(calls(b, "update_agent")).toHaveLength(0);
    expect(calls(b, "create_agent")).toHaveLength(0);

    // Choosing an offered model makes the agent saveable, with that model.
    const list = await openSelector(editor, "agent-model");
    await userEvent.click(list.getByText("Claude Opus 5"));
    expect(confirm).toBeEnabled();
    expect(screen.queryByTestId("agent-model-unavailable")).not.toBeInTheDocument();
    await userEvent.click(confirm);
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: { modelId: string } };
    expect(draft.modelId).toBe("claude-opus-5");
  });

  it("AGT-FR-17: returns the reasoning selector to the model default when the new model cannot honour it", async () => {
    backend({
      agents: [
        agent({
          id: "a1",
          nickname: "arch",
          modelId: "claude-thinker",
          reasoning: { kind: "effort", effort: "high" },
        }),
      ],
    });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).getByTestId("agent-reasoning")).toHaveTextContent("high");

    const list = await openSelector(editor, "agent-model");
    await userEvent.click(list.getByText("Claude Opus 5"));
    expect(within(editor).queryByTestId("agent-reasoning")).not.toBeInTheDocument();
  });

  it("AGT-FR-14: disables Confirm of an existing agent while no provider resolves", async () => {
    const b = backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    const confirm = within(editor).getByTestId("agent-editor-confirm");
    expect(confirm).toBeEnabled();

    // The active provider is cleared while the editor is open.
    b.active = none("none_selected");
    act(() => notifyAgentRegistryChanged());
    await waitFor(() => expect(confirm).toBeDisabled());
  });
});

describe("provider-switch refresh (AGT-FR-RFSH, AGT-FR-PRVQ)", () => {
  it("AGT-FR-RFSH, AGT-FR-PRVQ: re-reads the catalog when the registry revision changes and updates rows and the editor", async () => {
    const b = backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    let editor = await screen.findByTestId("agent-editor");
    expect(await optionTexts(editor)).toEqual(["Claude Opus 5", "Claude Thinker"]);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("agent-row-warning")).not.toBeInTheDocument();
    const reads = calls(b, "get_active_ai_api_catalog").length;

    // The author activates Custom in the AI API section of the same window.
    b.active = custom();
    act(() => notifyAgentRegistryChanged());

    await waitFor(() =>
      expect(calls(b, "get_active_ai_api_catalog").length).toBeGreaterThan(reads),
    );
    // The agent's model is not a gateway model: the row now says so.
    expect(await screen.findByTestId("agent-row-warning")).toHaveTextContent(
      /does not offer this model/,
    );
    editor = screen.getByTestId("agent-editor");
    expect(await optionTexts(editor)).toEqual([
      "Gateway Both · chat + responses",
      "Gateway Absent · chat + responses",
      "Gateway Chat · chat",
      "Gateway Resp · responses",
    ]);
  });

  it("AGT-FR-RFSH: a re-mounted section reads the new provider's catalog", async () => {
    const b = backend();
    const first = render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    expect(await optionTexts(await screen.findByTestId("agent-editor"))).toHaveLength(2);
    first.unmount();

    b.active = custom();
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    expect(await optionTexts(await screen.findByTestId("agent-editor"))).toHaveLength(4);
  });

  it("AGT-FR-PRVQ: marks an agent ready again after the provider switches back to one that offers its model", async () => {
    const b = backend({ active: custom(), agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    expect(await screen.findByTestId("agent-row-warning")).toBeInTheDocument();

    b.active = anthropic();
    act(() => notifyAgentRegistryChanged());
    await waitFor(() =>
      expect(screen.queryByTestId("agent-row-warning")).not.toBeInTheDocument(),
    );
  });
});

describe("the Custom gateway in the editor (AGT-FR-15, AGT-FR-16, AGT-FR-RFSH)", () => {
  it("AGT-FR-15: states that the list came from the Custom gateway and names each model's route", async () => {
    backend({ active: custom() });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");

    expect(within(editor).getByTestId("agent-model-origin")).toHaveTextContent(
      "from the Custom gateway",
    );
    const options = await optionTexts(editor);
    expect(options).toContain("Gateway Both · chat + responses");
    expect(options).toContain("Gateway Absent · chat + responses");
    expect(options).toContain("Gateway Chat · chat");
    expect(options).toContain("Gateway Resp · responses");
  });

  it("AGT-FR-15: filters by label or identifier, and commits the identifier", async () => {
    const b = backend({ active: custom() });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    await userEvent.type(within(editor).getByTestId("agent-nickname"), "gw");
    await openSelector(editor, "agent-model");

    await userEvent.type(screen.getByLabelText("Filter model"), "GW-RESP");
    expect(
      within(screen.getByRole("listbox")).getAllByRole("option").map((o) => o.textContent),
    ).toEqual(["Gateway Resp · responses"]);
    await userEvent.keyboard("{Enter}");
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));

    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: { modelId: string } };
    expect(draft.modelId).toBe("gw-resp");
  });

  it("AGT-FR-16, AGT-FR-RFSH: renders no reasoning selector for any Custom model", async () => {
    backend({ active: custom() });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    for (const label of ["Gateway Both", "Gateway Chat", "Gateway Resp"]) {
      const list = await openSelector(editor, "agent-model");
      await userEvent.click(list.getByText(new RegExp(`^${label}`)));
      expect(within(editor).queryByTestId("agent-reasoning")).not.toBeInTheDocument();
    }
  });

  it("AGT-FR-15: does not state a gateway origin for a provider that is not Custom", async () => {
    backend();
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).queryByTestId("agent-model-origin")).not.toBeInTheDocument();
  });
});

describe("the Global settings section without a provider (AGT-FR-14, AGT-FR-PRVQ)", () => {
  it("AGT-FR-14: renders the empty state naming Global settings -> AI API and disables New agent", async () => {
    backend({ active: none("none_configured") });
    render(<GlobalAgents />);
    expect(await screen.findByTestId("agents-no-provider")).toHaveTextContent(
      "Global settings → AI API",
    );
    expect(screen.getByTestId("agent-create")).toBeDisabled();
  });

  it("AGT-FR-14: also renders it when providers exist but none is active", async () => {
    backend({ active: none("none_selected") });
    render(<GlobalAgents />);
    expect(await screen.findByTestId("agents-no-provider")).toHaveTextContent(
      "Global settings → AI API",
    );
    expect(screen.getByTestId("agent-create")).toBeDisabled();
  });

  it("AGT-FR-PRVQ, AGR-FR-16: derives provider_unconfigured and provider_unverified rows from the resolution", async () => {
    const b = backend({
      active: none("none_configured"),
      agents: [agent({ id: "a1", nickname: "arch" })],
    });
    const { unmount } = render(<GlobalAgents />);
    expect(await screen.findByTestId("agent-row-warning")).toHaveTextContent(
      /No AI API provider is configured/,
    );
    unmount();

    b.active = none("override_unavailable");
    render(<GlobalAgents />);
    expect(await screen.findByTestId("agent-row-warning")).toHaveTextContent(
      /No AI API provider is active for this project/,
    );
    // The row names a model line and no provider (AGT-FR-10).
    expect(screen.getByTestId("agent-row")).toHaveTextContent("claude-opus-5");
  });

  it("AGT-FR-10: presents no provider name in a row", async () => {
    backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    const row = await screen.findByTestId("agent-row");
    expect(row).toHaveTextContent("@arch");
    expect(row).toHaveTextContent("Claude Opus 5");
    expect(row).not.toHaveTextContent(/anthropic/i);
  });
});

describe("the model line of the project section and the roster (AGT-FR-01, AGT-FR-04, AGT-FR-22)", () => {
  it("AGT-FR-22, AGT-FR-01: the project section names the model from the active catalog and no provider", async () => {
    const a = agent({ id: "a1", nickname: "arch", reasoning: { kind: "effort", effort: "high" } });
    backend({ agents: [a], projectAgents: [{ agent: a, availability: "ready" }] });
    render(<ProjectAgents />);
    const row = await screen.findByTestId("project-agent-row");
    await waitFor(() => expect(row).toHaveTextContent("Claude Opus 5 · high"));
    expect(row).not.toHaveTextContent(/anthropic/i);
  });

  it("AGT-FR-04, AGT-FR-RFSH: the roster names the model from the active catalog and follows a provider switch", async () => {
    const a = agent({ id: "a1", nickname: "arch", modelId: "gw-chat" });
    const b = backend({
      active: anthropic(),
      projectAgents: [{ agent: a, availability: "model_unavailable" }],
    });
    function Roster() {
      const [open, setOpen] = useState(false);
      return (
        <AgentsChromeControl
          onOverlayOpening={() => {}}
          onOpenAgentInSettings={() => {}}
          onOpenProjectSettings={() => {}}
          open={open}
          onOpenChange={setOpen}
        />
      );
    }
    render(<Roster />);
    await userEvent.click(await screen.findByTestId("chrome-agents-control"));
    const row = await screen.findByTestId("agents-roster-row");
    expect(row).toHaveTextContent("gw-chat");

    b.active = custom();
    act(() => notifyAgentRegistryChanged());
    await waitFor(() => expect(row).toHaveTextContent("Gateway Chat"));
    expect(row).not.toHaveTextContent(/Custom/);
  });
});
