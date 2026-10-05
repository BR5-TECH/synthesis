# Adapters

**Spec code:** `ADP`

## Intent
Agent adapters are the plugin category that powers the **injection** primitive (Notion: Injection & execution). An adapter knows the target agent's expected file layout, transforms Synthesis artifacts into that layout, places them in a target directory, and (when the run is interactive) launches the target agent and streams its output. This spec covers **agent adapters only**; AI-provider adapters and storage adapters are named in `PLG-plugins-system.md` and get their own specs later. The injection commands here have **no UI consumer**: no surface of the application invokes them, and the run they can produce reaches the user only through the Runs panel (`../ui/RUN-runs.md` RUN-FR-09), which reads runs it never starts. They stand as the backend half of a primitive whose front end the application does not render, so they may be removed if none materialises.

## Contract surface

### UI-facing (Tauri commands)
Injection (no UI consumer yet):
- `"list available adapters"` → `list_available_adapters` — returns enabled agent adapters in the currently open project, each as `{ id, name, kind: "agent" }`. Plugins in state `unsupported` are excluded.
- `"list configured MCP destinations"` → `list_configured_mcp_destinations` — returns `[]` in v1.
- `"perform injection (artifact id, target, adapter)"` → `perform_injection(artifact_id, target, adapter_id)` — returns an injection record `{ target, timestamp, run_id? }`. When the chosen adapter runs interactively, `run_id` is populated and the Runs panel subscribes to its events.

Runs (per `../ui/RUN-runs.md`):
- `"start run"` → `start_run(...)` — initiated externally, typically by `perform_injection`. Exposed as a command so callers outside the window (e.g. a CLI in future) can drive it; no UI consumer invokes it (per `../ui/RUN-runs.md` RUN-FR-09).
- `"cancel run"` → `cancel_run(run_id)`.
- `"stop run"` → `stop_run(run_id)`.
- Events emitted on the Tauri event bus:
  - `"run output line (text, level)"` → payload `{ run_id, text, level }`.
  - `"run state changed (running, finished, failed, cancelled)"` → payload `{ run_id, state ∈ { running, finished, failed, cancelled } }`.

### Plugin-facing (Rust trait outline)
Each agent adapter implements an `AgentAdapter` trait covering the four Injection concerns from the Notion source. The full trait surface is part of the deferred plugin-facing spec called out by `PLG-plugins-system.md`; for v1, the four concerns are:

- **Resolution** — given an `artifact_id` (and, in future, a playbook), resolve all referenced files, compute their checksums (via `FSA-filesystem-access.md::sha256_file`), and detect what has changed since the previous injection of the same artifact into the same target.
- **Transformation** — convert resolved Synthesis artifacts into the target agent's expected files and layout. Examples per the Notion source: Claude Code wants `CLAUDE.md` + `.claude/skills/`; Cursor wants `.cursorrules` or `.cursor/rules/`; Codex wants `AGENTS.md`; the Generic adapter copies raw Markdown to a configurable location.
- **Placement** — write the transformed files into the target directory and record an injection manifest there listing what was written, so the next injection can update or clean up.
- **Holdout separation** — exclude any scenarios marked as holdout from the injected set during execution; holdout content is only injected into the separate validation context the coding agent never sees.

### Built-in adapters
The v1 host ships four built-in agent adapters (per the Notion source). Only **Generic** is fully functional in the walking skeleton; the other three are registered and selectable but return a typed "not yet implemented" error when `perform_injection` is called against them.

- `claude-code` — stub (not yet implemented).
- `cursor` — stub (not yet implemented).
- `codex` — stub (not yet implemented).
- `generic` — functional. Copies the artifact's referenced files verbatim into the target directory, writes a minimal injection manifest, and (for now) does not launch any external process; `run_id` is omitted.

## Functional requirements
1. **ADP-FR-01** Agent adapters are registered with the plugin host (`PLG-plugins-system.md`). Built-in adapters are present from first launch; their entries in `list_installed_plugins` carry `category = agent_adapter`.
2. **ADP-FR-02** `list_available_adapters` returns the subset of registered agent adapters whose state in the currently open project is `enabled`. With no project open, the command returns a typed "no project open" error.
3. **ADP-FR-03** `list_configured_mcp_destinations` returns `[]` in v1. The command exists so that a caller choosing an injection destination has a list to read, empty though it is.
4. **ADP-FR-04** `perform_injection(artifact_id, target, adapter_id)` invokes the chosen adapter's resolution → transformation → placement → holdout-separation flow against `target`. On success it returns `{ target, timestamp, run_id? }`.
5. **ADP-FR-05** When the chosen adapter is interactive, `perform_injection` returns with a `run_id` and the adapter begins emitting `run output line` and `run state changed` events tagged with that `run_id`. The Runs panel (`../ui/RUN-runs.md`) subscribes to those events; this module does not render output itself.
6. **ADP-FR-06** The injection record returned by `perform_injection` is persisted into project storage so that an artifact's last-injected metadata is durable across launches. No surface reads it back in v1. The exact storage shape is owned by `PST-project-storage.md`; this module is a caller.
7. **ADP-FR-07** Re-injecting the same artifact into the same target is **idempotent**: the adapter updates files in place and refreshes the injection manifest rather than appending duplicates. This follows the Notion source's "track what was injected (injection manifest) for updates and cleanup" requirement.
8. **ADP-FR-08** Scenarios marked as **holdout** are excluded from the injected set during agent execution. The exclusion is enforced by every adapter, not by callers. (Holdout-scenario authoring is not part of v1 UI; the rule is captured here so the contract is in place when holdout authoring lands.)
9. **ADP-FR-09** `cancel_run(run_id)` requests cooperative cancellation: the adapter is notified and attempts to stop the underlying process. `stop_run(run_id)` is a harder request and may forcibly terminate the underlying process. Both transition the run to state `cancelled` once the adapter acknowledges. Both are best-effort.
10. **ADP-FR-10** The v1 host emits **one run at a time** (matches `../ui/RUN-runs.md` RUN-FR-02). Starting a second `perform_injection` while a run is in flight either queues behind the current run or returns a typed "run in progress" error; the choice is reported in the response shape, for the caller to act on.
11. **ADP-FR-11** The `generic` adapter is functional in the walking skeleton. The `claude-code`, `cursor`, and `codex` adapters are registered and visible in `list_available_adapters` (when enabled) but return a typed "not yet implemented" error from `perform_injection`.
12. **ADP-FR-12** All adapter-driven file writes into a target directory go through path-escape checks (per `FSA-filesystem-access.md` FSA-FR-10): an adapter must not write outside the target directory passed to it.

## Non-functional requirements
- Run output events stream incrementally; the Runs panel can render them as they arrive (`../ui/RUN-runs.md` Non-functional requirements).
- `cancel_run` and `stop_run` are best-effort; the contract guarantees the request is delivered, not that the process terminates instantly.
- Re-injection should perform incremental updates where possible (skip files whose `sha256_file` matches the previous manifest), to keep latency low on large playbooks. Optional in the walking skeleton.
