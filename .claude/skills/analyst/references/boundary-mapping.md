# Boundary mapping

Every request touches one or more of three layers. You must classify each distinct piece of the request before drafting, and confirm the classification with the user. This is where misunderstood requests are caught cheaply — once a draft exists, both you and the user start defending it.

## The three layers in synthesis

### UI layer
- **Spec location**: `specifications/ui/`
- **Code location**: `src/**` (React 19 + TypeScript + Vite)
- **Owns**: layout, rendering, client-side validation, input shape, keyboard handling, in-window navigation, persisted UI preferences (size/position), surfacing of errors to the user.

### Core layer
- **Spec location**: `specifications/core/`
- **Code location**: `src-tauri/**` (Rust)
- **Owns**: persistence, business logic, IPC contract, filesystem access, network access, OS integration, anything that survives a window reload, anything other processes can observe.

### Contract surface
- **Spec location**: the "UI contract boundary" section of each UI spec; the "Contract surface" section of each core spec.
- **Code location**: Tauri `invoke()` calls in `src/**` ↔ `#[tauri::command]` functions in `src-tauri/**`, registered in `tauri::Builder::invoke_handler` (see `CLAUDE.md`).
- **Owns**: the names and shapes of operations the UI invokes on the backend. This is the part that *both* sides must agree on.

## How to classify

For every distinct piece of the user's request (a feature, an FR, a behavior change), ask:

1. **Does the user observe this purely as something rendered or input on screen, with no state that survives a reload?** → **UI-only**.
2. **Does this involve persistence, the filesystem, the network, another process, OS integration, or any cross-window/cross-session state?** → at least partially **core**.
3. **Does the UI need to *trigger* a core-owned operation, or *receive* state owned by core?** → **contract-spanning**: both layers, plus a named operation.

Worked examples:

- "Add a button that opens a dropdown" → UI-only.
- "Remember the user's panel width across sessions" → contract-spanning (UI renders the resizer, core persists; named operation needed).
- "When the user opens a Git URL, validate it client-side first, then hand it to the backend" → contract-spanning (client validation = UI; clone/open = core).
- "Hash secrets with argon2 before storing them" → core-only.
- "Reorganize the dashboard tiles" → UI-only.

## Echo back to the user

After classifying, present the classification as a short table:

```
Piece of request                          Layer
----------------------------------        ----
Recent projects list rendering            UI
Persist recent projects across sessions   Contract-spanning (op: list_recent_projects, save_recent_project)
Resolve a Git URL into a local clone      Contract-spanning (op: open_project_from_git_url)
```

Then **derive the file scope** from this table and present it as the list of files you intend to touch in this turn:

```
Files this turn will produce or modify:
- specifications/ui/PPK-project-picker.md           (modify; adds FR for recent persistence)
- specifications/core/PST-project-storage.md               (create; new contract surface for list/save ops)
- specifications/ui/OVW-overview.md                 (modify; cross-reference to new core spec)
```

A single user request frequently touches several specs at once — a UI tweak with a backend implication, a new feature that needs paired UI + core specs, or a brownfield edit whose blast radius spreads to two or three referencing specs. Make the full file list visible *now*, with one-line reasons, before any drafting.

Ask: "Does this split look right, and does the file list match what you expected?"

Wait for the user's confirmation. If they correct the classification or the file list, update both and re-confirm. Only once they confirm do you move on to Phase 4.

Note: for brownfield requests, this initial file list is a working estimate. The blast-radius pass in Phase 4 may discover additional specs that need changes (e.g., one that cross-references a renamed FR). When that happens, re-present the expanded list to the user the same way before drafting.

## Brownfield-specific: the silent boundary crossing

A frequent failure mode in brownfield requests: the user describes the change in UI terms only, but the change actually requires a new core operation. Examples:

- "Make the recent list sort by most-recently-opened" — looks UI, but if recency isn't already tracked in the backend, this is contract-spanning.
- "Show an indicator when the project has uncommitted changes" — looks UI, but Git state lives in core.

When the request looks UI-only but plausibly needs core involvement, **explicitly check** the existing `core/` spec(s) for the data or operation the change depends on. If it's not there, flag it as a silent boundary crossing and route it through the contract-spanning path. Better to raise a false alarm than to draft a UI spec that quietly assumes a backend that doesn't exist.

## How the classification shows up in the spec

The classification drives the target-state content of each spec; it is not written down as a separate log of decisions.

- **Contract-spanning pieces**: the paired operations appear by name in the UI spec's "Delegated to backend" list and the core spec's "Contract surface", matching byte-for-byte. The pairing is self-evident from those names.
- **UI-only pieces**: state the boundary as a present-tense fact where it matters — e.g. an FR or NFR that says the state is in-memory and does not survive a reload. Describe what the feature does, not the alternative (persistence) you didn't take.
- **Core-only pieces with no UI yet**: mark the operation inline in the Contract surface as having no current UI consumer (e.g. `list_recent_projects (no UI consumer yet)`), so it's clear it may be removed if none materializes.
