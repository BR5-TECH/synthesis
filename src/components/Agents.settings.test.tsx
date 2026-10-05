// The three Agents surfaces — `specifications/ui/AGT-agents.md`, covering
// AGT-FR-01, AGT-FR-02, AGT-FR-03, AGT-FR-04 … AGT-FR-32.
//
// The backend is mocked at `invoke`, so every assertion is about what a surface
// renders and which operation it invokes. Whether an agent is *serviceable* —
// its provider verified, its model offered, its reasoning honoured — belongs to
// `AGR-agent-registry.md` and is covered by its own Rust tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type React from "react";
import { useState } from "react";

import {
  notifyAgentRegistryChanged,
  publishProjectAgents,
  readProjectAgents,
  resetAgentRegistry,
} from "../state/agentRegistry";
import {
  AgentsChromeControl,
  GlobalAgents,
  MentionComposer,
  ProjectAgents,
  agentErrorMessage,
  availabilityNote,
  matchingAgents,
  nicknameProblem,
} from "./Agents";
import type {
  Agent,
  AgentTurn,
  AiApiCatalog,
  ProjectAgent,
} from "../types";
import {
  artifactCommentOrigin,
} from "../test/origins";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Handler = (event: { payload: unknown }) => void;
const listeners = new Map<string, Handler[]>();
const listenMock = vi.fn(async (name: string, cb: Handler) => {
  listeners.set(name, [...(listeners.get(name) ?? []), cb]);
  return () => {
    listeners.set(name, (listeners.get(name) ?? []).filter((h) => h !== cb));
  };
});
vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) =>
    listenMock(args[0] as string, args[1] as Handler),
}));

function emit(name: string, payload: unknown) {
  for (const handler of listeners.get(name) ?? []) handler({ payload });
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function agent(over: Partial<Agent> & { id: string; nickname: string }): Agent {
  return {
    title: "",
    modelId: "anthropic/claude-opus-5",
    instructions: "",
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    reasoning: null,
    ...over,
  };
}

function enrolled(a: Agent, availability: ProjectAgent["availability"] = "ready") {
  return { agent: a, availability };
}

// The keychain-free shape these surfaces actually read: no `keyState`, no
// `maskedHint`, nothing that could only be known by opening the keychain.
function integration(over: Partial<AiApiCatalog> & { provider: AiApiCatalog["provider"] }): AiApiCatalog {
  return {
    displayName: over.provider === "openrouter" ? "OpenRouter" : over.provider,
    state: "verified",
    models: [
      { id: "anthropic/claude-opus-5", label: "Claude Opus 5" },
      {
        id: "reasoner",
        label: "Reasoner",
        reasoning: {
          mandatory: false,
          supportedEfforts: ["high", "medium", "low"],
        },
      },
    ],
    ...over,
  };
}

function turn(over: Partial<AgentTurn> & { agentId: string }): AgentTurn {
  return {
    id: "turn-1",
    nickname: "sec",
    origin: artifactCommentOrigin("t1"),
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    // AGC-FR-33: no tool call active unless a test says otherwise.
    activeToolCalls: [],
    // AGC-FR-31: no offer to retry unless a test says otherwise.
    retryPermitted: false,
    // AGC-FR-37: no picture was omitted unless a test says otherwise.
    imagesOmitted: false,
    startedAt: "2026-01-01T00:00:00Z",
    endedAt: null,
    ...over,
  };
}

interface Backend {
  projectAgents: ProjectAgent[];
  agents: Agent[];
  integrations: AiApiCatalog[];
  turns: AgentTurn[];
  calls: { cmd: string; args: unknown }[];
  createError?: string;
}

function wire(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "list_project_agents":
        return b.projectAgents;
      case "list_agents":
        return b.agents;
      case "get_active_ai_api_catalog": {
        // The provider active for the open project: the first verified record.
        const catalog = b.integrations.find((i) => i.state === "verified") ?? null;
        return {
          resolution: catalog ? "inherited" : "none_configured",
          catalog,
        };
      }
      case "list_agent_turns":
        return b.turns;
      case "create_agent": {
        if (b.createError) throw b.createError;
        const { draft } = args as { draft: Agent };
        return agent({ ...draft, id: "new" });
      }
      case "update_agent": {
        const { id, draft } = args as { id: string; draft: Agent };
        return agent({ ...draft, id });
      }
      case "delete_agent": {
        const { id } = args as { id: string };
        b.agents = b.agents.filter((a) => a.id !== id);
        return b.agents;
      }
      case "enrol_project_agent": {
        const { agentId } = args as { agentId: string };
        const found = b.agents.find((a) => a.id === agentId)!;
        b.projectAgents = [...b.projectAgents, enrolled(found)];
        return b.projectAgents;
      }
      case "remove_project_agent": {
        const { agentId } = args as { agentId: string };
        b.projectAgents = b.projectAgents.filter((p) => p.agent.id !== agentId);
        return b.projectAgents;
      }
      default:
        throw new Error(`unexpected invoke ${cmd}`);
    }
  });
}

