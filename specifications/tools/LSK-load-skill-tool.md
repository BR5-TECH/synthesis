# Load skill tool

**Spec code:** `LSK`

## Intent
The tool an agent reaches for once it has decided which skill to follow. `SST-skill-search-tool.md` and `SLT-skill-list-tool.md` answer *which* skill, and they answer it with a name, a sentence, and a path — deliberately, because an agent weighing forty skills cannot read forty bodies. This tool answers the question that comes next: it takes a name and returns that one skill's instructions in full, so the agent can actually carry them out. Separating the two is what keeps the choosing cheap: a survey costs one sentence per skill and a commitment costs one skill's whole text, and nothing pays the second price until it has made the first decision. It is the only tool in this group that reads a file, and it reads exactly one — the `SKILL.md` of a skill the registry of `../core/DSL-dynamic-skills-loading.md` already holds, never a path the model composed. Out of scope: choosing a skill, which is the other two tools'; carrying out what a skill says, which is the model's alone and which nothing in this group does; reading any other file in the project, which this tool cannot do and which an agent's own file-reading tool does instead; and reaching a skill the registry excludes, since eligibility is that spec's and this tool has no way past it.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`load_skill` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Read one skill's full instructions, so you can follow them. Use this once `search_skills` or `list_skills` has told you which skill you want and you are ready to carry out its procedure rather than to weigh whether it applies. Give the skill's name exactly as it was reported to you; add `ecosystem` only when you were shown two skills sharing a name and you mean a particular one. Returns the skill's instructions as plain text, without the short header block carrying the name and description you have already been given. This tool reads one skill and nothing else — it opens no other file in the project, and it does not carry out what the skill says, which is yours to do.

