# OpenSpec spec index

**Spec code:** `OSI`

## Intent
In OpenSpec mode the specification map (per `../ui/SMP-specification-map.md`) shows the project's own OpenSpec specifications and not a demo index. This module builds that index from the reads of `OSM-openspec-mode.md`: the live capabilities and their requirements, put in groups by the first segment of the capability id; a chip for each active change on each capability that it touches, in the active worktree and on every live work stream; and the result of one strict validation as the completeness of each requirement. The architect then sees in one place what the live specifications hold, which of them are not valid, and which changes are about to alter them. OpenSpec records no dependency between capabilities and no organization of the author, so the index holds neither. Out of scope: the rendering of the map, which is the map specs' own; any write; the index of a project that is not in OpenSpec mode, which stays the demo index of `../ui/SMP-specification-map.md` SMP-FR-HWIC; and a cache of the index beyond the map session.

## Functional requirements
1. **OSI-FR-OSWD** `load_specification_map()` answers `{ source: "demo" }` where a project is open and OpenSpec mode is not active (per `OSM-openspec-mode.md` OSM-FR-UILN), and starts no process. Otherwise it answers `{ source: "openspec", index }`, with the index that this module builds.
2. **OSI-FR-XUJG** The index has exactly three levels, named `groups` and `group`, `capabilities` and `capability`, and `requirements` and `requirement`. A root node is a **capability group**, the children of a group are capability nodes, and the children of a capability node are requirement nodes, which are the spec nodes of `../ui/SMP-specification-map.md` SMP-FR-FEJV.
3. **OSI-FR-SJNC** The live capabilities are read in the active worktree: `list_capabilities` once (per OSM-FR-JTOQ), then `read_capability` once for each capability (per OSM-FR-SIBT). A read that fails fails the load with the code of the read.
4. **OSI-FR-MAUQ** A capability whose id holds a `/` belongs to the group of the first path segment of its id. A capability whose id holds no `/` belongs to the **root group**. The root group comes first where it has a capability, and the other groups follow in the ascending order of their segments.
5. **OSI-FR-WFJU** A group has the id `group:<segment>` and the label `<segment>`, and the root group has the id `group:/` and the label `specs`. A group's summary is empty. Capability nodes follow the ascending order of their ids.
6. **OSI-FR-OHJE** A capability node has the id `capability:<capability-id>`, the `title` of the capability as its label, or its id where the title is empty, and its `overview` as its summary.
7. **OSI-FR-SEYB** A requirement node stands for one requirement of the capability, in the order of the spec file. Its `code` is `<capability-id>#<n>`, `n` being its position from 1, its `badge` is `R<n>`, its label is the requirement name, its summary is the requirement text, `requirements` is 1, and `scenarios` is its count of scenarios.
8. **OSI-FR-CWLE** Every requirement node of a capability has the `path` `openspec/specs/<capability-id>/spec.md`, relative to the project root.
9. **OSI-FR-JIJM** The index holds no dependency, so `dependencies` is empty, and no node has a hue.
10. **OSI-FR-DVLQ** The load runs `validate_project` in the active worktree with the scope `all` (per OSM-FR-BXIF). An issue of a spec item whose `path` starts with `requirements[<i>]` belongs to the requirement at the position `i`, counted from 0, of that capability. Every other issue of a spec item belongs to the capability.
11. **OSI-FR-OJRE** A requirement node is `gap` where an issue of level `ERROR` or `WARNING` belongs to it or to its capability, and `built` otherwise. The index gives no node the state `verified` or `drafted`.
    - *Why:* OpenSpec records whether a requirement is valid, and nothing that says a test verifies it or that it is only drafted.
