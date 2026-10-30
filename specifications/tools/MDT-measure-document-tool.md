# Measure document tool

**Spec code:** `MDT`

## Intent
The tool an agent reaches for before it decides to read a whole document. It takes a project file or a draft and returns the size of its text — lines, characters, and an estimate of tokens — together with the size limit for documents in this conversation and whether the document is above it. A document can be far larger than an agent's context, for example a design description of fifty pages, and an agent that reads such a document whole leaves no room for the conversation. With this tool the agent learns the size first and then reads only the chunks it needs, with the line ranges of `RFT-read-file-tool.md` and `RDT-read-draft-tool.md`. Out of scope: reading any text of the document, which those two tools do; splitting or shortening a document, which is the author's work; and the limit itself, whose percent is a Global setting (`../core/GSS-global-settings-storage.md` GSS-FR-PCTL) and whose context window is the model's (`../core/AAP-ai-api-integrations.md` AAP-FR-RANS).

## Functional requirements
 1. **MDT-FR-DRFB** The tool exists as a `rig` portable tool named `measure_document` and conforms to `TLC-tool-conventions.md` in full: its name, description, parameter schema, output shape, refusal shape, logging, and read-only posture are that spec's rules applied to this tool.
 2. **MDT-FR-YFZQ** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05). Neither varies by turn, by window, or by limit.
 3. **MDT-FR-EEFA** The tool is constructed per turn (per TLC-FR-15). Its constructor binds the turn's session, the turn's context window, and the size-limit percent read when the turn starts. No argument names a window, a percent, or a model.
    - *Why:* The limit depends on the model that serves the turn, and a model must not be able to state a larger limit for itself.
 4. **MDT-FR-OTUZ** A call carries exactly one of `path` and `draft_id`. A call carrying both, or neither, is the retryable **not one target** refusal of the contract surface.
 5. **MDT-FR-TWVZ** `path` is project-relative and is resolved against the project's active worktree on the terms `RFT-read-file-tool.md` resolves its own `path`. A path that names no file, names a directory, is absolute, or escapes the project root is the retryable `NotFound` refusal of the contract surface.
 6. **MDT-FR-TVWI** `draft_id` is resolved, and the draft's live prompt is read, on the terms of `RDT-read-draft-tool.md` (RDT-FR-03, RDT-FR-04). An unknown draft is the retryable unknown-draft refusal, and a draft without its single live prompt is the retryable inconsistent-draft refusal.
 7. **MDT-FR-LWAK** `lines` counts the lines of the text with the line rule of `RFT-read-file-tool.md`: a line ends at a line feed, a last line without one still counts, and an empty text has 0 lines. The count agrees with the `offset` and `limit` ranges of `read_file` and `read_draft`.
 8. **MDT-FR-MYNF** `characters` counts the Unicode scalar values of the text as it decodes, with no line-ending normalisation.
 9. **MDT-FR-QYLP** `estimated_tokens` is `characters` divided by 4, rounded up: the fixed characters-per-token estimate of `../ai/CVL-conversation-loop.md` CVL-FR-PGII. The description and the parameter text call it an estimate. It is never a provider-reported usage value.
10. **MDT-FR-CTKN** `limit_tokens` is the turn's context window multiplied by the size-limit percent and divided by 100, rounded down. The window is the effective context size of the turn's model (per `../core/AAP-ai-api-integrations.md` AAP-FR-RANS), and the percent is the Global setting of `../core/GSS-global-settings-storage.md` GSS-FR-PCTL.
11. **MDT-FR-FHWP** `above_limit` is true when `estimated_tokens` is greater than `limit_tokens`, and false otherwise.
13. **MDT-FR-PPCH** The output carries the five fields of the contract surface and no text of the document, no path, and no metadata of the draft.
14. **MDT-FR-VHHI** A file or a prompt that does not decode as UTF-8, or whose read fails, is the retryable unreadable refusal of the contract surface, normalised to kind `Other`. No byte of it reaches the message or a log record.
15. **MDT-FR-OPKM** With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13).
16. **MDT-FR-CGXO** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file, no draft, and no store, and it touches no repository state. It answers from disk as it stands and waits on no index, scan, or watcher (per TLC-FR-16).
17. **MDT-FR-DAVA** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the target argument, and `estimated_tokens`, and a `WARN` record naming the tool, the target argument, and the reason when it refuses. `path` and `draft_id` are loggable, each bounded to 512 characters. No record carries text of the document.
18. **MDT-FR-VTTV** The tool reaches no network and touches no credential (per TLC-FR-18).

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`measure_document` — a `rig` portable tool, constructed per turn (MDT-FR-EEFA).

### The description
The fixed text the model reads (per TLC-FR-05):

> Measure the size of a project file or of a draft's prompt without reading it. Use this before you decide to read a whole document: it tells you how many lines and characters the document holds, an estimate of how many tokens it takes, and whether it is above the size limit for documents in this conversation. Pass `path` for a project file or `draft_id` for a draft, not both. The token count is an estimate, not an exact count. When the document is above the limit, do not read it whole: read only the parts you need, with `offset` and `limit`. It returns no text of the document and changes nothing.

### Arguments
```
MeasureDocumentArgs {
  path:     string?,   // project-relative path of a file; exactly one of path, draft_id
  draft_id: string?    // the id of a draft, whose live prompt is measured
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `path` — *"The project-relative path of a file to measure, exactly as it was reported to you. Leave it out when you pass `draft_id`."*
- `draft_id` — *"The ID of a draft to measure; its current prompt is measured. Leave it out when you pass `path`."*

### Output
Data (per TLC-FR-08):

```
MeasureDocumentOutput {
  lines:            integer,          // MDT-FR-LWAK
  characters:       integer,          // MDT-FR-MYNF
  estimated_tokens: integer,          // an estimate: characters / 4, rounded up (MDT-FR-QYLP)
  limit_tokens:     integer,          // never null; the window is always known (MDT-FR-CTKN)
  above_limit:      bool              // MDT-FR-FHWP
}
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **not one target** — kind `InvalidArgs`, retryable: *"Pass exactly one of `path`, for a project file, or `draft_id`, for a draft."*
- **no such file** — kind `NotFound`, retryable: *"The project holds no file at that path. Check the path, or search for the file again."*
- **unknown draft ID** — kind `NotFound`, retryable: *"No draft exists with that ID in the open project. Check the ID or search for the draft again."*
- **inconsistent draft** — kind `Other`, retryable: *"That draft does not contain its required single live prompt, so it cannot be measured. It must be resolved outside this tool before a later call can succeed."*
- **unreadable** — kind `Other`, retryable: *"That document could not be read. It may not be text, or it may have changed since it was found."*

### Side effects
None. The tool is read-only (MDT-FR-CGXO).

## Non-functional requirements
- A call costs one resolution and one whole-text read, the same as a whole read by `read_file` or `read_draft`, and returns a few numbers in place of the text.
- The estimate is deliberately simple. It is the same rule the conversation loop uses for its own budget, so the size an agent reads here and the size the discussion surface shows to the author agree.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences: this one measures a document, `RFT-read-file-tool.md`'s reads a project file, and `RDT-read-draft-tool.md`'s reads a draft.
