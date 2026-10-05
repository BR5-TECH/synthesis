# Skill search tool

**Spec code:** `SST`

## Intent
The tool an agent reaches for when it has been given a job and does not know whether the project already provides a skill for it. It takes the agent's own description of what it is trying to do, ranks every skill the project makes invocable against that description, and hands back the best few by name, description, and path so the agent can read the one it chose and follow it. Ranking is what makes the tool worth having over a directory listing: an agent that must pick one skill out of forty cannot read forty bodies, and a skill's own description is the sentence it wrote to be chosen by, so a relevance query over descriptions answers "which skill is for this?" in one call. Everything it knows comes from `../core/DSL-dynamic-skills-loading.md`, whose registry and BM25 index it is the first consumer of; it enumerates nothing itself and applies no eligibility rule of its own. Out of scope: reading a skill's body, which is `LSK-load-skill-tool.md`'s once this one has named the skill; listing the whole set, which is `SLT-skill-list-tool.md`'s; deciding whether to follow a skill that was found, which is the model's alone; and running one, which nothing in this group does.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`search_skills` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19). The name is shared with `../core/DSL-dynamic-skills-loading.md`'s internal call of the same name, which this tool exists to expose to a model.

### The description
The fixed text the model reads (per TLC-FR-05):

> Find the skills most relevant to a task, ranked by how well each skill's own description matches what you are trying to do. Use this when you have a job to do and want to know whether this project already provides a skill for it — search with a plain description of the task, not with a guessed skill name. Returns the best matches first, each with the skill's name, the description it wrote about itself, the ecosystem it belongs to, and the path to its SKILL.md file; read that file to actually use the skill. Matching is over each skill's short self-description rather than its full text, so describe the task rather than quoting words you expect to find inside the skill. Only skills this project makes available to models are searched. Each name appears once: where two ecosystems provide a skill under the same name, you are shown whichever of them matched your query better. Returns an empty list when nothing matches, which means this project has no skill for that task.

### Arguments
```
SkillSearchArgs {
  query:  string,     // required
  limit:  integer?    // optional; default 10, clamped to 1..=25
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `query` — *"A plain description of the task you are trying to accomplish, in your own words. For example: 'review a specification for internal contradictions'."*
- `limit` — *"How many skills to return, best match first. Defaults to 10. Values below 1 or above 25 are clamped into that range."*

### Output
```
SkillMatch {
  name,          // the skill's declared name (DSL-FR-10)
  description,   // the skill's own description (DSL-FR-09)
  path,          // project-relative path of the SKILL.md to read
  ecosystem,     // "claude" | "codex" | "github" | "opencode"
  score          // orders this result set (SST-FR-11)
}

SkillSearchOutput { skills: SkillMatch[] }
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **empty query** — kind `InvalidArgs`, retryable: *"The query must describe the task you are trying to accomplish. Call again with a short plain-language description of the job."*

