# Read draft tool

**Spec code:** `RDT`

## Intent
The tool an agent reaches for once it knows which draft it wants and needs the whole of what that draft currently says. It takes a draft id and returns that draft's stable metadata together with the complete text of its **current live prompt**, so that a search which named a draft can be followed by reading it. It is the retrieval half of the pair whose other half is `DST-draft-search-tool.md`, exactly as `RFT-read-file-tool.md` is `SPS-specification-search-tool.md`'s: ranking tells an agent which planned work bears on a topic and this tool tells it what that work actually says, and an agent that can do both weighs a recommendation against the drafts already written rather than against their titles. It returns the **live** prompt and only that: the accepted history beside it is a record of versions the author settled and moved on from (`../core/DHS-draft-history.md`), and an agent handed a superseded version would be reasoning about a draft its author has already left behind. It is not a second `read_file`: a draft is addressed by its id rather than by a path, because a draft's position on disk is a fact about how drafts are filed rather than something a model should compose. Out of scope: discovering which draft to ask for, which `DST-draft-search-tool.md` answers; reading any version but the live one, which no argument here selects; reading a proposal's candidate, a review log, or a conversation log, none of which this tool reaches; and writing, editing, graduating, archiving, or deleting anything, this tool being read-only.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full, its output taking that spec's data-with-document shape for the reason RDT-FR-05 gives (per TLC-FR-08). It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`read_draft` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Read the complete current live prompt of a known draft. Use this after `search_drafts` gives you a draft ID, or when you already know that ID. It returns the draft's stable metadata together with its full current prompt; use the returned prompt path only as information, not as an argument to `read_file`. It never returns an accepted-history snapshot or another version, does not search for drafts, and changes nothing.

### Arguments
```
ReadDraftArgs {
  draft_id: string     // required
}
```

Its description in the parameter schema (per TLC-FR-06):

- `draft_id` — *"The draft ID returned by `search_drafts` or otherwise already known. This is not a file path, prompt path, history-entry ID, or version selector."*

