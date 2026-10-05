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
// AGT-FR-01, AGT-FR-02, AGT-FR-03, AGT-FR-04 … AGT-FR-11, AGT-FR-12: the chrome control and its roster
// ---------------------------------------------------------------------------

const noop = () => {};

/** The shell owns the roster's open state (SNV-FR-56); this stands in for it. */
function Roster(props: Partial<React.ComponentProps<typeof AgentsChromeControl>>) {
  const [open, setOpen] = useState(false);
  return (
    <AgentsChromeControl
      onOverlayOpening={noop}
      onOpenAgentInSettings={noop}
      onOpenProjectSettings={noop}
      open={open}
      onOpenChange={setOpen}
      {...props}
    />
  );
}

describe("AGT-FR-02 … AGT-FR-08: the chrome control and its roster", () => {

  it("carries the enrolled count and opens a roster naming each agent's model", async () => {
    // AGT-FR-02, AGT-FR-03, AGT-FR-04 / AGT-FR-01, FR-02, FR-03, FR-04.
    const b = backend({
      projectAgents: [
        enrolled(agent({ id: "a1", nickname: "arch", reasoning: { kind: "effort", effort: "high" } })),
        enrolled(agent({ id: "a2", nickname: "sec" })),
        enrolled(agent({ id: "a3", nickname: "scribe" })),
      ],
    });
    render(<Roster onOverlayOpening={noop} onOpenAgentInSettings={noop} onOpenProjectSettings={noop} />);

    const control = await screen.findByTestId("chrome-agents-control");
    await waitFor(() => expect(control).toHaveTextContent("3"));
    expect(calls(b, "list_project_agents").length).toBeGreaterThan(0);

    await userEvent.click(control);
    const roster = await screen.findByTestId("agents-roster");
    const rows = within(roster).getAllByTestId("agents-roster-row");
    expect(rows.map((r) => r.textContent)).toEqual([
      expect.stringContaining("@arch"),
      expect.stringContaining("@sec"),
      expect.stringContaining("@scribe"),
    ]);
    expect(rows[0]).toHaveTextContent("high");

    // AGT-FR-01: no base URL, key, masked hint, or verification state anywhere.
    for (const forbidden of ["https://", "cdef", "verified", "sk-"]) {
      expect(roster.textContent ?? "").not.toContain(forbidden);
    }
  });

  it("closes every other overlay when it opens, and dismisses on Escape", async () => {
    // AGT-FR-03, per SNV-FR-56.
    backend({ projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))] });
    const onOverlayOpening = vi.fn();
    render(<Roster onOverlayOpening={onOverlayOpening} onOpenAgentInSettings={noop} onOpenProjectSettings={noop} />);
    const control = await screen.findByTestId("chrome-agents-control");
    await userEvent.click(control);
    expect(onOverlayOpening).toHaveBeenCalledTimes(1);
    await screen.findByTestId("agents-roster");

    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByTestId("agents-roster")).not.toBeInTheDocument(),
    );

    // A second activation of the control closes it too.
    await userEvent.click(control);
    await screen.findByTestId("agents-roster");
    await userEvent.click(control);
    await waitFor(() =>
      expect(screen.queryByTestId("agents-roster")).not.toBeInTheDocument(),
    );
  });

  it("marks an agent that is answering, and clears the marker when its turn ends", async () => {
    // AGT-FR-05.
    backend({
      projectAgents: [
        enrolled(agent({ id: "a1", nickname: "arch" })),
        enrolled(agent({ id: "a2", nickname: "sec" })),
      ],
      turns: [turn({ agentId: "a2" })],
    });
    render(<Roster onOverlayOpening={noop} onOpenAgentInSettings={noop} onOpenProjectSettings={noop} />);
    await userEvent.click(await screen.findByTestId("chrome-agents-control"));

    const marker = await screen.findByTestId("agents-roster-answering");
    expect(marker.closest("[data-testid='agents-roster-row']")).toHaveTextContent(
      "@sec",
    );

    // The event clears it without the user reopening anything.
    act(() =>
      emit("agent-turn-state-changed", {
        ...turn({ agentId: "a2" }),
        state: "delivered",
        endedAt: "2026-01-01T00:00:05Z",
      }),
    );
    await waitFor(() =>
      expect(screen.queryByTestId("agents-roster-answering")).not.toBeInTheDocument(),
    );
  });

  it("does not mark an agent that is waiting on the author", async () => {
    // AGT-FR-05's third clause / AGT-FR-05. `list_agent_turns` returns a turn
    // that is `awaiting_reply` (AGC-FR-22), but an agent waiting on a person is
    // not working — and the marker would never clear either, retirement
    // emitting no event (AGC-FR-29).
    backend({
      projectAgents: [
        enrolled(agent({ id: "a1", nickname: "arch" })),
        enrolled(agent({ id: "a2", nickname: "sec" })),
      ],
      turns: [
        {
          ...turn({ agentId: "a2" }),
          state: "awaiting_reply",
          endedAt: "2026-01-01T00:00:05Z",
        },
      ],
    });
    render(<Roster onOverlayOpening={noop} onOpenAgentInSettings={noop} onOpenProjectSettings={noop} />);
    await userEvent.click(await screen.findByTestId("chrome-agents-control"));

    await screen.findAllByTestId("agents-roster-row");
    expect(
      screen.queryByTestId("agents-roster-answering"),
    ).not.toBeInTheDocument();

    // And it stays unmarked when the roster is closed and reopened, which is
    // the path the initial read serves.
    await userEvent.click(screen.getByTestId("chrome-agents-control"));
    await userEvent.click(screen.getByTestId("chrome-agents-control"));
    await screen.findAllByTestId("agents-roster-row");
    expect(
      screen.queryByTestId("agents-roster-answering"),
    ).not.toBeInTheDocument();
  });

  it("sends a row and the add action to the Global settings editor", async () => {
    // AGT-FR-07 / AGT-FR-06, FR-07. The roster performs no edit itself.
    backend({ projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))] });
    const onOpenAgentInSettings = vi.fn();
    render(<Roster onOverlayOpening={noop} onOpenAgentInSettings={onOpenAgentInSettings} onOpenProjectSettings={noop} />);
    await userEvent.click(await screen.findByTestId("chrome-agents-control"));
    await userEvent.click(await screen.findByTestId("agents-roster-row"));
    expect(onOpenAgentInSettings).toHaveBeenCalledWith("a1");
    // The roster closed rather than staying open behind the tab.
    expect(screen.queryByTestId("agents-roster")).not.toBeInTheDocument();

    await userEvent.click(screen.getByTestId("chrome-agents-control"));
    await userEvent.click(await screen.findByTestId("agents-roster-add"));
    expect(onOpenAgentInSettings).toHaveBeenLastCalledWith(null);
  });

  it("renders a first-class empty state for a project enrolling nobody", async () => {
    // AGT-FR-31 / AGT-FR-08, FR-31. Still a control, still an add action.
    backend({ projectAgents: [] });
    const onOpenProjectSettings = vi.fn();
    render(<Roster onOverlayOpening={noop} onOpenAgentInSettings={noop} onOpenProjectSettings={onOpenProjectSettings} />);
    const control = await screen.findByTestId("chrome-agents-control");
    await waitFor(() => expect(control).toHaveTextContent("0"));
    await userEvent.click(control);

    const empty = await screen.findByTestId("agents-roster-empty");
    expect(empty).toHaveTextContent(/Project settings/);
    expect(screen.getByTestId("agents-roster-add")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    await userEvent.click(within(empty).getByRole("button"));
    expect(onOpenProjectSettings).toHaveBeenCalled();
  });

  it("says a roster could not be read instead of advising an enrolment", async () => {
    // The empty state is advice — "enrol one in Project settings" — and advice
    // is wrong when the read failed: an author who has already enrolled an
    // agent is told to go and do the thing they did, with nothing naming the
    // real problem. Distinguishable, because the two call for different actions.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_project_agents") throw "no_project_open";
      if (cmd === "list_agent_turns") return [];
      return [];
    });
    render(<Roster />);
    const control = await screen.findByTestId("chrome-agents-control");
    await userEvent.click(control);

    const shown = await screen.findByTestId("agents-roster-error");
    expect(screen.queryByTestId("agents-roster-empty")).not.toBeInTheDocument();
    // It has to say something. A bare marker would satisfy the two assertions
    // above while telling the author nothing, and the whole point here is that
    // the empty state's advice was the wrong thing to say.
    expect(shown.textContent?.trim()).not.toBe("");
    expect(shown).not.toHaveTextContent(/Project settings/);
  });

  it("replaces a failed read's error with the roster once it succeeds", async () => {
    // An error that outlives its cause is the same defect from the other side:
    // the author fixes the thing and the roster goes on accusing them of it.
    let fail = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_project_agents") {
        if (fail) throw "no_project_open";
        return [enrolled(agent({ id: "a1", nickname: "arch" }))];
      }
      if (cmd === "list_agent_turns") return [];
      return [];
    });
    render(<Roster />);
    const control = await screen.findByTestId("chrome-agents-control");
    await userEvent.click(control);
    await screen.findByTestId("agents-roster-error");

    // Closed, the cause corrected, re-opened — the roster's own re-read path.
    await userEvent.click(control);
    fail = false;
    await userEvent.click(control);

    await screen.findByTestId("agents-roster-row");
    expect(screen.queryByTestId("agents-roster-error")).not.toBeInTheDocument();
    await waitFor(() => expect(control).toHaveTextContent("1"));
  });

  it("does not leave a previous read's rows standing beneath the error", async () => {
    // The count beside the control and the rows inside it describe one moment.
    // Left alone on failure they describe the previous one, so the roster reads
    // as three working agents with an error floating above them.
    let fail = false;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_project_agents") {
        if (fail) throw "no_project_open";
        return [enrolled(agent({ id: "a1", nickname: "arch" }))];
      }
      if (cmd === "list_agent_turns") return [];
      return [];
    });
    render(<Roster />);
    const control = await screen.findByTestId("chrome-agents-control");
    await userEvent.click(control);
    await screen.findByTestId("agents-roster-row");

    await userEvent.click(control);
    fail = true;
    await userEvent.click(control);

    await screen.findByTestId("agents-roster-error");
    expect(screen.queryByTestId("agents-roster-row")).not.toBeInTheDocument();
    await waitFor(() => expect(control).toHaveTextContent("0"));
  });

  it("re-reads its count when an agent is enrolled from another surface", async () => {
    // The roster's count is a fact about the open project, and it changes from
    // surfaces this control cannot see. Read only on mount, it goes on
    // reporting a number that is no longer true — which is exactly what an
    // author sees after enrolling an agent with the roster already on screen.
    const b = backend({
      agents: [agent({ id: "a1", nickname: "arch" })],
      projectAgents: [],
    });
    render(
      <>
        <Roster />
        <ProjectAgents />
      </>,
    );
    const control = await screen.findByTestId("chrome-agents-control");
    await waitFor(() => expect(control).toHaveTextContent("0"));

    // Enrolled from the Project settings section, which the roster cannot see.
    await userEvent.click(await screen.findByTestId("project-agent-add"));
    await userEvent.click(
      (await screen.findAllByTestId("project-agent-option"))[0],
    );
    await waitFor(() => expect(calls(b, "enrol_project_agent")).toHaveLength(1));

    await waitFor(() => expect(control).toHaveTextContent("1"));
    await userEvent.click(control);
    expect(await screen.findByTestId("agents-roster-row")).toHaveTextContent(
      "@arch",
    );
    expect(screen.queryByTestId("agents-roster-empty")).not.toBeInTheDocument();
  });

  it("carries an icon that names what the control is for", async () => {
    // AGT-FR-02: the chrome control is one of a row of unlabelled icon buttons,
    // so its icon is the only thing that says what it opens.
    backend({ projectAgents: [] });
    render(<Roster />);
    const control = await screen.findByTestId("chrome-agents-control");
    expect(control).toHaveAccessibleName(/Agents/);
    expect(control.querySelector("svg")).toBeInTheDocument();
  });

  it("dispatches, cancels, and renders nothing about a conversation", async () => {
    // AGT-FR-32.
    const b = backend({
      projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))],
      turns: [turn({ agentId: "a1" })],
    });
    render(<Roster onOverlayOpening={noop} onOpenAgentInSettings={noop} onOpenProjectSettings={noop} />);
    await userEvent.click(await screen.findByTestId("chrome-agents-control"));
    await screen.findByTestId("agents-roster-answering");
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
    expect(calls(b, "cancel_agent_turn")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Pure rules
// ---------------------------------------------------------------------------

describe("the rules the surfaces share", () => {
  it("AGT-FR-10, AGT-FR-22, AGT-FR-26: names each degraded state and where it is corrected", () => {
    expect(availabilityNote("ready")).toBe("");
    for (const state of ["provider_unconfigured", "provider_unverified"] as const) {
      expect(availabilityNote(state)).toMatch(/in Global settings → AI API\./);
    }
    // The model is chosen in the editor, and no note speaks of "this provider".
    expect(availabilityNote("model_unavailable")).toBe(
      "The active AI API provider does not offer this model. Choose another model in the editor.",
    );
    for (const state of [
      "provider_unconfigured",
      "provider_unverified",
      "model_unavailable",
    ] as const) {
      expect(availabilityNote(state)).not.toMatch(/this provider|this agent's provider/i);
    }
  });

  it("gates a nickname client-side on exactly the backend's rule", () => {
    // AGT-FR-13, mirroring AGR-FR-04 — including its length bound, without
    // which the confirm action would make a call the backend refuses.
    expect(nicknameProblem("arch")).toBe("");
    expect(nicknameProblem("")).toMatch(/required/);
    expect(nicknameProblem("two words")).toMatch(/spaces/);
    expect(nicknameProblem("a@b")).toMatch(/@/);
    expect(nicknameProblem("a".repeat(64))).toBe("");
    expect(nicknameProblem("a".repeat(65))).toMatch(/64/);
    // AGT-FR-41 / AGR-FR-04: the reserved handle, in any case. Stated rather
    // than merely refused — AGT-FR-13 has the field say why the name is taken.
    for (const written of ["all", "ALL", "All"]) {
      expect(nicknameProblem(written)).toMatch(/every agent/);
    }
    // Nothing merely *containing* it is reserved.
    expect(nicknameProblem("allan")).toBe("");
    expect(nicknameProblem("call")).toBe("");
  });

  it("clears the shared roster on mount, so a project switch cannot bold the wrong one", async () => {
    // The roster the chrome publishes for its peers (AGT-FR-29 via CMP-FR-18)
    // lives at module scope, so it outlives the remount `App` performs when the
    // author switches project — it keys that whole subtree by project path.
    // Without the clear, the newly-mounted Comments panel reads the *previous*
    // project's enrolment for one IPC round trip and bolds a tag naming an agent
    // this project never enrolled: the exact failure the empty-until-read
    // behaviour exists to prevent, and one no amount of waiting reveals because
    // it resolves itself a moment later.
    publishProjectAgents([enrolled(agent({ id: "old", nickname: "leftover" }))]);
    expect(readProjectAgents()).toHaveLength(1);

    // A read that never resolves, standing in for the IPC round trip's duration.
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_project_agents") return new Promise(() => {});
      return Promise.resolve([]);
    });
    render(<Roster />);

    // Cleared before the read was issued, not after it came back.
    await waitFor(() => expect(readProjectAgents()).toEqual([]));
  });

  it("does not clear the shared roster when an agent changes in this project", async () => {
    // A revision bump means something changed in the project we are *in*, so
    // clearing would unbold every tag on screen for one round trip to no purpose.
    const b = backend({
      projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))],
    });
    render(<Roster />);
    await waitFor(() =>
      expect(readProjectAgents().map((a) => a.agent.nickname)).toEqual(["arch"]),
    );

    // Nothing enrolled or withdrawn: the same roster comes back.
    b.projectAgents = [enrolled(agent({ id: "a1", nickname: "arch" }))];
    act(() => notifyAgentRegistryChanged());
    // Never observed empty in between — the reference is even unchanged, the
    // re-read describing the same enrolment (`sameRoster`).
    expect(readProjectAgents().map((a) => a.agent.nickname)).toEqual(["arch"]);
    await waitFor(() =>
      expect(readProjectAgents().map((a) => a.agent.nickname)).toEqual(["arch"]),
    );
  });

  it("carries a message for the reserved nickname the backend refuses", () => {
    // AGT-FR-13, AGT-FR-19, AGR-FR-04's last clause / AGT-FR-41. The field gates it client-side, so
    // this path is only reached by a route that did not pass the field — but the
    // vocabulary has to name it, or the editor would render the raw slug.
    expect(agentErrorMessage("nickname_reserved")).toMatch(/every agent/);
    // And the neighbouring refusals still say their own thing.
    expect(agentErrorMessage("nickname_taken")).toMatch(/already answers/);
    expect(agentErrorMessage("nickname_invalid")).toMatch(/spaces or @/);
  });

  it("matches only a nickname the fragment begins, and ranks by nickname", () => {
    // AGT-FR-33. Deliberately unsorted input: the ordering is a property of this
    // function rather than of whatever order the roster arrived in, because
    // AGT-FR-27 completes the *first* entry on Tab.
    const agents = [
      enrolled(agent({ id: "a1", nickname: "Scribe" })),
      enrolled(agent({ id: "a2", nickname: "discuss" })),
      enrolled(agent({ id: "a3", nickname: "scout" })),
      enrolled(agent({ id: "a4", nickname: "sec" })),
    ];

    // `sc` keeps the two nicknames it begins, ordered without regard to case —
    // and drops `@discuss`, which merely contains it, and `@sec`, which does not.
    expect(matchingAgents(agents, "sc").map((a) => a.agent.nickname)).toEqual([
      "scout",
      "Scribe",
    ]);
    // Matching disregards case in both directions.
    expect(matchingAgents(agents, "SCR").map((a) => a.agent.nickname)).toEqual([
      "Scribe",
    ]);
    // An empty fragment offers every enrolled agent, still ranked.
    expect(matchingAgents(agents, "").map((a) => a.agent.nickname)).toEqual([
      "discuss",
      "scout",
      "Scribe",
      "sec",
    ]);
  });

  it("leaves the caller's roster order untouched", () => {
    // `matchingAgents` sorts; a component that re-ranks its own props in place
    // would corrupt the roster every other surface reads.
    const agents = [
      enrolled(agent({ id: "a1", nickname: "zed" })),
      enrolled(agent({ id: "a2", nickname: "abe" })),
    ];
    matchingAgents(agents, "");
    expect(agents.map((a) => a.agent.nickname)).toEqual(["zed", "abe"]);
  });

  it("AGR-FR-16, AAP-FR-APRV: reads no surface's model labels through the credential listing", async () => {
    // AGR-FR-16: every surface here renders with the keychain shut.
    // `list_ai_api_integrations` probes the keychain per record, so a surface
    // calling it for a model *label* makes listing personas depend on a key
    // being readable — and, on a machine whose keychain asks first, puts a
    // system prompt in front of an author who opened a list of names.
    //
    // Asserted over every surface at once, because the cost of one of them
    // drifting back is invisible in jsdom: the mock answers either command.
    const b = backend({
      agents: [agent({ id: "a1", nickname: "arch" })],
      projectAgents: [enrolled(agent({ id: "a1", nickname: "arch" }))],
    });
    render(
      <>
        <Roster />
        <GlobalAgents />
        <ProjectAgents />
      </>,
    );
    await screen.findByTestId("agent-row");
    await userEvent.click(await screen.findByTestId("chrome-agents-control"));
    await screen.findByTestId("agents-roster-row");

    expect(calls(b, "get_active_ai_api_catalog").length).toBeGreaterThan(0);
    expect(calls(b, "list_ai_api_integrations")).toHaveLength(0);
  });
});