function backend(over: Partial<Backend> = {}): Backend {
  const b: Backend = {
    projectAgents: [],
    agents: [],
    integrations: [integration({ provider: "openrouter" })],
    turns: [],
    calls: [],
    ...over,
  };
  wire(b);
  return b;
}

const calls = (b: Backend, cmd: string) => b.calls.filter((c) => c.cmd === cmd);

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockClear();
  listeners.clear();
  // The write signal is module-level, so one test's writes must not be served
  // to the next.
  resetAgentRegistry();
});
afterEach(cleanup);

// ---------------------------------------------------------------------------
// AGT-FR-09, AGT-FR-22 … AGT-FR-21, GLS-FR-24: the Global settings section and its editor
// ---------------------------------------------------------------------------

describe("AGT-FR-09 … AGT-FR-21: the Global settings Agents section", () => {
  it("lists the whole registry, ordered by nickname without regard to case", async () => {
    // AGT-FR-22's first half / AGT-FR-09.
    const b = backend({
      agents: [
        agent({ id: "a1", nickname: "arch" }),
        agent({ id: "a2", nickname: "bee" }),
        agent({ id: "a3", nickname: "Zed" }),
      ],
      projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))],
    });
    render(<GlobalAgents />);
    await waitFor(() => expect(screen.getAllByTestId("agent-row")).toHaveLength(3));
    expect(calls(b, "list_agents")).toHaveLength(1);
    expect(
      screen.getAllByTestId("agent-row-nickname").map((r) => r.textContent),
    ).toEqual(["@arch", "@bee", "@Zed"]);
  });

  it("AGT-FR-10, AGT-FR-MDQW, AGT-FR-PRVQ: keeps an agent whose model is unavailable listed, names no provider, and states why", async () => {
    backend({
      agents: [
        agent({ id: "a1", nickname: "ready" }),
        agent({ id: "a2", nickname: "gone", modelId: "no-such-model" }),
      ],
    });
    render(<GlobalAgents />);
    await waitFor(() => expect(screen.getAllByTestId("agent-row")).toHaveLength(2));
    // Availability is derived from the active catalog, which arrives in its own
    // read - so the warning lands a render after the rows do.
    const warnings = await screen.findAllByTestId("agent-row-warning");
    expect(warnings).toHaveLength(1);
    expect(warnings[0]).toHaveTextContent(
      "The active AI API provider does not offer this model. Choose another model in the editor.",
    );
    const rows = screen.getAllByTestId("agent-row");
    expect(rows[0]).toHaveTextContent("Claude Opus 5");
    expect(rows[0]).not.toHaveTextContent(/OpenRouter/);
    expect(rows[1]).toHaveTextContent("no-such-model");
  });

  it("AGT-FR-12: opens a modal editor with the five controls in order, empty or seeded", async () => {
    // AGT-FR-12 / AGT-FR-11, FR-12, and AGT-FR-42, AGT-FR-19, AGR-FR-23's ordering clause.
    backend({
      agents: [
        agent({
          id: "a1",
          nickname: "arch",
          title: "Architect",
          instructions: "Argue about structure.",
        }),
      ],
    });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).getByTestId("agent-nickname")).toHaveValue("");
    expect(within(editor).getByTestId("agent-title")).toHaveValue("");
    expect(within(editor).getByTestId("agent-instructions")).toHaveValue("");
    // Field order: nickname, title, model, [reasoning], instructions.
    const order = Array.from(
      editor.querySelectorAll("[data-testid^='agent-']"),
    ).map((e) => e.getAttribute("data-testid"));
    for (const [before, after] of [
      ["agent-nickname", "agent-title"],
      ["agent-title", "agent-model"],
      ["agent-model", "agent-instructions"],
    ]) {
      expect(order.indexOf(before)).toBeLessThan(order.indexOf(after));
    }
    // AGT-FR-12: the editor has no provider selector.
    expect(order).not.toContain("agent-provider");
    expect(within(editor).queryByText("Provider")).not.toBeInTheDocument();

    await userEvent.click(within(editor).getByRole("button", { name: "Cancel" }));
    await userEvent.click(screen.getByTestId("agent-edit"));
    const seeded = await screen.findByTestId("agent-editor");
    expect(within(seeded).getByTestId("agent-nickname")).toHaveValue("arch");
    // AGT-FR-42: seeded from the stored value like every other field.
    expect(within(seeded).getByTestId("agent-title")).toHaveValue("Architect");
    expect(within(seeded).getByTestId("agent-instructions")).toHaveValue(
      "Argue about structure.",
    );
  });

  it("carries the title on save and never blocks the confirm action", async () => {
    // AGT-FR-12, AGT-FR-11, AGR-FR-23 / AGT-FR-42, AGT-FR-19. Optional means saving succeeds with the
    // field untouched, and what is typed is sent as typed — the trim that
    // decides what is stored is the registry's (AGR-FR-23).
    const b = backend({ agents: [] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    const confirm = within(editor).getByTestId("agent-editor-confirm");

    // AGT-FR-42: labelled, and stated optional. An unlabelled box with no hint
    // satisfies every value assertion below it.
    expect(within(editor).getByLabelText(/Title/)).toBe(
      within(editor).getByTestId("agent-title"),
    );
    expect(editor).toHaveTextContent(/Optional\./);

    await userEvent.type(within(editor).getByTestId("agent-nickname"), "arch");
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Claude Opus 5"));
    // An empty title leaves the confirm enabled: it is not a required field.
    expect(confirm).toBeEnabled();

    await userEvent.type(
      within(editor).getByTestId("agent-title"),
      "  Lead  UI/UX   Designer  ",
    );
    expect(confirm).toBeEnabled();
    await userEvent.click(confirm);

    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: Agent };
    // Sent as typed: the editor does not second-guess the registry about what
    // the stored form of a title with stray whitespace is (AGR-FR-23).
    expect(draft.title).toBe("  Lead  UI/UX   Designer  ");
  });

  it("carries the title on an update, not only on a create", async () => {
    // AGT-FR-12, AGT-FR-42, AGT-FR-19, AGT-FR-11, AGR-FR-23's update clause. `create` and `update` build the draft at the
    // same site today, but nothing says they must — and an update that dropped
    // the title would silently clear it on every other edit.
    const b = backend({
      agents: [agent({ id: "a1", nickname: "arch", title: "Developer" })],
    });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    const title = within(editor).getByTestId("agent-title");
    await userEvent.clear(title);
    await userEvent.type(title, "Architect");
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));

    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: Agent };
    expect(draft.title).toBe("Architect");
    // And nothing else about the persona moved with it.
    expect(draft.nickname).toBe("arch");
  });

  it("saves an untitled agent with an empty title rather than omitting the field", async () => {
    // AGT-FR-12, AGT-FR-19, AGT-FR-11, AGR-FR-23 / AGT-FR-42: leaving the field alone is an ordinary save.
    const b = backend({ agents: [] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    await userEvent.type(within(editor).getByTestId("agent-nickname"), "sec");
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Claude Opus 5"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));

    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: Agent };
    expect(draft.title).toBe("");
    expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument();
  });

  it("gates the confirm action on the nickname and surfaces a taken one inline", async () => {
    // AGT-FR-19 / AGT-FR-13, FR-19.
    const b = backend({
      agents: [agent({ id: "a1", nickname: "arch" })],
      createError: "nickname_taken",
    });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    const nickname = within(editor).getByTestId("agent-nickname");
    const confirm = within(editor).getByTestId("agent-editor-confirm");

    // AGT-FR-41, AGT-FR-13, AGT-FR-19, AGR-FR-04's first half: the reserved handle is refused client-side in every
    // case, alongside the rules AGR-FR-04 shares with it.
    for (const bad of ["two words", "a@b", "", "all", "ALL", "All"]) {
      await userEvent.clear(nickname);
      if (bad) await userEvent.type(nickname, bad);
      expect(confirm).toBeDisabled();
    }
    // AGT-FR-13: the field states *why* the reserved one is unavailable, so an
    // author reads that `all` is already spoken for rather than only that it was
    // rejected. (The field still holds `All` from the loop above.)
    expect(screen.getByTestId("agent-nickname-error")).toHaveTextContent(
      /every agent/,
    );
    expect(calls(b, "create_agent")).toHaveLength(0);

    await userEvent.clear(nickname);
    await userEvent.type(nickname, "arch");
    // A model has to be chosen before the backend can be asked anything.
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(
      within(await screen.findByRole("listbox")).getByText("Claude Opus 5"),
    );
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));

    // The editor stays open with its fields intact and the typed error inline.
    expect(await screen.findByTestId("agent-editor-error")).toHaveTextContent(
      /already/i,
    );
    expect(screen.getByTestId("agent-nickname")).toHaveValue("arch");
  });

  it("AGT-FR-14: offers the active provider's models, and says so when no provider resolves", async () => {
    backend({
      agents: [],
      integrations: [
        integration({
          provider: "anthropic",
          models: [{ id: "only-here", label: "Only here" }],
        }),
        integration({ provider: "openai", state: "unconfigured" }),
      ],
    });
    const { unmount } = render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    await userEvent.click(await screen.findByTestId("agent-model"));
    expect(
      within(await screen.findByRole("listbox"))
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Only here"]);
    unmount();

    backend({
      agents: [],
      integrations: [integration({ provider: "openrouter", state: "unconfigured" })],
    });
    render(<GlobalAgents />);
    // AGT-FR-14: the empty state must *name the section where one is
    // configured*. Anchored on that clause rather than on "AI API" alone, which
    // the sentence's own opening words would satisfy without it.
    expect(await screen.findByTestId("agents-no-provider")).toHaveTextContent(
      /Configure one in Global settings → AI API/,
    );
    expect(screen.getByTestId("agent-create")).toBeDisabled();
  });

  it("renders the reasoning row only for a model that declares reasoning", async () => {
    // AGT-FR-16 / AGT-FR-12, FR-16.
    backend({ agents: [] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");

    // A model declaring none: no reasoning row at all, and the instructions
    // field sits directly beneath the model selector.
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Claude Opus 5"));
    expect(screen.queryByTestId("agent-reasoning")).not.toBeInTheDocument();

    // One declaring a ladder: the model-default entry followed by its levels.
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Reasoner"));
    await userEvent.click(await screen.findByTestId("agent-reasoning"));
    const listbox = await screen.findByRole("listbox");
    expect(
      within(listbox).getAllByRole("option").map((o) => o.textContent),
    ).toEqual(["Model default", "Off", "high", "medium", "low"]);
  });

  it("returns the reasoning selector to the model default when the new model cannot honour it", async () => {
    // AGT-FR-17's second clause / AGT-FR-17: the editor never holds a
    // combination the backend would reject.
    backend({ agents: [] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");

    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Reasoner"));
    await userEvent.click(await screen.findByTestId("agent-reasoning"));
    await userEvent.click(await screen.findByText("high"));
    expect(within(editor).getByTestId("agent-reasoning")).toHaveTextContent("high");

    // A model that declares no reasoning at all: the row goes with the choice.
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Claude Opus 5"));
    await waitFor(() =>
      expect(screen.queryByTestId("agent-reasoning")).not.toBeInTheDocument(),
    );

    // And back to the reasoning model: the selector is on the model default
    // rather than on the level chosen against the previous one.
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Reasoner"));
    await waitFor(() =>
      expect(within(editor).getByTestId("agent-reasoning")).toHaveTextContent(
        "Model default",
      ),
    );
  });

  it("dismisses the editor on Escape and on the backdrop, invoking nothing", async () => {
    // AGT-FR-11: a modal with a confirm action below a short window's fold and
    // no other way out is a trap. Escape and the backdrop are the two ways out.
    const b = backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    await screen.findByTestId("agent-editor");
    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument(),
    );

    await userEvent.click(screen.getByTestId("agent-edit"));
    await screen.findByTestId("agent-editor");
    // The backdrop is what keeps the row beneath from being activated at all.
    await userEvent.click(screen.getByTestId("agent-editor-backdrop"));
    await waitFor(() =>
      expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument(),
    );
    expect(calls(b, "update_agent")).toHaveLength(0);
  });

  it("creates with empty instructions and carries multi-line ones verbatim", async () => {
    // AGT-FR-19 / AGT-FR-18, FR-19.
    const b = backend({ agents: [] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-create"));
    const editor = await screen.findByTestId("agent-editor");
    await userEvent.type(within(editor).getByTestId("agent-nickname"), "arch");
    await userEvent.click(within(editor).getByTestId("agent-model"));
    await userEvent.click(await screen.findByText("Claude Opus 5"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));

    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: Agent };
    expect(draft.instructions).toBe("");
    expect(draft.nickname).toBe("arch");
    expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument();
  });

  it("invokes nothing when the editor is cancelled", async () => {
    // AGT-FR-19.
    const b = backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    await userEvent.type(within(editor).getByTestId("agent-nickname"), "-changed");
    await userEvent.click(within(editor).getByRole("button", { name: "Cancel" }));
    expect(calls(b, "update_agent")).toHaveLength(0);
    expect(screen.getByTestId("agent-row")).toHaveTextContent("@arch");
  });

  it("confirms a deletion, naming what it costs, and invokes nothing if dismissed", async () => {
    // AGT-FR-20.
    const b = backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents />);
    await userEvent.click(await screen.findByTestId("agent-delete"));
    const confirm = await screen.findByTestId("agent-delete-confirm");
    expect(confirm).toHaveTextContent("@arch");
    expect(confirm).toHaveTextContent(/every project that enrolled it/);
    expect(calls(b, "delete_agent")).toHaveLength(0);

    await userEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(calls(b, "delete_agent")).toHaveLength(0);
    expect(screen.getByTestId("agent-row")).toBeInTheDocument();

    await userEvent.click(screen.getByTestId("agent-delete"));
    await userEvent.click(await screen.findByTestId("agent-delete-confirm-ok"));
    await waitFor(() => expect(calls(b, "delete_agent")).toHaveLength(1));
    await waitFor(() =>
      expect(screen.queryByTestId("agent-row")).not.toBeInTheDocument(),
    );
  });

  it("opens on the editor the roster asked for, and says so once", async () => {
    // AGT-FR-07's other half / AGT-FR-06 as the section sees it.
    backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    const handled = vi.fn();
    const { unmount } = render(
      <GlobalAgents
        editorRequest={{ agentId: "a1", nonce: 1 }}
        onEditorRequestHandled={handled}
      />,
    );
    const editor = await screen.findByTestId("agent-editor");
    expect(within(editor).getByTestId("agent-nickname")).toHaveValue("arch");
    // The acknowledgement is what stops the request being honoured again.
    await waitFor(() => expect(handled).toHaveBeenCalled());
    unmount();

    backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    render(<GlobalAgents editorRequest={{ agentId: null, nonce: 1 }} />);
    expect(
      within(await screen.findByTestId("agent-editor")).getByTestId(
        "agent-nickname",
      ),
    ).toHaveValue("");
  });

  it("opens no editor when the roster asked for none", async () => {
    // The bug this shape exists to prevent: an editor that reopens every time
    // the section is revisited, because a one-shot request was held as a value.
    backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    const { unmount } = render(<GlobalAgents editorRequest={null} />);
    await screen.findByTestId("agent-row");
    expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument();
    unmount();

    // And a request that has been honoured and cleared does not come back.
    backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    function Host() {
      const [request, setRequest] = useState<{
        agentId: string | null;
        nonce: number;
      } | null>({ agentId: null, nonce: 1 });
      return (
        <GlobalAgents
          editorRequest={request}
          onEditorRequestHandled={() => setRequest(null)}
        />
      );
    }
    render(<Host />);
    await screen.findByTestId("agent-editor");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument(),
    );
    // A re-render for any other reason must not bring it back.
    await userEvent.click(screen.getByTestId("agent-row"));
    expect(screen.queryByTestId("agent-editor")).not.toBeInTheDocument();
  });

  it("does not send an author to fix providers when the registry read failed", async () => {
    // The same wrong-advice defect as the roster's, and the worst-phrased of
    // the three: "No AI API provider is verified yet. Configure one in Global
    // settings → AI API" accuses a provider that is fine
    // of a fault it does not have, because `list_agents` failed.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_agents") throw "keychain_unavailable";
      if (cmd === "list_agent_turns") return [];
      return [];
    });
    render(<GlobalAgents />);

    await screen.findByTestId("agents-error");
    expect(screen.queryByTestId("agents-no-provider")).not.toBeInTheDocument();
    expect(screen.queryByTestId("agents-empty")).not.toBeInTheDocument();
  });

  it("keeps listing agents when only the provider catalogue cannot be read", async () => {
    // The two reads answer different questions, and joining them cost the
    // section its agents whenever the catalogue failed. The label degrades to
    // the model's identifier; the row itself stays.
    backend({ agents: [agent({ id: "a1", nickname: "arch" })] });
    const listAgents = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "list_ai_api_catalogs") throw "keychain_unavailable";
      return listAgents(cmd, args);
    });
    render(<GlobalAgents />);

    expect(await screen.findByTestId("agent-row")).toHaveTextContent("@arch");
    expect(screen.queryByTestId("agents-error")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// AGT-FR-22, AGT-FR-23 … AGT-FR-22: the Project settings section
// ---------------------------------------------------------------------------

describe("AGT-FR-22 / AGT-FR-23: the Project settings Agents section", () => {
  it("lists only the enrolment, enrols and removes at once, and never offers an edit", async () => {
    // AGT-FR-09's second half and AGT-FR-23 / AGT-FR-22, FR-23.
    const b = backend({
      agents: [
        agent({ id: "a1", nickname: "arch" }),
        agent({ id: "a2", nickname: "sec" }),
        agent({ id: "a3", nickname: "scribe" }),
      ],
      projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))],
    });
    render(<ProjectAgents />);
    await waitFor(() =>
      expect(screen.getAllByTestId("project-agent-row")).toHaveLength(1),
    );
    expect(calls(b, "list_project_agents")).toHaveLength(1);
    expect(screen.queryByTestId("agent-edit")).not.toBeInTheDocument();

    await userEvent.click(screen.getByTestId("project-agent-add"));
    const picker = await screen.findByTestId("project-agent-picker");
    const options = within(picker).getAllByTestId("project-agent-option");
    expect(options.map((o) => o.textContent)).toEqual(["@sec", "@scribe"]);

    await userEvent.click(options[0]);
    await waitFor(() => expect(calls(b, "enrol_project_agent")).toHaveLength(1));
    await waitFor(() =>
      expect(screen.getAllByTestId("project-agent-row")).toHaveLength(2),
    );

    await userEvent.click(screen.getAllByTestId("project-agent-remove")[0]);
    await waitFor(() => expect(calls(b, "remove_project_agent")).toHaveLength(1));
    // Removal leaves the description alone: the registry read is unchanged.
    expect(b.agents).toHaveLength(3);
  });

  it("says there is nothing left to add, and where a persona is described", async () => {
    // AGT-FR-31 / AGT-FR-23, FR-31.
    backend({
      agents: [agent({ id: "a1", nickname: "arch" })],
      projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))],
    });
    const { unmount } = render(<ProjectAgents />);
    const add = await screen.findByTestId("project-agent-add");
    await waitFor(() => expect(add).toHaveTextContent(/Nothing left to add/i));
    expect(add).toBeDisabled();
    unmount();

    backend({ agents: [], projectAgents: [] });
    render(<ProjectAgents />);
    expect(
      await screen.findByTestId("project-agents-none-described"),
    ).toHaveTextContent(/Global settings/);
  });

  it("does not claim nothing is described when the read failed", async () => {
    // "No agents are described on this machine yet" is a statement of fact, not
    // advice, and a failed read is no basis for it — the author's registry may
    // be full. Withheld, so the error is the only thing claimed.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_project_agents") throw "no_project_open";
      if (cmd === "list_agent_turns") return [];
      return [];
    });
    render(<ProjectAgents />);

    await screen.findByTestId("project-agents-error");
    expect(
      screen.queryByTestId("project-agents-none-described"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("project-agents-empty")).not.toBeInTheDocument();
  });

  it("states a degraded agent's reason and still offers to remove it", async () => {
    // AGT-FR-22.
    backend({
      agents: [agent({ id: "a1", nickname: "arch" })],
      projectAgents: [
        enrolled(agent({ id: "a1", nickname: "arch" }), "provider_unverified"),
      ],
    });
    render(<ProjectAgents />);
    expect(await screen.findByTestId("project-agent-warning")).toHaveTextContent(
      /No AI API provider is active for this project/,
    );
    expect(screen.getByTestId("project-agent-remove")).toBeInTheDocument();
    expect(screen.queryByTestId("agent-edit")).not.toBeInTheDocument();
  });
});
