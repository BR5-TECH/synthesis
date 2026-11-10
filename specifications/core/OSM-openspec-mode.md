# OpenSpec mode

**Spec code:** `OSM`

## Intent
OpenSpec mode is the optional mode in which the application works with a project that uses OpenSpec on the terms of OpenSpec. OpenSpec keeps the live specifications of a project in `openspec/specs/<capability>/spec.md` and each planned change in `openspec/changes/<name>/`, and its CLI reads both and reports them as JSON. This module decides whether the open project is in OpenSpec mode, finds the OpenSpec CLI on this machine, and gives the rest of the backend one typed seam that reads capabilities, requirements, and changes through that CLI, and validates a change. A read runs in a **location**: the active worktree, or the working copy of one work stream of the project, because a change that a graduation run proposes lives on its stream before the author activates that stream. It parses no OpenSpec Markdown itself, so the OpenSpec version and the schemas of the project decide what a capability and a change contain. A project that does not use OpenSpec, and a project whose author sets the mode to off, work as if this module did not exist. Out of scope: the classification of OpenSpec files, which is `ASC-artifact-scanning.md`'s; the storage of the mode setting, which is `PSS-project-settings-storage.md`'s; the tools that give these reads to an agent, which are `../tools/LOS-list-openspec-specs-tool.md`, `../tools/ROS-read-openspec-spec-tool.md`, and `../tools/LOC-list-openspec-changes-tool.md`; and any write to a capability or a change, which this module never makes.

## Functional requirements
1. **OSM-FR-LLRV** The open project **uses OpenSpec** when its active worktree holds `openspec/config.yaml` as a regular file, or holds at least one regular file named `spec.md` below `openspec/specs/`.
2. **OSM-FR-QNFI** Detection reads the active worktree through `FSA-filesystem-access.md`, runs no program, and stops at the first file that satisfies OSM-FR-LLRV.
3. **OSM-FR-JGYD** Detection runs again at each read of the mode and keeps no result between two reads. A project that gains or loses its `openspec/` folder changes its mode at the next read.
4. **OSM-FR-UILN** **OpenSpec mode is active** when the OpenSpec mode setting of the project is `auto` (per `PSS-project-settings-storage.md` PSS-FR-IMDN) and the project uses OpenSpec. In every other case, and while no project is open, the mode is inactive.
5. **OSM-FR-RMUD** While OpenSpec mode is inactive, the application behaves as for a project that does not use OpenSpec. The classification of OpenSpec files is the one exception (per `ASC-artifact-scanning.md` ASC-FR-NBNP).
6. **OSM-FR-EZKW** The **OpenSpec CLI** is the first executable named `openspec` (`openspec.cmd` on Windows) in the directories of `AIC-agentic-integrations.md` AIC-FR-03, then in the per-user Node directories of the contract surface. Each read resolves it again.
7. **OSM-FR-ZNVZ** The CLI is **usable** when `openspec --version` exits with status zero and prints a version whose major number is 1 or more. A check result is kept for one executable path and one modification time. A `failed` result is kept for 30 seconds at most, and the next read after that checks again.
   - *Why:* Version 1 is the first with `status --json`, a check at every read starts Node for nothing, and one timeout must not turn the mode off until a restart.
8. **OSM-FR-SKUP** The CLI state is exactly one of `available` with its path and version, `not_found`, `unsupported` with its path and version, and `failed` with its path and a reason code.
9. **OSM-FR-HHUD** Each CLI process runs in the root of the working copy of its location (OSM-FR-MFVN), the active worktree where none is named, with its standard input closed, with `OPENSPEC_TELEMETRY=0`, `DO_NOT_TRACK=1`, and `NO_COLOR=1` set, and with the directory of the executable first on its `PATH`.
   - *Why:* The CLI is a Node script that finds Node through `PATH`, and an application started from the desktop inherits a short `PATH`.
