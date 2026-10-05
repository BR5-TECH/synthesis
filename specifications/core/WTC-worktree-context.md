# Worktree context

**Spec code:** `WTC`

## Intent
The backend module that gives a project its two-level identity: a project **is** a Git repository, and at any moment exactly one of that repository's worktrees is the project's **content root**. It enumerates the repository's worktrees and — while the repository's own checkout is the active one — the branches that have none, for the selector (`../ui/WTS-worktree-selector.md`); brings that enumeration up to date against the remote on request, so a branch a colleague pushed is reachable without dropping to a terminal; performs the three ways of changing the active worktree (activate an existing one, check a branch out in place, create a new one); and owns the re-rooting that follows — remounting the scanner, the watchers, and the `.synthesis/` reads onto the new content root so every surface reads from the checkout the author selected. It also remembers which worktree a project was last working in, so reopening a project resumes it. While a direct graduation run works in a worktree, no route moves the project off that worktree or its branch. It also owns the one primitive that removes a linked worktree, which `GTC-git.md`'s branch deletion composes. Out of scope: this module's own commands remove no worktree, delete no branch, and rewrite no history; branch deletion and the diff, commit, push, pull, and pull-request operations belong to `GTC-git.md`, and the read-only change-set computation belongs to `CHC-changes.md`.

## Contract surface
Tauri commands — names match `../ui/WTS-worktree-selector.md` byte-for-byte:

- `"list worktrees and branches"` → `list_worktrees_and_branches()` → `WorktreeContext`.
- `"get active worktree"` → `get_active_worktree()` → the `WorktreeEntry` currently rooting the project.
- `"activate worktree (path)"` → `activate_worktree(path)` → the re-rooted `WorktreeContext`. Side effect: the project's content root moves to `path`.
- `"check out branch in active worktree (branch)"` → `check_out_branch_in_active_worktree(branch)` → the re-rooted `WorktreeContext`. Accepted only while the primary worktree is active (WTC-FR-21). Side effect: the active worktree's checkout changes; its working tree is rewritten by Git.
- `"propose worktree path (branch)"` → `propose_worktree_path(branch)` → an absolute path. No side effect.
- `"create worktree (branch, path)"` → `create_worktree(branch, path)` → the re-rooted `WorktreeContext`. Side effects: a new worktree directory at `path`, a new branch when `branch` did not exist, and the project's content root moves to `path`.
- `"refresh worktrees and branches"` → `refresh_worktrees_and_branches()` → `RefreshOutcome`. Side effect: the repository's remote-tracking refs are brought into agreement with the primary remote. Changes no checkout (WTC-FR-24).

### Events (Tauri event bus)
- `"worktree context changed"` — emitted whenever the active worktree or the branch checked out in it changes, by any route. Payload: `{ active_worktree_path, branch?, is_detached }`. Consumed by `../ui/WTS-worktree-selector.md` WTS-FR-25 to relabel the chrome control and by the surfaces bound to the content root.
- `"branches changed"` — emitted whenever the repository's set of branches has been re-read or changed by a branch deletion or by a work stream's creation or deletion, and may differ from what consumers hold. Payload: `{ repository_root }`. Consumed by `../ui/WTS-worktree-selector.md` WTS-FR-32 and by the Git panel's branches section (`../ui/GIT-git.md` GIT-FR-11), each of which reloads its own listing rather than reading a branch set off the event.

### Payload shapes
```
WorktreeContext {
  repository_root,          // absolute path of the repository's primary worktree
  active_worktree_path,     // absolute path of the worktree currently rooting the project
  worktrees: [WorktreeEntry],
  branches:  [BranchEntry]  // only branches with no worktree
}

WorktreeEntry {
  path,                     // absolute path of the worktree directory
  name,                     // display label (the directory's basename)
  branch?,                  // checked-out branch; absent when is_detached
  head_short_hash,          // abbreviated commit id at HEAD
  is_detached,
  is_active,                // rooting the project right now
  is_primary,               // the repository's non-linked worktree
  is_missing,               // Git lists it but its directory is gone
  stream?,                  // WorktreeStream, when this worktree is a work
                            //   stream's working copy (WTC-FR-QKZD)
}

WorktreeStream {
  stream_id,
  stream_name,
  busy_run_id               // the run holding the stream, or null
}

BranchEntry {
  name,                     // local branch name, or the remote-tracking ref's short name
  kind: "local" | "remote",
  upstream?,                // upstream ref, when a local branch has one
  head_short_hash
}

RefreshOutcome {
  context,                  // a freshly enumerated WorktreeContext; always present (WTC-FR-23)
  remote_state: "refreshed" | "skipped" | "failed",
  remote_error?             // the typed error, when remote_state is "failed"
}
```

