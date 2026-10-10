# Project settings storage

**Spec code:** `PSS`

## Intent
The persistence home for every **project-scoped** setting, partitioned into the two project storage scopes so the right fact lives in the right file: settings that are part of the project and committed to its Git repository, and settings that are per-machine state for the current checkout. Both stores live inside the project's **active worktree**, so a fact that legitimately differs between branches — the adapter configuration a branch expects, the comparison target a branch is reviewed against — differs with the checkout. Exposes typed Tauri commands per UI section so that the per-section dirty/save behaviour of `../ui/SET-project-settings.md` has a one-to-one backend counterpart. User-global (cross-project) settings, and the per-project facts that must stay stable across worktrees, live in the paired `GSS-global-settings-storage.md`.

## Contract surface
The module owns two persisted stores and one set of Tauri commands per UI consumer.

### Storage scopes
Both stores are resolved inside the project's active worktree (per `WTC-worktree-context.md` WTC-FR-03), which is the project root `PST-project-storage.md` PST-FR-21 resolves every path against.

- **Project-public** — `.synthesis/project.toml` inside the active worktree. Committed to the project's Git repository. Holds **target repo bindings**, **adapter configuration by id**, **plugin enablement** (which installed plugins are active in this project), **MCP server connections**, the **line-ending convention** every artifact write obeys, the optional **draft template** every new draft is created holding, the **Docker image configuration** the project's agentic work runs in — one entry per agentic CLI vendor (PSS-FR-22) — the three **shared loop settings** the agent loops are bounded by (PSS-FR-TQMV, PSS-FR-HDBN, PSS-FR-WPKS), the **GitHub polling settings** — the selected GitHub Project and the polling interval (PSS-FR-OPQD) — the **GitHub publication settings** — the parent issue Types, the sub-issue Type, and the sub-issue milestone policy (PSS-FR-HWBG) — and other project-shared configuration.
- **Project-local** — `.synthesis/local.toml` inside the active worktree. **Gitignored**. Holds the **Changes panel state** (the selected mode and the configured target branch), the **Project panel state** (which folders the tree renders expanded and the panel's two local filters), the **Notes panel state** (the scope selector's position and the panel's filter text), the **publication remote selection** (the GitHub remote a draft publication reuses), the **GitHub pending claims** (PSS-FR-TXBB), the **Drafts panel state** (which drafts folders the tree renders expanded and the panel's two filters), and any other state specific to this machine for this checkout.

