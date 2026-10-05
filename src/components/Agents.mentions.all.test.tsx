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

  // -- the `@all` handle (AGT-FR-35 … AGT-FR-40) --------------------------

  it("leads the picker with @all, naming the agents it stands for", async () => {
    // AGT-FR-35, AGT-FR-40, AGT-FR-33 / AGT-FR-39: the handle leads every nickname however they sort —
    // `@arch` ranks first among them and still follows it — and AGT-FR-40 has the
    // completion insert the handle itself rather than the names.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "arch" })),
          enrolled(agent({ id: "a2", nickname: "scout" })),
          enrolled(agent({ id: "a3", nickname: "scribe" })),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");

    const options = await screen.findAllByTestId("mention-picker-option");
    expect(options[0]).toHaveAttribute("data-entry", "all");
    expect(options[0]).toHaveTextContent("@all");
    // AGT-FR-39: the names it will tag, where a nickname's row carries its model.
    expect(options[0]).toHaveTextContent("@arch, @scout, @scribe");
    expect(options[1]).toHaveTextContent("@arch");
    // It is the top match, so Tab completes it.
    expect(options[0]).toHaveAttribute("data-highlighted", "true");

    await userEvent.keyboard("{Tab}");
    // The handle goes in as itself: no nickname anywhere in the composer.
    await waitFor(() => expect(composer).toHaveValue("@all "));
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("offers @all on a bare sigil and on every prefix of the word", async () => {
    // AGT-FR-39: an empty fragment is a prefix of `all` like any other, and the
    // handle drops out as soon as the fragment stops being one.
    backend();
    render(<Composer agents={[enrolled(agent({ id: "a1", nickname: "arch" }))]} />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);

    // Uppercase among them: the fragment is matched without regard to case, as
    // every other tag is (AGT-FR-24).
    for (const fragment of ["", "a", "al", "all", "AL", "ALL", "Al"]) {
      await userEvent.clear(composer);
      await userEvent.type(composer, `@${fragment}`);
      const options = await screen.findAllByTestId("mention-picker-option");
      expect(options[0]).toHaveAttribute("data-entry", "all");
    }

    // `@ar` is no prefix of `all`, so only the nickname is offered.
    await userEvent.clear(composer);
    await userEvent.type(composer, "@ar");
    await waitFor(() => {
      const options = screen.getAllByTestId("mention-picker-option");
      expect(options).toHaveLength(1);
      expect(options[0]).toHaveAttribute("data-entry", "agent");
    });
  });

  it("names only the agents that can answer, and refuses when none can", async () => {
    // AGT-FR-39, AGT-FR-26 / AGT-FR-36: a degraded agent is left out of what the handle
    // stands for, because a dispatch there would read as its own silence.
    backend();
    const { unmount } = render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "arch" })),
          enrolled(agent({ id: "a2", nickname: "scout" })),
          enrolled(agent({ id: "a3", nickname: "scribe" }), "provider_unverified"),
        ]}
      />,
    );
    let composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");
    let options = await screen.findAllByTestId("mention-picker-option");
    expect(options[0]).toHaveTextContent("@arch, @scout");
    expect(options[0]).not.toHaveTextContent("@scribe");
    expect(options[0]).not.toBeDisabled();
    unmount();

    // AGT-FR-38: with every enrolled agent degraded the handle resolves to
    // nobody, so it is still offered first with the reason stated and completed
    // by no key — an author reaching for it learns why rather than not finding it.
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "arch" }), "provider_unverified"),
        ]}
      />,
    );
    composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");
    options = await screen.findAllByTestId("mention-picker-option");
    expect(options[0]).toHaveAttribute("data-entry", "all");
    expect(options[0]).toHaveTextContent(/can answer/);
    expect(options[0]).toBeDisabled();
    expect(options[0]).toHaveAttribute("data-highlighted", "false");

    await userEvent.keyboard("{Tab}");
    expect(composer).toHaveValue("@a");
  });

  it("never yields the tag's own width to the text describing it", async () => {
    // AGT-FR-39: the row has to legibly say what Tab is about to insert. Both
    // spans shrinking together — the flex default — splits the shortfall
    // proportionally, which cut `@all` to `@a…` beside a long roster summary that
    // kept most of its own width. Asserted as the style contract because jsdom
    // lays nothing out; the visual result was confirmed in a browser.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "arch" })),
          enrolled(
            agent({ id: "a2", nickname: "specification-reviewer-extraordinaire" }),
          ),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");

    const options = await screen.findAllByTestId("mention-picker-option");
    for (const option of options) {
      const [tag, describing] = Array.from(option.querySelectorAll("span"));
      // The tag does not shrink…
      expect(tag.style.flexShrink).toBe("0");
      // …but is capped, so a nickname long enough to eat the row truncates too.
      expect(tag.style.maxWidth).toBe("70%");
      // The describing span is what gives, which needs a zero min-width to be
      // allowed to shrink below its content at all.
      expect(describing.style.minWidth).toBe("0px");
    }
  });

  it("elides the roster on the @all row when it does not fit the line", async () => {
    // AGT-FR-39: bounded to the one line every entry occupies, with a count
    // standing in for the rest — the picker is bounded by the composer it is
    // anchored to, and a dozen nicknames would push the row past it.
    backend();
    render(
      <Composer
        agents={["ant", "arch", "aide", "scout", "scribe"].map((nickname, i) =>
          enrolled(agent({ id: `a${i}`, nickname })),
        )}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");
    const options = await screen.findAllByTestId("mention-picker-option");
    expect(options[0]).toHaveTextContent("@aide, @ant, @arch, +2");
  });

  it("says so when a filter matches nothing", async () => {
    // AGT-FR-34's last clause / AGT-FR-27.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.type(composer, "@zzz");
    expect(await screen.findByTestId("mention-picker-empty")).toHaveTextContent(
      "zzz",
    );
  });

  it("renders no picker at all in a project enrolling nobody", async () => {
    backend();
    render(<Composer agents={[]} />);
    await userEvent.type(screen.getByTestId("composer"), "@arch");
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("opens only on an @ that begins a word", async () => {
    // AGT-FR-24, AGT-FR-28 / AGT-FR-25. The whole point of the boundary is that an author
    // writing an email address is not interrupted.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);

    await userEvent.type(composer, "mail me at me@");
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
    await userEvent.type(composer, "example.com");
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();

    // Whitespace, an opening bracket and a dash each begin a word.
    await userEvent.type(composer, " (@ar");
    expect(await screen.findByTestId("mention-picker")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await userEvent.type(composer, "x —@ar");
    expect(await screen.findByTestId("mention-picker")).toBeInTheDocument();
  });

  it("completes the top match on Tab without the author touching the arrows", async () => {
    // AGT-FR-27, AGT-FR-33.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "scribe" })),
          enrolled(agent({ id: "a2", nickname: "scout" })),
          enrolled(agent({ id: "a3", nickname: "discuss" })),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @sc");

    // `@discuss` merely contains `sc` and is therefore not offered at all.
    const options = await screen.findAllByTestId("mention-picker-option");
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("@scout"),
      expect.stringContaining("@scribe"),
    ]);

    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("ask @scout "));
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();
  });

  it("moves the top match with the fragment", async () => {
    // AGT-FR-27, AGT-FR-33's last clause: the highlight returns to the top match whenever
    // the fragment changes, so Tab follows what the author has narrowed to.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "scribe" })),
          enrolled(agent({ id: "a2", nickname: "scout" })),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @scr");
    await screen.findByTestId("mention-picker");
    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("ask @scribe "));
  });

  it("returns the highlight to the top match when the fragment narrows under it", async () => {
    // AGT-FR-27. The discriminating case for the reset: the author has arrowed
    // the highlight onto the *second* entry and then types a character that
    // drops it from the list. Without the reset the stale index points past the
    // end of what is showing and Tab silently completes nothing.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "aide" })),
          enrolled(agent({ id: "a2", nickname: "ant" })),
          enrolled(agent({ id: "a3", nickname: "arch" })),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@a");
    await screen.findByTestId("mention-picker");

    // The handle leads (AGT-FR-39) and the highlight starts on it, so one step
    // down reaches the *first nickname* — `@aide`. Asserted by nickname rather
    // than by row index, which the handle's arrival would otherwise shift under
    // the test without changing what it appears to say.
    await userEvent.keyboard("{ArrowDown}");
    await waitFor(() => {
      const highlighted = screen
        .getAllByTestId("mention-picker-option")
        .find((o) => o.dataset.highlighted === "true");
      expect(highlighted).toHaveTextContent("@aide");
    });
    // Two more steps put it on `@arch`, the third nickname — the highlight really
    // is walking the list rather than sitting where it started.
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    await waitFor(() => {
      const highlighted = screen
        .getAllByTestId("mention-picker-option")
        .find((o) => o.dataset.highlighted === "true");
      expect(highlighted).toHaveTextContent("@arch");
    });

    // Typing `r` leaves only `@arch`, so the highlight must come back to the top.
    await userEvent.type(composer, "r");
    await waitFor(() =>
      expect(screen.getAllByTestId("mention-picker-option")).toHaveLength(1),
    );
    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("@arch "));
  });

  it("completes the entry the arrows highlighted, agreeing with Enter", async () => {
    // AGT-FR-34 / AGT-FR-27: Tab and Enter never disagree about what the list
    // shows highlighted.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "ask @");
    await screen.findByTestId("mention-picker");
    // Two steps down from the leading `@all` entry (AGT-FR-39) reaches `@scribe`,
    // and Tab completes exactly what the list shows highlighted.
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Tab}");
    await waitFor(() => expect(composer).toHaveValue("ask @scribe "));
  });

  it("lets Tab do its ordinary thing when there is nothing to complete", async () => {
    // AGT-FR-33, AGT-FR-34 / AGT-FR-27. A key with nothing to complete must not strand the
    // author's focus, so it is not prevented and moves focus onward.
    backend();
    render(
      <>
        <Composer />
        <button data-testid="after">after</button>
      </>,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@zzz");
    // The picker is still showing — it says there is nothing there.
    expect(await screen.findByTestId("mention-picker-empty")).toBeInTheDocument();

    await userEvent.keyboard("{Tab}");
    expect(composer).toHaveValue("@zzz");
    expect(screen.getByTestId("after")).toHaveFocus();
  });

  it("refuses Tab on an agent that cannot answer", async () => {
    // AGT-FR-27, AGT-FR-33 / AGT-FR-26: the highlight passes over it and no key completes
    // it, so Tab falls through rather than inserting a tag that reaches nobody.
    backend();
    render(
      <>
        <Composer
          agents={[enrolled(agent({ id: "a1", nickname: "scribe" }), "provider_unverified")]}
        />
        <button data-testid="after">after</button>
      </>,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@scr");
    const option = await screen.findByTestId("mention-picker-option");
    expect(option).toBeDisabled();
    expect(option).toHaveAttribute("data-highlighted", "false");

    await userEvent.keyboard("{Tab}");
    expect(composer).toHaveValue("@scr");
    expect(screen.getByTestId("after")).toHaveFocus();
  });

  it("skips the degraded top match and completes the next one on Tab", async () => {
    // AGT-FR-27 / AGT-FR-26, AGT-FR-33: `@scribe` ranks first but cannot answer,
    // so the top *selectable* match is what the highlight rests on.
    backend();
    render(
      <Composer
        agents={[
          enrolled(agent({ id: "a1", nickname: "scrub" })),
          enrolled(agent({ id: "a2", nickname: "scribe" }), "provider_unverified"),
        ]}
      />,
    );
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@scr");

    const options = await screen.findAllByTestId("mention-picker-option");
    expect(options.map((o) => o.textContent)).toEqual([
      expect.stringContaining("@scribe"),
      expect.stringContaining("@scrub"),
    ]);
    expect(options[0]).toHaveAttribute("data-highlighted", "false");
    expect(options[1]).toHaveAttribute("data-highlighted", "true");

    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("@scrub "));
  });

  it("restores the picker when the author deletes back into the fragment", async () => {
    // AGT-FR-27, AGT-FR-33 / AGT-FR-34: a mistyped nickname is corrected without the
    // sigil being retyped.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@scx");
    expect(await screen.findByTestId("mention-picker-empty")).toBeInTheDocument();

    await userEvent.keyboard("{Backspace}");
    await waitFor(() =>
      expect(screen.getAllByTestId("mention-picker-option")).toHaveLength(1),
    );
    expect(screen.queryByTestId("mention-picker-empty")).not.toBeInTheDocument();
  });

  it("closes on whitespace, leaving what was typed as ordinary characters", async () => {
    // AGT-FR-27, AGT-FR-33 / AGT-FR-34: a nickname carries no whitespace, so the word being
    // typed is no longer a tag.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@scx");
    await screen.findByTestId("mention-picker-empty");

    await userEvent.type(composer, " ");
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );
    expect(composer).toHaveValue("@scx ");
  });

  it("spends an Escape dismissal once the caret leaves the candidate tag", async () => {
    // AGT-FR-34: the dismissal holds while the caret is still in that tag, and
    // is spent when it leaves — a sigil typed at the same offset later must not
    // find the picker permanently poisoned.
    backend();
    render(<Composer />);
    const composer = screen.getByTestId("composer");
    await userEvent.click(composer);
    await userEvent.type(composer, "@ar");
    await screen.findByTestId("mention-picker");

    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument(),
    );
    // Still inside the dismissed tag: typing more of the nickname keeps it shut.
    await userEvent.type(composer, "c");
    expect(screen.queryByTestId("mention-picker")).not.toBeInTheDocument();

    // Clearing the field takes the caret out of that candidate tag, which spends
    // the dismissal — the same offset now opens the picker again.
    await userEvent.clear(composer);
    await userEvent.type(composer, "@ar");
    expect(await screen.findByTestId("mention-picker")).toBeInTheDocument();
  });
});
