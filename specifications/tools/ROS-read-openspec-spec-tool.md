# Read OpenSpec spec tool

**Spec code:** `ROS`

## Intent
The tool an agent uses to read what one capability of an OpenSpec project requires. It takes a capability id and returns the title, the overview, and each requirement of that capability with its scenarios, as the OpenSpec CLI of the project parses them, or only the one requirement that the agent names. An agent that helps to shape a draft must ground its advice in what the project has already specified, and in OpenSpec a requirement is known by its name, which is also the name a change cites when it modifies or removes that requirement. So this tool returns each requirement under that name. It is the read half of the pair whose list half is `LOS-list-openspec-specs-tool.md`. Out of scope: a delta file of a change, which this tool never reads; a search by topic, which `SPS-specification-search-tool.md` does; and any write.

## Functional requirements
1. **ROS-FR-XPYM** The tool exists as a `rig` portable tool named `read_openspec_spec` and conforms to `TLC-tool-conventions.md` in full.
2. **ROS-FR-AZNY** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05).
3. **ROS-FR-MHZE** The tool is attached to a conversational turn only while OpenSpec mode is active and its CLI is usable (per `../ai/CVL-conversation-loop.md` CVL-FR-DPWP).
4. **ROS-FR-XEHR** The tool answers through `../core/OSM-openspec-mode.md`'s `read_capability(id)` (OSM-FR-SIBT) at the moment of the call, with `capability` as the id. It runs no CLI process of its own and reads no file of its own.
5. **ROS-FR-BDJJ** With `requirement` absent, the output holds every requirement of the capability, in the order of the spec file, each with all its scenarios.
6. **ROS-FR-CSZP** With `requirement` present, the output holds only the requirements whose name equals it after both are trimmed and compared without regard to letter case. `id`, `title`, and `overview` do not change.
7. **ROS-FR-GNFU** A `requirement` that matches no requirement of the capability gives the retryable **unknown requirement** refusal.
8. **ROS-FR-FQHM** A `capability` that is blank once trimmed, or that `read_capability(id)` refuses with `invalid_capability_id`, gives the retryable **invalid capability** refusal.
9. **ROS-FR-RZRC** A read that fails with `capability_not_found` gives the retryable **unknown capability** refusal.
10. **ROS-FR-MOFU** A read that fails with `openspec_mode_inactive` or `cli_unavailable` gives the **OpenSpec unavailable** refusal, and a read that fails with `cli_timed_out`, `cli_failed`, or `cli_output_unreadable` gives the **OpenSpec read failed** refusal. No CLI output reaches the message.
11. **ROS-FR-QSEO** With no project open, the tool gives the shared no-open-project refusal (per TLC-FR-13).
12. **ROS-FR-SSKT** The output carries the text of each requirement and scenario whole. There is no size limit, no truncation, and no summary.
13. **ROS-FR-PZWT** The tool is read-only (per TLC-FR-17). It changes no file, no setting, no index, and no other state.
14. **ROS-FR-PRDW** Each call emits under the `ai` and `backend` domains (per TLC-FR-14). An `INFO` record names the tool, the `capability` the model asked for, whether a `requirement` was given, and how many requirements it returned. A `WARN` record names the tool, the `capability`, and the refusal. A `capability` longer than 512 characters is logged as its first 512 and an ellipsis.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full, with the data-with-document output of TLC-FR-08. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-DPWP). It has no Tauri command, no event, and no frontend reach.

### The tool
`read_openspec_spec` — a `rig` portable tool, made by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> Read one capability of this OpenSpec project from `openspec/specs/<capability>/spec.md`: its title, its overview, and each requirement with its scenarios. Use this when you need what a capability requires, or the exact name of a requirement. Get the capability id from `list_openspec_specs` first. Pass `requirement` with a requirement name to read only that requirement. A change cites a requirement by this name when it modifies or removes it. This does not read a change, does not search, and changes nothing.

### Arguments
```
ReadOpenSpecSpecArgs {
  capability:   string,    // required
  requirement:  string?    // optional; a requirement name
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `capability` — *"The capability id that `list_openspec_specs` returned, for example `auth` or `ui/theme`. This is not a file path."*
- `requirement` — *"The name of one requirement to read, as this tool or a change gives it. Letter case does not matter. Leave it out to read every requirement of the capability."*

### Output
```
ReadOpenSpecSpecOutput {
  id:            string,
  title:         string,
  overview:      string,
  requirements:  [
    {
      name:       string,
      text:       string,
      scenarios:  [ { name: string, text: string } ]
    }
  ]
}
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **invalid capability** — kind `InvalidArgs`, retryable: *"The capability must be an id that `list_openspec_specs` returned, such as `auth` or `ui/theme`. Call again with such an id."*
- **unknown capability** — kind `NotFound`, retryable: *"No capability with that id exists in this project. Call `list_openspec_specs` to see the ids that exist."*
- **unknown requirement** — kind `NotFound`, retryable: *"That capability has no requirement with that name. Call again without `requirement` to see the names of its requirements."*
- **OpenSpec unavailable** — kind `Other`, not retryable: *"OpenSpec cannot be read in this project now. The OpenSpec mode is off, or the OpenSpec CLI is not installed on this machine."*
- **OpenSpec read failed** — kind `Other`, not retryable: *"The OpenSpec CLI could not read this capability. Its spec file may be malformed."*

## Non-functional requirements
- A call costs one `read_capability(id)` read of `../core/OSM-openspec-mode.md`, which is one CLI process, whether or not a `requirement` is given.
- The first sentence of the description tells it apart from `list_openspec_specs`, `list_openspec_changes`, and `read_file`.
- Nothing in this tool needs network access.
