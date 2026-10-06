# Work stream selector

**Spec code:** `WSS`

## Intent
The surface an author manages work streams from: creating one, seeing what each holds, bringing one up to the branch it came from, merging it back into that branch, and removing it when the work has landed. A base branch moves outside the application, so a row says how far behind its base a stream stands and offers to update it in place. A stream is where graduation runs do their work, so this is also where the author sees which stream an agent is busy in and how far ahead of its base each stream stands. A merge is an author-started operation that Git settles first. A merge Git settles on its own completes here at once, and this surface shows what landed, with no run. A merge Git cannot settle is handed to a **merge run**, a graduation run named `Merge <stream>` that stands in the Runs panel and is watched, paused, continued, answered and discarded there (per `GRU-graduation-runs.md`); this surface shows its status on the stream's row and opens it in Runs. An update is agent work too: Git settles what it can and one semantic turn settles the rest. An update runs on its own and may stop to ask the author a question, so what it settled stands on the row until they act on it, and the question is answered in a window this surface opens. It sits beside the worktree selector in the top chrome, because a stream is a checkout of the same repository and the author moves between the two the same way. Out of scope: the runs themselves, merge runs included, which are `GRU-graduation-runs.md`'s; the ordinary worktrees, which are `WTS-worktree-selector.md`'s.

## Functional requirements
1. **WSS-FR-JVUF** The selector is a top-chrome control rendered immediately after the **Push** control, which follows the refresh control of the worktree selector (per `SNV-shell-navigation.md` SNV-FR-32 and `GIT-git.md` GIT-FR-XXLE), and only while the open project's content root sits inside a Git repository.
2. **WSS-FR-DGAG** The control's resting label names how many live streams the project holds, and says when one of them is busy.
3. **WSS-FR-SOAS** Activating the control opens a dropdown listing every stream, each row naming the stream, its branch, how many commits it stands ahead of its base branch, how many it stands behind, and how many runs are queued on it.
4. **WSS-FR-JMWA** A row names at most the first few of any path set it carries and counts the rest, the whole set standing in the row's accessible semantics and, for an update, in the update resolution window. No row grows past the listing's own scrolling region.
    - *Why:* A merge handoff or an update may leave a dozen paths unsettled, and a row that named them all would put the stream's own name and its action out of view together.
