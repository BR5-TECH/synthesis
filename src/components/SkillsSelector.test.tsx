// The Project skills selector of the persona editor —
// `specifications/ui/AGT-agents.md` AGT-FR-KVDA, AGT-FR-XGWD, AGT-FR-LCMT,
// AGT-FR-WTSR, and the seventh control of AGT-FR-12.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAgentRegistry } from "../state/agentRegistry";
import { resetLogBufferForTest } from "../logging";
import { PersonalAgents, ProjectAgents } from "./Agents";
import { BUILTIN_SKILLS_NOTE, selectorEntries } from "./ProjectSkillsSelector";
import type { Agent, AgentDraft, AiApiCatalog, ProjectAgent, ProjectSkill } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => undefined,
}));

const SKILLS: ProjectSkill[] = [
  {
    path: ".claude/skills/analyst/SKILL.md",
    ecosystem: "claude",
    name: "analyst",
    description: "Capture specifications.",
  },
  {
    path: ".codex/skills/review/SKILL.md",
    ecosystem: "codex",
    name: "review",
    description: "Review a change.",
  },
];

const CATALOG: AiApiCatalog = {
  provider: "openrouter",
  displayName: "OpenRouter",
  state: "verified",
  models: [{ id: "m", label: "Model M" }],
};

function agent(over: Partial<Agent> & { id: string; nickname: string }): Agent {
  return {
    title: "",
    modelId: "m",
    instructions: "",
    createdAt: "",
    updatedAt: "",
    reasoning: null,
    ...over,
  };
}

interface Backend {
  projectAgents: ProjectAgent[];
  personal: Agent[];
  /** What `list_project_skills` answers, or an error code it rejects with. */
  skills: ProjectSkill[] | string;
  calls: { cmd: string; args: unknown }[];
}

function backend(over: Partial<Backend> = {}): Backend {
  const b: Backend = { projectAgents: [], personal: [], skills: SKILLS, calls: [], ...over };
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "list_project_agents":
        return Promise.resolve(b.projectAgents);
      case "list_personal_agents":
        return Promise.resolve(
          b.personal.map((a) => ({ agent: a, scope: "personal", availability: "ready" })),
        );
      case "get_active_ai_api_catalog":
        return Promise.resolve({ resolution: "inherited", catalog: CATALOG });
      case "list_agent_turns":
        return Promise.resolve([]);
      case "list_voice_ai_integrations":
        return Promise.resolve({ active: null, integrations: [] });
      case "list_voice_ai_voices":
        return Promise.resolve(null);
      case "list_project_skills":
        return typeof b.skills === "string"
          ? Promise.reject(b.skills)
          : Promise.resolve(b.skills);
      case "create_agent":
      case "update_agent": {
        const { draft, id } = args as { draft: AgentDraft; id?: string };
        return Promise.resolve(agent({ ...draft, id: id ?? "new" }));
      }
      default:
        return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    }
  });
  return b;
}

const calls = (b: Backend, cmd: string) => b.calls.filter((c) => c.cmd === cmd);

beforeEach(() => {
  invokeMock.mockReset();
  resetAgentRegistry();
  resetLogBufferForTest();
});
afterEach(cleanup);

async function openNewEditor() {
  await userEvent.click(await screen.findByTestId("agent-create"));
  return screen.findByTestId("agent-editor");
}

async function fillRequired(editor: HTMLElement) {
  await userEvent.type(within(editor).getByTestId("agent-nickname"), "arch");
  await userEvent.click(within(editor).getByTestId("agent-model"));
  await userEvent.click(await screen.findByText("Model M"));
}