10. **OSM-FR-ITZI** A CLI process gets no credential and no variable that the application keeps for an agent launch. It gets the environment of the application, changed only as OSM-FR-HHUD states.
11. **OSM-FR-VALR** The seam runs only the CLI subcommands `--version`, `list`, `show`, `status`, and `validate`. It never runs a subcommand that writes, such as `init`, `update`, `new`, `archive`, or `config set`.
12. **OSM-FR-ZIMO** A CLI process that runs for more than 30 seconds is stopped, and its read fails with `cli_timed_out`.
13. **OSM-FR-WWWK** A CLI process that exits with a status other than zero fails its read with `cli_failed`, which carries the exit status. Output that does not decode as JSON of the expected shape fails the read with `cli_output_unreadable`. A field that the expected shape does not name is ignored.
14. **OSM-FR-XSOC** A read while OpenSpec mode is inactive fails with `openspec_mode_inactive` and starts no process. A read while the CLI is not usable fails with `cli_unavailable`, which carries the CLI state.
15. **OSM-FR-QHRV** At most four CLI processes run at the same time for one application. A read that would start a fifth waits until one ends.
16. **OSM-FR-JTOQ** `list_capabilities()` runs `openspec list --specs --json` and returns one `Capability` for each entry, ordered by id.
17. **OSM-FR-JAIC** `read_capability(id)` refuses an id that is empty, absolute, holds a `..` segment, or holds a character outside `A-Z`, `a-z`, `0-9`, `.`, `_`, `-`, and `/`, with `invalid_capability_id`. It runs no process for that id.
18. **OSM-FR-SIBT** `read_capability(id)` fails with `capability_not_found` when `openspec/specs/<id>/spec.md` is not a regular file in the active worktree. Otherwise it runs `openspec show <id> --type spec --json --no-interactive` and returns the `CapabilitySpec`.
19. **OSM-FR-OPZF** `list_changes()` returns one `ChangeSummary` for each active change. It runs `openspec list --changes --json` and `openspec status --all --json` once each, and does not include a change below `openspec/changes/archive/`.
20. **OSM-FR-FCMY** The `capabilities` of a `ChangeSummary` are the ids of the capabilities that the change holds delta files for: each folder path below `openspec/changes/<name>/specs/` that holds a `spec.md`, ordered by id. The list comes from the directory listing alone.
21. **OSM-FR-YXUC** `get_openspec_status()` returns the stored setting, whether the project uses OpenSpec, whether the mode is active, and the CLI state. It checks the CLI also while the mode is inactive, and it fails for no CLI state.
22. **OSM-FR-IFWM** Every read emits through `LGC-logging.md` under the `backend` domain. An `INFO` record names the subcommand, the exit status, the duration, and the byte count of the output. A failure emits a `WARN` record with its code. No record holds an argument value, a path inside the project, or any CLI output.
23. **OSM-FR-MFVN** A read names a **location**: the active worktree, or a work stream of the project and its working copy (per `WKS-work-streams.md` WKS-FR-JGCA). A stream that is not live fails the read with `unknown_stream` and starts no process. Only the graduation loop names a third kind, the **review checkout** of a run (per `../ai/GRL-graduation-loop.md` GRL-FR-YKRI).
24. **OSM-FR-DDHL** Whether OpenSpec mode is active is decided on the active worktree for every location (OSM-FR-UILN). Files of a change are read through `FSA-filesystem-access.md` in the working copy of its location.
25. **OSM-FR-IHUO** A **change name** is valid when it is 1 to 64 characters of lowercase `a-z` and `0-9` in groups joined by single `-`, and is not `archive`. `valid_change_name(name)` answers this, and every read that takes a change name refuses an invalid one with `invalid_change_name` and starts no process.
26. **OSM-FR-BCJO** `read_change(location, name)` fails with `change_not_found` when `openspec/changes/<name>/` is not a folder in the working copy of its location. Otherwise it runs `openspec status --change <name> --json` and `openspec show <name> --type change --json --deltas-only --no-interactive` once each.
27. **OSM-FR-LLIO** `read_change` returns each artifact of the change's schema in the build order of the schema, with its status and the change-relative paths of its files. Each file carries its whole text, read in the working copy. A file that cannot be read as UTF-8 text carries no text and is marked `unreadable`.
28. **OSM-FR-NMHS** `read_change` returns each delta with its capability, its operation (`ADDED`, `MODIFIED`, `REMOVED`, or `RENAMED`), and the requirement it carries, or the names of a rename. For a delta of a capability that exists in the working copy, it carries the **live requirement** of the same name, read through `read_capability` (OSM-FR-SIBT) once for each capability.
   - *Why:* A requirement diff needs the text the change replaces, and OpenSpec matches a delta to a live requirement by name.
