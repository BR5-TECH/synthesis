# Specification search tool

**Spec code:** `SPS`

## Intent
The tool an agent reaches for when it needs to know what this project has already decided about a topic. It takes the agent's own description of that topic, ranks the project's specifications against it, and hands back the best few by path, each carrying the passage inside it that matched, so the agent can tell from the excerpt alone whether the file is worth reading whole. It exists because a project's specifications are its accumulated decisions and an agent asked to change behaviour three of them already constrain cannot find those three by guessing filenames — a relevance query over the text answers "what bears on this?" in one call, which a filename listing cannot. Everything it knows comes from `../core/BMI-bm25-indexing.md`, whose `spec` index it is the first consumer of, and what counts as a specification is `../core/ASC-artifact-scanning.md`'s classification rather than any rule of this tool's. Out of scope: reading a specification in full, which is `RFT-read-file-tool.md`'s once this one has named it; searching anything the project holds that is not a specification; writing, editing, or creating one; and deciding whether a specification that was found actually governs the job in hand, which is the model's alone.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`search_specifications` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Find the specifications most relevant to a topic, ranked by how well each one's text matches what you describe. Use this when you need to know what this project has already decided about something — search with a plain description of the topic or the behaviour, not with a guessed filename. Returns the best matches first, each with the specification's path, the passage inside it that matched, and a score that orders the results. That passage is one section of the file rather than the whole of it, so read the file at that path when you need the rest. Each specification appears at most once, however many of its sections matched your query. Only specifications are searched — not source code, not skills, not notes — and nothing that is found is changed. Returns an empty list when nothing matches, which means this project has no specification bearing on that topic.

### Arguments
```
SpecificationSearchArgs {
  query:  string,     // required
  limit:  integer?    // optional; default 5, clamped to 1..=20
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `query` — *"A plain description of the topic or behaviour you need this project's decisions about, in your own words. For example: 'how the file tree decides which files are artifacts'."*
- `limit` — *"How many specifications to return, best match first. Defaults to 5. Values below 1 or above 20 are clamped into that range."*

### Output
```
SpecificationMatch {
  path,          // project-relative path of the specification file
  excerpt,       // the text of the best-matching section of that file
  score          // orders this result set (SPS-FR-09)
}

SpecificationSearchOutput { specifications: SpecificationMatch[] }
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **empty query** — kind `InvalidArgs`, retryable: *"The query must describe the topic you need this project's specifications about. Call again with a short plain-language description of it."*

