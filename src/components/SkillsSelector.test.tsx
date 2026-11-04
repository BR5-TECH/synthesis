// The Skills selector of the persona editor —
// `specifications/ui/AGT-agents.md` AGT-FR-KVDA, AGT-FR-PQSM, AGT-FR-JRWE,
// AGT-FR-XGWD, AGT-FR-LCMT, AGT-FR-WTSR, and the seventh control of AGT-FR-12.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAgentRegistry } from "../state/agentRegistry";
import { resetLogBufferForTest } from "../logging";
import { PersonalAgents, ProjectAgents } from "./Agents";
import {
  BUILTIN_SKILLS_NOTE,
  selectorEntries,
  synthesisEntries,
  toggledOptOuts,
} from "./SkillsSelector";
import type { Agent, AgentDraft, AiApiCatalog, ProjectAgent, ProjectSkill } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => undefined,
}));

const GLOSSARY = ".synthesis/skills/glossary/SKILL.md";
const STYLE = ".synthesis/skills/style/SKILL.md";

const SYNTHESIS: ProjectSkill[] = [
  { path: GLOSSARY, ecosystem: "synthesis", name: "glossary", description: "Use the glossary." },
  { path: STYLE, ecosystem: "synthesis", name: "style", description: "Use the house style." },
];