Typed errors returned by these commands: `"not a git repository"`, `"not a worktree"`, `"worktree missing"`, `"unknown branch"`, `"branch already checked out"`, `"checkout blocked by local changes"`, `"checkout not allowed in a linked worktree"`, `"worktree path exists"`, `"work stream busy"`, `"direct graduation active"`. The removal primitive returns `worktree_is_primary`, `worktree_is_active`, and `worktree_belongs_to_work_stream`.

### Internal (Rust API)
- `remove_linked_worktree(path)` — removes one linked worktree's directory and its registration (WTC-FR-RDVK). It is not registered as a Tauri command; `GTC-git.md`'s `delete_branch` composes it.
- Resolution of the repository that owns a given path, and of that repository's primary worktree — the anchor the recent-projects list records (`GSS-global-settings-storage.md` GSS-FR-18).
- The re-rooting sequence itself: unmounting the previous content root's in-memory state and watchers and remounting them on the new one.

## Functional requirements
1. **WTC-FR-01** Every operation listed in the contract surface exists as a Tauri command with a typed payload. In the walking-skeleton build, implementations may return a canned `WorktreeContext`, acknowledge the three switching operations without touching disk, and answer a refresh with a canned `RefreshOutcome` that contacts no remote; the worktree selector, its refresh control, and the remote outcomes it reports must all be fully renderable and exercisable against these stubs.
2. **WTC-FR-02** A project's identity is the Git repository that contains its content root, resolved exactly as `GTC-git.md` GTC-FR-02 resolves it: in standalone mode the project's own repository, in co-located mode the host code repository. When the content root is not inside a Git repository, every command in this module returns a typed `"not a git repository"` error.
3. **WTC-FR-03** Exactly one of the repository's worktrees is the project's content root at any moment. That worktree's directory is the project root every other core module resolves against — `.synthesis/project.toml`, `.synthesis/local.toml`, and `.synthesis/library.toml` are read from and written to the active worktree's copy.
4. **WTC-FR-04** `list_worktrees_and_branches()` returns every worktree Git knows for the repository, ordered with the primary worktree first and the linked worktrees after it by path, each carrying `is_active`, `is_primary`, `is_detached`, `is_missing`, and its work-stream state. A work stream's working copy (per `WKS-work-streams.md` WKS-FR-MFDW) is one of them and is excluded from nothing.
   - *Why:* A stream is a checkout the author opens, reads and diffs before they merge it, so a listing that hid it would hide the only place that work can be judged.
