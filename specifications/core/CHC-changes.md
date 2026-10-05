# Changes

**Spec code:** `CHC`

## Intent
Backend module that computes what has changed in the project's Git repository and hands it to the Changes panel (`../ui/CHG-changes.md`) as a flat, fully-annotated change set. It answers two questions — what is modified but not yet committed, and what the current branch introduces relative to a target branch — and enriches every entry with the per-file line-count summary and the artifact-type classification the panel needs to group, tag, and filter without a second round-trip. It composes the classifier of `ASC-artifact-scanning.md` so a changed file carries exactly the type it carries in the Project panel, and it watches the repository so the panel stays live. Out of scope: this module is strictly read-only and exposes no way to alter the repository; every mutating Git operation the Changes panel performs — the commit it makes, the push that follows, and the rollback that returns a selection to `HEAD` — and the diff payload a Diff tab renders, all belong to `GTC-git.md`.

## Contract surface
Tauri commands — names match `../ui/CHG-changes.md` byte-for-byte:

- `"list uncommitted changes"` → `list_uncommitted_changes()` → `ChangeSet` whose `comparison.kind` is `"uncommitted"`.
- `"list branch changes (target branch)"` → `list_branch_changes(target_branch)` → `ChangeSet` whose `comparison.kind` is `"branch"`.
- `"get default branch"` → `get_default_branch()` → the repository's default branch name.
- `"list comparison branches"` → `list_comparison_branches()` → `[BranchOption]`, the candidate targets for the panel's picker.
- `"get uncommitted diff totals"` → `get_uncommitted_diff_totals()` → `DiffTotals`, the summed line counts of the uncommitted change set.

What the Changes panel's Diff tab renders is **not** owned here either: that tab derives its own comparison from the two revisions `GTC-git.md`'s `"get file revisions for comparison"` serves for the scope this module's `Comparison` names (`../ui/DFV-diff-viewer.md` DFV-FR-43).

### Events (Tauri event bus)
- `"changes updated"` — emitted, debounced, when the repository or the working tree changes in a way that could alter the current change set. Payload: `{ change_count }`. The UI responds by reloading (per `../ui/CHG-changes.md` CHG-FR-19, `../ui/DFV-diff-viewer.md` DFV-FR-26, and `../ui/STB-status-bar.md` STB-FR-27); bursts within the debounce window coalesce into a single event.

### Payload shapes
```
ChangeSet {
  comparison: Comparison,
  entries: [ChangeEntry]      // flat; grouping into a tree is the UI's job
}

Comparison =
  | { kind: "uncommitted" }
  | { kind: "branch", target_branch, merge_base }   // merge_base = resolved commit id

ChangeEntry {
  id,                          // stable project-relative-path key (ASC-artifact-scanning.md ASC-FR-13)
  path,                        // project-relative path at its current location
  name,                        // basename
  change_status: "added" | "modified" | "deleted" | "renamed" | "untracked",
  previous_path?,              // renamed entries only
  added_lines?,                // null when is_binary
  removed_lines?,              // null when is_binary
  is_binary,
  artifact_type?,              // one of the eight built-in types, when classified
  type_source?: "inferred" | "assigned" | "inherited"
}

BranchOption {
  name,
  is_current,
  is_default
}

DiffTotals {
  added_lines,                 // summed across non-binary entries
  removed_lines,               // summed across non-binary entries
  file_count                   // every entry, binary included
}
```

Typed errors returned by these commands: `"not a git repository"`, `"unknown branch"`, `"no merge base"`.