const PROJECT: ProjectSkill[] = [
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

/** What `list_project_skills` answers: both kinds, in path order. */
const SKILLS: ProjectSkill[] = [...PROJECT, ...SYNTHESIS];

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

/** The options of one group of the open editor. */
function options(editor: HTMLElement, group: "agent-synthesis-skills" | "agent-project-skills") {
  return within(within(editor).getByTestId(group)).queryAllByTestId("agent-skill-option");
}

const checkedOf = (opts: HTMLElement[]) =>
  opts.map((o) => (within(o).getByRole("checkbox") as HTMLInputElement).checked);

const pathsOf = (opts: HTMLElement[]) => opts.map((o) => o.getAttribute("data-path"));

describe("the Skills selector", () => {
  it("AGT-FR-12, AGT-FR-KVDA: a Project agent's editor shows both groups as a seventh control, after the voice", async () => {
    const b = backend();
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    await within(editor).findByTestId("agent-synthesis-skills");
    expect(pathsOf(options(editor, "agent-synthesis-skills"))).toEqual([GLOSSARY, STYLE]);
    const project = options(editor, "agent-project-skills");
    expect(pathsOf(project)).toEqual(PROJECT.map((s) => s.path));
    expect(project[0]).toHaveTextContent("analyst");
    expect(project[0]).toHaveTextContent(".claude/skills/analyst/SKILL.md");
    expect(within(editor).getByTestId("agent-skills-builtin-note")).toHaveTextContent(
      BUILTIN_SKILLS_NOTE,
    );
    expect(editor).not.toHaveTextContent("mermaid");
    expect(calls(b, "list_project_skills")).toHaveLength(1);

    const order = Array.from(editor.querySelectorAll("[data-testid^='agent-']")).map((e) =>
      e.getAttribute("data-testid"),
    );
    expect(order.indexOf("agent-voice")).toBeLessThan(order.indexOf("agent-skills"));
    expect(order.indexOf("agent-synthesis-skills")).toBeLessThan(
      order.indexOf("agent-project-skills"),
    );
    expect(
      within(editor).getByRole("group", { name: "Synthesis skills — on by default" }),
    ).toBeInTheDocument();
    expect(
      within(editor).getByRole("group", { name: "Project skills — Project agents only" }),
    ).toBeInTheDocument();
  });

  it("AGT-FR-PQSM, AGT-FR-19: a new agent has every synthesis skill on, and the confirm carries the ones the author clears", async () => {
    const b = backend();
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    await within(editor).findByTestId("agent-synthesis-skills");
    const synthesis = options(editor, "agent-synthesis-skills");
    expect(checkedOf(synthesis)).toEqual([true, true]);

    await fillRequired(editor);
    await userEvent.click(within(synthesis[1]).getByRole("checkbox"));
    await userEvent.click(within(synthesis[0]).getByRole("checkbox"));
    await userEvent.click(within(synthesis[0]).getByRole("checkbox"));
    expect(checkedOf(options(editor, "agent-synthesis-skills"))).toEqual([true, false]);
    expect(calls(b, "create_agent")).toHaveLength(0);
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { draft } = calls(b, "create_agent")[0].args as { draft: AgentDraft };
    expect(draft.disabledSkills).toEqual([STYLE]);
    expect(draft.skills).toEqual([]);
  });

  it("AGT-FR-PQSM, AGT-FR-JRWE: an existing agent opens on its opt-outs, and a stored path this project lacks is not listed and is sent unchanged", async () => {
    const elsewhere = ".synthesis/skills/from-another-project/SKILL.md";
    const b = backend({
      personal: [agent({ id: "p1", nickname: "mine", disabledSkills: [elsewhere, GLOSSARY] })],
    });
    render(<PersonalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    await within(editor).findByTestId("agent-synthesis-skills");
    const synthesis = options(editor, "agent-synthesis-skills");
    expect(pathsOf(synthesis)).toEqual([GLOSSARY, STYLE]);
    expect(checkedOf(synthesis)).toEqual([false, true]);
    expect(editor).not.toHaveTextContent("from-another-project");

    await userEvent.click(within(synthesis[0]).getByRole("checkbox"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { scope, draft } = calls(b, "update_agent")[0].args as {
      scope: string;
      draft: AgentDraft;
    };
    expect(scope).toBe("personal");
    expect(draft.disabledSkills).toEqual([elsewhere]);
  });

  it("AGT-FR-XGWD, AGT-FR-19: a new Project agent selects no project skill, and the confirm carries what the author selects", async () => {
    const b = backend();
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    await within(editor).findByTestId("agent-project-skills");
    const project = options(editor, "agent-project-skills");
    expect(checkedOf(project)).toEqual([false, false]);

    await fillRequired(editor);
    await userEvent.click(within(project[1]).getByRole("checkbox"));
    expect(calls(b, "create_agent")).toHaveLength(0);
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { scope, draft } = calls(b, "create_agent")[0].args as {
      scope: string;
      draft: AgentDraft;
    };
    expect(scope).toBe("project");
    expect(draft.skills).toEqual([".codex/skills/review/SKILL.md"]);
    expect(draft.disabledSkills).toEqual([]);
  });

  it("AGT-FR-XGWD: an existing Project agent opens on its selection, and a stored path the project lacks stays listed and can be cleared", async () => {
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
    await within(editor).findByTestId("agent-project-skills");
    const project = options(editor, "agent-project-skills");
    expect(pathsOf(project)).toEqual([...PROJECT.map((s) => s.path), stale]);
    expect(checkedOf(project)).toEqual([true, false, true]);
    expect(within(project[2]).getByTestId("agent-skill-missing")).toHaveTextContent(
      "Not found in this worktree.",
    );

    await userEvent.click(within(project[2]).getByRole("checkbox"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: AgentDraft };
    expect(draft.skills).toEqual([".claude/skills/analyst/SKILL.md"]);
  });

  it("AGT-FR-LCMT: a project with no skill shows an empty state in each group, naming its folders", async () => {
    backend({ skills: [] });
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    const synthesisEmpty = await within(editor).findByTestId("agent-synthesis-skills-empty");
    expect(synthesisEmpty).toHaveTextContent(".synthesis/skills");
    const projectEmpty = within(editor).getByTestId("agent-skills-empty");
    for (const folder of [".claude/skills", ".codex/skills", ".github/skills", ".opencode/skills"]) {
      expect(projectEmpty).toHaveTextContent(folder);
    }
    expect(within(editor).queryAllByTestId("agent-skill-option")).toHaveLength(0);
  });

  it("AGT-FR-LCMT: a project with only project skills shows the synthesis empty state beside them", async () => {
    backend({ skills: PROJECT });
    render(<ProjectAgents />);
    const editor = await openNewEditor();
    expect(await within(editor).findByTestId("agent-synthesis-skills-empty")).toBeInTheDocument();
    expect(within(editor).queryByTestId("agent-skills-empty")).not.toBeInTheDocument();
    expect(options(editor, "agent-project-skills")).toHaveLength(2);
  });

  it("AGT-FR-LCMT: a failed read keeps both stored lists and the confirm enabled", async () => {
    const b = backend({
      skills: "index_failed",
      projectAgents: [
        {
          agent: agent({
            id: "a1",
            nickname: "arch",
            skills: [".claude/skills/analyst/SKILL.md"],
            disabledSkills: [GLOSSARY],
          }),
          scope: "project",
          availability: "ready",
        },
      ],
    });
    render(<ProjectAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    expect(await within(editor).findByTestId("agent-skills-error")).toHaveTextContent(
      "The skills could not be read",
    );
    expect(within(editor).queryByTestId("agent-synthesis-skills-empty")).not.toBeInTheDocument();
    expect(within(editor).getByTestId("agent-editor-confirm")).toBeEnabled();
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: AgentDraft };
    expect(draft.skills).toEqual([".claude/skills/analyst/SKILL.md"]);
    expect(draft.disabledSkills).toEqual([GLOSSARY]);
  });

  it("AGT-FR-LCMT: with no project open, the selector says so and keeps the stored opt-outs", async () => {
    const b = backend({
      skills: "no_project_open",
      personal: [agent({ id: "p1", nickname: "mine", disabledSkills: [GLOSSARY] })],
    });
    render(<PersonalAgents />);
    await userEvent.click(await screen.findByTestId("agent-edit"));
    const editor = await screen.findByTestId("agent-editor");
    expect(await within(editor).findByTestId("agent-skills-error")).toHaveTextContent(
      "No project is open",
    );
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "update_agent")).toHaveLength(1));
    const { draft } = calls(b, "update_agent")[0].args as { draft: AgentDraft };
    expect(draft.disabledSkills).toEqual([GLOSSARY]);
  });

  it("AGT-FR-WTSR, AGT-FR-12: a Personal agent's editor has the synthesis group and no project group, and sends no project skill", async () => {
    const b = backend();
    render(<PersonalAgents />);
    const editor = await openNewEditor();
    await fillRequired(editor);
    await within(editor).findByTestId("agent-synthesis-skills");
    expect(within(editor).queryByTestId("agent-project-skills")).not.toBeInTheDocument();
    expect(editor).not.toHaveTextContent("analyst");
    expect(within(editor).getByTestId("agent-skills-builtin-note")).toBeInTheDocument();
    const synthesis = options(editor, "agent-synthesis-skills");
    expect(checkedOf(synthesis)).toEqual([true, true]);
    await userEvent.click(within(synthesis[0]).getByRole("checkbox"));
    await userEvent.click(within(editor).getByTestId("agent-editor-confirm"));
    await waitFor(() => expect(calls(b, "create_agent")).toHaveLength(1));
    const { scope, draft } = calls(b, "create_agent")[0].args as {
      scope: string;
      draft: AgentDraft;
    };
    expect(scope).toBe("personal");
    expect(draft.skills).toEqual([]);
    expect(draft.disabledSkills).toEqual([GLOSSARY]);
  });

  it("AGT-FR-XGWD: the project entries are the registry's project skills, then the stored paths it lacks", () => {
    expect(selectorEntries(SKILLS, ["x/SKILL.md", PROJECT[0].path])).toEqual([
      { path: PROJECT[0].path, name: "analyst" },
      { path: PROJECT[1].path, name: "review" },
      { path: "x/SKILL.md", name: null },
    ]);
  });

  it("AGT-FR-KVDA, AGT-FR-JRWE: the synthesis entries are the registry's synthesis skills alone", () => {
    expect(synthesisEntries(SKILLS)).toEqual([
      { path: GLOSSARY, name: "glossary" },
      { path: STYLE, name: "style" },
    ]);
  });

  it("AGT-FR-PQSM, AGT-FR-JRWE: a toggle changes only its own path", () => {
    expect(toggledOptOuts(["a", "b"], "c", false)).toEqual(["a", "b", "c"]);
    expect(toggledOptOuts(["a", "c"], "c", false)).toEqual(["a", "c"]);
    expect(toggledOptOuts(["a", "c", "b"], "c", true)).toEqual(["a", "b"]);
    expect(toggledOptOuts(["a"], "z", true)).toEqual(["a"]);
  });
});
