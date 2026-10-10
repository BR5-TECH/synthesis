# Git

**Spec code:** `GTC`

## Intent
Backend anchor for the UI Git surface (`../ui/GIT-git.md`). Locks the v1 operation names and payload shapes today so the walking skeleton can render the Git panel against typed stub data; the Git engine specification (gitoxide-driven operations, conflict surfacing, output streaming) is deferred to a follow-up spec. The same module reads the commit history of the active worktree and of a branch, answers what a commit changed, answers what a branch changed against the branch it grows from, deletes a local branch together with its linked worktree after re-checking the state it stands in, and reads a pull request's description and full timeline from GitHub. Git is first-class in v1 because the interactive loop (`author → commit → external agent picks up artifacts`) depends on commit and push. Operations that reach GitHub authenticate with the token the open project resolves to; this module presents that credential but does not own it, store it, or decide which one applies — all of that belongs to `GTS-github-token-storage.md`. Out of scope: multi-remote support and Git-provider pluggability; the PR surface assumes a single primary remote per project.

## Contract surface
Tauri commands — names match `../ui/GIT-git.md` byte-for-byte:

- `"get working tree status"` → `get_working_tree_status(expected_worktree)` → `WorkingTreeStatus` — the complete set of changed paths in the active worktree, each carrying its **staged status and its unstaged status separately** so a path that is both is described as both (GTC-FR-30). `expected_worktree` is optional and is the identity guard of GTC-FR-31: supplied, the read is bound to that one checkout and refuses rather than quietly describing another. The name carries no third term because the guard is optional and a caller that supplies none is served exactly as before. No UI consumer: the change set the Changes panel renders is computed by `CHC-changes.md`.
- `"get diff (for path, for staged set, for branch comparison)"` → `get_diff(scope)` where `scope` is a path, the staged set (no UI consumer), or a branch comparison — a path taken against the merge-base of the current branch and a named target branch, resolved as `CHC-changes.md` CHC-FR-04 resolves it. Returns a typed diff payload (unified-diff hunks for text; an "untouched binary" marker for non-text). The Git panel's commit log does not use it (it uses `get commit file diff`). It is **not** what a Diff tab renders: that tab derives its own comparison from the two revisions it holds, because the target half of it is a buffer the author is editing and a diff computed on disk cannot describe one (`../ui/DFV-diff-viewer.md` DFV-FR-43).
- `"get file revisions for comparison"` → `get_file_revisions(scope)` where `scope` is the same scope `get_diff` takes, and already names the path — passing it a second time would give one call two sources for the file it is about, and a disagreement between them would diff one file against another. Returns `{ old: string | null, new: string | null, is_binary: bool }` — the file's complete text on each side of the comparison, with `null` on a side the comparison has no version for (`old = null` for a file the comparison adds, `new = null` for one it deletes) and both texts omitted when the file is binary. A side that exists but is empty is `""`, not `null`. The staged set names no single file, so it is not a scope this operation serves and it returns a typed error. It reads and returns content only; it computes no diff and writes nothing. Its `old` side is the original revision every mode of `../ui/DFV-diff-viewer.md` compares against, and its `new` side is what tells that tab whether the comparison adds or deletes the file at all (DFV-FR-25); the target the tab renders and edits comes from the artifact's own load and save path instead, which is the one that carries the baseline checksum a write needs.
- `"commit paths (message, paths)"` → `commit_paths(message, paths, expected_worktree)` → `CommitOutcome { commit_id, committed_paths }`. Creates a commit holding exactly the named paths and reports what it recorded (GTC-FR-19). `expected_worktree` is the same optional identity guard `get_working_tree_status` takes (GTC-FR-31), so a caller committing into a checkout it resolved earlier cannot have its commit land in another one; the operation name carries no third term because a caller that supplies none — the Changes panel, committing the checkout it is rendering at that moment — is served exactly as before. This is the commit the application makes; it is invoked by `../ui/CMW-commit-message.md`, whose caller closes the Diff tabs those paths name (`../ui/TAB-tabs.md` TAB-FR-22).
- `"rollback paths (paths)"` → `rollback_paths(paths)` → `RollbackOutcome`. Returns exactly the named paths to the state `HEAD` holds for them in the active worktree — restoring the ones Git tracks and removing the ones it does not — and reports what became of each path on its own (GTC-FR-23, GTC-FR-25). This is the discard the Changes panel performs (`../ui/CHG-changes.md` CHG-FR-62). It is **per-path rather than atomic**, so its outcome names the paths it confirmed on disk and carries a typed failure for every one it did not reach, which is what lets the caller reset the artifact sessions behind the successful paths alone.
- `"get upstream sync state"` → `get_upstream_sync_state()` → `{ has_remote, has_upstream, ahead, behind }`, the current branch's standing against its upstream, resolved from local refs alone (GTC-FR-21). It is what decides whether a push is offered (`../ui/CHG-changes.md` CHG-FR-37).
- `"stage / unstage paths"` → `stage_paths(paths)` / `unstage_paths(paths)` (no UI consumer).
- `"commit (message, staged set)"` → `commit(message)`. Commits the currently staged set (no UI consumer).
- `"list commit history"` → `list_commit_history()` → `CommitHistory` — at most the latest 100 commits reachable from the active worktree's `HEAD`, newest first (GTC-FR-BQNM).
- `"list commit files (commit id)"` → `list_commit_files(commit_id)` → `[CommitFile]` — the paths one commit changed (GTC-FR-YCEV).
- `"get commit file diff (commit id, path)"` → `get_commit_file_diff(commit_id, path)` → `DiffPayload` — one commit's change to one path (GTC-FR-PDSK).
- `"list branches"` → `list_branches` — returns local and remote branches with the current branch marked.
- `"list branch compare files (name, kind)"` → `list_branch_compare_files(name, kind)` → `BranchComparison` — the paths one branch changed against its base (GTC-FR-YQVD, GTC-FR-MBBH).
- `"get branch compare file diff (name, kind, path)"` → `get_branch_compare_file_diff(name, kind, path)` → `DiffPayload` — one branch's change to one path against its base (GTC-FR-PYVV).
- `"get branch information (name, kind)"` → `get_branch_information(name, kind)` → `BranchInformation` — what one branch is and holds (GTC-FR-TGOI).
- `"inspect branch deletion (name)"` → `inspect_branch_deletion(name)` → `BranchDeletionPlan` — read-only; what deleting a local branch would remove and discard, or the typed refusal (GTC-FR-UMXA).
- `"delete branch (name, delete remote, discard uncommitted)"` → `delete_branch(name, delete_remote, discard_uncommitted)` → `BranchDeletionOutcome` — deletes a local branch, its linked worktree, and optionally its remote branch (GTC-FR-WNZH).
- `"create branch"` → `create_branch(name, from?)`.
- `"checkout branch"` → `checkout_branch(name)`. Accepted only while the project's active worktree is the repository's primary one (GTC-FR-08).
- `"push current branch"` → `push_current_branch` — emits push output incrementally through an event stream owned by this module (mirrors `ADP-adapters.md`'s event-bus pattern for long-running output). Authenticated against a GitHub remote (GTC-FR-09).
- `"pull current branch"` → `pull_current_branch` — emits pull output incrementally. Authenticated against a GitHub remote (GTC-FR-09).
- `"list pull requests (state)"` → `list_pull_requests(state)` → `[PullRequestSummary]` — the PRs of the project's primary GitHub remote, where `state` is `"open"` or `"closed"`. Authenticated (GTC-FR-09, GTC-FR-GXUB).
- `"create pull request (title, body, base, head, draft)"` → `create_pull_request(title, body, base, head, draft)` → `CreatedPullRequest` — opens a pull request from `head` into `base` on the project's primary GitHub remote (GTC-FR-MMFM). It pushes and commits nothing. Authenticated (GTC-FR-09).
- `"get pull request head state (head, base)"` → `get_pull_request_head_state(head, base)` → `PullRequestHeadState` — what stands between a local branch and a pull request for it: whether the remote holds the branch, whether it holds all of it, whether its checkout is clean, and whether it holds a commit its base lacks (GTC-FR-NEIW). Local reads only.
- `"get pull request detail"` → `get_pull_request_detail(id)` → `PullRequestDetail` — the PR's description and header fields, where `id` is the PR number. Authenticated (GTC-FR-09, GTC-FR-ZIHE).
- `"list pull request timeline (id)"` → `list_pull_request_timeline(id)` → `PullRequestTimeline` — the PR's full conversation and activity. Authenticated (GTC-FR-09, GTC-FR-CKTM).
- `"list pull request review comments"` → `list_pull_request_review_comments(id)` (no UI consumer). Authenticated (GTC-FR-09).

### Events
- `"branches changed"` — owned by `WTC-worktree-context.md`; `delete_branch` emits it on the terms of GTC-FR-NPCT.

### Internal (Rust API)
- `push_branch_at(checkout)` — pushes the branch that checkout has out, to the project's primary remote (GTC-FR-YHDU). Authenticated against a GitHub remote (GTC-FR-09). It is not registered as a Tauri command; `push_current_branch` composes it for the active worktree, and `GRD-graduation.md`'s run driver composes it for a work stream's checkout.
- `fetch_remote_branches()` — updates the repository's remote-tracking refs from the project's primary remote and drops the ones whose upstream branch is gone (GTC-FR-12). Authenticated against a GitHub remote (GTC-FR-09). It is not registered as a Tauri command, so no frontend call can reach it; it is the single fetch primitive in the codebase and `WTC-worktree-context.md` WTC-FR-22 composes it.

### Payload shapes
```
RollbackOutcome {
  entries: [RollbackEntryOutcome]    // one per path named in the request, in the order given
}

RollbackEntryOutcome {
  id,                        // the entry's stable path-derived key (ASC-artifact-scanning.md ASC-FR-13)
  path,                      // the project-relative current path that was named
  previous_path?,            // the pre-rename path; present only for a renamed entry
  outcome,                   // "restored" | "removed" | "failed"
  restored_paths: [path],    // project-relative paths confirmed written back to HEAD
  removed_paths: [path],     // project-relative paths confirmed deleted from the working tree
  failures: [PathFailure]    // one per path that did not succeed; empty unless outcome is "failed"
}

PathFailure {
  path,                      // the project-relative path that failed
  kind                       // "not_found" | "permission_denied" | "write_failed"
                             //   | "path_outside_content_root" | "is_directory"
}

WorkingTreeStatus {
  worktree_path,             // absolute; the active worktree the status describes
  entries: [WorkingTreeEntry]
}

WorkingTreeEntry {
  path,                      // project-relative, at its current location
  staged_status,             // what the index holds against HEAD, or null where it holds nothing:
                             //   "added" | "modified" | "deleted" | "renamed" | "type_changed"
  unstaged_status,           // what the working tree holds against the index, or null where it
                             //   agrees with it: "modified" | "deleted" | "renamed"
                             //   | "type_changed" | "untracked"
  previous_path?             // the pre-rename path; present only for a renamed entry
}
```

At least one of the two statuses is non-null, and both are non-null for a path staged and then changed again — the combined state Git reports as two letters (GTC-FR-30).

```
CommitHistory {
  branch?,                   // the active worktree's current branch; absent when detached
  is_detached,
  head_id?,                  // full id at HEAD; absent when the branch has no commit
  commits: [CommitSummary]   // at most 100, newest first
}

CommitSummary {
  id,                        // full commit id
  short_id,
  author_name,
  author_email,
  authored_at,               // Unix seconds, UTC
  subject,                   // first line of the message
  message,                   // the whole message
  refs: [string]             // short names of the local and remote-tracking branches whose tip is this commit
}

CommitFile {
  path,                      // project-relative, as the commit leaves it
  previous_path?,            // the pre-rename path; present only for a renamed or copied entry
  status,                    // "added" | "modified" | "deleted" | "renamed" | "copied" | "type_changed"
  is_binary
}

BranchInformation {
  name,                      // local branch name, or the remote-tracking branch's short name
  kind,                      // "local" | "remote"
  upstream?,                 // short name of the upstream; local branches only
  is_current,                // checked out in the active worktree
  worktree?,                 // { path, name, is_active, is_primary }: the worktree that has it checked out
  stream?,                   // { stream_id, stream_name }: the work stream whose branch it is
  tip,                       // CommitSummary of the branch tip
  commits: [CommitSummary]   // at most 100 reachable from the tip, newest first
}

BranchComparison {
  branch,                    // the compared branch, as named
  kind,                      // "local" | "remote"
  base,                      // the base branch's short name
  merge_base?,               // full id of the merge base; absent when same_as_base is true
  same_as_base,              // true when the branch is its own base; files is then empty
  files: [CommitFile]        // the paths changed from the merge base to the branch tip
}

BranchDeletionPlan {
  branch,
  worktree?,                 // { path, name, is_active, is_primary }; the linked worktree the deletion removes
  stream?,                   // { stream_id, stream_name, busy_run_id, ahead_of_base }; present for a work stream's branch
  uncommitted_paths: [path], // complete set the deletion would discard; empty when none
  remote_branch?             // short name of the associated remote-tracking branch, when one exists
}

BranchDeletionOutcome {
  branch,
  removed_worktree_path?,    // present when a linked worktree was removed
  remote: {
    requested,               // the caller's delete_remote choice
    state,                   // "not_requested" | "deleted" | "failed"
    branch?,                 // the remote branch, when one was named
    error?                   // the typed error, when state is "failed"
  }
}

PullRequestSummary {
  number, title,
  state,                     // "open" | "closed" | "merged"
  is_draft, author, head_branch, base_branch,
  created_at, updated_at     // RFC 3339
}

PullRequestDetail {
  number, title, state, is_draft, author, body, url,
  head_branch, base_branch,
  created_at, updated_at,
  closed_at?, merged_at?
}

CreatedPullRequest {
  number,                    // the pull request's number
  url                        // the address of its page on the host of the remote
}

PullRequestHeadState {
  head,                      // the local branch, as named
  base,                      // the base the answer is about; the default branch when none was named
  has_remote,                // the project has a primary remote
  remote_branch_exists,      // the primary remote has a remote-tracking ref of the same name
  unpushed,                  // commits head holds that the remote-tracking ref lacks; null when
                             //   remote_branch_exists is false
  uncommitted_paths,         // [path] the checkout that holds head reports uncommitted (GTC-FR-LEBC);
                             //   empty when no checkout holds head
  ahead_of_base              // commits head holds that base lacks
}

PullRequestTimeline {
  items: [PullRequestTimelineItem],  // oldest first
  truncated                          // true when GitHub held more than this module reads
}

PullRequestTimelineItem {
  id,
  kind,                      // "comment" | "review" | "review_comment" | "commit" | "event"
  actor?, created_at,
  body?,                     // Markdown text, where the item has one
  review_state?,             // "approved" | "changes_requested" | "commented" | "dismissed" | "pending"
  event?,                    // GitHub's event name, for kind "event"
  path?,                     // the file of a review comment
  commit_id?, subject?       // for kind "commit"
}
```

Typed errors returned by `rollback_paths` itself, in place of an outcome and with nothing written: `"not a git repository"` (GTC-FR-02) and `no_paths_selected` (GTC-FR-27).

Typed error returned by `get_working_tree_status` and `commit_paths` when the `expected_worktree` they were given is not the project's active worktree, having read nothing and written nothing: `worktree_identity_changed`, carrying the expected path and the active one (GTC-FR-31).

Typed errors of the history and branch operations: `"not a git repository"`, `"unknown branch"`, `unknown_commit`, `path_not_in_commit`, `no_comparison_base`, `no_merge_base`, `path_not_in_comparison`, `not_a_local_branch`, `branch_in_primary_worktree`, `branch_in_active_worktree`, `branch_belongs_to_work_stream`, `"direct graduation active"`, and `worktree_dirty` carrying the complete set of uncommitted paths. The pull request operations return `"no remote configured"`, `not_a_github_remote`, `pull_request_not_found`, the two token errors of GTC-FR-10, `github_token_rejected`, `github_host_mismatch`, and `github_unreachable`. `create_pull_request` also returns `pull_request_title_required`, `pull_request_exists`, and `pull_request_rejected: <reason>`, where the reason is the text GitHub gave. `get_pull_request_head_state` also returns `"unknown branch"`.

## Functional requirements
1. **GTC-FR-01** Every operation listed in the contract surface exists as a Tauri command with a typed payload. In the walking-skeleton build, implementations may return canned working-tree state, canned branch lists, canned diff payloads, and empty PR lists; the UI Git panel must be fully renderable against these stubs.
2. **GTC-FR-02** `get_working_tree_status`, `get_diff`, `get_file_revisions`, `commit_paths`, `rollback_paths`, `get_upstream_sync_state`, `stage_paths`, `unstage_paths`, `commit`, `list_branches`, `create_branch`, `checkout_branch`, and `list_commit_history` operate on the Git repository that owns the currently open project, and specifically on that project's **active worktree** — the checkout that roots the project (per `WTC-worktree-context.md` WTC-FR-03). In standalone mode the repository is the project's own; in co-located mode it is the host code repository. Working-tree state, staging, commits, rollbacks, and checkout all target the active worktree's index and working directory, never another worktree of the same repository. When the project is not inside a Git repository, every command returns a typed "not a git repository" error.
3. **GTC-FR-03** Every commit this module creates takes its authoring identity (name / email) from the user's Git configuration; this module does not own it. `commit_paths` commits the paths it is given (GTC-FR-19) and `commit` commits exactly the currently-staged set, each with the provided message.
4. **GTC-FR-04** `push_current_branch` and `pull_current_branch` emit their output incrementally via Tauri events on a stable channel name (`"git output line"`) and emit a terminal `"git operation finished"` event with the final status.
5. **GTC-FR-05** PR operations (`list_pull_requests`, `create_pull_request`, `get_pull_request_detail`, `list_pull_request_timeline`, `list_pull_request_review_comments`) target the project's primary remote. In the walking skeleton they return empty lists / typed "no remote configured" errors when no remote is available; they do not crash the UI.
6. **GTC-FR-06** Force-push and history rewrite are not exposed by this module (matches `../ui/GIT-git.md` GIT-FR-08). `delete_branch` is the only author-facing deletion of a branch or a worktree (GTC-FR-WNZH), and it refuses a work stream's branch (GTC-FR-JOWX). Dropping a stale remote-tracking ref (GTC-FR-13) and `rollback_paths` (GTC-FR-23) delete no branch and rewrite no history.
7. **GTC-FR-07** `list_branches` serves the Git panel's branches section and returns the repository's local and remote branches with the active worktree's branch marked current; it carries no worktree association, which is the concern of `WTC-worktree-context.md`'s richer `list_worktrees_and_branches`. The listing is unconditional — it describes the repository, and describing a branch is not offering to check it out, so it returns the same set whichever worktree is active. A work stream's branch is described like any other (per `WKS-work-streams.md` WKS-FR-MFDW), it being a line of work the author reads, diffs and merges rather than a scratch ref. `checkout_branch(name)` is the single checkout primitive in the codebase: `WTC-worktree-context.md` WTC-FR-10 composes it rather than reimplementing it, so a checkout requested from the Git panel and one requested from the worktree selector take the same path and produce the same re-rooting.
8. **GTC-FR-08** `checkout_branch(name)` returns a typed error and leaves the active worktree's branch, index, and working tree untouched when the checkout cannot proceed — because the active worktree is a linked one rather than the repository's primary checkout (per `WTC-worktree-context.md` WTC-FR-21), because the branch is checked out in another worktree of the repository, or because local modifications would be overwritten. The error variants are the ones `WTC-worktree-context.md` WTC-FR-12 and WTC-FR-21 surface. This command is the single checkout primitive (GTC-FR-07), so these refusals bind every route to a checkout in the application, not only the one that happened to be wired first.

9. **GTC-FR-09** The four PR operations and, when the remote is a GitHub HTTPS remote, `push_current_branch`, `pull_current_branch`, and `fetch_remote_branches`, authenticate with the secret `GTS-github-token-storage.md` resolves for the open project and the host of the remote (GTS-FR-13). This module obtains the secret at the moment it performs the operation, presents it to GitHub, and neither caches it nor includes it in a return payload; a remote that is not GitHub is unaffected and authenticates as it otherwise would.
    - *Why:* A remote on another provider must keep its own credentials, so only a host that is known to be GitHub receives a token.
10. **GTC-FR-10** When the project resolves no token, an authenticated operation performs no network request and returns a typed error that distinguishes the two cases the UI must respond to differently: `github_token_selection_required` when tokens are stored but the project has not been pointed at one, which the requesting surface answers by opening the token picker (`../ui/GIT-git.md` GIT-FR-10 for the Git panel, `../ui/CHG-changes.md` CHG-FR-45 for the Changes panel, `../ui/WTS-worktree-selector.md` WTS-FR-34 for the top-chrome refresh control), and `github_token_missing` when none is stored at all. The distinction is the one `GTS-github-token-storage.md` GTS-FR-10 draws.
11. **GTC-FR-11** No line emitted on the `"git output line"` channel, no `"git operation finished"` payload, and no error returned by any command in this module contains a token secret. A remote URL that would carry an embedded credential is redacted before it reaches the stream, so the push output area of `../ui/GIT-git.md` is safe to read, copy, and paste into an issue.
12. **GTC-FR-12** `fetch_remote_branches()` contacts the project's primary remote and brings the repository's remote-tracking refs into agreement with what that remote publishes: a branch that has appeared gains a remote-tracking ref, a branch that has moved has its ref advanced, and a remote-tracking ref whose upstream branch no longer exists on the remote is dropped. It writes nothing else — no local branch is created, moved, or deleted, `HEAD` is unchanged, and the index and working tree of every worktree are untouched — so a fetch never changes what is checked out anywhere in the repository.
13. **GTC-FR-13** The drop of a stale remote-tracking ref is part of the fetch rather than a separate operation, because a listing that keeps offering branches the remote no longer has misrepresents what can be checked out. It is bounded to remote-tracking refs alone: a local branch whose upstream has disappeared survives the fetch and keeps its content, and nothing on the remote is touched, so the no-deletion rule of GTC-FR-06 holds.
14. **GTC-FR-14** `fetch_remote_branches()` returns a typed `"no remote configured"` error when the project has no primary remote, and the two typed token errors of GTC-FR-10 when it resolves no credential for a GitHub HTTPS remote. In each case no network request is made and no ref is written. A remote that is reachable but refuses the credential, and a remote that cannot be reached at all, return distinguishable typed errors, because the caller presents them differently.
15. **GTC-FR-15** `fetch_remote_branches()` attributes its progress through `PRG-progress-reporting.md` under `kind = "git"` (per PRG-FR-11) and emits nothing on the `"git output line"` channel. A fetch is reported by the status bar for as long as it runs and leaves no transcript in the Git panel's push/pull output area, which stays the record of the operations the author invoked there. A fetch has no destination in the Git panel, so its operation carries no `activation` and its status bar row is not actionable (per `PRG-progress-reporting.md` PRG-FR-TBZN).
16. **GTC-FR-16** `get_file_revisions(scope)` returns each side's content as the comparison defines it: the **new** side is the file as it stands in the active worktree's working directory, for the branch comparison as much as the uncommitted one, because that is the side `get_diff` compares against in both; the **old** side is the blob the comparison's base revision holds — `HEAD` for the uncommitted scope, the merge-base tree for a branch scope, resolved as `get_diff` resolves it. A side the comparison has no version for is returned as `null` rather than as an empty string, so an added file and a file that became empty are distinguishable. A renamed entry's old side is read at its pre-rename path, or the old revision would read as absent and the whole file as an addition.
17. **GTC-FR-17** The two read operations never disagree about a path. A binary file returns neither text and is flagged as binary, matching the marker `get_diff` returns for it — including when the path is binary by `.gitattributes` rather than by its bytes. Line terminators are normalised on both sides, because `get_diff` compares the *filtered* working-directory text: under `core.autocrlf` or an `eol=crlf` attribute a file whose blob is LF and whose checkout is CRLF has no diff, and returning the raw bytes would make the whole-file modes render every line of a Windows checkout as changed under a diff that showed nothing.
18. **GTC-FR-18** A path that resolves outside the project's content root is refused with a typed error rather than read, and a read that fails for any reason other than the file's absence is an error rather than a `null` side — `null` means "this revision has no such file", which the caller renders as a deletion, and a permission failure is not a deletion.
19. **GTC-FR-19** `commit_paths(message, paths)` creates one commit holding exactly the named paths against the active worktree. A named path Git does not track is added to version control and included; a named path absent from the working tree is recorded as a deletion; a named path that was renamed is recorded at both its previous and its current location; every other named path is recorded with its current working-directory content. A path that is not named is left exactly as it was and is absent from the commit — including one that happened to be staged, which stays staged and uncommitted — so the commit holds what the caller asked for and nothing it did not. A successful call returns the new commit's id together with `committed_paths`: **every path the commit recorded**, which is the named set plus the previous location of each rename, because a caller acting on what was committed — the strip closing the Diff tabs a commit has finished with (`../ui/TAB-tabs.md` TAB-FR-22) — must be told what landed rather than left to re-derive it from what it asked for. A call that returns an error of GTC-FR-20 returns no outcome, no commit having been created. The operation is **reached from inside the application as well as from the Git surface**: `PST-project-storage.md` PST-FR-DQZT commits application-owned storage through this same contract, so the one place a commit of named paths is built stays this one, and a caller that names only its own folder gets the leave-the-rest-staged guarantee above for free rather than reimplementing it.
20. **GTC-FR-20** `commit_paths` validates before it writes anything. An `expected_worktree` that is not the active worktree returns `worktree_identity_changed` ahead of every other check (GTC-FR-31), because a commit aimed at one checkout must not be corrected into another; an empty or whitespace-only message returns `empty_commit_message`; an empty path list returns `no_paths_selected`; a path resolving outside the content root is refused exactly as GTC-FR-18 refuses one; and a set in which no named path differs from `HEAD` returns `nothing_to_commit`. Each of these leaves the index, the refs, and every working-tree file exactly as they were, and creates no commit object.
21. **GTC-FR-21** `get_upstream_sync_state()` reports the active worktree's current branch against its upstream: `has_remote` says whether the project has a primary remote at all, `has_upstream` whether the branch tracks a branch on it, and `ahead` and `behind` count the commits each side holds that the other does not. It resolves entirely from local refs, makes no network request, and writes nothing, so it describes the remote as the last fetch left it (GTC-FR-12). A branch with a remote configured but no upstream returns `has_upstream = false` with null counts, because it has never been published and a push would publish it; a detached `HEAD` returns the same, since it tracks nothing.
22. **GTC-FR-22** `push_current_branch` attributes its progress through `PRG-progress-reporting.md` under `kind = "git"` (per PRG-FR-11) as well as streaming on `"git output line"` (GTC-FR-04), so a push is visible in the status bar for as long as it runs. The operation carries the activation destination `{ type: "git_push", branch }` with the name of the branch the push publishes (per `PRG-progress-reporting.md` PRG-FR-KXQW), which the Git panel selects (per `../ui/GIT-git.md` GIT-FR-FZMS). Both channels are surface-agnostic: a push invoked from the Git panel's branches section and one invoked from the Changes panel (`../ui/CHG-changes.md` CHG-FR-43) emit identically and land in the same output area, because a push is one operation however it was started.

23. **GTC-FR-23** `rollback_paths(paths)` returns each named path to the state `HEAD` holds for it in the active worktree, discarding that path's staged and unstaged changes together rather than either alone. A modified path is rewritten with its `HEAD` content; a path deleted from the working tree is put back; a path added to the index is unstaged and its file removed, `HEAD` holding no such file; a renamed path is undone at both of its locations (GTC-FR-26); and a path Git does not track is removed from the working tree, an untracked file having no `HEAD` state to return to. A path the working tree and `HEAD` already agree on is not an error and needs nothing done to it.
24. **GTC-FR-24** Every filesystem removal this operation performs goes through `FSA-filesystem-access.md`'s `delete_path` (FSA-FR-11) with the file's own path, and no raw filesystem call stands beside it. Only the paths the caller named are removed: a parent directory the removal leaves empty stays exactly where it is, because the caller selected a file and discarding the folder around it would discard a location they did not choose. A named path that resolves to a directory rather than a file is reported as the typed `is_directory` failure and nothing beneath it is touched.
25. **GTC-FR-25** The operation is **per-path rather than atomic**. Each named path is attempted on its own and reported on its own, and a failure at one path neither aborts the run nor undoes a path that has already succeeded. `restored_paths` and `removed_paths` hold only what the operation confirmed on disk; a path that failed appears in `failures` with its typed cause and in neither list, and no path appears in both. An entry's `outcome` follows from those lists: `"failed"` when any of its paths failed, and otherwise `"removed"` when every path it acted on was removed and `"restored"` when any path was written back to `HEAD`. The result therefore never reports as successful a path the operation did not reach — which is the property the caller relies on to decide whose in-memory edits it may discard (`../ui/CHG-changes.md` CHG-FR-63).
26. **GTC-FR-26** A renamed entry is **two path identities and is reported as both**. Its `path` names the current location and its `previous_path` the pre-rename one, and the rollback restores the previous path's `HEAD` content and removes the current one, `HEAD` holding the file only under the name it was committed with. Each identity lands in the list its own result belongs to — the previous path in `restored_paths`, the current path in `removed_paths` — and an identity that failed appears in `failures` while the other keeps whichever list it earned. A caller resetting the editing sessions behind that entry therefore resets exactly the identities the operation reached (`../ui/CHG-changes.md` CHG-FR-63).
27. **GTC-FR-27** `rollback_paths` validates before it writes anything. An empty path list returns `no_paths_selected`; a path resolving outside the content root is refused exactly as GTC-FR-18 refuses one; and a project that is not a Git repository returns the typed error of GTC-FR-02. Each of these returns no outcome at all and leaves the index, the refs, and every working-tree file exactly as they were. A refusal that binds one path alone — a path outside the content root among a set of valid ones — is that path's typed `path_outside_content_root` failure rather than a refusal of the call, on the per-path terms of GTC-FR-25.
28. **GTC-FR-28** The operation's whole reach is the working tree and the index at the paths it was given. It creates no commit, moves and creates and deletes no ref, rewrites no history, stages nothing it was not asked to unstage, and touches no other worktree of the repository; a path that was not named keeps its modification and its index state exactly as they were, including one that happened to be staged. The working-tree change it makes reaches the application as the ordinary debounced `"changes updated"` event of `CHC-changes.md` CHC-FR-16, this operation emitting nothing of its own.
29. **GTC-FR-29** `get_working_tree_status` describes the active worktree resolved at the moment of the call (GTC-FR-02, GTC-FR-31) and returns that worktree's path beside its entries, so a caller that must act on the same checkout it read can compare the two rather than assume they agree. The set is **complete and unclassified**: every path Git reports is present, and the only path it drops is one the repository's ignore rules exclude. It resolves no artifact type, applies no lens, and drops no path for any other reason, which is what makes it the primitive a precondition is read from rather than a surface's change set.
30. **GTC-FR-30** Each entry preserves **what Git reports on each side of the index**. `staged_status` says what the index holds against `HEAD` and is null where it holds nothing; `unstaged_status` says what the working tree holds against the index and is null where the two agree; at least one of the two is non-null, and a path staged and then changed again carries **both**, which is the combined state Git reports as two letters and which neither status alone describes. An untracked path carries `unstaged_status = "untracked"` with no staged status, a rename carries its pre-rename path beside whichever side reports it, and no entry collapses the pair into a single overall state — a caller that needs one composes it, and one that must show the author exactly what stands where has it.
31. **GTC-FR-31** `get_working_tree_status` and `commit_paths` each take an optional **`expected_worktree`**, and where one is given the operation is bound to that identity: it compares the path against the project's active worktree (per `WTC-worktree-context.md` WTC-FR-03) **before it reads or writes anything**, and a mismatch returns `worktree_identity_changed` carrying the expected path and the active one, having read no status, created no commit, and touched no index. Where none is given both behave exactly as they otherwise do, against the active worktree at the moment of the call. The guard exists because a status read and the commit that answers it are two calls the author may switch checkouts between, and a caller carrying one identity through both is what stops a commit aimed at the checkout it was shown from landing in another. A refusal writes nothing, changes no worktree, and leaves the caller to decide what a moved checkout means for the work it was doing.
32. **GTC-FR-32** `get_working_tree_status` is the single working-tree status primitive, and every caller that needs the state of a checkout composes it rather than reading the repository itself. It is read-only under the terms of GTC-FR-06: it writes no ref, no index entry, and no working-tree file.
33. **GTC-FR-YHDU** The push is one primitive, parameterised by the checkout it pushes from. `push_current_branch` composes it for the active worktree, and `GRD-graduation.md`'s run driver composes it for a work stream's checkout (per GRD-FR-PXVJ). Both authenticate on the terms of GTC-FR-09, and each reports where its own caller reports.
34. **GTC-FR-BQNM** `list_commit_history()` returns at most the 100 latest commits reachable from the active worktree's `HEAD`, newest first by commit time, with the current branch name or `is_detached`. A branch with no commit returns an empty list. It reads local objects only, writes nothing, and returns `"not a git repository"` outside a repository.
35. **GTC-FR-RFLW** Each `CommitSummary` carries the author's name and email from the commit, the author time, the subject, the whole message, and in `refs` the short names of every local and remote-tracking branch whose tip is that commit. A symbolic `HEAD` pointer of a remote is not a ref.
36. **GTC-FR-YCEV** `list_commit_files(commit_id)` returns the paths the commit changed against its first parent, or against the empty tree for a root commit. A rename carries its previous path. A binary file is flagged. An id that names no commit returns `unknown_commit`. It writes nothing.
37. **GTC-FR-PDSK** `get_commit_file_diff(commit_id, path)` returns the `DiffPayload` of that commit's change to that path against its first parent, with the same hunk and binary-marker shape as `get_diff`. A path the commit did not change returns `path_not_in_commit`, and an unknown id returns `unknown_commit`. It writes nothing.
38. **GTC-FR-TGOI** `get_branch_information(name, kind)` returns the branch's name and kind, its upstream for a local branch, whether it is current, the worktree that has it checked out, the work stream that owns it, its tip, and at most 100 commits reachable from the tip, newest first. A name that is not a branch of that kind returns `"unknown branch"`. It writes nothing.
39. **GTC-FR-UMXA** `inspect_branch_deletion(name)` reads, and writes nothing. It returns the plan of GTC-FR-WNZH for a local branch the deletion would accept, or the typed refusal that `delete_branch` would return at that moment. The plan names the linked worktree to remove, the work stream, the complete uncommitted path set, and the associated remote branch.
40. **GTC-FR-WNZH** `delete_branch` re-reads the repository before it changes anything and refuses, in this order: `"unknown branch"`, `not_a_local_branch`, `branch_in_primary_worktree`, `branch_in_active_worktree`, `branch_belongs_to_work_stream`, `"direct graduation active"`, and `worktree_dirty` when uncommitted paths exist and `discard_uncommitted` is false. A refusal changes no branch, worktree, file, or remote.
41. **GTC-FR-AVKD** A branch checked out in another, non-active linked worktree is deleted with that worktree: the worktree directory and its registration are removed through `WTC-worktree-context.md` WTC-FR-RDVK, then the local branch. A branch checked out nowhere loses only the branch. The operation switches no branch and no worktree, and the active worktree is untouched.
42. **GTC-FR-LEBC** The uncommitted paths of a worktree are every tracked path that differs from `HEAD` and every untracked path outside application-owned storage, read as `WKS-work-streams.md` WKS-FR-JVLM reads them. With `discard_uncommitted` true, `delete_branch` removes the worktree with those paths. A worktree whose directory is missing reports no path.
43. **GTC-FR-FSRQ** With `delete_remote` true, `delete_branch` first resolves the credential (GTC-FR-09) and refuses with the typed token error of GTC-FR-10 before any change. After the local deletion it deletes the associated remote branch and its remote-tracking ref. A remote failure reports `remote.state = "failed"` with its typed error, and local results stay. With `delete_remote` false, no remote is contacted.
44. **GTC-FR-JOWX** `inspect_branch_deletion` returns the `stream` of a work stream's branch with its working copy's uncommitted paths, its worktree, and `ahead_of_base` counted as WKS-FR-EIBC counts it, and `delete_branch` refuses that branch with `branch_belongs_to_work_stream` naming the stream id. Only `WKS-work-streams.md` WKS-FR-EIBC removes a stream, so no stream check is bypassed.
45. **GTC-FR-NPCT** A `delete_branch` that removes at least a branch emits `"branches changed"` once (per `WTC-worktree-context.md` WTC-FR-25), including when the remote part failed. It emits nothing on refusal. Every inspection, refusal, and result is logged, with counts and branch names but no path content and no secret.
46. **GTC-FR-GXUB** `list_pull_requests(state)` resolves the host, the owner, and the repository of the primary remote when it is a GitHub remote, and returns every PR of that state, newest update first, following GitHub's pages up to 1000 PRs. `"closed"` includes merged PRs, flagged `merged` in `state`. Any other `state` returns a typed error. A remote that is not a GitHub remote returns `not_a_github_remote`.
47. **GTC-FR-ZIHE** `get_pull_request_detail(id)` returns the PR's header fields and its description as written, including an empty one. An unknown number returns `pull_request_not_found`. A token GitHub refuses returns `github_token_rejected`, and an unreachable GitHub returns `github_unreachable`. No request is made while the project resolves no token (GTC-FR-10).
48. **GTC-FR-CKTM** `list_pull_request_timeline(id)` returns, oldest first, every comment, review, review comment, commit, and other activity event GitHub reports for the PR, reading every page up to 1000 items. Each item carries its actor, time, and body when it has one. When GitHub holds more, `truncated` is true. Secrets never appear in any item.
49. **GTC-FR-DWVY** The pull request operations send the token only to the API base that `GTS-github-token-storage.md` GTS-FR-PDWB resolves from the host of the remote, bound the time of every request, never cache the token, and write no token in a log or return value (GTC-FR-11). A failed page after the first returns an error rather than a partial list reported as complete.
50. **GTC-FR-YQVD** `list_branch_compare_files(name, kind)` compares a branch with its base. The base of a work stream's branch is the stream's `base_branch` (per `WKS-work-streams.md` WKS-FR-KDXF). The base of every other local or remote branch is the repository's default branch, resolved as `CHC-changes.md` CHC-FR-13 resolves it. It reads local objects only and writes nothing.
51. **GTC-FR-MBBH** The comparison returns the base, the merge base of the base tip and the branch tip, and the paths changed from the merge base tree to the branch tip tree, with the rename and binary flags of GTC-FR-YCEV. A branch that is its own base returns `same_as_base` true and no path.
52. **GTC-FR-PYVV** `get_branch_compare_file_diff(name, kind, path)` returns the `DiffPayload` of that path from the merge base tree to the branch tip tree, with the hunk and binary-marker shape of `get_diff`. A path the comparison did not change returns `path_not_in_comparison`. It writes nothing.
53. **GTC-FR-QVDE** Both comparison operations return `"unknown branch"` for a name that is not a branch of that kind, `no_comparison_base` when the base names no branch, and `no_merge_base` when the base and the branch share no commit. Each logs its start, its file count, and every error, with branch names but no path content.

54. **GTC-FR-MMFM** `create_pull_request(title, body, base, head, draft)` first checks its arguments: a title that holds nothing but white space returns `pull_request_title_required`, and an empty `head` or `base` returns `"unknown branch"`. It then resolves the owner and repository of the primary remote on the terms of GTC-FR-GXUB and the token on the terms of GTC-FR-10, before any request is made. It sends one request that creates the pull request from `head` into `base`, with the title and the description as given and the `draft` flag, and returns the number and the page address of the pull request GitHub created. It creates no commit, pushes no branch, and changes no ref and no file.
55. **GTC-FR-YQAE** `create_pull_request` returns `pull_request_exists` when GitHub reports that a pull request for the same head and base is already open, and `pull_request_rejected: <reason>` for any other refusal of the content, such as a head branch GitHub does not hold or a head with no commit its base lacks. GitHub's refusal of the token returns `github_token_rejected`, and so does an answer that the repository is not found, because a repository that the project's token cannot see is answered that way (as in GTC-FR-GXUB). A failure to reach GitHub returns `github_unreachable`, and so does a redirect answer: the operation does not follow a redirect, because a redirected request is not the creation of a pull request. When GitHub refuses a head branch or a base branch it does not hold, its answer names a field and a code and carries no message text, and the reason of `pull_request_rejected` then names that field and that code, for example `head invalid`. Neither the title, the description, nor the token is written to a log or to an error (GTC-FR-DWVY). Its log records hold the owner, the repository, the branch names, the draft flag, and the number of the pull request.
56. **GTC-FR-NEIW** `get_pull_request_head_state(head, base)` reads local state alone: it makes no network request and writes nothing. `head` must be a local branch, or the call returns `"unknown branch"`. An absent or empty `base` is the repository's default branch, resolved as `CHC-changes.md` CHC-FR-13 resolves it, and a `base` that is neither a local branch nor a remote-tracking branch returns `"unknown branch"`. It reports on the remote as the last fetch left it (GTC-FR-12).
57. **GTC-FR-YWCP** `remote_branch_exists` is true when the primary remote has a remote-tracking ref for `head`, and `unpushed` counts the commits reachable from `head` and not from that ref. `uncommitted_paths` is read from the checkout that holds `head`, whether it is the primary worktree, the active worktree, another linked worktree, or the working copy of a work stream, on the terms of GTC-FR-LEBC, and is empty when no checkout holds `head`. `ahead_of_base` counts the commits reachable from `head` and not from the base, counted as `WKS-work-streams.md` WKS-FR-EIBC counts the commits of a stream.
58. **GTC-FR-FSLC** A GitHub remote is an HTTPS remote whose host is `github.com`, ends in `.ghe.com`, or is the host of a stored token. When the token the project resolves belongs to another host, an authenticated operation returns the typed error `github_host_mismatch` (`GTS-github-token-storage.md` GTS-FR-OBAS), makes no network request, and writes no ref.
59. **GTC-FR-XUAC** The pull request operations use the API base of the host of the remote for every request. `create_pull_request` accepts the page address that GitHub returns only when it is an HTTPS address on that host.

## Non-functional requirements
- Long-running operations (push, pull, network calls) must stream output rather than block the Tauri invoke; the UI Git panel relies on streaming for its push/pull output area.
- `fetch_remote_branches` is the only operation in this module that reaches a remote without the author having asked for a transfer of their own work, so it is invoked only in response to an explicit request and never on a timer, on window mount, or as a side effect of a listing.
- The Git engine choice (gitoxide per the Notion tech-stack) is referenced but not specified here; this spec does not constrain implementation details beyond the contract surface.