## Functional requirements
1. **CHC-FR-01** Every operation listed in the contract surface exists as a Tauri command with a typed payload. In the walking-skeleton build, implementations may return canned change sets and canned branch lists; the Changes panel must be fully renderable against these stubs.
2. **CHC-FR-02** Every command operates on the Git repository that owns the currently open project and specifically on that project's active worktree, resolved exactly as `GTC-git.md` GTC-FR-02 resolves it: in standalone mode the project's own repository, in co-located mode the host code repository. The working tree both comparisons read is the active worktree's; a modification sitting in a sibling worktree of the same repository never appears in a returned change set. When the project is not inside a Git repository, every command returns a typed `"not a git repository"` error.
3. **CHC-FR-03** `list_uncommitted_changes()` compares the working tree against `HEAD`. Staged and unstaged modifications are both included and are reported identically — the returned entries carry no index state, because the consuming panel selects paths to commit rather than manipulating an index (per `../ui/CHG-changes.md` CHG-FR-21) and so has nothing to do with the distinction.
4. **CHC-FR-04** `list_branch_changes(target_branch)` compares the working tree against the merge-base of the current branch and `target_branch`. The result therefore contains every change the current branch introduces — committed and uncommitted alike — and contains nothing from commits that landed on `target_branch` after that merge-base.
5. **CHC-FR-05** When `HEAD` is detached, `list_branch_changes` still resolves: the checked-out commit stands in for the current branch tip when computing the merge-base, and the command returns a normal change set rather than an error.
6. **CHC-FR-06** A file that Git does not track carries `change_status = "untracked"`. Files excluded by the repository's ignore rules are absent from every change set this module returns, in either comparison.
7. **CHC-FR-07** `added_lines` and `removed_lines` are computed per entry as follows: an added or untracked file counts every line as added and none as removed; a deleted file counts every line as removed and none as added; a modified or renamed file counts the insertions and deletions of its unified diff against the comparison base.
8. **CHC-FR-08** An entry whose content Git treats as binary carries `is_binary = true` with `added_lines` and `removed_lines` both null. Line counts are never fabricated for binary content.
9. **CHC-FR-09** A renamed entry reports its current path in `path` and its pre-rename path in `previous_path`; `previous_path` is absent for every other change status.
10. **CHC-FR-10** Each entry's `artifact_type` and `type_source` are resolved by the classifier of `ASC-artifact-scanning.md` following its ASC-FR-06 precedence, so a changed file carries the same type it carries in the Project panel's tree. A deleted path is classified from path convention and stored assignments only, because the content tiebreak of ASC-FR-04 requires a file that is no longer on disk.
11. **CHC-FR-11** Each entry's `id` is the same stable path-derived key `ASC-artifact-scanning.md` ASC-FR-13 defines, so the UI can compare a change entry against an open tab under the identity rules of `../ui/TAB-tabs.md`.
12. **CHC-FR-12** `entries` is a flat list. This module performs no folder grouping, no ordering by folder, and no separation of untracked entries into a group; assembling the tree and the **Unrevisioned** group is owned by `../ui/CHG-changes.md` CHG-FR-08 and CHG-FR-09.
13. **CHC-FR-13** `get_default_branch()` resolves the repository's default branch from the primary remote's published HEAD. When no remote is configured or the remote publishes no HEAD, it falls back in order to a local `main`, then a local `master`, then the current branch.
14. **CHC-FR-14** `list_comparison_branches()` returns the local branches and remote-tracking branches available as comparison targets, each flagged with `is_current` and `is_default`.
15. **CHC-FR-15** `list_branch_changes(target_branch)` returns a typed `"unknown branch"` error when `target_branch` does not resolve to a branch in the repository, and a typed `"no merge base"` error when the current branch and `target_branch` share no common ancestor.
16. **CHC-FR-16** While a project is open, this module watches the repository's Git directory — `HEAD`, refs, and the index — and consumes the internal content-modification channel of `ASC-artifact-scanning.md` ASC-FR-15 for working-tree edits. A change on either source that could alter the current change set triggers a debounced `"changes updated"` event; multiple changes within the debounce window coalesce into a single event.
17. **CHC-FR-17** Closing a project (per `PST-project-storage.md` PST-FR-14) tears down this module's watch for that project; no `"changes updated"` event fires after close, and no command touches the closed project's files.
18. **CHC-FR-18** This module is read-only with respect to the repository. No command it exposes writes to the index, the object database, refs, or any file in the working tree, and none of them creates, deletes, or moves a branch. The two writes the Changes panel makes both go through `GTC-git.md` — the commit through GTC-FR-19 and the rollback of a checked selection through GTC-FR-23 — and each reaches this module only as the `"changes updated"` event that follows it (CHC-FR-16). Being read-only is what lets a rollback be described by this module immediately afterwards without its own computation having to be reconciled against a write it performed.
19. **CHC-FR-19** Changing the project's active worktree (per `WTC-worktree-context.md` WTC-FR-08) tears this module's watch down on the outgoing content root exactly as CHC-FR-17 does and remounts it on the new one, so no `"changes updated"` event for the previous worktree reaches the panel afterwards and the next change set is computed against the new active worktree.
20. **CHC-FR-20** `list_branch_changes(target_branch)` accepts a target that is checked out in another worktree of the repository and compares against it normally; the presence of a worktree for a branch neither qualifies nor disqualifies it as a comparison target. `list_comparison_branches()` likewise returns every candidate target regardless of whether it has a worktree, because the panel is choosing what to diff against rather than what to check out.

21. **CHC-FR-21** `get_uncommitted_diff_totals()` returns the added-line and removed-line counts summed over exactly the change set `list_uncommitted_changes()` returns for the active worktree (CHC-FR-02, CHC-FR-03), together with the number of entries in it. Binary entries count toward `file_count` and toward neither line total, because no line counts exist for them (CHC-FR-08). It returns the typed `"not a git repository"` error under the same condition as every other command in this surface.
22. **CHC-FR-22** `get_uncommitted_diff_totals()` is a standalone read: it neither requires nor is affected by any prior call to `list_uncommitted_changes()`, so `../ui/STB-status-bar.md` STB-FR-25 renders totals whether or not the Changes panel has ever been opened. A `"changes updated"` event (CHC-FR-16) signals that its result may have changed, exactly as it does for the two list commands, and it is read-only under CHC-FR-18 like the rest of the surface.

## Non-functional requirements
- Change-set computation must not block the Tauri invoke for a large branch comparison; the panel's incremental-render requirement (`../ui/CHG-changes.md`) assumes results can be produced without a long stall.
- Classification of change entries reuses the scan state `ASC-artifact-scanning.md` already maintains rather than triggering a fresh full-project walk per change-set request.
- The debounce window is an implementation choice (order of a few hundred milliseconds); the contract is only that bursts coalesce and the UI is not flooded.
- The Git engine choice is shared with `GTC-git.md` and is not constrained here beyond the contract surface.
- Both comparisons resolve without network access; neither fetches from a remote before computing a merge-base.