29. **OSM-FR-PYHP** A delta whose requirement has no live requirement of the same name carries none. A `RENAMED` delta carries the live requirement of its old name.
30. **OSM-FR-APVF** `validate_change(location, name)` runs `openspec validate <name> --type change --strict --json --no-interactive` and returns whether the change is valid, with each issue's level, path, and message.
31. **OSM-FR-XDRN** For `validate`, an exit status of 1 whose output decodes as the expected shape is an answer that the change is not valid. Any other status other than zero fails the validation with `cli_failed` (OSM-FR-WWWK).
32. **OSM-FR-MBTI** `"read openspec change"` is the Tauri command over `read_change`. It runs on a blocking thread and returns the `ChangeDetail` or the error code of the read.

## Contract surface
The module owns one Tauri command and an internal Rust API. The internal API has no Tauri command, and the tools of `../tools/` and `../ai/CVL-conversation-loop.md` are its consumers.

### Tauri commands
- `"get openspec status"` → `get_openspec_status()` → `OpenSpecStatus`. Read-only. It changes no setting and writes no file (OSM-FR-YXUC).
- `"read openspec change"` → `read_openspec_change(location, name)` → `ChangeDetail`, or the code of an `OpenSpecReadError`. Read-only (OSM-FR-MBTI).

```
OpenSpecModeSetting = "auto" | "off"

OpenSpecStatus {
  setting:        OpenSpecModeSetting,  // PSS-FR-IMDN; "auto" while unset
  uses_openspec:  boolean,              // OSM-FR-LLRV
  active:         boolean,              // OSM-FR-UILN
  cli:            OpenSpecCliState      // OSM-FR-SKUP
}

OpenSpecCliState =
  | { state: "available",   path: string, version: string }
  | { state: "not_found" }
  | { state: "unsupported", path: string, version: string }
  | { state: "failed",      path: string, reason: "not_executable" | "timed_out" | "no_version" }
```

### Internal API

