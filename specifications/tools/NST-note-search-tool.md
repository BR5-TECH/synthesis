# Note search tool

**Spec code:** `NST`

## Intent
The tool an agent reaches for when it needs to know what the author has written down for themselves about a topic. A note is where a small gap, a follow-up, or a thing to be done eventually is recorded — a few lines written in a hurry against a file, against a Flow, or against the project as a whole — and it is the one place in the project where such an item lives until somebody picks it up. This tool takes the agent's own description of a topic, ranks the notes of the active worktree against it, and hands each match back whole, so an agent working with an author on a draft can tell in one call whether the project already holds a note the work in front of it would close. A note is short by construction, so a match is the note itself rather than a passage of it and there is nothing further to read. Everything it knows comes from `../core/BMI-bm25-indexing.md`'s `notes` index, and every note it names is resolved through `../core/NTC-notes-storage.md` before it is returned. Out of scope: writing, editing, moving, resolving, or deleting a note, none of which any tool in this group does; reading a note's discussion, which is a conversation rather than material to gather; searching anything the project holds that is not a note's body; and deciding whether a note that was found is worth acting on now, which is the model's alone.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach. The Notes panel's own list and filter (`../ui/NTS-notes.md`) are a different surface over the same store and are reachable by no path from here.

### The tool
`search_notes` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Find the notes most relevant to a topic, ranked by how well each note's text matches what you describe. Use this to discover what the author has written down for themselves — a gap, a follow-up, a small thing to be done eventually — because one of those items is often work the job in front of you would close. Returns the best matches first, each with the note's id, what the note is filed against, the whole of the note's text, and a score that orders this result set. A note is short, so the complete note is returned and there is nothing further to read for it. Each note appears at most once. Only notes are searched — not specifications, not drafts, not project files — and nothing that is found is read out of, changed, resolved, or deleted. Returns an empty list when nothing matches, which means this project holds no note bearing on that topic.

### Arguments
```
NoteSearchArgs {
  query:  string,     // required
  limit:  integer?    // optional; default 5, clamped to 1..=20
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `query` — *"A plain description of the topic, the gap, or the follow-up you want the author's notes about, in your own words. For example: 'the project picker does not report a missing folder'."*
- `limit` — *"How many notes to return, best match first. Defaults to 5. Values below 1 or above 20 are clamped into that range."*

### Output
```
NoteMatch {
  id,            // the note's own id (NTC-FR-02)
  scope,         // the note's NoteScope as notes storage holds it (NTC-FR-03)
  body,          // the whole of the note's stored body
  score          // orders this result set (NST-FR-09)
}

NoteSearchOutput { notes: NoteMatch[] }
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **empty query** — kind `InvalidArgs`, retryable: *"The query must describe the topic you want this project's notes about. Call again with a short plain-language description of it."*

