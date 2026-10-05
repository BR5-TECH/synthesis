## Intent

Drafts and conversations must become persistent, shared, and committed to the project's Git repository.

## User journey

When multiple users work on the same project, they should see the same drafts and their complete collaboration history after they pull or clone the project. A user can create or update a draft on one checkout, and another user can receive the draft's prompt, history, proposals, assets, comments, and conversation through Git.

## Requirements

- The complete contents of each draft folder must be stored under the project's committed `.synthesis/drafts/` tree, including the prompt, history, proposals, assets, comments, and conversation.
- Draft-owned storage includes only durable draft data and draft metadata. Transient locks, temporary files, caches, runtime state, partial writes, and other non-durable implementation files must not be committed unless a specification explicitly defines them as draft data.
- The complete draft folder includes files owned by the application and its draft-storage modules. User-created files that are not part of the defined draft layout must not be silently deleted, rewritten, or added to an auto-commit.
- The `.gitattributes` rules for draft-owned append-only JSONL logs must be stored and maintained as application-owned project metadata, with ownership and update behavior consistent with the existing comments and statistics storage rules.
- Draft storage must no longer be covered by the Git ignore rules. Update the existing draft-storage specification and project scaffold rules that currently define `.synthesis/drafts/` as private or gitignored.
- On migration of an existing project, remove the ignore rules for draft storage and preserve all existing draft files in their current locations. Migration must not delete, move, rewrite, stage, or commit those files. Existing files become eligible for the next normal, scoped draft auto-commit after a draft-owned mutation; no separate migration commit is required.
- All draft files except append-only collaboration logs must remain ordinary Git-managed files. For concurrent changes to `draft.toml`, prompt files, proposal files, history files, and asset files, Git must handle conflicts; the application must not auto-resolve, silently choose a version, overwrite a conflict, or delete one side. The conflict must remain visible to the user through normal Git status and conflict handling.
- Append-only JSONL collaboration logs must remain individually diffable and mergeable. Configure suitable Git merge attributes for those logs, consistent with the existing comments and statistics storage rules. Union merge applies only to logs whose schema and ownership explicitly permit append-only merging; it must not apply to `draft.toml`, prompt files, proposal files, history files, asset files, or other non-JSONL files.
- Draft persistence must support multiple worktrees through the active worktree, consistent with the existing project and draft storage rules. Git operations must not read or write a sibling worktree.
- Draft changes must be auto-committed similarly to the statistics logs. The auto-commit must include only application-owned draft storage and must not include user-staged or unrelated project files.
- Auto-commits must be debounced, asynchronous, best-effort, and non-blocking. A Git failure must leave the draft data safely on disk for a later retry and must not fail or delay the user's draft operation.
- Draft deletion must remove the complete draft folder and its committed contents. It must not remove another draft's data or unrelated project files.

## Specification impact

Update every specification that currently treats draft-owned storage as private or gitignored, not only the draft-storage and project-scaffold specifications. At minimum, review and update `DRS-draft-storage.md`, `PST-project-storage.md`, `ASC-artifact-scanning.md`, `CMS-comments-storage.md`, `DAS-draft-assets.md`, `DHS-draft-history.md`, and `DCP-draft-change-proposals.md` so their storage scope, scan exclusions, merge attributes, deletion rules, migration behavior, and auto-commit ownership agree with this requirement. Preserve the existing active-worktree and path-containment rules.