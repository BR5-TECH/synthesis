# Draft search tool

**Spec code:** `DST`

## Intent
The tool an agent reaches for when it needs to know what this project is *planning* rather than what it has already decided. It takes the agent's own description of a topic, ranks the drafts of the active worktree against the prompts they currently hold, and hands back the best few by draft id, each carrying the passage inside it that matched, so the agent can tell from the excerpt alone whether that planned work bears on the question in hand. It exists because a draft is a decision in the making — an agent asked about a specification three unpublished prompts already anticipate cannot find those three by guessing, and a recommendation made without them argues for work the author has already planned. It ranks over **current live prompts alone**: a draft's accepted history is review provenance, a record of what the author settled and moved on from, and an agent that recommended on the strength of a superseded version would be recommending against the draft's own past. Everything it knows comes from `../core/BMI-bm25-indexing.md`'s `drafts` index, and every draft it names is resolved through `../core/DRS-draft-storage.md` before it is returned. Out of scope: reading a prompt in full, which is `RDT-read-draft-tool.md`'s once this one has named the draft; searching anything the project holds that is not a draft's live prompt; reading, offering, or making any change to a draft; and deciding whether a draft that was found actually bears on the job in hand, which is the model's alone.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach. The Tauri command `../core/DRS-draft-storage.md` registers under the same name (DRS-FR-17) is the Drafts panel's own substring filter and is a different operation on a different surface: it is not reachable from here, and nothing about this tool is reachable from it.

### The tool
`search_drafts` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Find planned drafts relevant to a topic, ranked by how well each current live prompt matches what you describe. Use this to discover related draft work when you do not already know its draft ID. Results are best first and include the draft ID to pass unchanged to `read_draft`, its name and status, an informational draft-relative prompt path, a matching excerpt, and a score that orders this result set. Only current live prompts are searched: accepted history and other snapshots are never searched. This does not read a complete prompt, modify a draft, or make its path usable with `read_file`. An empty result means no current readable draft matched.

### Arguments
```
DraftSearchArgs {
  query:  string,     // required
  limit:  integer?    // optional; default 10, clamped to 1..=20
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `query` — *"A plain description of the planned work or topic to find in current draft prompts. Do not supply a file path."*
- `limit` — *"How many distinct drafts to return, best match first. Defaults to 10. Values below 1 or above 20 are clamped into that range."*

### Output
```
DraftSearchMatch {
  draft_id,      // the draft's own id (DRS-FR-02); what read_draft accepts
  name,          // the draft's current name
  status,        // the draft's current DraftStatus (DRS-FR-03)
  prompt_path,   // draft-relative path of the live prompt; informational (DST-FR-11)
  excerpt,       // the text of the best-matching chunk of that prompt
  score          // orders this result set (DST-FR-09)
}

DraftSearchOutput { drafts: DraftSearchMatch[] }
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **empty query** — kind `InvalidArgs`, retryable: *"The query must describe the planned work or topic to find. Call again with a short plain-language description."*