describe("the Project skills selector", () => {
  it("AGT-FR-12, AGT-FR-KVDA: a Project agent's editor lists every project skill as a seventh control, after the voice", async () => {
    const b = backend();
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    const options = await within(editor).findAllByTestId("agent-skill-option");
    expect(options.map((o) => o.getAttribute("data-path"))).toEqual(SKILLS.map((s) => s.path));
    expect(options[0]).toHaveTextContent("analyst");
    expect(options[0]).toHaveTextContent(".claude/skills/analyst/SKILL.md");
    expect(within(editor).getByTestId("agent-skills-builtin-note")).toHaveTextContent(
      BUILTIN_SKILLS_NOTE,
    );
    expect(editor).not.toHaveTextContent("mermaid");
    expect(calls(b, "list_project_skills")).toHaveLength(1);

    const order = Array.from(editor.querySelectorAll("[data-testid^='agent-']")).map((e) =>
      e.getAttribute("data-testid"),
    );
    expect(order.indexOf("agent-voice")).toBeLessThan(order.indexOf("agent-skills"));
    expect(within(editor).getByRole("group", { name: "Project skills" })).toBeInTheDocument();
  });

  it("AGT-FR-XGWD, AGT-FR-19: a new agent selects nothing, and the confirm carries what the author selects", async () => {
    const b = backend();
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    const boxes = await within(editor).findAllByRole("checkbox");
    expect(boxes.every((box) => !(box as HTMLInputElement).checked)).toBe(true);

    await fillRequired(editor);
    await userEvent.click(boxes[1]);
    // Selecting writes nothing until the confirm.
    expect(calls(b, "create_agent")).toHaveLength(0);
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { scope, draft } = calls(b, "create_agent")[0].args as {
      scope: string;
      draft: AgentDraft;
    };
    expect(scope).toBe("project");
    expect(draft.skills).toEqual([".codex/skills/review/SKILL.md"]);
  });

  it("AGT-FR-XGWD: an existing agent opens on its selection, and a stored path the project lacks stays listed and can be cleared", async () => {
    const stale = ".opencode/skills/gone/SKILL.md";
    const b = backend({
      projectAgents: [
        {
          agent: agent({
            id: "a1",
            nickname: "arch",
            skills: [".claude/skills/analyst/SKILL.md", stale],
          }),
          scope: "project",
          availability: "ready",
        },
      ],
    });
    render(<ProjectAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    const options = await within(editor).findAllByTestId("agent-skill-option");
    expect(options.map((o) => o.getAttribute("data-path"))).toEqual([...SKILLS.map((s) => s.path), stale]);
    const checked = options.map((o) => (within(o).getByRole("checkbox") as HTMLInputElement).checked);
    expect(checked).toEqual([true, false, true]);
    expect(within(options[2]).getByTestId("agent-skill-missing")).toHaveTextContent(
      "Not found in this worktree.",
    );

    await userEvent.click(within(options[2]).getByRole("checkbox"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: AgentDraft };
    expect(draft.skills).toEqual([".claude/skills/analyst/SKILL.md"]);
  });

  it("AGT-FR-LCMT: a project with no skill renders an empty state naming the four folders", async () => {
    backend({ skills: [] });
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    const empty = await within(editor).findByTestId("agent-skills-empty");
    for (const folder of [".claude/skills", ".codex/skills", ".github/skills", ".opencode/skills"]) {
      expect(empty).toHaveTextContent(folder);
    }
    expect(within(editor).queryByTestId("agent-skills")).not.toBeInTheDocument();
  });

  it("AGT-FR-LCMT: a failed read keeps the stored selection and the confirm enabled", async () => {
    const b = backend({
      skills: "no_project_open",
      projectAgents: [
        {
          agent: agent({ id: "a1", nickname: "arch", skills: [".claude/skills/analyst/SKILL.md"] }),
          scope: "project",
          availability: "ready",
        },
      ],
    });
    render(<ProjectAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    expect(await within(editor).findByTestId("agent-skills-error")).toHaveTextContent(
      "The project skills could not be read",
    );
    expect(within(editor).getByTestId("agent-editor-confirm")).toBeEnabled();
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: AgentDraft };
    expect(draft.skills).toEqual([".claude/skills/analyst/SKILL.md"]);
  });

  it("AGT-FR-WTSR, AGT-FR-12: a Personal agent's editor has no selector, reads no skill, and sends an empty list", async () => {
    const b = backend();
    render(<PersonalAgents />);
    const editor = await openNewEditor();
    await fillRequired(editor);
    expect(within(editor).queryByTestId("agent-skills")).not.toBeInTheDocument();
    expect(within(editor).queryByTestId("agent-skills-builtin-note")).not.toBeInTheDocument();
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: AgentDraft };
    expect(draft.skills).toEqual([]);
    expect(calls(b, "list_project_skills")).toHaveLength(0);
  });

  it("AGT-FR-XGWD: the entries are the registry's skills, then the stored paths it lacks", () => {
    expect(selectorEntries(SKILLS, ["x/SKILL.md", SKILLS[0].path])).toEqual([
      { path: SKILLS[0].path, name: "analyst" },
      { path: SKILLS[1].path, name: "review" },
      { path: "x/SKILL.md", name: null },
    ]);
  });
});
