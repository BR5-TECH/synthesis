# List OpenSpec changes tool

**Spec code:** `LOC`

## Intent
The tool an agent uses to see which changes an OpenSpec project has in progress. It takes no argument and returns each active change of `openspec/changes/` with the state of each of its artifacts, its task progress, and the capabilities it holds deltas for. An agent that helps to shape a draft must know which planned work already touches the same capabilities, so that it can point the author to that work instead of advising a second change that collides with it. It also lets an agent answer how far a change has got, by text or by voice, from the state OpenSpec itself reports and not from a guess. Out of scope: an archived change, which this tool never lists; the text of a change's artifacts, which `RFT-read-file-tool.md` reads by path; and any write.

## Functional requirements
1. **LOC-FR-ZDJW** The tool exists as a `rig` portable tool named `list_openspec_changes` and conforms to `TLC-tool-conventions.md` in full.
2. **LOC-FR-YVDP** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns a JSON Schema for an object with no properties (per TLC-FR-05).
3. **LOC-FR-DHDA** The tool is attached to a conversational turn only while OpenSpec mode is active and its CLI is usable (per `../ai/CVL-conversation-loop.md` CVL-FR-DPWP).
4. **LOC-FR-DSMK** The tool answers through `../core/OSM-openspec-mode.md`'s `list_changes(location)` (OSM-FR-OPZF) in the active worktree location, at the moment of the call. It runs no CLI process of its own and reads no file of its own.
5. **LOC-FR-MGXX** The output holds one entry for each `ChangeSummary` that `list_changes(location)` returns, ordered by `last_modified` with the most recent first, and with every field of that summary.
6. **LOC-FR-XTUV** Each entry carries a `path`, which is `openspec/changes/<name>/`, so that the model can read an artifact of the change with `read_file`.
7. **LOC-FR-UJOO** A project with no active change is a success with an empty `changes` list (per TLC-FR-12).
8. **LOC-FR-XPNP** With no project open, the tool gives the shared no-open-project refusal (per TLC-FR-13).
9. **LOC-FR-YEQH** A read that fails with `openspec_mode_inactive`, `cli_unavailable`, or `cli_output_unreadable` gives the **OpenSpec unavailable** refusal. `cli_timed_out` and `cli_failed` give the **OpenSpec did not answer** refusal, and `cli_rejected` gives the **OpenSpec read failed** refusal. No CLI output and no diagnostic reaches the message.
10. **LOC-FR-UBZT** Every refusal of this tool is not retryable, because the tool takes no argument that could change the answer (per TLC-FR-11), and the seam already ran a transient read twice (per `../core/OFP-openspec-failure-policy.md` OFP-FR-PQNS).
11. **LOC-FR-IIQY** The tool is read-only (per TLC-FR-17). It changes no file, no setting, no index, and no other state.
12. **LOC-FR-KZDF** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record that names the tool and the number of changes it returned, or a `WARN` record that names the tool and the refusal.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-DPWP). It has no Tauri command, no event, and no frontend reach.

### The tool
`list_openspec_changes` — a `rig` portable tool, made by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> List the changes in progress in this OpenSpec project, from `openspec/changes/`. Each entry gives the change name, its schema, the state of each artifact (proposal, specs, design, tasks, or those of its schema), the tasks done and the tasks in total, and the capabilities it holds deltas for. Use this to find planned work that touches the same capabilities, or to say how far a change has got. Read an artifact with `read_file` below the folder `path` names, for example `<path>proposal.md`. Archived changes are not listed. This changes nothing.

### Arguments
None. The parameter schema is an object with no properties.

### Output
```
ListOpenSpecChangesOutput {
  changes: [
    {
      name:             string,
      path:             string,       // "openspec/changes/<name>/"
      schema:           string,
      artifacts:        [ { id: string, status: "done" | "ready" | "blocked" | "skipped" } ],
      is_complete:      boolean,
      completed_tasks:  integer,
      total_tasks:      integer,
      capabilities:     [string],
      last_modified:    string        // RFC 3339
    }
  ]
}
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **OpenSpec unavailable** — kind `Other`, not retryable: *"OpenSpec cannot be read in this project now. The OpenSpec mode is off, or the OpenSpec CLI on this machine is missing, too old, or does not work."*
- **OpenSpec did not answer** — kind `Other`, not retryable: *"The OpenSpec CLI did not finish, also after one more try. Do not call this tool again in this turn."*
- **OpenSpec read failed** — kind `Other`, not retryable: *"The OpenSpec CLI refused to list the changes of this project. The OpenSpec files may be malformed."*

## Non-functional requirements
- A call costs one `list_changes(location)` read of `../core/OSM-openspec-mode.md`: two CLI processes and one directory listing for each change.
- The first sentence of the description tells it apart from `list_openspec_specs`, `read_openspec_spec`, and `search_drafts`.
- Nothing in this tool needs network access.