5. **WSS-FR-XZRD** The dropdown and the New stream dialog are floating overlays of the main window, and every such overlay is mutually exclusive with every other (per `SNV-shell-navigation.md` SNV-FR-56).
6. **WSS-FR-PSXK** A stream a run holds renders flagged **busy**, naming the draft the run works on rather than the run's id. Where the stream's merge run holds it, the line names that merge run instead, because a merge run has no draft. Where the draft's name cannot be read, the line names no run and no id at all. Its **Merge stream**, **Update stream** and **Delete stream** actions are disabled while it is busy, and the reason is given in words on the row.
7. **WSS-FR-OQYG** A stream the backend reports as missing renders flagged missing and offers re-creation alone; its other actions are disabled.
8. **WSS-FR-YCAL** Selecting a stream row opens that stream's working copy as the project's content root, through `"activate worktree (path)"`. A busy stream is not selectable, and the refusal is rendered on the row rather than as a window-level message.
9. **WSS-FR-PDFX** The dropdown provides a **New stream…** action rendered outside the listing's scrolling region. Activating it closes the dropdown and opens the New stream dialog.
10. **WSS-FR-XZRO** The New stream dialog has exactly two inputs: a name, and the branch the stream is created from. The branch is pre-filled with the branch checked out in the active worktree and is chosen from the project's branches.
11. **WSS-FR-CRJD** The dialog's confirm invokes `"create work stream (name, base branch)"`. A duplicate name, an empty name, and a failed creation are each rendered inline against the field they are about, and the dialog stays open.
12. **WSS-FR-YPDA** A stream row offers **Merge stream**, which opens a confirmation naming the base branch the stream merges into, the commits it carries, and the choice between leaving the result uncommitted and committing it under a message the author writes. It is disabled where the stream stands ahead of its base by nothing.
13. **WSS-FR-VMNV** The merge confirmation states plainly that Git merges first. It states that a merge Git settles completes at once and makes no run, and that a merge Git cannot settle is handed to a **merge run** named `Merge <stream>` in the Runs panel, where an agent reconciles it and the author controls it. It states that neither branch changes until a review of the reconciled result has judged it ready.
14. **WSS-FR-OMAP** A merge that refuses because either side holds uncommitted paths renders the complete path set and routes to the Changes panel, rather than offering to commit or discard that work here.
15. **WSS-FR-TQBN** The confirmation's commit choice opens the commit message window for the message alone (per `CMW-commit-message.md` CMW-FR-KRVP). That window closes as soon as it has one, and this surface starts the merge with it. A dismissed window starts nothing.
16. **WSS-FR-HGWL** While the `"merge work stream (id, publication)"` call runs, the stream's row renders flagged **merging** and states `Merging <stream>`. The row's **Merge stream**, **Update stream** and **Delete stream** actions and its selection are disabled, and the row carries an accessible busy status. The row offers no cancel. The state is held by this surface for the length of the call alone, and it ends when the call settles, whatever it returns.
17. **WSS-FR-OFCU** A row renders the status of the stream's **merge run** from `mergeRun` in the listing (its run id, its name and its state) rather than from what this surface attempted. A merge handoff one window started therefore renders in every other, and a merge run that changed state while the dropdown was closed is rendered when it next opens. A stream whose listing holds no `mergeRun` renders no merge status.
18. **WSS-FR-RJTN** A row whose `mergeRun.state` is `awaiting_author` renders `Merge <stream>` as waiting on the author, says in words that the answer or the decision is made in Runs, and offers **Open in Runs…** (WSS-FR-AWRS). The row offers no **Answer…** and no **Continue** for a merge.
19. **WSS-FR-GBWE** A row whose `mergeRun.state` is `failed` says that the merge run failed, that neither branch was written, and that the cause is in Runs, and it offers **Open in Runs…** (WSS-FR-AWRS). The row keeps that status until the run is discarded or archived, and its **Merge stream** action stays offered so that the author can start the merge again.
20. **WSS-FR-KDVU** The **update resolution window** is a floating overlay of the main window, mutually exclusive with every other (per `SNV-shell-navigation.md` SNV-FR-56). It is dismissed by Escape and by its own close control, and the row that opened it opens it again.
    - *Why:* Answering the question means reading the files the update could not settle, so a window the author cannot put down would block the decision it asks for.