5. **WTC-FR-05** A worktree whose `HEAD` is not on a branch carries `is_detached = true` and no `branch`; a worktree Git still lists whose directory no longer exists on disk carries `is_missing = true` and is reported rather than omitted.
6. **WTC-FR-06** The `branches` list contains exactly the branches that could be checked out. While the primary worktree is active that is every local branch not checked out in any worktree, flagged `kind = "local"`, plus every remote-tracking branch with no local counterpart, flagged `kind = "remote"`. A work stream's branch is checked out in that stream's working copy, so it is not a candidate. While a **linked** worktree is active the list is empty, because no branch can be checked out from there (WTC-FR-21).
7. **WTC-FR-07** `get_active_worktree()` returns the `WorktreeEntry` currently rooting the project without enumerating the rest of the repository, so the chrome control can label itself in one cheap call.
8. **WTC-FR-08** `activate_worktree(path)` makes the worktree at `path` the project's content root. The previous root's in-memory state is unmounted (per `PST-project-storage.md` PST-FR-14), its scan and watcher are torn down (per `ASC-artifact-scanning.md` ASC-FR-14) together with the change watch (per `CHC-changes.md` CHC-FR-17), the BM25 indexes (per `BMI-bm25-indexing.md` BMI-FR-25), and the skill registry (per `DSL-dynamic-skills-loading.md` DSL-FR-22), and all five remount on the new root. No file in either worktree is created, modified, or deleted by the activation.
9. **WTC-FR-09** `activate_worktree(path)` returns a typed `"not a worktree"` error when `path` is not one of the repository's worktrees and a typed `"worktree missing"` error when it is one whose directory no longer exists. In both cases the content root is unchanged and nothing is unmounted.
10. **WTC-FR-10** `check_out_branch_in_active_worktree(branch)` checks `branch` out in the currently active worktree through `GTC-git.md`'s `checkout_branch` — which is where the primary-worktree rule of WTC-FR-21 is enforced — and then re-roots onto that same worktree exactly as WTC-FR-08 re-roots, because the checkout replaces the content the scan and the `.synthesis/` reads are based on.
11. **WTC-FR-11** When `branch` names a remote-tracking branch with no local counterpart, `check_out_branch_in_active_worktree` first creates a local branch of the same short name tracking it, then checks that local branch out. A `branch` that resolves to neither a local nor a remote-tracking branch returns a typed `"unknown branch"` error.
12. **WTC-FR-12** `check_out_branch_in_active_worktree` returns a typed `"branch already checked out"` error when `branch` is checked out in another worktree, and a typed `"checkout blocked by local changes"` error when Git refuses because working-tree modifications would be overwritten. In both cases the active worktree, its checked-out branch, and its working tree are left exactly as they were, and no re-rooting occurs.
13. **WTC-FR-13** `propose_worktree_path(branch)` returns an absolute path that is a sibling of the repository's primary worktree, named `<primary-directory-basename>-<leaf>`, where `leaf` is the **final segment** of the branch name: `feature/PROJ-12345/editor-scroll` proposes `<primary>-editor-scroll`. Branch names are routinely namespaced by team, ticket, or kind, and folding that namespace into a directory name yields something long and unreadable that distinguishes nothing the leaf does not. Characters in the leaf that are not valid in a path segment are replaced. A branch that is empty, or that has no non-empty segment, proposes the primary directory's basename alone rather than a name ending in a separator. The command is read-only: it creates nothing and does not check the path for availability.
14. **WTC-FR-14** `create_worktree(branch, path)` creates a worktree at `path` with `branch` checked out, creating `branch` from the active worktree's `HEAD` when no branch of that name exists, and then makes the new worktree the content root exactly as WTC-FR-08 does.
15. **WTC-FR-QVNL** Every linked worktree this application creates — this module's own and every one a graduation run is given alike — records the store it shares as a path **relative** to its own Git directory, which is what the `git` command line writes there. An agent turn reads a run's repository through a container that mounts that store at a path of the executor's own choosing (`../tools/EAC-execute-agent-cli.md` EAC-FR-FNFV), where an absolute host path names nothing and a relative record still resolves. The library this application creates worktrees through writes the absolute path, so the record is rewritten as part of the creation, before the worktree is reported as created; a creation that cannot write it removes the worktree and any branch it made and reports the creation as failed.
16. **WTC-FR-15** `create_worktree(branch, path)` returns a typed `"worktree path exists"` error when anything already exists at `path`, and a typed `"branch already checked out"` error when `branch` already has a worktree. In both cases no directory is created, no branch is created, and the content root is unchanged.
17. **WTC-FR-16** Each of `activate_worktree`, `check_out_branch_in_active_worktree`, and `create_worktree` emits `"worktree context changed"` on success, carrying the new active worktree's path, its branch when it has one, and its detached state. A failed operation emits nothing.
18. **WTC-FR-17** The active worktree of a project is persisted per project in the user-global store (`GSS-global-settings-storage.md` GSS-FR-16) whenever it changes, and is restored when that project is next opened. When the remembered worktree no longer exists, the project falls back to the repository's primary worktree and the fallback is reported in the returned context so the UI can surface it.
19. **WTC-FR-18** Opening a project at any of the repository's worktree paths resolves to the same project. The path opened becomes the active worktree, superseding the remembered one, and the repository's primary worktree is the path recorded in the recent-projects list (per `GSS-global-settings-storage.md` GSS-FR-18), so a repository with many worktrees is one entry rather than many.
20. **WTC-FR-19** No command of this module removes a worktree, prunes a registration, deletes a branch, discards working-tree content, or rewrites history. A graduation run's temporary branch and checkout are the application's own, created by `GSU-graduation-start.md` GSU-FR-ELZO and reclaimed by GRD-FR-GMTX. The stale remote-tracking refs a refresh drops (`GTC-git.md` GTC-FR-13) are a cache, and dropping one deletes no branch.
21. **WTC-FR-20** Closing a project (per `PST-project-storage.md` PST-FR-14) tears down this module's state for that repository: no `"worktree context changed"` event fires afterwards, and no command touches the closed project's files.
22. **WTC-FR-21** A branch is checked out only in the repository's **primary** worktree. When the active worktree is a linked one, `check_out_branch_in_active_worktree` returns a typed `"checkout not allowed in a linked worktree"` error and leaves that worktree's branch, index, and working tree exactly as they were; no re-rooting occurs and no event is emitted. A linked worktree exists to hold one branch, and moving it onto another silently invalidates what every other surface — and any process working in that directory — believes is checked out there. The rule is enforced in the checkout primitive itself (`GTC-git.md` GTC-FR-08) rather than at this module's boundary, so every route to a checkout is bound by it and none can be added that is not.
23. **WTC-FR-22** `refresh_worktrees_and_branches()` brings the branch set up to date in two steps: it fetches from the primary remote through `GTC-git.md`'s `fetch_remote_branches()` — the single fetch primitive, which it composes rather than reimplementing — and then re-enumerates worktrees and branches exactly as WTC-FR-04, WTC-FR-05, and WTC-FR-06 enumerate them, returning that enumeration as the `context` of its `RefreshOutcome`.
24. **WTC-FR-23** The local half of a refresh always completes. A remote leg that could not run reports `remote_state = "skipped"` when the project has no primary remote, and `remote_state = "failed"` carrying the typed `remote_error` for every other cause — no credential resolved, a credential GitHub refused, a remote that could not be reached — and in both cases the `context` is still a fresh enumeration of what is on disk. The command itself returns an error only when the content root is not inside a Git repository (WTC-FR-02), because a refresh that reaches no remote still tells the caller about a branch created or a worktree added since it last looked.
25. **WTC-FR-24** A refresh changes no checkout and re-roots nothing. The active worktree, every worktree's checked-out branch, index, and working tree, and `HEAD` are all left exactly as they were; no in-memory state, scanner, watcher, or change watch is torn down or remounted; the remembered active worktree of WTC-FR-17 is not rewritten; and no `"worktree context changed"` event is emitted, because nothing that event describes has changed.
26. **WTC-FR-25** Every refresh that returns emits exactly one `"branches changed"`, whether its remote leg refreshed, was skipped, or failed, because a branch created or deleted locally since the last enumeration changes the set just as a fetch does. A refresh refused under WTC-FR-02 emits nothing. The event announces that the repository's branch set may differ from what a consumer holds; it never implies the active checkout moved, which is what `"worktree context changed"` announces. A branch deletion that removed a branch emits one too (per `GTC-git.md` GTC-FR-NPCT), and so do a work stream's creation and deletion (per `WKS-work-streams.md` WKS-FR-SPVK).
27. **WTC-FR-QKZD** A `WorktreeEntry` carries the work stream it belongs to, when it belongs to one: the stream's id and name, and whether a run holds it. A worktree that is no stream's carries none of it.
28. **WTC-FR-JXBM** `activate_worktree` and `check_out_branch_in_active_worktree` refuse a work stream a run holds, with the typed `"work stream busy"` error naming the stream and the run. The content root is unchanged, nothing is unmounted, and no event is emitted.
    - *Why:* An agent turn is writing that tree, so re-rooting the application onto it would let the author edit files under a process that is rewriting them.