## Functional requirements
 1. **NST-FR-01** The tool exists as a `rig` portable tool named `search_notes` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
 2. **NST-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05).
 3. **NST-FR-03** The tool ranks by calling `../core/BMI-bm25-indexing.md`'s `search` (BMI-FR-10) against the `notes` index alone and returns no note that call did not produce. It reads no note file, enumerates no folder, holds no index of its own, and applies no ranking of its own.
 4. **NST-FR-04** What is searchable is the `notes` index's own membership and nothing beside it: the **body** of every note the active worktree persists (per `../core/BMI-bm25-indexing.md` BMI-FR-28). Project-scoped and entity-scoped notes, notes whose entity no longer resolves (`../core/NTC-notes-storage.md` NTC-FR-10), and notes authored against a historical revision (NTC-FR-18) are all searchable on identical terms. A note's scope, its last-known entity path, its reminder, its revision, and its timestamps are in no index, so a query matches on what the author wrote and never on where the note was filed or when.
 5. **NST-FR-05** Ranking is BM25 against the `notes` index alone (`../core/BMI-bm25-indexing.md` BMI-FR-11) and reaches no other index. The specifications, skills, drafts, flows, scenarios, prompts, instructions, agents, and scratchpads the other ten indexes hold are unreachable through this tool however well their text matches the query.
 6. **NST-FR-06** `limit` counts notes. It defaults to 5 when absent and is clamped into 1..=20, so a model's `0`, `-1`, or `1000` becomes `1`, `1`, and `20`. The tool never passes a limit of zero downward, so an empty result always means nothing matched rather than that nothing was asked for.
 7. **NST-FR-07** A result names each note at most once. A note is one whole document in the index rather than a set of chunks (per `../core/BMI-bm25-indexing.md` BMI-FR-05), so the underlying call produces at most one hit per note and there is no highest-scoring passage to choose between; a repeated id is nonetheless dropped rather than returned twice.
 8. **NST-FR-08** The tool makes **one** call to the underlying search per invocation, asking for the applied limit and nothing more, and never escalates, repeats, or widens it (per `TLC-tool-conventions.md`'s bound on unbounded work). Because one note is one document, that one call already counts notes, so a result carries fewer than `limit` notes only when fewer notes matched or when a hit was dropped under NST-FR-13.
 9. **NST-FR-09** Matches are ordered by descending `score`. The score orders this one result set and is not a relevance measure comparable between two calls (`../core/BMI-bm25-indexing.md` BMI-FR-11), which is why the description tells the model the results are best first rather than inviting it to interpret a number.
10. **NST-FR-10** Every match carries exactly `id`, `scope`, `body`, and `score`. The `index`, `path`, `node_id`, and `chunk_ordinal` the underlying hit also carries are omitted: the first is constant across every match this tool can return, the second names the note's own file inside `.synthesis/` and is no path a model may read or compose with, and the last two say nothing about a document that is never split.
11. **NST-FR-11** `body` is the **complete stored body** of the note and never a matching fragment of one. There is no excerpt here and no truncation, summary, re-wrapping, or annotation of any kind, and no text this tool composed appears in it: a note is bounded at 1 KiB by notes storage itself (`../core/NTC-notes-storage.md` NTC-FR-23), so returning the whole of it costs a model less than an excerpt plus the second call that would fetch the rest.
12. **NST-FR-12** `scope` and `body` are the note's **current record** as `../core/NTC-notes-storage.md` holds it at the moment of the call, resolved through NTC-FR-25 rather than carried on the indexed hit, so a note edited or moved since the last index pass is reported as it now stands. `scope` is that record's own `NoteScope` (NTC-FR-03) carried through unchanged and interpreted nowhere here: an entity scope carries the note's `entity_id` and its last-known project-relative `entity_path`, a project scope carries neither, and a scope shape notes storage later adds reaches this tool's results without a change to it.
13. **NST-FR-13** A hit is returned only where the note it names still resolves. A hit naming a note that has since been deleted, and one naming a note whose stored record can no longer be read (per `../core/NTC-notes-storage.md` NTC-FR-14), are each **omitted** rather than returned as a result the model could not act on. The omission is silent and costs the result set that row, so a call may return fewer notes than the index held documents for.
14. **NST-FR-14** A `query` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface rather than a success carrying an empty list. A model that sent no query made a mistake it can correct in one further call, and an empty result would tell it the opposite — that the author has noted nothing on the topic.
15. **NST-FR-15** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), rather than the empty list `../core/BMI-bm25-indexing.md` returns in that circumstance (BMI-FR-10), because a model told "no note matched" would conclude the project has none and stop looking.
16. **NST-FR-16** A query that matches no note in an open project is a success carrying an empty `notes` list (per TLC-FR-12). So is a worktree holding no note at all, and so is one whose every matching hit was dropped under NST-FR-13: all three are answers rather than failures, and the model is told which by the description's closing sentence.
17. **NST-FR-17** The tool never blocks on an index pass (per TLC-FR-16, and `../core/BMI-bm25-indexing.md` BMI-FR-10 and BMI-FR-12). A call made while the first index build after a project opens is still running ranks over what has been indexed so far and returns immediately rather than holding the agent's turn open until indexing settles.
18. **NST-FR-18** A change to the project's notes reaches this tool on the next index pass, in every direction (`../core/BMI-bm25-indexing.md` BMI-FR-29): an edited body becomes matchable and the text it replaced stops being, a note created becomes findable, a note deleted stops being returned, and the notes of a newly activated worktree replace the notes of the outgoing one. Moving a note between entities or to and from project scope changes nothing about whether it is found, its scope being indexed nowhere, and is reported on the next call through NST-FR-12 rather than through a pass. The tool holds nothing from a previous call, and it never widens what it searches to reach material a pass has not yet taken up.
19. **NST-FR-19** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no note, no note discussion, no project file, no index, and no other state, and it touches no repository state. Searching the notes leaves every one of them, and the worktree they sit in, exactly as they were — a note found here is not read, not resolved, and not marked in any way.
20. **NST-FR-20** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `query` it was called with, the `limit` the model asked for, the limit that was applied, and how many notes it returned; and a `WARN` record naming the tool, the `query`, the `limit` the model asked for, and the reason when it refuses. The `query` and the `limit` are the arguments this specification names as loggable under TLC-FR-14's exception, and it is answerable for them, on exactly the terms `SPS-specification-search-tool.md` is answerable for its own (SPS-FR-19): a record saying only that a search returned three notes does not say what was searched for, so the several searches a turn makes over one topic cannot be told apart when an agent's reasoning is being traced, and a refusal is unfollowable without the argument that caused it. The `query` is recorded as the model composed it — untrimmed — and bounded: a query longer than 512 characters is recorded as its first 512 followed by an ellipsis, so no query a model composes pushes a record past `../core/LGC-logging.md`'s per-record ceiling (LGC-FR-08). The `limit` is recorded as the value the tool was called with and is absent from the record when the model sent none or sent something no number could be made of; the applied limit is what NST-FR-06 normalised it to and appears on a success alone. **No returned note id, scope, entity path, score, or any part of any body is recorded** — a note is what an author wrote to themselves about their own project, and a body is that text verbatim.

## Non-functional requirements
- A call is one in-memory BM25 lookup and one record resolution per surviving hit, so it is cheap enough to sit inside an agent's reasoning loop and be called several times with different phrasings of one topic.
- The default `limit` of 5 and the ceiling of 20 are chosen against the model's context. A match here carries a whole note rather than a passage of a longer document, and a note is bounded at 1 KiB, so twenty of them is a bounded read whose worst case is known from the bound alone.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences — this one finds what the author has noted about a topic, `SPS-specification-search-tool.md`'s finds which decided specifications bear on it, `DST-draft-search-tool.md`'s finds which planned drafts do, and the skill tools answer about skills rather than about the project's own material.
- The tool returns whole notes because a note is small, which is the property notes storage guarantees rather than one this tool checks: the bound belongs to the store, and this tool would return a longer note whole if the store ever held one.
- Nothing in this tool requires network access.