21. **WSS-FR-ZMPC** The window opens for an update alone. It names the stream, the base branch, the update record it opened against, and what that record rests on: the unsettled paths, and the escalation's reason and question set. It renders questions on the terms `GEA-graduation-escalation-answering.md` sets, and offers **Send answers**, **Retry**, **Cancel** and **Dismiss** as that record permits.
22. **WSS-FR-NRCQ** For a merge the selector offers **Open in Runs…** and nothing that acts on the run: no **Continue**, **Answer…**, **Discard**, **Pause** or **Cancel merge**. Those are the Runs panel's (per `GRU-graduation-runs.md` GRU-FR-TOKG). No window of this surface renders a merge's escalation.
23. **WSS-FR-NPXC** Every typed refusal of a merge renders on the stream's own row rather than as a window-level message: `stream_busy`, `stream_missing`, `unknown_stream`, `not_a_git_repository`, `merge_in_progress`, `update_in_progress`, `base_not_checked_out`, the image refusals `vendor_image_unconfigured`, `vendor_image_invalid`, `vendor_execution_unsupported` and `docker_backend_unverified`, and the dirty refusals of WSS-FR-OMAP. A merge the author started is answered where they started it.
24. **WSS-FR-TKMB** A stream row offers **Update stream**, placed between **Merge stream** and **Delete stream**. It brings the stream up to the branch the stream records as its base, whichever branch the active worktree holds. The three actions are compact buttons, and each label names what it acts on. The row centers them in the dropdown's width.
25. **WSS-FR-HZVQ** **Update stream** is enabled only where the stream stands behind its base branch and holds no queued and no active run. It is disabled where the stream is up to date with its base and where a run is queued on it or holds it, and the row says which in words.
26. **WSS-FR-NLXD** **Update stream** opens the **stream update window**, a floating overlay mutually exclusive with every other (per `SNV-shell-navigation.md` SNV-FR-56). It names the stream and the base branch, shows the base branch's revision as the listing read it, and lists the commits the stream is missing on the terms WSS-FR-JMWA sets.
27. **WSS-FR-WPGR** The window offers **Merge source into stream**, **Rebase stream onto source** and **Cancel**. Confirming either strategy invokes `"update work stream (id, strategy, base revision)"` with the revision the window displayed, and the window closes. **Cancel** starts nothing.
28. **WSS-FR-CJYE** An update refused with `stale_base_revision` says the base branch moved since the window read it, states that nothing was written, and asks the author to open **Update stream** again for a new revision. The surface re-runs nothing on its own.
29. **WSS-FR-QSAF** An update refused because either working copy holds uncommitted paths names each dirty worktree, renders its complete path set, and routes to the Changes panel, on the terms WSS-FR-OMAP sets for a merge. A `base_not_checked_out` refusal says the base branch has no checkout.
30. **WSS-FR-BDMU** A stream whose update is running renders flagged **updating**, naming the strategy, which semantic turn of the bound it is on, and the paths that turn is reconciling. It offers **Cancel update**, which invokes `"cancel work stream update (id)"`; a cancelled update changes neither branch and neither working copy.
31. **WSS-FR-XRHT** **Merge stream** and **Update stream** are both disabled while a merge call of that stream runs (WSS-FR-HGWL), while an update of that stream runs, until a cancellation of an update settles, and while the stream's `mergeRun` is in a state that is not `completed` and not `failed`. The row says which in words. One reconciliation of a stream is startable at a time.
32. **WSS-FR-FVKO** A row renders the stream's **update record** rather than what this surface attempted, so an update one window started renders in every other, and an update that settled while the dropdown was closed is rendered when it next opens. The record survives a reload and a relaunch.
33. **WSS-FR-GTQL** A stream whose update rests **escalated** offers **Answer…**, and one that rests **conflicted**, **failed** or **cancelled** says what stopped it, names the paths still unsettled, and offers **Review update…**. Each opens the update resolution window against the update record.
34. **WSS-FR-PMYA** The update resolution window invokes the update operations alone — `"answer work stream update escalation (id, answers)"`, `"retry work stream update (id)"`, `"cancel work stream update (id)"` and `"clear work stream update (id)"` — and never `"merge work stream (id, publication)"` or an operation on a graduation run.
35. **WSS-FR-ZWCB** Every typed refusal of an update renders on the stream's own row: `stream_busy`, `stream_missing`, `stale_base_revision`, `update_in_progress`, `merge_in_progress`, `update_attempts_exhausted`, `update_interrupted`, `update_state_not_permitted`, and the refusals of WSS-FR-QSAF.
36. **WSS-FR-UFZP** A stream row offers **Delete stream**, which confirms before it removes anything. For a stream holding commits its base branch does not hold, the confirmation says how many and offers **Merge instead** and **Delete anyway**. **Delete anyway** continues to the confirmation of WSS-FR-BRMT and WSS-FR-NHCV, which then passes `force` true.
37. **WSS-FR-JBYF** The surface renders from what the backend reports rather than from what it attempted. Every `"work streams changed"` event reloads the listing, whether a stream's merge run state, its update record, or the set of streams changed.
38. **WSS-FR-RWLB** The whole surface is operable by keyboard alone, and every state — busy, missing, ahead of base, behind base, merging, merge run status, updating, refused — is carried in words and in accessible semantics rather than by colour alone.
39. **WSS-FR-KMHD** A merge call that returns the `merged` kind renders its result on the stream's row: the stream merged, how many paths it merged, whether the result was left uncommitted in the base worktree or committed (with the commit in short form and the whole id in accessible semantics), and that no run was made. A call that returns the `nothing_to_merge` kind renders that the stream holds nothing its base branch does not hold, and that nothing was written. Neither result makes a run, and neither adds a row to the Runs panel. The result is the answer to a finished call and is no record: it stays on the row until the dropdown closes or the author starts another merge, update or delete of that stream.
40. **WSS-FR-PLVE** A merge call that returns the `conflicted` kind makes the surface reload the listing and render `Merge <stream>` from the stream's `mergeRun` with its state in words (WSS-FR-OFCU). The row also says that Git could not settle the merge, that neither branch was written, how many paths conflicted, and the first few of them (WSS-FR-JMWA), taken from the call's `conflictedPaths`. The row offers **Open in Runs…**. The surface renders no merge status from the call's result alone.
41. **WSS-FR-AWRS** **Open in Runs…** opens the bottom panel on the Runs surface with its graduation section active and the run named by the row's `mergeRun.runId` selected, closing the dropdown. The result is the one a `run` address has (per `NTF-notifications.md` NTF-FR-17), and the section selects the run on the terms of `GRU-graduation-runs.md` GRU-FR-PAHN.
42. **WSS-FR-DTYB** A row names a merge run's `mergeRun.state` in words, one word set per state: `queued` as waiting, `working` as reconciling, `reviewing` as being reviewed, `blocked` as blocked, `interrupted` as interrupted, `awaiting_author` as waiting on the author, `failed` as failed, and `completed` as merged. Each row of a non-terminal state offers **Open in Runs…**, and a `completed` row names the run and offers it too.
43. **WSS-FR-BRMT** Opening the **Delete stream** confirmation reads `"get work stream uncommitted paths (id)"`. The confirmation shows that it is loading, and its **Delete** action stays disabled until the read answers. A failed read shows its error in the confirmation, offers a retry, and leaves **Delete** disabled.
44. **WSS-FR-NHCV** When the stream's working copy holds uncommitted paths, the confirmation warns in words that deleting discards them, with their count and the first few paths (WSS-FR-JMWA). Its confirm action then names the discard and invokes `"delete work stream (id, force, discard uncommitted)"` with `discard uncommitted` true. Without uncommitted paths it passes false. Cancel changes nothing.
45. **WSS-FR-PKQD** The refusals `stream_busy`, `stream_has_runs`, `stream_active`, `stream_unmerged`, and `stream_dirty` of a deletion render on the stream's own row. A `stream_dirty` refusal shows the warning of WSS-FR-NHCV with the paths it carries. No confirmation carries a way to remove a stream that a run holds, that has a non-terminal run, or that is the active worktree.

