# List OpenSpec specs tool

**Spec code:** `LOS`

## Intent
The tool an agent uses to see which capabilities an OpenSpec project has specified. It takes no argument and returns each capability of `openspec/specs/` by id with its requirement count, as the OpenSpec CLI of the project reports them. An agent that helps to shape a draft in an OpenSpec project must know which capabilities already exist before it proposes a new one or a change to one, and a list of capability ids answers that in one call. It is the first tool of the set whose other tools read one capability (`ROS-read-openspec-spec-tool.md`) and list the changes in progress (`LOC-list-openspec-changes-tool.md`). Out of scope: the text of a capability, which `ROS-read-openspec-spec-tool.md` reads; a search by topic, which `SPS-specification-search-tool.md` does over the same files; and any write.

## Functional requirements
1. **LOS-FR-QKCD** The tool exists as a `rig` portable tool named `list_openspec_specs` and conforms to `TLC-tool-conventions.md` in full.
2. **LOS-FR-UPKL** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns a JSON Schema for an object with no properties (per TLC-FR-05).
3. **LOS-FR-OCCY** The tool is attached to a conversational turn only while OpenSpec mode is active and its CLI is usable (per `../ai/CVL-conversation-loop.md` CVL-FR-DPWP).
4. **LOS-FR-RWGX** The tool answers through `../core/OSM-openspec-mode.md`'s `list_capabilities()` (OSM-FR-JTOQ) at the moment of the call. It runs no CLI process of its own and reads no file of its own.
5. **LOS-FR-XHLX** The output holds one entry for each capability that `list_capabilities()` returns, in the same order, with the same `id` and `requirement_count`.
6. **LOS-FR-GHCZ** A project with no capability is a success with an empty `capabilities` list (per TLC-FR-12).
7. **LOS-FR-VOAB** With no project open, the tool gives the shared no-open-project refusal (per TLC-FR-13).
8. **LOS-FR-SBNB** A read that fails with `openspec_mode_inactive` or `cli_unavailable` gives the **OpenSpec unavailable** refusal of the contract surface.
9. **LOS-FR-IEDQ** A read that fails with `cli_timed_out`, `cli_failed`, or `cli_output_unreadable` gives the **OpenSpec read failed** refusal of the contract surface. No CLI output reaches the message.
10. **LOS-FR-GPCY** Every refusal of this tool is not retryable, because the tool takes no argument that could change the answer (per TLC-FR-11).
11. **LOS-FR-POAE** The tool is read-only (per TLC-FR-17). It changes no file, no setting, no index, and no other state.
12. **LOS-FR-PAFH** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record that names the tool and the number of capabilities it returned, or a `WARN` record that names the tool and the refusal.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-DPWP). It has no Tauri command, no event, and no frontend reach.

### The tool
`list_openspec_specs` — a `rig` portable tool, made by its own constructor and attached by a caller that names it (per TLC-FR-19).

### The description
The fixed text the model reads (per TLC-FR-05):

> List the capabilities this OpenSpec project has specified in `openspec/specs/`. Each entry gives the capability id and how many requirements it holds. Use this to find out which capabilities exist before you talk about a new one or a change to one. Then use `read_openspec_spec` with an id to read its requirements and scenarios. This does not list changes in progress, does not search, and changes nothing.

### Arguments
None. The parameter schema is an object with no properties.

### Output
```
ListOpenSpecSpecsOutput {
  capabilities: [
    {
      id:                 string,   // capability id, e.g. "auth" or "ui/theme"
      requirement_count:  integer
    }
  ]
}
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **OpenSpec unavailable** — kind `Other`, not retryable: *"OpenSpec cannot be read in this project now. The OpenSpec mode is off, or the OpenSpec CLI is not installed on this machine."*
- **OpenSpec read failed** — kind `Other`, not retryable: *"The OpenSpec CLI could not list the specs of this project. The OpenSpec files may be malformed."*

## Non-functional requirements
- A call costs one `list_capabilities()` read of `../core/OSM-openspec-mode.md`, which is one CLI process.
- The first sentence of the description tells it apart from `read_openspec_spec`, `list_openspec_changes`, and `search_specifications`.
- Nothing in this tool needs network access.