The third scope, **user-global**, is not owned here; it lives in `GSS-global-settings-storage.md` (recent projects, app preferences — theme and the main window's full-screen state — plugin and adapter registries, the GitHub token registry, the agentic integration registry, the AI API provider registry, the Docker backend configuration and its verification state, and the per-project slot holding layout preferences, the active worktree, the GitHub token binding, the agentic integration override, and the AI API integration override).

### Tauri commands

Shell / layout (per `../ui/SNV-shell-navigation.md`; persisted in the user-global per-project slot, not in either project store — payload per `GSS-global-settings-storage.md` GSS-FR-17, with the vertical panel's width carried as a fraction of the shell's inner width):
- `"load layout preferences"` → `load_layout_preferences`
- `"save layout preferences"` → `save_layout_preferences`

Project settings window (per `../ui/SET-project-settings.md`):
- `"load project config"` → `load_project_config` (project-public)
- `"save project config"` → `save_project_config` (project-public)
- `"enable plugin"` → `enable_plugin` (writes project-public)
- `"disable plugin"` → `disable_plugin` (writes project-public)
- `"configure plugin (id, config)"` → `configure_plugin` (writes project-public)
- `"configure adapter (id, config)"` → `configure_adapter` (writes project-public)
- `"list MCP server connections"` → `list_mcp_server_connections` (project-public)
- `"add MCP server connection"` → `add_mcp_server_connection` (project-public)
- `"remove MCP server connection"` → `remove_mcp_server_connection` (project-public)
- `"test MCP server connection"` → `test_mcp_server_connection` (read-only)
- `"load project docker images"` → `load_project_docker_images` (project-public) → `ProjectVendorImageStatus[]`, one per agentic CLI vendor in a stable order, configured or not
- `"save project vendor image"` → `save_project_vendor_image(vendor, entry)` (writes project-public) → the updated `ProjectVendorImageStatus`, or a typed validation error
- `"build project vendor image"` → `build_project_vendor_image(vendor)` → `{ operation_id }`, or a typed refusal. Builds the vendor's configured Dockerfile locally; publishes nothing
- `"cancel project vendor image build"` → `cancel_project_vendor_image_build(operation_id)` → the operation is asked to stop; the terminal result arrives on the event below

```
AgenticCliVendor = "claude_code" | "codex" | "opencode"

ProjectVendorImage {
  vendor:      AgenticCliVendor,
  image_name:  string,          // required
  tag:         string | null,   // optional; Docker's own default tag applies when absent
  dockerfile:  string | null    // optional; project-relative
}

ProjectVendorImageStatus {
  vendor,
  configuration:     "unset" | "configured",
  image_reference:   string | null,     // "<image_name>" or "<image_name>:<tag>"
  dockerfile:        string | null,
  dockerfile_state:  "absent" | "valid" | "invalid",
  dockerfile_problem:
      "absolute_path" | "escapes_project_root" | "not_a_regular_file" | null,
  graduation_state:  "usable" | "image_name_missing" | "dockerfile_invalid"
                   | "execution_unsupported"
}
```

The shared loop settings ride in the project-public payload `load_project_config` returns and `save_project_config` writes:

```
SharedLoopSettings {
  execution_timeout_ms:      integer | null,   // PSS-FR-TQMV; 1_000 ..= 21_600_000
  provider_call_deadline_ms: integer | null,   // PSS-FR-HDBN; 1_000 ..= 3_600_000
  retry_budget:              integer | null    // PSS-FR-WPKS; 1 ..= 10
}
```

`null` is the unset state of PSS-FR-ZLCF, in which each loop uses its own default.

The graduation concurrency limit rides in the same payload:

```
GraduationConcurrencyLimit = integer | "unlimited"   // PSS-FR-JRWC; integer is 1 or more

ProjectConfigGraduation {
  graduation_concurrency_limit: GraduationConcurrencyLimit   // read: always present; write: absent leaves it unchanged
}
```

Validation refusals of `save_project_vendor_image`: `image_name_empty`, `dockerfile_absolute`, `dockerfile_escapes_project_root`, `dockerfile_not_a_regular_file`. Refusals of `build_project_vendor_image`: `dockerfile_unset`, `image_name_empty`, `build_already_running`, and `docker_backend_unverified` carried through from `GSS-global-settings-storage.md` GSS-FR-40.

Changes panel (per `../ui/CHG-changes.md`; project-local scope):
- `"load changes panel state"` → `load_changes_panel_state`
- `"save changes panel state"` → `save_changes_panel_state`

Project panel (per `../ui/LIB-library.md`; project-local scope):
- `"load library panel state"` → `load_library_panel_state`
- `"save library panel state"` → `save_library_panel_state`

Notes panel (per `../ui/NTS-notes.md`; project-local scope):
- `"load notes panel state"` → `load_notes_panel_state`
- `"save notes panel state"` → `save_notes_panel_state`

Drafts panel (per `../ui/DRP-drafts-panel.md`; project-local scope):
- `"load drafts panel state"` → `load_drafts_panel_state`
- `"save drafts panel state"` → `save_drafts_panel_state`

### Events (Tauri event bus)

- `"project image build progress"` — emitted while a build runs. Payload: `{ vendor, operation_id, phase, completed?, total?, message }`.
- `"project image build finished"` — emitted exactly once per build. Payload: `{ vendor, operation_id, outcome: "succeeded" | "failed" | "cancelled", image_reference?, diagnostic? }`.

### Internal (Rust API, not registered as a Tauri command)

- `load_publication_remote_selection()` → `{ name, url } | null` and `save_publication_remote_selection(selection)` — the project-local GitHub remote a draft publication reuses, `null` clearing it (PSS-FR-TQFB). Neither is registered as a Tauri command; `GHP-github-publication.md` (GHP-FR-PWXA) is the only caller.
- `load_github_polling_settings()` → `{ project_node_id, interval_minutes }` and `save_github_polling_settings(settings)` — the project-public GitHub polling settings (PSS-FR-OPQD). Neither is registered as a Tauri command; `GPP-github-polling.md` (GPP-FR-NLPG) is the only caller.
- `load_github_publication_settings()` → `{ parent_issue_types, sub_issue_type, sub_issue_milestone_policy }` and `save_github_publication_settings(settings)` — the project-public GitHub publication settings (PSS-FR-HWBG). Neither is registered as a Tauri command; `GHP-github-publication.md` (GHP-FR-KVRH) is the only caller.
- `load_github_pending_claims()` → `GithubPendingClaim[]` and `save_github_pending_claims(claims)` — the project-local pending claims (PSS-FR-TXBB), in the shape `GPP-github-polling.md` declares. Neither is registered as a Tauri command; `GPP-github-polling.md` (GPP-FR-DHQM) is the only caller.
- `resolve_project_vendor_image(vendor)` — returns the image reference the open project's committed configuration holds for that vendor, or the typed refusal `vendor_image_unconfigured`. It is the single read path an execution takes to the project's image (PSS-FR-30), it is not registered as a Tauri command, and it resolves to nothing but what `.synthesis/project.toml` records.

This module exposes **no recently-edited artifacts list** and no command that records or serves one (PSS-FR-12, PSS-FR-13). The Dashboard's Recently edited widget is served from the project's current artifact records and the filesystem instead, by `PST-project-storage.md`'s `list_recently_edited_artifacts` (PST-FR-31).

The registry reads `list_installed_plugins` and `list_agent_adapters` are owned by `GSS-global-settings-storage.md`; the Project settings window consumes them to render its enablement/configuration controls but the enablement/configuration **writes** above are owned here.

## Functional requirements
1. **PSS-FR-01** Project-public and project-local are two independent stores; together with the user-global store (`GSS-global-settings-storage.md` GSS-FR-02) the three scopes are mutually exclusive — a key appears in exactly one scope, and if a value must be observed in more than one scope the spec designates the canonical one.
2. **PSS-FR-02** Project-public config is read from `<active_worktree>/.synthesis/project.toml`. The file is created by `PST-project-storage.md::create_project` (PST-FR-02) and is always present for an opened project; it is committed to the project's Git repository, so a repository with several worktrees holds one copy per worktree and the active one is authoritative.
3. **PSS-FR-03** Project-local config is read from `<active_worktree>/.synthesis/local.toml`. The file is gitignored via `FSA-filesystem-access.md::ensure_gitignored`. If missing on open, defaults are returned and the file is created on first save. Because it is gitignored it is genuinely per-worktree: each checkout accumulates its own, and none is ever copied between worktrees.
4. **PSS-FR-04** Every save command writes its store atomically via `FSA-filesystem-access.md::write_toml_atomic`.
5. **PSS-FR-05** Each save command corresponds to one logical section of `../ui/SET-project-settings.md` so that the UI's per-section dirty/save model (SET-FR-08 of that spec) is one-to-one with backend operations. `save_layout_preferences`, `save_changes_panel_state`, `save_library_panel_state`, and `save_notes_panel_state` are independent of every other save command and are not tied to any Project settings tab section — they are driven by the shell, the Changes panel, the Project panel, and the Notes panel respectively, not by the settings UI.
6. **PSS-FR-06** `load_layout_preferences` and `save_layout_preferences` read and write the open project's layout through the user-global per-project slot (`GSS-global-settings-storage.md` GSS-FR-17) rather than either project store, so the shell's shape is a property of the project and survives a change of active worktree unchanged. When no project is open, the pair falls back to a global default slot under `FSA-filesystem-access.md::app_data_dir()`, so the first project opened inherits a sensible layout.
7. **PSS-FR-07** Plugin **enablement** lives in project-public; `enable_plugin`, `disable_plugin`, and `configure_plugin` mutate only project-public. Plugin **installation** is user-global and owned by `GSS-global-settings-storage.md` GSS-FR-11.
8. **PSS-FR-08** Adapter **configuration** lives in project-public; `configure_adapter` mutates only project-public. Adapter **installation** is user-global and owned by `GSS-global-settings-storage.md` GSS-FR-12.
9. **PSS-FR-09** MCP server connections are project-public; `add_mcp_server_connection` and `remove_mcp_server_connection` mutate project-public, and `test_mcp_server_connection` is read-only and mutates no store.
10. **PSS-FR-10** Reading a missing or malformed project-local file is repaired to defaults; reading a missing or malformed project-public file is reported as a typed error (since it implies a damaged or wrong-versioned project), which `../ui/SET-project-settings.md` surfaces inline.
11. **PSS-FR-11** All commands listed above exist with typed payloads in the walking-skeleton build. Stub implementations may keep state in memory and persist nothing, provided the same shape is returned to the UI.
12. **PSS-FR-12** The project-local store holds **no recently-edited artifacts list**. No command of this module writes one, no artifact save records into one (per `PST-project-storage.md` PST-FR-17), and `.synthesis/local.toml` gains no such key however much a project is edited. The Dashboard's Recently edited widget reads the project's current artifact records and the modification times of their source files instead (per `PST-project-storage.md` PST-FR-31), so what the widget shows is the state of the project rather than a per-machine history of what this application saved.
13. **PSS-FR-13** This module's contract surface carries **no recently-edited command**: nothing here records an artifact as recently edited and nothing here serves an ordered list of artifact ids for that purpose. There is therefore no operation for a caller to invoke, and no second opinion anywhere about which artifacts are recent.
14. **PSS-FR-14** A `recently_edited` key found in an existing `.synthesis/local.toml` is **ignored on every read and dropped on the next write**. Reading the project-local store never applies it to any returned value, so persisted data from an earlier layout influences no command and no widget; and the next `save_*` command that rewrites that file writes the store without it, through `FSA-filesystem-access.md::write_toml_atomic` (PSS-FR-04), so the key does not survive. No migration command exists and none is needed: the key is dropped by the ordinary write path, and a project never written to again keeps a key that changes nothing.
15. **PSS-FR-15** The project-local store holds the **Changes panel state**: the panel's selected mode (`uncommitted` or `branch`) and its configured target branch name. `save_changes_panel_state(state)` persists it and `load_changes_panel_state()` returns it, so each machine — and each worktree, since the store is per-worktree (PSS-FR-03) — keeps its own comparison target rather than committing it to the repository. When no state has been persisted for the active worktree, `load_changes_panel_state()` returns the `uncommitted` mode with no target branch, and `../ui/CHG-changes.md` CHG-FR-06 supplies the initial target from the repository's default branch. This store never validates that the persisted branch still exists; a stale branch surfaces in the UI per `../ui/CHG-changes.md` CHG-FR-24.
16. **PSS-FR-16** Changing the project's active worktree (per `WTC-worktree-context.md` WTC-FR-08) re-resolves both project stores against the new content root: the next `load_project_config`, `load_changes_panel_state`, `load_library_panel_state`, `load_notes_panel_state`, and `load_drafts_panel_state` read the new worktree's files, and every subsequent save writes there. No value is carried over from the outgoing worktree's stores and none is copied into the new one.

17. **PSS-FR-17** The project-public store holds the project's **line-ending convention** — `lf` or `crlf` — read through `load_project_config` and written through `save_project_config`. It is project-public because it is a convention of the project that everyone who checks it out shares, and it defaults to `lf` when the key is absent. `PST-project-storage.md` PST-FR-22 normalises every artifact write to it, and it is the single value edited by both `../ui/STB-status-bar.md` STB-FR-16 and `../ui/SET-project-settings.md` SET-FR-10. Because `save_project_config` is a whole-store write, persisting a change to the convention carries every other project-public section through unchanged, and persisting any other section carries the convention through unchanged.

18. **PSS-FR-18** The project-local store holds the **Project panel state**: `expanded_paths` (the worktree-root-relative paths of the folders the Project panel renders expanded), `artifact_type_filter` (`all_artifacts`, `all_files`, or one of the eight built-in artifact types of `ASC-artifact-scanning.md` ASC-FR-02), and `text_filter` (the panel's in-tree search string). `save_library_panel_state(state)` writes the record whole — a caller changing one field supplies the other two unchanged — and `load_library_panel_state()` returns it, so each machine and each worktree keeps its own tree shape rather than committing it to the repository. When nothing has been persisted for the active worktree, `load_library_panel_state()` returns `{ expanded_paths: [], artifact_type_filter: "all_artifacts", text_filter: "" }`, the state `../ui/LIB-library.md` LIB-FR-12 and LIB-FR-15 render for a project first opened on this machine. This store never validates that a persisted path still names an existing folder: an absent path is returned unchanged rather than pruned, and `../ui/LIB-library.md` LIB-FR-16 decides what the tree does with it.

19. **PSS-FR-19** The project-local store holds the **Notes panel state**: `scope_position` (`entity`, `project`, or `all`) and `text_filter` (the panel's filter string). `save_notes_panel_state(state)` writes the record whole — a caller changing one field supplies the other unchanged — and `load_notes_panel_state()` returns it, so each machine and each worktree keeps its own view of the Notes panel rather than committing it to the repository. When nothing has been persisted for the active worktree, `load_notes_panel_state()` returns `{ scope_position: "entity", text_filter: "" }`. This store never validates that the persisted position is renderable: `entity` is returned unchanged even when no entity-scoped tab is active, and `../ui/NTS-notes.md` NTS-FR-09 decides what the panel renders in that case.

20. **PSS-FR-20** The project-local store holds the **Drafts panel state**: `status_filter` (`active`, `archived`, `graduated`, or `all`), `text_filter` (the panel's filter string), and `expanded_folders` (the drafts-root-relative paths of the drafts folders the panel renders expanded, per `DRS-draft-storage.md` DRS-FR-29). `save_drafts_panel_state(state)` writes the record whole — a caller changing one field supplies the other two unchanged — and `load_drafts_panel_state()` returns it, so each machine and each worktree keeps its own view of the drafts tree rather than committing it to the repository. When nothing has been persisted for the active worktree, `load_drafts_panel_state()` returns `{ status_filter: "active", text_filter: "", expanded_folders: [] }`, the state `../ui/DRP-drafts-panel.md` DRP-FR-07 and DRP-FR-14 render for a worktree first opened on this machine. This store never validates that a persisted path still names an existing drafts folder: an absent path is returned unchanged rather than pruned, and `../ui/DRP-drafts-panel.md` DRP-FR-14 decides what the tree does with it. The paths recorded here name the panel's own folders and never a path inside a draft's file tree, which no store holds.

21. **PSS-FR-21** The project-public store holds the project's optional **draft template**: the Markdown text a new draft is created holding (per `DRS-draft-storage.md` DRS-FR-39). It is project-public because it is a convention of the project that everyone who checks it out shares, so it is committed with `.synthesis/project.toml` and resolves from the active worktree like every other project-public value (PSS-FR-02, PSS-FR-16). It is read through `load_project_config` and written through `save_project_config`, which is a whole-store write on the terms of PSS-FR-17: persisting a template carries every other project-public section through unchanged, and persisting any other section carries the template through unchanged. **Unset and stored are distinct states.** While no template is configured the key is absent from `project.toml` and `load_project_config` reports the template as unset rather than as empty text, which is what tells `DRS-draft-storage.md` to create a draft on its empty-prompt terms (DRS-FR-06). A `save_project_config` carrying a template whose text is empty **removes the key** rather than recording an empty value, so deleting a template's content returns the project to the unset state and no configured empty template is ever persisted. A missing or malformed `project.toml` is the typed error of PSS-FR-10 rather than an absent template, so a damaged store is never read as a cleared one and `../ui/SET-project-settings.md` SET-FR-18 surfaces it instead of replacing what is configured. This store holds the template's text and nothing about how it is edited: the template is bytes here, exactly as the line-ending convention is a value here.

22. **PSS-FR-22** The project-public store holds the project's **Docker image configuration**: one entry per agentic CLI vendor — `claude_code`, `codex`, and `opencode` — each carrying an image name, an optional tag, and an optional project-relative Dockerfile path. It is project-public because the image a project's agentic work runs in is a property of the project that everyone who checks it out shares, so it is committed with `.synthesis/project.toml` and resolves from the active worktree like every other project-public value (PSS-FR-02, PSS-FR-16). `load_project_docker_images()` returns one status per vendor whether or not that vendor is configured, so a caller never has to reason about an absent record, and `save_project_vendor_image(vendor, entry)` writes one vendor's entry on the whole-store terms of PSS-FR-17: persisting an image entry carries every other project-public section through unchanged, and persisting any other section carries every image entry through unchanged. No Docker backend value is stored here — the mode, the endpoint, and the CLI path are user-global and live in `GSS-global-settings-storage.md` (GSS-FR-35).
23. **PSS-FR-23** The **image name is required** and the **tag is optional**. An entry whose image name is empty once trimmed is refused with `image_name_empty` and nothing is written. Where a tag is configured the entry's image reference is `<image_name>:<tag>`; where it is omitted the reference is the image name alone and Docker applies its own default tag, `latest`. The reference this store composes is the single string a build targets (PSS-FR-26) and an execution resolves (PSS-FR-30), so neither composes one of its own.
24. **PSS-FR-24** The **Dockerfile path is optional and project-relative**. A configured path is validated through `FSA-filesystem-access.md` rather than a bare path predicate, against the project root `PST-project-storage.md` PST-FR-21 resolves every path against, and three things are refused: an absolute path with `dockerfile_absolute`, a path that escapes the project root with `dockerfile_escapes_project_root`, and a path that does not identify a regular file with `dockerfile_not_a_regular_file`. A refused save writes nothing and leaves the entry exactly as it was. An entry that configures a Dockerfile must also carry an image name, because that name is the build target and the reference graduation runs (PSS-FR-23).
25. **PSS-FR-25** `load_project_docker_images()` reports **what each vendor's entry is worth to this project**, in fields a surface can render without a second opinion. `configuration` is `unset` while the committed project holds no entry for that vendor and `configured` once it holds one. `dockerfile_state` tells an **absent optional Dockerfile** (`absent`) apart from a **configured one that is invalid for this project** (`invalid`, carrying the problem of PSS-FR-24) and from one that is valid here (`valid`). `graduation_state` is `execution_unsupported` for `opencode`, whose configuration is accepted and stored but which cannot be executed; `image_name_missing` where no image name is configured; `dockerfile_invalid` where a configured Dockerfile fails PSS-FR-24; and `usable` otherwise. Reading the store validates and mutates nothing: a Dockerfile that has been deleted since it was configured is reported `invalid` rather than removed from the entry.
26. **PSS-FR-26** `build_project_vendor_image(vendor)` builds the vendor's configured Dockerfile **locally**, using the entry's image reference as the build target and the **project root as the build context**, through the verified backend `GSS-global-settings-storage.md`'s `resolve_docker_backend()` returns (GSS-FR-40); an unverified backend refuses with `docker_backend_unverified` and starts nothing. In Bollard / Docker Engine mode the build runs over the Docker Engine connection at the configured endpoint and streams that connection's image-build output, and the Docker CLI path takes no part in it. In Docker CLI mode the build invokes the verified executable with the equivalent of `docker build --file <project-relative-Dockerfile> --tag <image[:tag]> <project-root>`. Whichever backend runs it, the build **does not push, publish, log in to a registry, or modify registry state** — no `--push`, no registry login, no publish operation — and it writes neither an image reference nor a digest into the shipped vendor-image manifest (per `../infra/AVI-agent-vendor-images.md` AVI-FR-07). A vendor with no configured Dockerfile refuses with `dockerfile_unset`.
27. **PSS-FR-27** A build **starts as an indeterminate operation and may become determinate** once the backend reports a total. Each `"project image build progress"` event carries the vendor, the operation id, the phase the build is in, the optional completed and total values, and a message written to be displayed to the author. Backend output is read and republished as it arrives rather than at the end, and reporting never blocks the build or the window: a build that produces output faster than a consumer reads it still runs at the speed of the build. The operation additionally attributes through `PRG-progress-reporting.md` on that module's opt-in terms (PRG-FR-11), so the status bar shows a build in flight without this module reporting progress twice to one surface.
28. **PSS-FR-28** A build has **exactly one terminal result** — `succeeded`, `failed`, or `cancelled` — delivered once as `"project image build finished"`. A success names the image reference that was built. A failure carries a safe, user-displayable diagnostic: it names what the backend reported and carries no credential, no registry secret, and nothing the Docker daemon holds beyond the build's own output. A cancellation is explicit — it follows `cancel_project_vendor_image_build` and nothing else — and leaves the project's image configuration exactly as it was. No terminal result of any kind rewrites `.synthesis/project.toml`: a build reads the entry and never edits it.
29. **PSS-FR-29** **At most one build runs for a project at a time.** A second `build_project_vendor_image` while one is running refuses with `build_already_running` and starts nothing. A terminal result of any kind releases the project at once, so a failed build neither blocks nor delays a later one; and nothing here retries a build, whatever it failed on — another build happens because the author asked for one.
30. **PSS-FR-30** `resolve_project_vendor_image(vendor)` is the single read path an execution takes to the project's image, and it resolves **only what the committed project configures**: the entry's image reference, or `vendor_image_unconfigured` where the project holds no entry or no image name for that vendor. There is no fallback to the shipped vendor image of `../infra/AVI-agent-vendor-images.md` and no other source it can resolve from, so a project that configures nothing runs nothing. It is not registered as a Tauri command, so no frontend call reaches it; `../tools/EAC-execute-agent-cli.md` calls it on every launch (EAC-FR-38).
31. **PSS-FR-JRWC** The project-public store holds the **graduation concurrency limit**: how many graduation runs may hold a project slot at the same time (per `GRD-graduation.md` GRD-FR-KKKN). Stream runs and direct runs count together. The limit is a positive integer or the named value `unlimited`, and it defaults to one.
    - *Why:* One is the safe default because a second agent working at the same time competes for the machine, and the author raises it only when they want that.
32. **PSS-FR-KMBT** A stored integer below one, a stored value that is not an integer, and a stored text other than `unlimited` are repaired to one on read, on the terms PSS-FR-10 repairs any malformed project-public value. A stored positive integer is read as it is, even where the Graduation section does not list it.
33. **PSS-FR-FHQU** `unlimited` is stored as the text `unlimited` and never as an integer. `load_project_config` returns the limit as a positive integer or the text `unlimited`. `save_project_config` writes an integer below one as one.
34. **PSS-FR-ZVSD** A `save_project_config` payload that names no concurrency limit leaves the stored limit as it stands. A payload that names one stores it. A saved limit that is higher than the stored one offers every graduation queue a dispatch at once (per `GRD-graduation.md` GRD-FR-IJKV).
35. **PSS-FR-LGKY** A project that stores its limit under the earlier key `streamConcurrencyLimit` keeps that limit. Where `graduationConcurrencyLimit` is absent, `load_project_config` reads `streamConcurrencyLimit` as the graduation concurrency limit, and PSS-FR-KMBT repairs it in the same way. Where `graduationConcurrencyLimit` is present, it is the limit and `streamConcurrencyLimit` is not read. A `save_project_config` payload that names a limit stores it under `graduationConcurrencyLimit` and removes `streamConcurrencyLimit` from `project.toml`. A payload that names no limit leaves both keys as they stand.
36. **PSS-FR-TQMV** The project-public store holds one shared **execution timeout**: how long one agent turn may run before it is stopped, in milliseconds. It governs the graduation loop (per `../ai/GRL-graduation-loop.md` GRL-FR-REPL) and every conversation turn whose provider stores no turn timeout of its own (per `../ai/CVL-conversation-loop.md` CVL-FR-16). Valid values are integers from 1,000 to 21,600,000.
37. **PSS-FR-HDBN** The project-public store holds one shared **provider-call deadline**: how long one call to a model may run before it is abandoned, in milliseconds. It governs every loop that calls a model, which today is `../ai/CVL-conversation-loop.md` alone (CVL-FR-17). Valid values are integers from 1,000 to 3,600,000.
38. **PSS-FR-WPKS** The project-public store holds one shared **retry budget**: how many attempts a bounded loop may spend before it stops for the author. Valid values are integers from 1 to 10. Each loop keeps its own meaning for the number — the graduation loop counts passes, and the conversation loop counts physical attempts of one provider call.
39. **PSS-FR-ZLCF** Each shared setting is **unset or stored**, and the two states are distinct. An unset setting has no key in `project.toml` and leaves each loop at its own default. A value outside its bounds, or one that is not an integer, is repaired to unset on read; a malformed file stays the typed error of PSS-FR-10.
    - *Why:* One value for two loops is only safe where the author chose it, so an absent setting must leave each loop at the bound its own module states.
40. **PSS-FR-JXOU** The three shared settings and the graduation concurrency limit are read through `load_project_config` and written through `save_project_config`, which is a whole-store write on the terms of PSS-FR-17. A caller that reads a setting reads the committed project and no cache, so every read reports what the store holds at that moment.

41. **PSS-FR-TQFB** The project-local store holds the **publication remote selection**: the `name` of a configured Git remote and its canonicalized `url`, written by `save_publication_remote_selection(selection)` and read by `load_publication_remote_selection()`. It is project-local because it is one author's choice on one machine rather than a convention of the project.
42. **PSS-FR-NKRE** The selection is a **hint and never an authority**: this module validates neither the remote's presence nor the token's access, and returns a stored selection unchanged for its reader to reject (per `GHP-github-publication.md` GHP-FR-HVQG). `save_publication_remote_selection(null)` clears it, and a missing or malformed value reads as `null`.
43. **PSS-FR-OPQD** The project-public store holds the **GitHub polling settings**: the selected GitHub Project's stable node ID and the polling interval in minutes. Each is unset or stored, an unset value has no key, and writing one carries every other project-public section through unchanged on the terms of PSS-FR-17.
44. **PSS-FR-CNZO** A stored interval that is not one of `1`, `5`, `15`, `30`, and `60`, and a stored Project node ID that is not a non-empty string, each read as unset. A malformed file stays the typed error of PSS-FR-10.
45. **PSS-FR-TXBB** The project-local store holds the **GitHub pending claims**: a list of records, each naming a repository host, owner, and name, an issue number, an issue URL, a Project node ID, an optional shadow draft ID, and the claim instant. Writing the list carries every other project-local section through unchanged.
46. **PSS-FR-TXJV** A pending claim record that is malformed or incomplete is dropped on read. A record without a repository host reads as `github.com`. A missing list reads as empty. The pending claims are never written to the project-public store, so they are never committed.
47. **PSS-FR-HWBG** The project-public store holds the **GitHub publication settings** in a table of their own, separate from the polling settings: the parent issue Types, the sub-issue Type, and the sub-issue milestone policy. A value that is unset reads as its default: `Feature` for the parent Types, `Task` for the sub-issue Type, and `inherit_parent` for the policy.
48. **PSS-FR-RDXE** A stored publication value that is not valid reads as its default: an empty Type list, an empty or blank Type name, a policy outside `inherit_parent`, `no_milestone`, and `author_selected`. A Type that GitHub does not list is valid and is returned unchanged. A malformed file stays the typed error of PSS-FR-10.
49. **PSS-FR-VCZM** Writing the publication settings carries every other project-public section through unchanged on the terms of PSS-FR-17, the polling settings included. Writing the polling settings carries the publication settings through unchanged. The settings are never written to the project-local store.

## Non-functional requirements
- All project settings load on project open without network access.
- Settings reads on the hot path (e.g. layout-preferences load during window mount) must complete in a single round-trip via Tauri commands; no chained loads.
- Missing project-local files are repaired silently; missing project-public files surface as typed errors.
- `load_project_docker_images` completes without network access, without reaching a Docker daemon, and without running any executable: it reads the committed configuration and stats the paths that configuration names.
- A build is the one operation here that reaches a container runtime, and it reaches the one the author already verified rather than probing for one of its own.