## Functional requirements
 1. **DST-FR-01** The tool exists as a `rig` portable tool named `search_drafts` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
 2. **DST-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05).
 3. **DST-FR-03** The tool ranks by calling `../core/BMI-bm25-indexing.md`'s `search` (BMI-FR-10) against the `drafts` index alone and returns no draft that call did not produce. It enumerates no drafts folder, walks no drafts hierarchy, reads no prompt, holds no index of its own, and applies no ranking of its own.
 4. **DST-FR-04** What is searchable is the `drafts` index's own membership and nothing beside it: each draft's **current live prompt** at the `prompt_path` of `../core/DRS-draft-storage.md` DRS-FR-01 (per `../core/BMI-bm25-indexing.md` BMI-FR-04). A draft's accepted history entries (`../core/DHS-draft-history.md` DHS-FR-01), the candidates of the changes agents have proposed to it (`../core/DCP-draft-change-proposals.md` DCP-FR-01), its review logs, and its conversation log are in no index and are unreachable through this tool by any query, so a passage that a live prompt no longer holds is a passage this tool cannot match on.
 5. **DST-FR-05** Ranking is BM25 against the `drafts` index alone (`../core/BMI-bm25-indexing.md` BMI-FR-11) and reaches no other index. The specifications, skills, notes, flows, scenarios, prompts, instructions, agents, and scratchpads the other ten indexes hold are unreachable through this tool however well their text matches the query.
 6. **DST-FR-06** `limit` counts drafts rather than passages. It defaults to 10 when absent and is clamped into 1..=20, so a model's `0`, `-1`, or `1000` becomes `1`, `1`, and `20`. The tool never passes a limit of zero downward, so an empty result always means nothing matched rather than that nothing was asked for.
 7. **DST-FR-07** A result names each draft at most once. The underlying call returns chunks and one prompt holds several of them (`../core/BMI-bm25-indexing.md` BMI-FR-05), so several chunks of one prompt may match a single query; the highest-scoring of them stands for the draft and the rest are dropped. The `excerpt` and `score` a match carries are that surviving chunk's.
 8. **DST-FR-08** To fill `limit` with distinct drafts the tool asks the underlying call for more chunks than `limit` and deduplicates what comes back, so a query the worktree can satisfy returns `limit` drafts rather than the two or three that a limit spent on chunks would have collapsed to. It makes one such call per invocation and never escalates it, repeats it, or widens it (per `TLC-tool-conventions.md`'s bound on unbounded work), so a result carries fewer than `limit` drafts only when the chunks it asked for came from fewer distinct drafts or when a hit was dropped under DST-FR-13.
 9. **DST-FR-09** Matches are ordered by descending `score` — each draft's being the score of the chunk that stands for it. The score orders this one result set and is not a relevance measure comparable between two calls (`../core/BMI-bm25-indexing.md` BMI-FR-11), which is why the description tells the model the results are best first rather than inviting it to interpret a number.
10. **DST-FR-10** Every match carries exactly `draft_id`, `name`, `status`, `prompt_path`, `excerpt`, and `score`. The `index`, `path`, `node_id`, and `chunk_ordinal` the underlying hit also carries are omitted: the first is constant across every match this tool can return, the second is the record's own `prompt_path` said twice, and a model can act on neither of the others.
11. **DST-FR-11** `prompt_path` is **draft-relative and informational**. It names the prompt's position inside the draft's own storage rather than a position in the project, so it is not a path `RFT-read-file-tool.md`'s tool resolves to this prompt and the description tells the model so. What a model carries forward from a match is the `draft_id`, which `RDT-read-draft-tool.md`'s tool accepts unchanged, so a draft found here is read in full by passing back exactly what this tool gave rather than by composing a path.
12. **DST-FR-12** `name`, `status`, and `prompt_path` are the draft's **current record** as `../core/DRS-draft-storage.md` holds it at the moment of the call, resolved through DRS-FR-38 rather than carried on the indexed hit, so a draft renamed or archived since the last index pass is reported as it now stands. `status` is that record's own `DraftStatus` (DRS-FR-03) carried through unchanged and interpreted nowhere here: every status draft storage retains is searchable on identical terms, and a status added to that type reaches this tool's results without a change to it.
13. **DST-FR-13** A hit is returned only where the draft it names still resolves and is the valid single-prompt draft `../core/DRS-draft-storage.md` DRS-FR-11 requires. A hit naming a draft that has since been removed, and one naming a draft that has become inconsistent (DRS-FR-15), are each **omitted** rather than returned as a result the model could not then read. The omission is silent and costs the result set that row, so a call may return fewer drafts than the index held chunks for; a result the model is given is one `RDT-read-draft-tool.md` can load.
14. **DST-FR-14** `excerpt` is the matching chunk's text as the underlying call supplied it. It is neither truncated, summarised, re-wrapped, nor annotated, and no text this tool composed appears in it.
15. **DST-FR-15** A `query` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface rather than a success carrying an empty list. A model that sent no query made a mistake it can correct in one further call, and an empty result would tell it the opposite — that this project is planning nothing on the topic.
16. **DST-FR-16** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), rather than the empty list `../core/BMI-bm25-indexing.md` returns in that circumstance (BMI-FR-10), because a model told "no draft matched" would conclude the project has none and stop looking.
17. **DST-FR-17** A query that matches no draft in an open project is a success carrying an empty `drafts` list (per TLC-FR-12). So is a worktree holding no draft at all, and so is one whose every matching hit was dropped under DST-FR-13: all three are answers rather than failures, and the model is told which by the description's closing sentence.
18. **DST-FR-18** The tool never blocks on an index pass (per TLC-FR-16, and `../core/BMI-bm25-indexing.md` BMI-FR-10 and BMI-FR-12). A call made while the first index build after a project opens is still running ranks over what has been indexed so far and returns immediately rather than holding the agent's turn open until indexing settles. A search is therefore as current as the index at the moment of the call, which is the freshness `RDT-read-draft-tool.md` does not share, that tool reading the prompt from disk (RDT-FR-03).
19. **DST-FR-19** A change to a draft's live prompt reaches this tool on the next index pass, in every direction (`../core/BMI-bm25-indexing.md` BMI-FR-19): saved text becomes matchable and the text it replaced stops being, a draft created becomes findable, and a draft deleted stops being returned. The tool holds nothing from a previous call, and it never widens what it searches to reach material a pass has not yet taken up.
20. **DST-FR-20** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no project file, no draft file, no history entry, no proposal, no index, and no other state, and it touches no repository state. Searching for a draft leaves the worktree exactly as it was.
21. **DST-FR-21** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `query` it was called with, the `limit` the model asked for, the limit that was applied, and how many drafts it returned; and a `WARN` record naming the tool, the `query`, the `limit` the model asked for, and the reason when it refuses. The `query` and the `limit` are the arguments this specification names as loggable under TLC-FR-14's exception, and it is answerable for them, on exactly the terms `SPS-specification-search-tool.md` is answerable for its own (SPS-FR-19): a record saying only that a search returned three drafts does not say what was searched for, so the several searches a turn makes over one topic cannot be told apart when an agent's reasoning is being traced, and a refusal is unfollowable without the argument that caused it. The `query` is recorded as the model composed it — untrimmed — and bounded: a query longer than 512 characters is recorded as its first 512 followed by an ellipsis, so no query a model composes pushes a record past `../core/LGC-logging.md`'s per-record ceiling (LGC-FR-08). The `limit` is recorded as the value the tool was called with and is absent from the record when the model sent none or sent something no number could be made of; the applied limit is what DST-FR-06 normalised it to and appears on a success alone. **No returned draft id, name, status, prompt path, score, or any part of any excerpt is recorded** — a draft's prompt is the author's own unpublished material, and an excerpt of it is a quotation of that material.

## Non-functional requirements
- A call is one in-memory BM25 lookup and one record resolution per surviving hit, so it is cheap enough to sit inside an agent's reasoning loop and be called several times with different phrasings of one topic. It reads no prompt, so its cost does not grow with how long the drafts have become.
- The default `limit` of 10 and the ceiling of 20 are chosen against the model's context. A match here carries one chunk of a prompt rather than a whole heading section of a specification, so ten of them is a comparable read to `SPS-specification-search-tool.md`'s five.
- The number of chunks DST-FR-08 asks for in order to fill `limit` is an implementation choice; the contract is only that one bounded call is made and that `limit` counts drafts.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences — this one finds which planned drafts bear on a topic, `SPS-specification-search-tool.md`'s finds which decided specifications do, `RDT-read-draft-tool.md`'s reads a draft it is given the id of, and `RFT-read-file-tool.md`'s reads a project file it is given the path of.
- Nothing in this tool requires network access.
