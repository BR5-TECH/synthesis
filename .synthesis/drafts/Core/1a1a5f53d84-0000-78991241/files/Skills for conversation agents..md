## Intent

Every conversational agent turn can find and load skills (instructions plus reference files) that extend what the agent knows how to do. Two sources exist: **built-in skills** embedded in the application, always available; and **project skills** found in the project's skill folders, available to a Project agent only when the author allows them in that agent's persona. The existing tools `search_skills`, `list_skills` and `load_skill` serve both sources.

## User journey

### Built-in skills

- User discusses a document with agent
- User asks agent to write a specific diagram, e.g. mermaid agentflow diagram.
- Conversational Agent checks available skills and finds `mermaid` skill that matches user’s intent. 
- Conversational agent loads the skill, and reads that the diagram user asked for is in referenced file stored in `references/agentflow-beta.md`
- Conversational agent loads the referenced file by calling `load_skill` with the `file` argument `references/agentflow-beta.md`.
- Conversational agent uses the knowledge to construct the diagram user requested.
- Conversational agent proposes changes to the document.
- User reviews the document and accepts/rejects/discusses it.

### User-defined skills

- Author opens the persona editor of a **Project agent** (Project settings → Project Agents).
- The editor shows a multi-select of the project skills the registry holds. It is empty by default: a project skill is not available to an agent until the author selects it.
- Author selects skills and confirms. From the next turn, that agent finds and loads the selected project skills, together with all built-in skills.
- A **Personal agent** has no multi-select and never receives project skills. It receives built-in skills only.
- The selection applies to every conversational turn of that agent, on every target (artifact, draft, note), not to draft discussions only. 

## Requirements

- Claude-compatible skills are expected `resources/skills/<name>/SKILL.md`
- subfolders support is expected, i.e. examples or references.
- all skills from `resources/skills/` are embedded into the Rust binary.
- Two skill sources are available to agents: built-in skills (always, for every agent) and project skills from `.claude`, `.codex`, `.github`, `.opencode` skill folders (only those selected in the agent's persona).
- The existing tools `search_skills`, `list_skills` and `load_skill` are extended; no new tools are added. They are already attached to every conversational turn (CVL-FR-08), so the tool set does not change.
- A built-in skill appears in `list_skills` and `search_skills` results with the new `ecosystem` value `builtin`, and a `path` that is virtual (not a file on disk). It is indexed in the `skills` index like any other skill. A built-in skill and a project skill with the same name both stay present, told apart by `ecosystem` (per DSL-FR-17).
- Built-in skills are not affected by the persona selection and cannot be disabled.
- A project skill that is not selected in the current agent's persona is absent from that agent's `list_skills` and `search_skills` results, and `load_skill` reports it as unknown.
- The selection is stored per Project agent, identified by the skill's `path` (a skill name is not its identity). The default for every agent, including existing ones, is no project skills.
- skills are expected to be loadable automatically by the agent based on the frontmatter description, following best practices of agentic development.
- Look and feel of the discussions and changes to proposal are not expected: experience must stay the same.
- A project skill is a folder in one of the four ecosystem folders that holds a `SKILL.md`, with eligibility rules unchanged (DSL).
- `load_skill` gets an optional `file` argument: a path relative to the skill's own folder (for example `references/agentflow-beta.md`). Without it, behaviour is unchanged and `SKILL.md` is returned. With it, that file's text is returned without frontmatter stripping.
- For a project skill, `file` is read only through the `FsAccess` helper, so a path that escapes the skill's folder, or reaches through a symbolic link, is refused. For a built-in skill, `file` is resolved only inside that skill's embedded folder and the same escape rule applies. A missing file is a retryable refusal.
- The `load_skill` description tells the model that `file` exists and that a skill's `SKILL.md` names the files it refers to.
- The persona editor gets one more control for Project agents: the project-skills multi-select. This changes the "exactly six controls" rule (AGT-FR-12). The control is absent in Personal agents' editor.

## Expected deliverables

- new skill `mermaid` added to `resources/skills/mermaid/` folder, with every diagram type supported by Mermaid 12 in `references` folder, the skills
- Tools `search_skills`, `list_skills` and `load_skill` extended for built-in skills, the `builtin` ecosystem value, persona-based filtering, and the `file` argument of `load_skill`.
- Embedded skills registered in the same registry as project skills.
- Conversational agent prompt is adjusted to include skills instructions.
- Project agent definition extended to hold the list of allowed project skills (by `path`), and the persona editor extended with the multi-select.
- Specifications updated: `DSL-dynamic-skills-loading.md` (embedded source, `builtin` ecosystem, per-agent filter, "no user-facing control" statement), `SST-skill-search-tool.md`, `SLT-skill-list-tool.md`, `LSK-load-skill-tool.md` (descriptions, output shapes, `file` argument, no-project behaviour for built-in skills), `AGR-agent-registry.md` and `AGT-agents.md` (stored list, editor control, AGT-FR-12), `SET-project-settings.md` (Project Agents section), `CVL-conversation-loop.md` (skill instructions in the agent prompt).

## Out of scope

- Selecting project skills for Personal agents.
- Letting the author edit, disable, or override built-in skills.
- Running scripts that a skill ships; skills are read as text only.
- Any change to the look and feel of discussions and proposals. 