## Wireframes

```
┌ streams (3) · 1 busy ─────────────────────────────┐
│ ⟳ editor-work        4 ahead · 2 queued  ● busy   │
│   synthesis/stream/editor-work                    │
│   "Editor scroll fix" is working in it            │
│                                                   │
│ ⟳ docs-pass           1 ahead · 0 queued ● merging│
│   synthesis/stream/docs-pass                      │
│   Merging docs-pass                               │
│ ⟳ bounds-work    2 ahead · 0 queued ● asks you    │
│   synthesis/stream/bounds-work                    │
│   Merge bounds-work waits on you: answer in Runs  │
│                                [ Open in Runs… ]  │
│ ⟳ spec-split     1 ahead · 0 queued  ● reconciling│
│   synthesis/stream/spec-split                     │
│   Git could not settle 3 paths. Nothing written.  │
│   TAB-tabs.md, tabs.ts, tabs.test.ts              │
│   Merge spec-split: reconciling                   │
│                                [ Open in Runs… ]  │
│ ⟳ ui-polish      2 ahead · 0 queued               │
│   synthesis/stream/ui-polish                      │
│   Merged 5 paths, left uncommitted. No run made.  │
│ ⟳ api-shim   0 ahead · 3 behind · 0 queued        │
│   synthesis/stream/api-shim                       │
│   3 commits on main are not in this stream        │
│  [Merge stream] [Update stream] [Delete stream]   │
│   spike-undo   0 ahead · 0 behind · 0 queued      │
│  [Merge stream] [Update stream] [Delete stream]   │
├───────────────────────────────────────────────────┤
│ + New stream…                                     │
└───────────────────────────────────────────────────┘
```

```
┌ Update stream "api-shim" ─────────────────────────┐
│ Source: main at 9f2c1ab                           │
│ 3 commits are not in this stream:                 │
│   9f2c1ab  tighten adapter timeouts               │
│   41b0d7e  drop the legacy shim                   │
│   c30a915  bump the toolchain                     │
│                                                   │
│ [ Merge source into stream ]                      │
│ [ Rebase stream onto source ]              [ Cancel ]│
└───────────────────────────────────────────────────┘
```