### Output
```
ReadDraftOutput {
  draft_id,      // the draft's own id (DRS-FR-02), as it was resolved
  name,          // the draft's current name
  status,        // the draft's current DraftStatus (DRS-FR-03)
  prompt_path,   // draft-relative path of the live prompt; informational (RDT-FR-09)
  content        // the complete text of the live prompt (RDT-FR-06)
}
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank draft ID** — kind `InvalidArgs`, retryable: *"The draft ID must name a draft to read. Call again with the ID returned by `search_drafts`."*
- **unknown draft ID** — kind `NotFound`, retryable: *"No draft exists with that ID in the open project. Check the ID or search for the draft again."*
- **inconsistent draft** — kind `Other`, retryable: *"That draft does not contain its required single live prompt, so it cannot be read. It must be resolved outside this tool before a later call can succeed."*
- **unreadable prompt** — kind `Other`, retryable: *"That draft's prompt could not be read. It may not be text, or it may have changed since it was found."*

## Functional requirements
 1. **RDT-FR-01** The tool exists as a `rig` portable tool named `read_draft` and conforms to `TLC-tool-conventions.md` in full — its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
 2. **RDT-FR-02** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the one documented argument with the documented parameter description (per TLC-FR-05).
 3. **RDT-FR-03** The tool resolves the draft through `../core/DRS-draft-storage.md` (DRS-FR-38) at the moment of the call and reads the live prompt from that draft's own `files/` folder through the same resolution, composing no path of its own from the id (per DRS-FR-36). It consults no index, so a successful read reflects the newest persisted live prompt even while `../core/BMI-bm25-indexing.md`'s drafts index is still catching up, and a prompt saved a moment ago reads back at once.
 4. **RDT-FR-04** The tool reads the **live prompt and nothing else**. It never reads an accepted history entry (`../core/DHS-draft-history.md` DHS-FR-01), the candidate of a proposed change (`../core/DCP-draft-change-proposals.md` DCP-FR-01), a review log, or a conversation log, and it exposes no argument by which a model could name one: there is no version selector, no entry id, no revision, no timestamp, and no path. A draft's settled versions are review provenance the author reads in their own rail, and an agent recommending on the strength of one would be recommending against the draft as it now stands.
 5. **RDT-FR-05** The output is the structured `ReadDraftOutput` of the contract surface — a JSON object with stable named fields, carrying the complete prompt under `content` — which is the data-with-document case of TLC-FR-08 rather than the unwrapped-document form `RFT-read-file-tool.md` returns. The two arrive together because neither is usable alone: a prompt without its draft's name and status is material the model cannot place, and the metadata without the prompt is a row it cannot act on, so returning the document unwrapped would cost a second call for facts this call already resolved.
 6. **RDT-FR-06** `content` is the **complete** text of the live prompt, whatever its length. There is no size ceiling, no truncation, no elision, and no summarisation, and the tool exposes no `offset`, `limit`, or equivalent: an agent asked to weigh planned work needs the whole of what that work says before it recommends anything, and a partial prompt would produce a recommendation about a passage rather than about a draft.
 7. **RDT-FR-07** The result carries exactly `draft_id`, `name`, `status`, `prompt_path`, and `content`. The draft's `created_at`, `updated_at`, its filed folder, its build state, its graduation run, and its pending-proposal flag are omitted: each is a fact about how the author is managing the draft rather than about what it says, and a model can act on none of them.
 8. **RDT-FR-08** `name` and `status` are the draft's current record as `../core/DRS-draft-storage.md` holds it (DRS-FR-03). `status` is that record's own `DraftStatus` carried through unchanged and interpreted nowhere here: **every status draft storage retains is readable on identical terms**, this tool carrying no precondition on status and refusing for none, and a status added to that type reaches this tool's output without a change to it.
 9. **RDT-FR-09** `prompt_path` is draft-relative and informational, on the same terms `DST-draft-search-tool.md` returns it (DST-FR-11): it names the prompt's position inside the draft's own storage rather than a position in the project, so it is not a path `RFT-read-file-tool.md`'s tool resolves to this prompt, and the description tells the model so. It is returned because a model reasoning about a draft is helped by knowing what the prompt is called, not because it is an argument to anything.
10. **RDT-FR-10** `draft_id` is a draft's own opaque id (`../core/DRS-draft-storage.md` DRS-FR-02) and is what `DST-draft-search-tool.md` returns, accepted unchanged. It is not a path of any kind, not a `prompt_path`, not a history entry's id, and not a version selector; a value that is any of those names no draft and is therefore the unknown-draft refusal rather than being interpreted as something else.
11. **RDT-FR-11** A `draft_id` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface.
12. **RDT-FR-12** A `draft_id` naming no draft in the active worktree is the retryable `NotFound` refusal. A model that misremembered an id corrects it on a further call — or searches again — so telling it otherwise would cost it a draft the worktree holds (per TLC-FR-11).
13. **RDT-FR-13** A draft whose `files/` is not the single live prompt `../core/DRS-draft-storage.md` DRS-FR-11 requires is the retryable inconsistent-draft refusal, normalized to kind `Other`, carrying the typed `draft_not_single_file` that resolution reported (DRS-FR-15). It is retryable because the model can name another draft on its next call, and its message says the condition itself has to be resolved elsewhere so the model does not spend turns re-calling for this one. The tool chooses no file as the prompt on the author's behalf and repairs nothing.
14. **RDT-FR-14** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13), there being no worktree to resolve a draft in.
15. **RDT-FR-15** The tool answers from disk as it stands and never blocks on background work (per TLC-FR-16). It waits on no index pass, no scan, and no watcher, so a draft created since the last pass is readable before any pass has observed it, and one deleted since produces the unknown-draft refusal although a search taken a moment earlier still named it.
16. **RDT-FR-16** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no project file, no draft file, no history entry, no proposal, no comment log, no index, and no other state, and it touches no repository state. Reading a draft changes neither its `updated_at` nor its status, emits no `"drafts changed"` event, and leaves the prompt's own modification time as it was.
17. **RDT-FR-17** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `draft_id` the model asked for, and how many bytes of prompt it returned, and a `WARN` record naming the tool, the `draft_id` the model asked for, and the reason when it refuses. The `draft_id` is the argument this specification names as loggable under TLC-FR-14's exception, and it is answerable for it: a record saying only that a draft was read does not say which draft, so a turn's reads cannot be followed when an agent's reasoning is being traced, and a refusal is unfollowable without the argument that caused it. It is recorded as the model composed it — untrimmed — and bounded: a `draft_id` longer than 512 characters is recorded as its first 512 followed by an ellipsis, so that no value a model composes pushes a record past `../core/LGC-logging.md`'s per-record ceiling (LGC-FR-08). **No record carries the prompt's text, the draft's name, its status, or its prompt path** — the prompt is the author's own unpublished material and the rest are fields this call returned.
18. **RDT-FR-18** A draft that resolves and holds its one prompt, whose prompt nonetheless cannot be read — it does not decode as UTF-8, or the read itself failed — is the retryable unreadable-prompt refusal, normalized to kind `Other`. It is told apart from the two refusals either side of it because both would say something false about such a draft: the draft demonstrably exists, so the unknown-draft sentence would send the model searching for an id that was never wrong, and its `files/` holds exactly the one file it should, so the inconsistent-draft sentence would misdescribe its file set. No byte of the prompt reaches the message or any log record. A draft in this state is unreachable through `DST-draft-search-tool.md` in the first place — a prompt that does not decode contributes no chunks to any index (`../core/BMI-bm25-indexing.md` BMI-FR-09) — so the two tools never disagree about it, and this refusal is reached only by a model that already held the id.

## Non-functional requirements
- A call costs one draft resolution and one whole-prompt read, and nothing grows with how many drafts the worktree holds or how long a draft's history has become: the history folder is neither walked nor read.
- The absence of a size ceiling (RDT-FR-06) is the deliberate difference from `RFT-read-file-tool.md`, which offers paging for a file too large to want whole. A draft is one prompt an author is writing by hand, so it is bounded by what a person writes rather than by what a repository accumulates, and the paging that earns its place over a repository does not earn it here.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences — this one reads a draft it is given the id of, `DST-draft-search-tool.md`'s finds which drafts bear on a topic, `RFT-read-file-tool.md`'s reads a project file it is given the path of, and `SPS-specification-search-tool.md`'s finds which specifications bear on a topic.
- Nothing in this tool requires network access.