12. **OSI-FR-SBVF** Each node holds in `issues` the issues that belong to it, each with its level and its message, in the order the CLI gives them. An issue of a spec item whose id names no live capability belongs to no node.
13. **OSI-FR-OIOB** A validation that fails gives every requirement node the state `built` and no issue, and the index then holds `validation: "unavailable"`. A validation that answers gives `valid` where every item is valid, and `invalid` otherwise.
14. **OSI-FR-QELX** The active changes are read with `list_changes` (per OSM-FR-OPZF) in the active worktree and in the working copy of each live work stream of the project (per `WKS-work-streams.md` WKS-FR-SGCM). Where the active worktree is the working copy of a stream, it is read once, as the location of that stream.
15. **OSI-FR-ALQI** Each active change gives one **change chip** on the capability node of each id in its `capabilities` (per OSM-FR-FCMY). A chip names the change and its location: the active worktree, or a stream with its id and its name. The chips of a node are ordered by change name, then the active worktree first, then by stream name.
16. **OSI-FR-VOOS** A capability id of a change that names no live capability gives one **planned capability node** under its group (OSI-FR-MAUQ), with `planned` true, the capability id as its label, an empty summary, and no requirement node. It holds the chips of every change that names that id.
17. **OSI-FR-DENT** A location whose `list_changes` fails gives no chip, and the load does not fail for it. The index holds in `unreadLocations` the count of such locations.
18. **OSI-FR-GRCV** An issue of a change item of the validation belongs to the chip of that change at the location the validation ran in: the active worktree location, or the stream whose working copy the active worktree is (OSI-FR-QELX). That chip holds it in `issues`. A chip of any other location holds no issue.
19. **OSI-FR-VTAD** `"load specification map"` runs on a blocking thread, and its CLI processes are bounded by OSM-FR-QHRV. This module keeps no result between two loads, and the map session holds the index (per `../ui/SMP-specification-map.md` SMP-FR-QNUH).
20. **OSI-FR-PJJO** The load fails with `no_project_open` where no project is open. Every other failure of the load is the code of the read of OSI-FR-SJNC that failed, `cli_unavailable` included.
21. **OSI-FR-YVZW** Each load emits through `LGC-logging.md` under the `backend` domain an `INFO` record with the counts of groups, capabilities, requirements, chips, issues, and unread locations, and the duration, or a `WARN` record with the failure code. No record holds a label, a summary, a text, an issue message, a change name, or a capability id.
22. **OSI-FR-QLCS** Every behaviour of this module is exercisable with no OpenSpec CLI and no network, through the process seam of `OSM-openspec-mode.md`.

## Contract surface

### Tauri commands
Names match `../ui/SMP-specification-map.md` byte-for-byte:

- `"load specification map"` → `load_specification_map()` → `SpecificationMapLoad`, or the failure code of OSI-FR-PJJO. Read-only.

### Payload shapes
```text
SpecificationMapLoad =
  | { source: "demo" }                         // OSI-FR-OSWD: the UI shows its demo index
  | { source: "openspec", index: OpenSpecMapIndex }

OpenSpecMapIndex {
  levels,              // [{ plural: "groups", singular: "group" },
                       //  { plural: "capabilities", singular: "capability" },
                       //  { plural: "requirements", singular: "requirement" }]
  roots,               // [IndexNode], the groups (OSI-FR-MAUQ)
  dependencies,        // [], always (OSI-FR-JIJM)
  validation,          // "valid" | "invalid" | "unavailable" (OSI-FR-OIOB)
  unreadLocations      // integer (OSI-FR-DENT)
}

IndexNode {
  id, label, summary,
  children,            // [IndexNode] for a group, [SpecNode] for a capability
  planned?,            // true for a planned capability node (OSI-FR-VOOS)
  issues?,             // [MapIssue] of a capability (OSI-FR-SBVF)
  changes?             // [ChangeChip] of a capability (OSI-FR-ALQI)
}

SpecNode {
  code,                // "<capability-id>#<n>"
  badge,               // "R<n>"
  path,                // "openspec/specs/<capability-id>/spec.md", project-relative
  label, summary,
  requirements,        // 1
  scenarios,
  state,               // "built" | "gap" (OSI-FR-OJRE)
  issues?              // [MapIssue]
}

ChangeChip {
  change,
  location,            // { kind: "active_worktree" } | { kind: "stream", streamId, streamName }
  issues?              // [MapIssue], the location the validation ran in alone (OSI-FR-GRCV)
}

MapIssue {
  level,               // "ERROR" | "WARNING" | "INFO"
  message
}
```

## Non-functional requirements
- A load costs one `list` process, one `show` process for each capability, one `validate` process, and two processes for each location of OSI-FR-QELX, with at most four at the same time. A project of 40 capabilities and 5 streams loads in a few seconds.
- No load needs network access, and no load writes a file.