### Arguments
```
LoadSkillArgs {
  name:       string,     // required
  ecosystem:  string?     // optional; "claude" | "codex" | "github" | "opencode"
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `name` — *"The skill's name, exactly as `list_skills` or `search_skills` reported it. Leading and trailing spaces and differences in letter case are ignored."*
- `ecosystem` — *"Which ecosystem's copy to load, when two skills share a name: one of `claude`, `codex`, `github`, or `opencode`. Leave it out unless you have been told the name is ambiguous."*

### Output
The skill's instructions, as text (per TLC-FR-08). There is no wrapping object, no named field, and no metadata alongside: the result *is* the document, and a document reaches the model as text rather than as a JSON string field that would escape every line break in it.

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank name** — kind `InvalidArgs`, retryable: *"The name must name a skill. Call `list_skills` to see which names this project offers, then call again with one of them."*
- **unknown skill** — kind `NotFound`, retryable: *"No skill by that name is available in this project. Call `list_skills` to see which names are, then call again with one of them."*
- **ambiguous name** — kind `InvalidArgs`, retryable: *"More than one skill goes by that name. Call again with `ecosystem` set to one of: "* followed by the ecosystems that hold it.
- **name is not unique** — kind `Other`, not retryable: *"Two skills in the same place declare that name, so it does not identify one of them. No further call can resolve this; read the skill's file directly instead."*
- **unreadable skill** — kind `NotFound`, retryable: *"That skill's file could not be read. It may have changed since the list was built; call `list_skills` to see what is available now."*

## Functional requirements
1. **LSK-FR-01** The tool exists as a `rig` portable tool named `load_skill` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
2. **LSK-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions, declaring `name` required and `ecosystem` optional (per TLC-FR-05, TLC-FR-06).
3. **LSK-FR-03** The tool resolves a name against `../core/DSL-dynamic-skills-loading.md`'s `list_skills()` (DSL-FR-15) and reads exactly one file: the `SKILL.md` at the `path` of the descriptor it resolved to. It enumerates no folder, holds no registry, and never reads a path the model supplied, so no argument any model can compose reaches a file the registry does not already hold.
4. **LSK-FR-04** Eligibility is `../core/DSL-dynamic-skills-loading.md`'s alone. A skill excluded there — one opting out of model invocation (DSL-FR-08), one declaring no description (DSL-FR-09), one reached through a symlink (DSL-FR-05), one whose frontmatter does not parse (DSL-FR-07), one that does not decode as UTF-8 or exceeds the indexing ceiling (DSL-FR-23) — has no descriptor to resolve to and is therefore unloadable through this tool, which reports it as the unknown-skill refusal. A skill that opts out of model invocation is unreachable through every tool in this group, this one included: opting out means a model may not use the skill, not merely that it may not find it.
5. **LSK-FR-05** `name` is matched against the resolved `name` of each descriptor (DSL-FR-10) after both are trimmed of surrounding whitespace, compared without regard to letter case. A model retyping a name it was shown therefore reaches the skill it meant, which is the forgiveness TLC-FR-07 asks for applied to the one required argument this tool has.
6. **LSK-FR-06** A `name` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface. Guessing which skill was meant would answer a question the model did not ask (per TLC-FR-07).
7. **LSK-FR-07** `ecosystem` narrows the candidates to those found in that folder family and nothing else. It is absent by default, in which case all four are considered. A value naming none of the four narrows the candidate set to nothing rather than being refused as malformed, so it reaches the model as the unknown-skill refusal along with every other way of naming a skill that is not there.
8. **LSK-FR-08** Exactly one candidate is a success carrying that skill's instructions. This is the ordinary outcome, a name being unique in all but the collision DSL-FR-17 permits.
9. **LSK-FR-09** No candidate is the retryable `NotFound` refusal of the contract surface, whether the name matches nothing, or matches only skills in other ecosystems than the one requested, or matches only skills the registry excludes. The refusal names `list_skills` as the way to learn what is available, because a model that guessed a name needs the set rather than another guess.
10. **LSK-FR-10** More than one candidate spanning more than one ecosystem is the retryable `InvalidArgs` refusal of the contract surface, and the message names the ecosystems that hold the name. The tool never picks one: `SLT-skill-list-tool.md` shows the model whichever copy sorts first (SLT-FR-13) and `SST-skill-search-tool.md` shows it whichever matched better (SST-FR-12), so a silent choice here could hand back instructions the model never saw and believed it was following the ones it did.
11. **LSK-FR-11** More than one candidate within a *single* ecosystem is the not-retryable `Other` refusal of the contract surface. A skill declares the name of its own folder, so two in one folder family declaring one name is a project that made its own name ambiguous rather than a call the model composed badly: `ecosystem` is the only disambiguator there is and it is already exhausted, so the message says outright that no further call resolves it and points at reading the file instead. Repeating the ecosystem advice here would send the model back for a call that refuses identically while the retryable flag promised otherwise (per TLC-FR-11).
12. **LSK-FR-12** The result is delivered as text and not as a JSON object (per TLC-FR-08). The tool wraps it in no structure, adds no field, and prefixes and suffixes it with nothing it wrote itself, so what the model reads is what the skill's author wrote and nothing else.
13. **LSK-FR-13** The result is the file's content below its frontmatter: when the file opens with a `---` delimiter line, everything through the matching closing delimiter line and the line break that ends it is removed, and the remainder is returned byte-for-byte. The header is dropped because the model was already given the `name` and `description` it carries by whichever tool told it this skill existed, and because the rest of the frontmatter addresses the agent CLI that runs the ecosystem rather than the model reading the instructions. A file with no such opening delimiter is returned whole, and one that holds only frontmatter is a success carrying empty text rather than a refusal (per TLC-FR-12).
14. **LSK-FR-14** The instructions are never truncated, paged, elided, or summarised, and the tool imposes no size ceiling of its own. A model that named one skill asked for that skill's procedure, and half a procedure is worse than none — it reads as complete. The registry has already excluded a `SKILL.md` above the indexing ceiling (DSL-FR-23), so a file large enough to matter here is one that grew after the pass that admitted it.
15. **LSK-FR-15** A read that fails — the file gone since the last pass, its path no longer readable, its content no longer valid UTF-8 — is the retryable `NotFound` refusal of the contract surface rather than a panic or an empty success. Every read goes through `../core/FSA-filesystem-access.md`'s `read_text` and is subject to its path-escape rejection (FSA-FR-10) and its symlink refusal (FSA-FR-17), a `SKILL.md` that became a symbolic link since it was enumerated being refused rather than followed.
16. **LSK-FR-16** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), rather than the empty list `../core/DSL-dynamic-skills-loading.md` returns in that circumstance (DSL-FR-15), which would otherwise reach the model as "no skill by that name" and tell it the project lacks a skill it has.
17. **LSK-FR-17** The tool never blocks on an index pass (per TLC-FR-16 and DSL-FR-15). It resolves against the registry as it stands, so a skill added since the last pass is not yet loadable and reports as unknown, and a skill whose file has changed since that pass is returned as the file reads now rather than as the registry remembers it.
18. **LSK-FR-18** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state. Loading a skill leaves the project exactly as it was, the one file it opens being opened for reading alone.
19. **LSK-FR-19** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool and the size in bytes of the instructions it returned, and a `WARN` record naming the tool and the reason when it refuses. Neither the requested name, nor the resolved path, nor the ecosystem, nor any part of the instructions is recorded — the name was composed by a model out of the conversation it is having, and the instructions are project material.

## Non-functional requirements
- A call is one lookup over the in-memory registry and one read of one file. It is the only tool in this group that touches disk, and the cost of a call is the cost of reading one skill, which is why the two tools that survey the set are the ones an agent calls freely and this one is the one it calls once it has decided.
- The result is the largest a tool in this group produces, and its size is set by the skill's author rather than by anything here. That is deliberate: bounding it would mean choosing which half of somebody's procedure a model is allowed to follow.
- Frontmatter is removed textually by locating the delimiters, not by re-parsing YAML and re-emitting what remains, so the instructions below it are returned exactly as written whatever the header contained.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences: one ranks skills against a task, one enumerates the set, and this one reads a single skill in full, while the two that answer about the project's specifications rather than its skills say so in their own opening words.
- Nothing in this tool requires network access.