## Functional requirements
 1. **SPS-FR-01** The tool exists as a `rig` portable tool named `search_specifications` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
 2. **SPS-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05).
 3. **SPS-FR-03** The tool answers by calling `../core/BMI-bm25-indexing.md`'s `search` (BMI-FR-10) against the `spec` index alone and returns nothing that call did not produce. It reads no file, enumerates no folder, holds no index of its own, and applies no ranking of its own.
 4. **SPS-FR-04** What counts as a specification is `../core/ASC-artifact-scanning.md`'s classification alone: a file is reachable here exactly when its resolved artifact type is `spec` (ASC-FR-06), whether that type was inferred from its path, assigned to the file directly, or inherited from a folder-scope assignment. This tool adds no path rule of its own, so a specification kept outside `specifications/` is found on the same terms as one inside it, and a file under `specifications/` carrying a different resolved type is not found at all.
 5. **SPS-FR-05** Ranking is BM25 against the `spec` index alone (`../core/BMI-bm25-indexing.md` BMI-FR-11) and reaches no other index. The skills, drafts, notes, flows, scenarios, prompts, instructions, agents, and scratchpads the other ten indexes hold are unreachable through this tool however well their text matches the query.
 6. **SPS-FR-06** `limit` counts specifications rather than passages. It defaults to 5 when absent and is clamped into 1..=20, so a model's `0`, `-1`, or `1000` becomes `1`, `1`, and `20`. The tool never passes a limit of zero downward, so an empty result always means nothing matched rather than that nothing was asked for.
 7. **SPS-FR-07** A result names each specification at most once. The underlying call returns passages and one file holds many of them (`../core/BMI-bm25-indexing.md` BMI-FR-05), so several passages of one specification may match a single query; the highest-scoring of them stands for the file and the rest are dropped. The `excerpt` and `score` a match carries are that surviving passage's.
 8. **SPS-FR-08** To fill `limit` with distinct specifications the tool asks the underlying call for more passages than `limit` and deduplicates what comes back, so a query the project can satisfy returns `limit` specifications rather than the two or three that a limit spent on passages would have collapsed to. It makes one such call per invocation and never escalates it, repeats it, or widens it (per `TLC-tool-conventions.md`'s bound on unbounded work), so a result carries fewer than `limit` specifications only when the passages it asked for came from fewer distinct files.
 9. **SPS-FR-09** Matches are ordered by descending `score` — each file's being the score of the passage that stands for it. The score orders this one result set and is not a relevance measure comparable between two calls (`../core/BMI-bm25-indexing.md` BMI-FR-11), which is why the description tells the model the results are ordered best-first rather than inviting it to interpret a number.
10. **SPS-FR-10** Every match carries exactly `path`, `excerpt`, and `score`. The `index`, `node_id`, and `chunk_ordinal` the underlying hit also carries are omitted: the first is constant across every match this tool can return, and a model refers to a specification by path and can act on neither of the others.
11. **SPS-FR-11** `path` is the project-relative path of the specification file, and is the path `RFT-read-file-tool.md`'s tool accepts unchanged, so a model reads a match in full by passing back exactly what it was given rather than transforming it.
12. **SPS-FR-12** `excerpt` is the matching passage's text as the underlying call supplied it — one section of the file, running from a heading to the next heading of any level (`../core/BMI-bm25-indexing.md` BMI-FR-05). It is neither truncated, summarised, re-wrapped, nor annotated, and no text this tool composed appears in it.
13. **SPS-FR-13** A `query` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface rather than a success carrying an empty list. A model that sent no query made a mistake it can correct in one further call, and an empty result would tell it the opposite — that this project has no specification on the topic.
14. **SPS-FR-14** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), rather than the empty list `../core/BMI-bm25-indexing.md` returns in that circumstance (BMI-FR-10), because a model told "no specification matched" would conclude the project has none and stop looking.
15. **SPS-FR-15** A query that matches no specification in an open project is a success carrying an empty `specifications` list (per TLC-FR-12). So is a project holding no file classified `spec` at all: both are answers rather than failures, and the model is told which by the description's closing sentence.
16. **SPS-FR-16** The tool never blocks on an index pass (per TLC-FR-16, and `../core/BMI-bm25-indexing.md` BMI-FR-10 and BMI-FR-12). A call made while the first index build after a project opens is still running ranks over what has been indexed so far and returns immediately rather than holding the agent's turn open until indexing settles.
17. **SPS-FR-17** A change to what the project's specifications say reaches this tool on the next index pass, in every direction (`../core/BMI-bm25-indexing.md` BMI-FR-15, BMI-FR-16, BMI-FR-17): edited text becomes matchable and the text it replaced stops being, a deleted file stops being returned, a file that gains the `spec` type becomes findable, and one that loses it stops being. The tool holds nothing from a previous call.
18. **SPS-FR-18** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state. Searching for a specification leaves the project exactly as it was.
19. **SPS-FR-19** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `query` it was called with, the `limit` the model asked for, the limit that was applied, and how many specifications it returned; and a `WARN` record naming the tool, the `query`, the `limit` the model asked for, and the reason when it refuses. The `query` and the `limit` are the arguments this specification names as loggable under TLC-FR-14's exception, and it is answerable for them: a record saying only that a search returned three matches does not say what was searched for, so the several searches a turn makes over one topic cannot be told apart or followed when an agent's reasoning is being traced, and a refusal is unfollowable without the argument that caused it. The `query` is recorded as the model composed it — untrimmed — and bounded: a query longer than 512 characters is recorded as its first 512 followed by an ellipsis, so no query a model composes pushes a record past `../core/LGC-logging.md`'s per-record ceiling (LGC-FR-08) and costs it the tool, the count, and the reason. The `limit` is recorded as the value the tool was called with — the number TLC-FR-07's leniency made of what the model wrote — and is absent from the record when the model sent none, or sent something no number could be made of, those two being the same call as far as this tool is concerned; the applied limit is what SPS-FR-06 normalised it to and appears on a success alone, so a clamped `1000` is readable as the `20` that produced the result. No returned path, excerpt, or score is recorded — those are the project's own material.

## Non-functional requirements
- A call is a single delegation to an in-memory BM25 lookup and performs no I/O of its own (per `../core/BMI-bm25-indexing.md`'s non-functional posture), so it is cheap enough to sit inside an agent's reasoning loop and be called several times with different phrasings of one topic.
- The default `limit` of 5 and the ceiling of 20 are chosen against the model's context rather than the corpus's size, and are deliberately lower than `SST-skill-search-tool.md`'s: a match there is a name and a sentence, while a match here carries a whole heading section, so the payload per result is an order of magnitude larger and five of them is already a substantial read.
- The number of passages SPS-FR-08 asks for in order to fill `limit` is an implementation choice; the contract is only that one bounded call is made and that `limit` counts specifications.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences — this one ranks the project's specifications against a topic, `DST-draft-search-tool.md`'s ranks its unpublished drafts against one, `RFT-read-file-tool.md`'s reads a named project file, `RDT-read-draft-tool.md`'s reads a named draft, and the three skill tools answer about skills rather than about specifications.
- Nothing in this tool requires network access.
