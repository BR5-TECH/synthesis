# History

**Spec code:** `HIS`

## Intent
Backend anchor for the History viewer (`../ui/HVW-history-viewer.md`). Locks the operation surface today (version listing, version-detail loading, restore-as-forward-commit) so the walking skeleton can render both surfaces — the version list in the bottom panel and the History detail tab in the main viewport — against canned data. The Git-history walking strategy, diff payload format, archive-container transparency, and binary metadata-only handling are deliberately deferred to a follow-up spec.

## Contract surface
Tauri commands — names match `../ui/HVW-history-viewer.md` byte-for-byte:

- `"list versions for entity (commit message, hash, timestamp, author)"` → `list_versions_for_entity(entity_id)` — returns the historical versions of the entity (currently artifact-scoped per `../ui/HVW-history-viewer.md` HVW-FR-08). Each entry: `{ commit_message, hash, short_hash, timestamp, author }`.
- `"load version detail (Markdown artifact: multi-file diff payload; Flow: full revision render including resolved artifact references at the same revision)"` → `load_version_detail(entity_id, version_hash, compare_to?)` — returns a typed payload that the UI can render. For Markdown artifacts: a multi-file diff against either the current revision or `compare_to`. For Flow artifacts: a full revision render of the canvas, with the graph's artifact references resolved at the same revision (project-time-travel, per `../ui/HVW-history-viewer.md` HVW-FR-05).
- `"restore version (writes the historical state as a new forward commit on the current branch)"` → `restore_version(entity_id, version_hash)` — writes the historical state of the entity onto the current branch as a **new forward commit**; never rewrites history.

## Functional requirements
1. **HIS-FR-01** All three commands exist with typed payloads. In the walking-skeleton build, implementations may return canned version lists, canned diff payloads, and acknowledge `restore_version` without writing to disk; the UI History surfaces must be exercisable against these stubs.
2. **HIS-FR-02** `list_versions_for_entity` is artifact-scoped in v1 (per `../ui/HVW-history-viewer.md` HVW-FR-08). For any other entity kind, the command returns an empty list. Workstream / role / playbook history is deferred.
3. **HIS-FR-03** `load_version_detail` returns different payload shapes for Markdown artifacts vs Flow artifacts (the two shapes described in `../ui/HVW-history-viewer.md` HVW-FR-04 and HVW-FR-05). The payload carries the kind hint so the UI selects the right renderer.
4. **HIS-FR-04** The Markdown diff payload is a multi-file diff. The compare target is the current revision by default; when `compare_to` is supplied, the diff is computed against that revision instead.
5. **HIS-FR-05** The Flow payload renders the canvas at the chosen revision; every artifact reference the graph's nodes carry is resolved at the same Git commit (project-time-travel). When a referenced artifact did not yet exist at that revision, the reference is included with a typed "missing at revision" marker so the UI can render it as a placeholder.
6. **HIS-FR-06** `restore_version(entity_id, version_hash)` creates a **new commit on the current branch** that contains the entity's state from `version_hash`. It does not rewrite, amend, force-push, or otherwise modify existing history. The contract is "irreversible only at the Git level", matching `../ui/HVW-history-viewer.md` Non-functional requirements.
7. **HIS-FR-07** Restore is whole-version only. Partial restore (cherry-picking specific changes) is not exposed by this module in v1, matching `../ui/HVW-history-viewer.md` HVW-FR-07.
8. **HIS-FR-08** Every command in this module resolves against the repository that owns the currently open project and against that project's active worktree (per `WTC-worktree-context.md` WTC-FR-03): version listing walks the history reachable from the active worktree's `HEAD`, and `restore_version` writes its forward commit onto the branch that worktree has checked out, into that worktree's working tree. When the project is not inside a Git repository, every command returns a typed "not a git repository" error.
9. **HIS-FR-09** The version-history cache under `.synthesis/cache/` (introduced by `PST-project-storage.md`) is permitted but never authoritative; this module always reconciles its results against Git.

## Non-functional requirements
- Diff payload size is bounded by the artifact; this module does not assume small files and the UI does not assume small files either.
- History walking uses path-filtered Git log (Notion: Project storage decisions) so that history queries over `.synthesis/artifacts/<id>.toml` remain efficient even in large repositories; the engine that implements this is deferred to a follow-up spec.
- Archive-container transparency and binary metadata-only handling are backend responsibilities that surface through the standard diff payload shape; the specifics are deferred.
