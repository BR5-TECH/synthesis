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
// AGT-FR-25, AGT-FR-33 … AGT-FR-27, AGT-FR-34, AGT-FR-30, SNV-FR-56, CMT-FR-32: the mention picker
// ---------------------------------------------------------------------------

describe("AGT-FR-25 … AGT-FR-27: the mention picker", () => {
  const AGENTS = [
    enrolled(agent({ id: "a1", nickname: "arch" })),
    enrolled(agent({ id: "a2", nickname: "scribe" })),
    enrolled(agent({ id: "a3", nickname: "sec" })),
  ];

  function Composer({ agents = AGENTS }: { agents?: ProjectAgent[] }) {
    const [value, setValue] = useState("");
    return (
      <MentionComposer
        value={value}
        onChange={setValue}
        agents={agents}
        ariaLabel="Comment"
        testId="composer"
      />
    );
  }

  /**
   * The picker's **agent** rows alone.
   *
   * `@all` leads the list whenever the fragment is a prefix of `all` (AGT-FR-39),
   * an empty fragment included — so a test about how nicknames rank says so by
   * asking for nicknames rather than by counting rows around the handle.
   */
  const agentOptions = () =>
    screen
      .getAllByTestId("mention-picker-option")
      .filter((o) => o.dataset.entry === "agent");

  it("opens on @, narrows as the author types, and inserts the full tag", async () => {
    // AGT-FR-33 / AGT-FR-25.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@");
    const picker = await screen.findByTestId("mention-picker");
    // AGT-FR-39: the handle leads, then the nicknames in their own order.
    expect(
      within(picker).getAllByTestId("mention-picker-option").map((o) => o.textContent),
    ).toEqual([
      expect.stringContaining("@all"),
      expect.stringContaining("@arch"),
      expect.stringContaining("@scribe"),
      expect.stringContaining("@sec"),
    ]);

    // Typing `se` keeps `@sec` alone, which is the mirror of the `sc` case.
    await userEvent.type(composer, "se");
    await waitFor(() =>
      expect(screen.getAllByTestId("mention-picker-option")).toHaveLength(1),
    );
    await userEvent.keyboard("{Backspace}{Backspace}");

    // AGT-FR-33: a fragment matches only a nickname it *begins*. `sc` therefore
    // keeps `@scribe` and drops `@sec`, whose nickname starts `se`.
    await userEvent.type(composer, "sc");
    await waitFor(() =>
      expect(screen.getAllByTestId("mention-picker-option")).toHaveLength(1),
    );

    await userEvent.click(screen.getAllByTestId("mention-picker-option")[0]);
    await waitFor(() => expect(composer).toHaveValue("@scribe "));
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("offers a degraded agent with its reason, and refuses to select it", async () => {
    // AGT-FR-27, AGT-FR-33 / AGT-FR-26.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "arch" })),
          enrolled(agent({ id: "a2", nickname: "scribe" }), "provider_unverified"),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.type(composer, "@");
    await screen.findByTestId("mention-picker");
    // Asked for by kind rather than by index: the handle leads the list
    // (AGT-FR-39), and this is a claim about the two agents.
    const options = agentOptions();
    expect(options[1]).toHaveTextContent(/not verified/);
    expect(options[1]).toBeDisabled();
    expect(options[0]).not.toBeDisabled();
  });

  it("is operable from the keyboard alone and leaves the text on Escape", async () => {
    // AGT-FR-34 / AGT-FR-27.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @");
    await screen.findByTestId("mention-picker");
    // The highlight starts on `@all`, which leads the list (AGT-FR-39), so two
    // steps reach the second nickname.
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    await waitFor(() => expect(composer).toHaveValue("ask @scribe "));

    await userEvent.type(composer, "and @a");
    await screen.findByTestId("mention-picker");
    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );
    // Escape changed nothing about what was typed.
    expect(composer).toHaveValue("ask @scribe and @a");
  });

  it("walks the arrows past an entry that cannot answer, in both directions", async () => {
    // AGT-FR-26 / AGT-FR-27: the highlight never parks on a degraded entry, so
    // `@aide` is stepped over whichever way round the list the arrows go. The
    // list here is `@all`, `@aide`, `@arch` — the handle leading (AGT-FR-39) and
    // able to answer, `@arch` being ready — so a step in either direction has to
    // clear the degraded entry between them.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "arch" })),
          enrolled(agent({ id: "a2", nickname: "aide" }), "provider_unverified"),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @a");
    await screen.findByTestId("mention-picker");

    // The initial highlight is the handle *because it is the first selectable
    // row* — `findIndex(selectable)`, not a hard-coded zero. Stated so the
    // degraded-skip on the opening highlight stays asserted here rather than
    // resting on the handle happening to lead.
    const rows = await screen.findAllByTestId("mention-picker-option");
    expect(rows[0]).toHaveAttribute("data-entry", "all");
    expect(rows[0]).toHaveAttribute("data-highlighted", "true");
    expect(rows[1]).toHaveTextContent("@aide");
    expect(rows[1]).toBeDisabled();

    // Down from `@all` skips `@aide` and lands on `@arch`.
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() =>
      expect(screen.getAllByTestId("mention-picker-option")[2]).toHaveAttribute(
        "data-highlighted",
        "true",
      ),
    );
    // Up from `@arch` skips it again and returns to the handle.
    await userEvent.keyboard("{ArrowUp}");
    await waitFor(() =>
      expect(screen.getAllByTestId("mention-picker-option")[0]).toHaveAttribute(
        "data-highlighted",
        "true",
      ),
    );
    // The degraded entry was never highlighted in either direction.
    expect(screen.getAllByTestId("mention-picker-option")[1]).toHaveAttribute(
      "data-highlighted",
      "false",
    );

    await userEvent.keyboard("{ArrowDown}{Enter}");
    await waitFor(() => expect(composer).toHaveValue("ask @arch "));
  });

  it("never lets Enter reach the composer when there is nothing to choose", async () => {
    // AGT-FR-26 / AGT-FR-27. An Enter that was not prevented puts a newline into
    // the message the author is writing — the exact cost of guarding
    // `preventDefault` behind the selectable check. Every match here is degraded,
    // so nothing is highlighted and `choose` is never reached; the guard is the
    // only thing keeping the newline out.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "aide" }), "provider_unverified"),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @a");
    await screen.findByTestId("mention-picker");
    // Neither row can be chosen: `@aide` cannot answer, and `@all` resolves to
    // nobody because no enrolled agent can (AGT-FR-38), so it is offered with
    // that reason and completed by no key.
    const options = screen.getAllByTestId("mention-picker-option");
    expect(options).toHaveLength(2);
    expect(options[0]).toHaveTextContent(/can answer/);
    for (const option of options) {
      expect(option).toBeDisabled();
      expect(option).toHaveAttribute("data-highlighted", "false");
    }

    await userEvent.keyboard("{Enter}");
    expect(composer).toHaveValue("ask @a");
    expect(composer).not.toHaveValue(expect.stringContaining("\n"));

    // The same holds when the fragment matches nothing at all.
    await userEvent.type(composer, "zzz");
    await screen.findByTestId("mention-picker-empty");
    await userEvent.keyboard("{Enter}");
    expect(composer).toHaveValue("ask @azzz");
  });

  it("keeps the caret in the composer, after the tag, once a completion lands", async () => {
    // AGT-FR-25: the author is mid-sentence, so a completion must not cost them
    // focus or drop the caret somewhere else in the message.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer") as HTMLTextAreaElement;
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @scr and more");
    // Put the caret back at the end of the fragment the picker is filtering on.
    act(() => composer.setSelectionRange(8, 8));
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}");
    await screen.findByTestId("mention-picker");

    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("ask @scribe and more"));
    expect(composer).toHaveFocus();
    // Directly after the tag — the text already continued, so no space was added.
    expect(composer.selectionStart).toBe("ask @scribe".length);
  });

  it("does not complete on a modified Tab", async () => {
    // AGT-FR-27: Tab is the picker's key, but Shift+Tab is the author moving
    // focus backwards and must keep doing that.
    backend();
    render(
      <>
        <button data-testid="before">before</button>
        <Composer />
      </>,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@ar");
    await screen.findByTestId("mention-picker");

    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(composer).toHaveValue("@ar");
    expect(screen.getByTestId("before")).toHaveFocus();
  });

  it("closes when the caret leaves the candidate tag and offers again on the way back", async () => {
    // AGT-FR-34: the picker follows the caret, and it does so for the arrow keys
    // exactly as it does for a click — summoning an agent never requires a
    // pointer.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@ar done");
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );

    // Back over the space, into the tag: the picker is offered again.
    await userEvent.keyboard("{ArrowLeft}{ArrowLeft}{ArrowLeft}{ArrowLeft}{ArrowLeft}");
    expect(await screen.findByTestId("mention-picker")).toBeInTheDocument();

    // And forward out of it again.
    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );
  });

  it("does not reopen on the nickname it just completed", async () => {
    // The counterpart to the test above: a completion made mid-sentence leaves
    // the caret at the end of a tag that is still a candidate, and the picker
    // must not spring back open on the agent the author has just chosen.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @scr and more");
    act(() => (composer as HTMLTextAreaElement).setSelectionRange(8, 8));
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}");
    await screen.findByTestId("mention-picker");

    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("ask @scribe and more"));
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("lets the author delete back into a tag completed by pointer", async () => {
    // AGT-FR-34. The pointer path fires no keyup, so any suppression that waits
    // for one to spend it would leave this candidate tag permanently shut — the
    // author could never correct a nickname they had clicked.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@ar");
    await userEvent.click(await screen.findByTestId("mention-picker-option"));
    await waitFor(() => expect(composer).toHaveValue("@arch "));

    // Back into the tag: the picker is offered again against the shorter
    // fragment, exactly as it is after a Tab completion.
    await userEvent.keyboard("{Backspace}{Backspace}{Backspace}");
    await waitFor(() => expect(composer).toHaveValue("@ar"));
    expect(await screen.findByTestId("mention-picker")).toBeInTheDocument();
  });

  it("does not reopen on a nickname completed by pointer either", async () => {
    // The counterpart: Tab and the pointer must leave the composer in the same
    // state, so neither springs the picker back open on what was just chosen.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @scr and more");
    act(() => (composer as HTMLTextAreaElement).setSelectionRange(8, 8));
    await userEvent.keyboard("{ArrowLeft}{ArrowRight}");
    await userEvent.click(await screen.findByTestId("mention-picker-option"));

    await waitFor(() => expect(composer).toHaveValue("ask @scribe and more"));
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("closes when the composer loses focus", async () => {
    // AGT-FR-34: a caret that has left the composer is not sitting at the end of
    // a candidate tag, so a picker left floating over an abandoned composer is a
    // surface with no owner.
    backend();
    render(
      <>
        <Composer />
        <button data-testid="after">after</button>
      </>,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@ar");
    await screen.findByTestId("mention-picker");

    await userEvent.click(screen.getByTestId("after"));
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );
  });

  it("inserts nothing when a degraded entry is clicked", async () => {
    // AGT-FR-26: not selectable by any route, pointer included.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "aide" }), "provider_unverified"),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");
    await screen.findByTestId("mention-picker");
    // `@all` leads and is itself unselectable here, no enrolled agent being able
    // to answer (AGT-FR-38); the claim is about the agent row, so ask for it.
    const [option] = agentOptions();

    await userEvent.click(option);
    expect(composer).toHaveValue("@a");
  });
});
