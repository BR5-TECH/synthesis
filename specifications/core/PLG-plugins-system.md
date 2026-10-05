# Plugins system

**Spec code:** `PLG`

## Intent
The plugin system is foundational architecture, not a deferred feature. Core Synthesis owns the artifact model, Git integration, editor, and a plugin host; non-core capabilities are plugins. This spec captures the **minimum decisions** needed for the walking skeleton: the plugin categories the host recognises, the lifecycle states the UI can observe, and the Settings-tab contract surface. The on-disk plugin format, loader mechanism, sandboxing model, and the Rust trait surface plugins implement are deliberately left open and will be filled in by follow-up specs as plugin needs grow.

## Contract surface
This module's contract has two faces. Only the first is exposed to the UI in v1.

### UI-facing (Tauri commands)
The six plugin commands; persistence scopes for the underlying state are owned by `GSS-global-settings-storage.md` (registry: install / uninstall / list) and `PSS-project-settings-storage.md` (enablement: enable / disable / configure). The install / uninstall / list commands are surfaced by `../ui/GLS-global-settings.md`; the enable / disable / configure commands by `../ui/SET-project-settings.md`:

- `"list installed plugins"` → `list_installed_plugins` — returns the full registry of plugins installed in user-global, each entry annotated with its enablement state in the currently open project (if any) and its current lifecycle state.
- `"enable plugin"` → `enable_plugin(id)` — sets enablement to enabled in the currently open project. Errors if no project is open.
- `"disable plugin"` → `disable_plugin(id)` — sets enablement to disabled in the currently open project. Errors if no project is open.
- `"configure plugin (id, config)"` → `configure_plugin(id, config)` — stores opaque, plugin-defined configuration alongside the project-public enablement entry.
- `"install plugin (source)"` → `install_plugin(source)` — adds an entry to the user-global installed-plugin registry from `source` (a path, URL, or other locator).
- `"uninstall plugin"` → `uninstall_plugin(id)` — removes the entry from the user-global registry and cascades a disable in the currently-open project (per `GSS-global-settings-storage.md` GSS-FR-11).

### Plugin-facing (deferred)
The Rust trait a plugin implements, the manifest format on disk, the entry-point and capability declarations, and the lifecycle hooks the host invokes are **out of scope for this spec**. They will be specified in a follow-up once v1 needs more than the built-in plugins this spec already covers. The UI-facing command names and payload shapes here will not change when the plugin-facing surface is filled in.

## Plugin categories
The host recognises six categories (Notion: Plugin system page). Only **agent adapter** has a fully specified contract in v1 (`ADP-adapters.md`); the others are named for completeness so that `list_installed_plugins` can return category-tagged entries today without locking in their semantics.

- Backend service (sharing, sync)
- AI provider adapter (Claude, ChatGPT, OpenRouter, local models)
- Marketplace integration
- Storage adapter (beyond local/Git, e.g. S3)
- **Agent adapter** (Claude Code, Cursor, Codex, Generic) — see `ADP-adapters.md`
- Artifact-type / editor extension

## Lifecycle states
A plugin entry returned by `list_installed_plugins` carries one of the following states, observable by the UI for its toggle and configuration affordances:

- `registered` — installed in user-global, no per-project decision yet.
- `enabled` — enabled in the currently open project.
- `disabled` — explicitly disabled in the currently open project.
- `unsupported` — the plugin is installed but the current build cannot host it (e.g. the category is not yet implemented, or the host is missing a required capability). The UI renders such entries as read-only with an explanatory tooltip.

No other lifecycle granularity (e.g. paused, crashed, errored-on-load) is part of v1.

## Functional requirements
1. **PLG-FR-01** `list_installed_plugins` returns the union of the user-global installed-plugin registry and the built-in plugins compiled into the host. Each entry carries `{ id, name, version, category, state, project_config? }`.
2. **PLG-FR-02** When no project is open, every entry's `state` is either `registered` or `unsupported`; entries are never `enabled` or `disabled` outside the context of an open project.
3. **PLG-FR-03** `enable_plugin(id)`, `disable_plugin(id)`, and `configure_plugin(id, config)` all require an open project. Without one, each returns a typed "no project open" error.
4. **PLG-FR-04** `enable_plugin(id)` transitions the plugin to `enabled` in the currently open project. If the plugin's category is not supported by the current build (`unsupported`), the call returns a typed "category unsupported" error and no state change occurs.
5. **PLG-FR-05** `install_plugin(source)` accepts a string `source` and adds an entry to the user-global registry. In the walking skeleton, the operation returns a typed "not yet supported in this build" error and does not panic; the command surface exists so the UI Settings → Plugins install flow remains exercisable.
6. **PLG-FR-06** `uninstall_plugin(id)` removes the entry from user-global and triggers the cascade described in `GSS-global-settings-storage.md` GSS-FR-11. Uninstalling a built-in plugin returns a typed "cannot uninstall built-in" error.
7. **PLG-FR-07** Plugin **categorisation** is fixed at install time; the host does not allow a plugin's category to change between calls.
8. **PLG-FR-08** Built-in plugins (compiled into the host) are present in `list_installed_plugins` from first launch. The v1 set is just the built-in agent adapters from `ADP-adapters.md`.
9. **PLG-FR-09** The shape of `config` passed to `configure_plugin` is opaque to the host; the host stores it verbatim alongside the project-public enablement entry. Validation of `config` is the plugin's responsibility (deferred).
10. **PLG-FR-10** No plugin-facing trait, no manifest format, and no on-disk loader is defined by this spec. Their absence does not prevent any UI command from being callable in the walking skeleton.

## Non-functional requirements
- The walking skeleton ships built-in plugins only; no third-party plugin loading is exercised.
- The Settings → Plugins section must be navigable and the install action must surface its "not yet supported" error inline rather than crashing.
- The contract here is forward-stable: filling in the plugin-facing surface in a follow-up spec must not require renaming any of the six Tauri commands above.

## Open questions (preserved from Notion)
- Plugin discovery, loading, and sandboxing.
- Rust-side plugins (traits / dynamic loading) vs TypeScript-side plugins (editor extensions) vs both.
- Plugin manifest format.
- Plugin lifecycle granularity beyond `registered` / `enabled` / `disabled` / `unsupported`.