```
openspec_mode_active() -> bool                     // OSM-FR-UILN
openspec_cli_state() -> OpenSpecCliState           // OSM-FR-ZNVZ
list_capabilities() -> Result<[Capability], OpenSpecReadError>
read_capability(id: string) -> Result<CapabilitySpec, OpenSpecReadError>
list_changes() -> Result<[ChangeSummary], OpenSpecReadError>
valid_change_name(name: string) -> bool                                         // OSM-FR-IHUO
read_change(location: ChangeLocation, name: string) -> Result<ChangeDetail, OpenSpecReadError>
validate_change(location: ChangeLocation, name: string) -> Result<ChangeValidation, OpenSpecReadError>

ChangeLocation =
  | { kind: "active_worktree" }
  | { kind: "stream", stream_id: string }
  | { kind: "review_checkout", root: string }   // the graduation loop alone (OSM-FR-MFVN)

ChangeDetail {
  name:             string,
  location:         ChangeLocation,
  schema:           string,
  is_complete:      boolean,
  completed_tasks:  integer,
  total_tasks:      integer,
  artifacts:        [ChangeArtifact],     // in the build order of the schema (OSM-FR-LLIO)
  deltas:           [ChangeDelta]         // in the order the CLI gives them (OSM-FR-NMHS)
}

ChangeArtifact {
  id:      string,
  status:  "done" | "ready" | "blocked" | "skipped",
  files:   [{ path: string, text: string | null, unreadable: boolean }]   // change-relative paths
}

ChangeDelta {
  capability:  string,
  operation:   "ADDED" | "MODIFIED" | "REMOVED" | "RENAMED",
  requirement: Requirement | null,        // null for a RENAMED delta
  rename:      { from: string, to: string } | null,
  live:        Requirement | null         // OSM-FR-NMHS, OSM-FR-PYHP
}

ChangeValidation {
  valid:   boolean,
  issues:  [{ level: "ERROR" | "WARNING" | "INFO", path: string, message: string }]
}

Capability {
  id:                 string,     // "auth", "ui/theme"
  requirement_count:  integer
}

CapabilitySpec {
  id:            string,
  title:         string,
  overview:      string,
  requirements:  [Requirement]    // in the order of the spec file
}

Requirement {
  name:       string,             // the name OpenSpec matches deltas on
  text:       string,
  scenarios:  [Scenario]
}

Scenario {
  name:  string,
  text:  string                   // the raw text of the scenario section
}

ChangeSummary {
  name:             string,
  schema:           string,       // the schema name the change resolves to
  artifacts:        [ArtifactState],   // in the build order of the schema
  is_complete:      boolean,      // every artifact of the schema is done
  completed_tasks:  integer,
  total_tasks:      integer,
  capabilities:     [string],     // OSM-FR-FCMY
  last_modified:    string        // RFC 3339
}

ArtifactState {
  id:      string,                // "proposal", "specs", "design", "tasks"
  status:  "done" | "ready" | "blocked" | "skipped"
}

OpenSpecReadError =
  | "no_open_project"
  | "openspec_mode_inactive"
  | { code: "cli_unavailable", cli: OpenSpecCliState }
  | "invalid_capability_id"
  | "capability_not_found"
  | "cli_timed_out"
  | { code: "cli_failed", exit_status: integer }
  | "cli_output_unreadable"
  | "unknown_stream"                     // OSM-FR-MFVN
  | "invalid_change_name"                // OSM-FR-IHUO
  | "change_not_found"                   // OSM-FR-BCJO
```

### CLI fields read
- `list --specs --json`: `specs[].id`, `specs[].requirementCount`.
- `show <id> --type spec --json`: `id`, `title`, `overview`, `requirements[].name`, `requirements[].text`, `requirements[].scenarios[].name`, `requirements[].scenarios[].rawText`.
- `list --changes --json`: `changes[].name`, `changes[].completedTasks`, `changes[].totalTasks`, `changes[].lastModified`.
- `status --all --json`: `changes[].changeName`, `changes[].schemaName`, `changes[].isComplete`, `changes[].artifacts[].id`, `changes[].artifacts[].status`.
- `status --change <name> --json`: `schemaName`, `isComplete`, `artifacts[].id`, `artifacts[].status`, `artifactPaths.<id>.existingOutputPaths`.
- `show <name> --type change --json --deltas-only`: `deltas[].spec`, `deltas[].operation`, `deltas[].requirement` (`name`, `text`, `scenarios[].name`, `scenarios[].rawText`), `deltas[].rename.from`, `deltas[].rename.to`.
- `validate <name> --type change --strict --json`: `items[0].valid`, `items[0].issues[].level`, `items[0].issues[].path`, `items[0].issues[].message`.

### Per-user Node directories
Searched after the directories of `AIC-agentic-integrations.md` AIC-FR-03, in this order:
- `~/.local/share/pnpm`
- `~/Library/pnpm` (macOS)
- each `~/.nvm/versions/node/<version>/bin`, the highest version first

## Non-functional requirements
- No read needs network access. The CLI runs with its telemetry off.
- A read costs one CLI process for each subcommand it names, plus file probes. The cost of `list_changes()` does not grow with the number of changes beyond one directory listing for each change.
- The seam works with any OpenSpec schema that a project defines, because it reads artifact ids and states from the CLI and names no artifact itself.
- Every requirement here can be tested without the OpenSpec CLI, through a process seam that returns fixed output.
