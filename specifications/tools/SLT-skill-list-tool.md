# Skill list tool

**Spec code:** `SLT`

## Intent
The tool an agent reaches for when it wants to know everything the project makes available to it rather than the best match for one task. It returns every invocable skill in the project — from all four folders the agent ecosystems keep their skills in — each with the description it wrote about itself and the path to read it at. It exists because ranking answers a question the agent already has and enumeration answers one it does not yet: an agent orienting in an unfamiliar project, or one whose search for a specific task came back empty, needs to see the set before it can tell whether the capability it wants exists under a name it would not have guessed. Completeness is the whole point — a name absent from this list names nothing the agent can use, and that is a fact it can rely on. Everything it knows comes from `../core/DSL-dynamic-skills-loading.md`, whose registry it enumerates and whose eligibility rules are the only ones that apply. Out of scope: reading a skill's body, which is `LSK-load-skill-tool.md`'s once this one has named the skill; ranking the set against a task, which is `SST-skill-search-tool.md`'s; filtering the set, since the tool takes no parameters and a model that wants a subset can read one out of the list it was given; and running a skill, which nothing in this group does.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`list_skills` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19). The name is shared with `../core/DSL-dynamic-skills-loading.md`'s internal call of the same name, which this tool exists to expose to a model.

### The description
The fixed text the model reads (per TLC-FR-05):

> List every skill this project makes available to you, each with the description it wrote about itself. Use this to survey what the project can do, or when a search for a specific task came back with nothing useful and you want to see the whole set before concluding no skill applies. Each entry gives the skill's name, its description, the ecosystem it belongs to, and the path to its SKILL.md file; read that file to actually use the skill. Every skill name available to you appears exactly once, so a name that is not in this list names nothing you can use. This tool takes no parameters. When you already know what task you need a skill for, prefer `search_skills`, which ranks the same set against that task.

### Arguments
The tool takes no parameters. Its `parameters()` is an object schema declaring no properties and requiring none, so a model calls it with `{}`.