29. **WTC-FR-FBJQ** From the first dispatch of a direct graduation run until that run is terminal, `activate_worktree`, `check_out_branch_in_active_worktree` and `create_worktree` refuse with the typed `"direct graduation active"` naming the run. A direct run that holds its claim counts as dispatched even before its run record is saved as dispatched, so a switch made between the claim and that save is refused too. A queued run that was never dispatched blocks nothing. The refusal leaves the content root, the checkout and the disk unchanged and emits no event.
30. **WTC-FR-TXKY** The refusal of WTC-FR-FBJQ binds **every** route that changes the project's active worktree or its branch, including every route that opens or creates a project — open at a path, open from a Git URL, and create project — when it resolves to another worktree of the open project (WTC-FR-18). A route added later is bound by it as WTC-FR-21 binds the checkout primitive.
31. **WTC-FR-UMGY** A successful switching route emits `"worktree context changed"` and then offers every queued direct run a dispatch (per `GRD-graduation.md` GRD-FR-XRDY), so a run held for its branch starts when the author restores that branch.
32. **WTC-FR-RDVK** `remove_linked_worktree(path)` removes the directory of one linked worktree and prunes its registration, through `delete_author_tree` of `FSA-filesystem-access.md` (FSA-FR-OWVT), built with the worktree's parent directory as its author root. A symbolic link in the worktree, such as an installed dependency store, is unlinked and never followed, so its target is untouched and the removal does not refuse over it. It refuses the primary worktree with `worktree_is_primary`, the active worktree with `worktree_is_active`, and a work stream's working copy with `worktree_belongs_to_work_stream`. A refusal changes nothing. A worktree whose directory is gone has its registration pruned.
33. **WTC-FR-BHFE** The checks that decide a branch deletion read the worktree enumeration of WTC-FR-04 at the moment of the deletion: which worktree has the branch checked out, whether that worktree is primary or active, and which work stream owns it. A result read earlier is never trusted.

## Non-functional requirements
- Enumerating worktrees and branches resolves without network access: `list_worktrees_and_branches` and `get_active_worktree` answer from what is on disk and never fetch. `refresh_worktrees_and_branches` is the one operation here that reaches a remote, and it does so only because a caller asked for exactly that.
- `get_active_worktree` is cheap enough to run on window mount without delaying first paint; it does not walk the repository's refs.
- Re-rooting is a teardown-then-remount, not a merge: no scan state, watcher registration, or cached classification from the previous content root survives into the new one.
- The Git engine choice is shared with `GTC-git.md` and `CHC-changes.md` and is not constrained here beyond the contract surface.
- A worktree whose directory has been removed outside the application is reported rather than silently pruned, so the user sees why an entry cannot be activated instead of watching it disappear.