## Functional requirements
1. **SST-FR-01** The tool exists as a `rig` portable tool named `search_skills` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
2. **SST-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05).
3. **SST-FR-03** The tool answers by calling `../core/DSL-dynamic-skills-loading.md`'s `search_skills(query, limit)` (DSL-FR-16) and returns nothing that call did not produce. It reads no file, enumerates no folder, holds no registry, consults no index directly, and applies no ranking of its own.
4. **SST-FR-04** Ranking is BM25 against the `skills` index alone (`../core/BMI-bm25-indexing.md` BMI-FR-11), whose documents are each skill's declared name and description and nothing else (DSL-FR-13). A skill is therefore ranked on what it says about itself and never on the length or contents of its body, which is what the description means when it tells the model to describe the task rather than quote expected words.
5. **SST-FR-05** `limit` defaults to 10 when absent and is clamped into 1..=25, so a model's `0`, `-1`, or `1000` becomes `1`, `1`, and `25`. The tool never passes a limit of zero downward, so an empty result always means nothing matched rather than that nothing was asked for.
6. **SST-FR-06** A `query` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface rather than a success carrying an empty list. A model that sent no query made a mistake it can correct in one further call, and an empty result would tell it the opposite — that this project has no skill for the task.
7. **SST-FR-07** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), rather than the empty list `../core/DSL-dynamic-skills-loading.md` returns in that circumstance (DSL-FR-15), because a model told "no skill matched" would conclude the project has none and stop looking.
8. **SST-FR-08** A query that matches no skill in an open project is a success carrying an empty `skills` list (per TLC-FR-12). So is a project whose registry holds no eligible skill at all: both are answers rather than failures, and the model is told which by the description's closing sentence.
9. **SST-FR-09** Every match carries exactly `name`, `description`, `path`, `ecosystem`, and `score` — the fields of the `SkillDescriptor` a model can act on, plus the score. No field is invented, no part of the skill's body appears anywhere in the output, and the `folder_name` the descriptor also carries is omitted because `path` already contains it and a model refers to a skill by name or by path.
10. **SST-FR-10** `path` is the project-relative path of the skill's `SKILL.md`, which is the file the description tells the model to read. It is returned for every match whatever else is known about the skill, because it is the only identity a skill always has (per DSL-FR-12).
11. **SST-FR-11** Matches are ordered by descending `score`. The score orders this one result set and is not a relevance measure comparable between two calls (`../core/BMI-bm25-indexing.md` BMI-FR-11), which is why the description tells the model the results are ordered best-first rather than inviting it to interpret a number.
12. **SST-FR-12** A name appears at most once in a result set. Where the registry holds two skills declaring the same `name` in different ecosystems (per DSL-FR-17), the higher-scoring one is kept and the other dropped — which, the underlying call already ordering by descending score, is the first of the two to arrive. A model choosing between two skills that describe themselves identically is choosing on a fact it cannot weigh, so the ranking makes the choice with the evidence it has. Deduplication runs over what that call returned for the requested `limit` rather than over the whole registry, so a result set containing a collision carries fewer matches than `limit` allowed and nothing is drawn up from lower in the ranking to replace what was dropped.
13. **SST-FR-13** Eligibility is `../core/DSL-dynamic-skills-loading.md`'s alone. A skill excluded there — one opting out of model invocation (DSL-FR-08), one declaring no description (DSL-FR-09), one reached through a symlink (DSL-FR-05), one whose frontmatter does not parse (DSL-FR-07) — is unreachable through this tool. A skill that opts out of model invocation is therefore never ranked, never returned, and never named to a model here. Deduplication (SST-FR-12) is the only filtering this tool applies of its own, and it drops a duplicate of a name it still reports rather than a capability.
14. **SST-FR-14** The tool never blocks on an index pass (per TLC-FR-16 and DSL-FR-16). A call made while the first index build after a project opens is still running ranks over what has been indexed so far and returns immediately rather than holding the agent's turn open until indexing settles.
15. **SST-FR-15** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state. Searching for a skill leaves the project exactly as it was.
16. **SST-FR-16** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool and how many matches it returned, and a `WARN` record naming the tool and the reason when it refuses. Neither the query text nor any returned name, description, or path is recorded, the query being composed by a model out of the conversation it is having.

## Non-functional requirements
- A call is a single delegation to an in-memory BM25 lookup and performs no I/O of its own (per `../core/DSL-dynamic-skills-loading.md`'s non-functional posture), so it is cheap enough to sit inside an agent's reasoning loop and be called several times with different phrasings of one task.
- The default `limit` of 10 and the ceiling of 25 are chosen against the model's context rather than the registry's size: a match is a name, a sentence, and a path, and twenty-five of them is a list a model can weigh without the tool result crowding out the work it was doing.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences — this one ranks skills against a task, `SLT-skill-list-tool.md`'s enumerates the set, `LSK-load-skill-tool.md`'s reads one skill in full, and the two that answer about the project's specifications rather than its skills say so in their own opening words.
- Nothing in this tool requires network access.