### Output
```
SkillEntry {
  name,          // the skill's declared name (DSL-FR-10)
  description,   // the skill's own description (DSL-FR-09)
  path,          // project-relative path of the SKILL.md to read
  ecosystem      // "claude" | "codex" | "github" | "opencode"
}

SkillListOutput { skills: SkillEntry[] }
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.

This is the only refusal the tool can produce. It takes no argument that could be malformed, and every other outcome is a list.

## Functional requirements
1. **SLT-FR-01** The tool exists as a `rig` portable tool named `list_skills` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
2. **SLT-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary (per TLC-FR-05).
3. **SLT-FR-03** The tool declares no parameter. Its `parameters()` is a JSON Schema object with an empty properties set and an empty required set, which is what tells a model to call it with `{}` rather than to invent an argument for it.
4. **SLT-FR-04** A call carrying fields the schema does not declare is answered rather than refused (per TLC-FR-07), because a model that added a plausible filter has asked a question this tool can still answer completely, and refusing would cost a turn to teach it nothing it could not read off the description.
5. **SLT-FR-05** The tool answers by calling `../core/DSL-dynamic-skills-loading.md`'s `list_skills()` (DSL-FR-15) and returns nothing that call did not produce. It reads no file, enumerates no folder, and holds no registry of its own.
6. **SLT-FR-06** The set is every eligible skill of the open project's active worktree, drawn from the four folders of `../core/DSL-dynamic-skills-loading.md` DSL-FR-02 — `.claude/skills`, `.codex/skills`, `.github/skills`, and `.opencode/skills`, matched at the project root alone. Eligibility is that spec's alone: a skill opting out of model invocation (DSL-FR-08), one declaring no description (DSL-FR-09), one reached through a symlink (DSL-FR-05), and one whose frontmatter does not parse (DSL-FR-07) are each absent here, and this tool adds no inclusion of its own. A skill that opts out of model invocation is therefore never listed, never named to a model, and never reachable through this tool. Deduplication (SLT-FR-13) is the only filtering this tool applies of its own, and it drops a duplicate of a name the list still carries rather than a capability.
7. **SLT-FR-07** The list carries every name and is never truncated. There is no limit parameter, no page, no cap, and no elision of a name, because the description tells the model that a name absent from the list names nothing available to it and a silently shortened list would make that statement false. The entry deduplication drops (SLT-FR-13) is a second skill under a name the list already carries, so the statement holds: no name is lost to it.
8. **SLT-FR-08** Entries are ordered by ascending `path` (per DSL-FR-15), so two calls in one session against an unchanged project return the same set in the same order and a model can refer to what it saw. Deduplication removes entries from that order without reordering what remains.
9. **SLT-FR-09** Every entry carries exactly `name`, `description`, `path`, and `ecosystem`. No part of the skill's body appears anywhere in the output, and no score is carried, this tool ranking nothing.
10. **SLT-FR-10** `path` is the project-relative path of the skill's `SKILL.md`, which is the file the description tells the model to read, and is the only identity a skill always has (per DSL-FR-12).
11. **SLT-FR-11** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), rather than the empty list `../core/DSL-dynamic-skills-loading.md` returns in that circumstance (DSL-FR-15), because a model told the list is empty would conclude the project offers no skills and stop looking.
12. **SLT-FR-12** An open project whose four folders hold no eligible skill is a success carrying an empty `skills` list (per TLC-FR-12). A project that ships no skill is a fact the model can act on, not a failure.
13. **SLT-FR-13** A name appears at most once in the list. Where the registry holds two skills declaring the same `name` in different ecosystems (per DSL-FR-17), the first in ascending `path` order is kept and every later one dropped — which prefers `.claude` over `.codex`, `.codex` over `.github`, and `.github` over `.opencode`, those folder names sorting that way. Nothing is lost by it: the dropped skill is still loadable by name and ecosystem through `LSK-load-skill-tool.md`, and a model surveying the set is spared having to choose between two skills that told it the same thing about themselves.
14. **SLT-FR-14** This tool and `SST-skill-search-tool.md` answer from one set of names: every name this tool lists is one `search_skills` can return, and every name that tool returns appears in this one's list, because both read the same registry and the registry and its index are derived together in one pass (per DSL-FR-18). Which skill stands behind a colliding name can differ between them — this tool keeps whichever sorts first by `path` and that one keeps whichever matched the query better (SST-FR-12) — so a name means the same capability in both while `path` and `ecosystem` need not agree.
15. **SLT-FR-15** The tool never blocks on an index pass (per TLC-FR-16 and DSL-FR-15). It answers from the registry as it stands, so a call made while the first pass after a project opens is still running returns immediately with what has been enumerated so far.
16. **SLT-FR-16** A change to what a skill declares reaches this tool on the next pass, in both directions (per DSL-FR-20): a skill that gains a description or drops its opt-out appears in the next call's list, and one that adds an opt-out or loses its description disappears from it. The tool holds nothing from a previous call.
17. **SLT-FR-17** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state.
18. **SLT-FR-18** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool and how many skills it returned, and a `WARN` record naming the tool and the reason when it refuses. No skill's name, description, or path is recorded.

## Non-functional requirements
- A call is a single delegation to an in-memory registry and performs no I/O of its own, so the whole list costs less than one file read however many skills a project holds.
- The list's size in a model's context is bounded by what a descriptor is: a name, a sentence or two, a path, and an ecosystem (per `../core/DSL-dynamic-skills-loading.md`'s non-functional posture). A project holding a hundred skills therefore produces a result a model can read in one pass, which is what makes an uncapped list affordable and why `SST-skill-search-tool.md` bounds its own result while this one does not.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences, and it names `SST-skill-search-tool.md`'s tool explicitly so a model holding a specific task is pointed at the ranked call rather than reading the whole set to find one skill.
- Nothing in this tool requires network access.