- Layout notes: the listing scrolls inside its own region and the New stream action stays fixed beneath it. A row's actions sit on the row, centered in the dropdown's width, and are reachable without a hover. The busy line replaces the actions rather than sitting beside them, so a busy stream offers nothing that would be refused. A row whose merge call runs disables its actions and carries `Merging <stream>` on its own line. A row with a merge run keeps **Open in Runs…** beside the run's status in words; a long path truncates from the end rather than wrapping the row. The paths a row names are cut to the first few with the rest counted (WSS-FR-JMWA), so no row outgrows the region it scrolls inside. Nothing scrolls horizontally at any width; a long branch name truncates from the middle and carries its whole value in accessible semantics. A row that is up to date with its base keeps **Update stream** in place and disabled rather than removing it, so the actions of two rows stay in one column. The stream update window states the source revision in short form beside the branch name and carries the whole revision in accessible semantics; its commit list scrolls inside its own region, and the two strategies stand one above the other so neither reads as the default.

## UI contract boundary

**Owned by the UI**: the resting label, the dropdown's grouping and scrolling, the New stream dialog and its validation, the merge, update and delete confirmations, the discard warning of a delete, and their wording, the stream update window's strategy choice and its display of the pinned source revision, the rendering of a merge call in progress, of a clean merge's result, of a merge run's status and of an update in progress and of what it settled, the update resolution window's placement, measure and treatment, the **Open in Runs…** control, which actions are enabled, and every inline refusal. The surface starts a merge and starts and cancels an update; it performs none of it, and it reads no repository of its own. It acts on no merge run.

**Delegated to backend (abstract)**:
- `"create work stream (name, base branch)"` — owned by `../core/WKS-work-streams.md`.
- `"list work streams"` — owned by `../core/WKS-work-streams.md`.
- `get_graduation_run` — owned by `../core/GRD-graduation.md` (GRD-FR-LGDV). The surface reads it once for each busy stream, for the name of the draft the run holding the stream works on (`input.draftName`). It reads nothing else of the run, and it changes nothing.
- `"get work stream (id)"` — owned by `../core/WKS-work-streams.md`.
- `"merge work stream (id, publication)"` — owned by `../core/WKS-work-streams.md`. It returns a result of one kind of three: `nothing_to_merge`, `merged` with `mergedPaths` and an optional `commit`, or `conflicted` with `runId` and `conflictedPaths`.
- `"update work stream (id, strategy, base revision)"` — owned by `../core/WKS-work-streams.md`.
- `"get work stream update (id)"` — owned by `../core/WKS-work-streams.md`.
- `"answer work stream update escalation (id, answers)"` — owned by `../core/WKS-work-streams.md`.
- `"retry work stream update (id)"` — owned by `../core/WKS-work-streams.md`.
- `"clear work stream update (id)"` — owned by `../core/WKS-work-streams.md`.
- `"cancel work stream update (id)"` — owned by `../core/WKS-work-streams.md`.
- `"delete work stream (id, force, discard uncommitted)"` — owned by `../core/WKS-work-streams.md`.
- `"get work stream uncommitted paths (id)"` — owned by `../core/WKS-work-streams.md`.
- `"activate worktree (path)"` — owned by `../core/WTC-worktree-context.md`.
- `"list branches"` — owned by `../core/GTC-git.md`.
- the event `"work stream update progress"` — owned by `../core/WKS-work-streams.md`.
- the event `"work streams changed"` — owned by `../core/WKS-work-streams.md`.

## Non-functional requirements
- The listing is one read, and a `"work streams changed"` event reloads it rather than the surface polling.
- The merge call and an update each run for as long as they take. Nothing in this surface blocks on one: the dropdown opens, closes and reopens while either runs, and every other stream stays readable.
- A merge run's status and an update that stopped to ask the author are still on the row after a window reload and after a relaunch, because the row renders the listing's `mergeRun` and the stream's update record.
- The stream update window opens from the listing the surface already holds and reads nothing further, so it appears without waiting on the backend.
- The dropdown renders a project with dozens of streams without blocking the chrome.
