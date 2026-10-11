## Intent

Agents for discussions must defined in a project, persistent in the project and committed to the repo.

This should allow other contributors to the project to use the same agent personas with the same isntructions.

There are two scopes. **Project agents** are stored in the project, are committed, and are shared by every contributor. **Personal agents** are stored on the user's machine and follow the user into every project. The old model (a global registry plus per-project enrolment of ids) is removed.

## User journey

### Project-level Agents

- User is opens a single settings window
- User opens the **Project Agents** section in the Project group of the Settings window. It lists the agents of this project. User creates, edits, and deletes them with the same persona editor as today (nickname, title, model, reasoning, instructions).
- Configuration applied by user becomes saved to project settings in `.synthesis/` folder and can be committed and pushed to GitHub repo.
- Other users contributing to the project see these changes once they sync their cloned repos. Project agents are addressable at once, with no enrolment step.
- When the user changes the Project Agents of a project, the roster and the mention picker show the change in that project.

### Personal Agents:

- User is opens a single settings window
- In Global section of the settings checks “Personal Agents” tab.
- Personal agents are configured in exactly the same way as Project Agents (same editor, same fields), but are never written into the project. They stay in the user-global storage. The Personal Agents section is available when no project is open.
- Personal agents are automatically available in every project the user works with, without any action from the user. A project cannot exclude or hide a Personal agent, and there is no per-project toggle.

### Roster and existing agents

- The roster in the top chrome lists Project agents and Personal agents together, ordered by nickname. Each row shows its scope (Project or Personal). The count on the chrome control includes both.
- User activates a roster row. The Settings window opens on the Project Agents or the Personal Agents section (the scope of that agent) with the editor of that agent open.
- User activates **Add an agent** in the roster. The Settings window opens on Project Agents with a fresh editor. When no project is open, the roster does not exist, so no route is needed.
- A Personal agent and a Project agent can have the same nickname. The Project agent wins in that project. The Personal agent is shown as shadowed and unavailable in that project, in the roster and in the Personal Agents section, and it cannot be addressed there. It works again in projects with no Project agent of that nickname.
- User who has agents from an earlier version: on first start after the update, every existing agent becomes a Personal agent. Per-project enrolment is dropped. No agent is copied into any project.

## Requirements

- All configuration options must stay respected.
- If model configured for the agent is not available for one or more agent it must be visibile like it is today.
- Unavailable agents should work the same way as they are today.
- A Project agent is stored under `.synthesis/` in a layout that does not cause git merge conflicts when two contributors change different agents (one file per agent). It holds persona fields only: nickname, title, model id, reasoning, instructions. It holds no provider, no URL, no key, and no secret.
- Model availability is computed on each user's machine from the provider that the project resolves to for that user (as today). A Project agent whose model is not offered on a contributor's machine stays defined and shows the existing unavailable state. Nothing is rewritten in the committed file.
- Nicknames are unique inside one scope, case-insensitive, and the reserved handle `all` is still refused in both scopes. Across scopes the Project agent wins (see above).
- Deleting a Project agent removes it for the project and cancels its turns in flight. Deleting a Personal agent removes it from every project and cancels its turns in flight.
- Resolving a nickname in a project looks at that project's Project agents first, then at Personal agents that are not shadowed. A nickname that matches nothing still refuses `agent_not_found`. A match that is not `ready` still refuses `agent_unavailable`.
- Existing conversations that name an agent by nickname keep working when the agent is migrated.
- The UI uses the Settings window of the draft "Single settings window" (Global group and Project group). The Agents section appears in both groups. If that draft does not graduate first, the same sections live in Global settings and Project settings, and the behaviour here does not change.

## Specifications to change

- `specifications/core/AGR-agent-registry.md`: replace the registry-plus-enrolment model. Change AGR-FR-12 to AGR-FR-15 (enrolment, `list_project_agents`, `enrol_project_agent`, `remove_project_agent`), AGR-FR-18 (resolution over Project agents then Personal agents), AGR-FR-19 (one definition per scope), and the delete rule that prunes enrolment.
- `specifications/core/GSS-global-settings-storage.md`: GSS-FR-30 (the registry holds Personal agents only) and GSS-FR-31 (remove enrolled agents from the per-project slot). Add the one-time migration.
- `specifications/core/PSS-project-settings-storage.md`: add storage of Project agents in `.synthesis/`, with the no-secret rule.
- `specifications/ui/AGT-agents.md`: AGT-FR-02, AGT-FR-03, AGT-FR-06 to AGT-FR-10 and AGT-FR-22, AGT-FR-23 (roster content and scope marker, row and Add routing, empty state, the Global Agents section becomes Personal Agents, the Project Agents section creates and edits).
- `specifications/ui/SET-project-settings.md`: SET-FR-15 (the Agents section creates, edits, and deletes Project agents instead of enrolling).
- `specifications/ui/GLS-global-settings.md`: GLS-FR-24 and the intro (Personal agents, not "enrolled per project").
- `specifications/ui/SWN-settings-windows.md`: the routes of SWN-FR-06 and SWN-FR-13 that name an Agents section.
- `specifications/core/AGC-agent-conversations.md`: turn cancel on agent delete (AGC-FR-20) and resolution wording.