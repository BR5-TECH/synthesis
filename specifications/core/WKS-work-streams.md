# Work streams

**Spec code:** `WKS`

## Intent
A work stream is a named branch and working copy that the application owns and that lives longer than one graduation run. Runs execute in the stream's working copy and commit onto its branch, so each run starts from the commit the run before it made. This is what removes reconciliation from between runs: the runs of one stream share a base and cannot diverge from each other. The author reads a stream through the ordinary change surfaces, updates it from its base branch while the work continues, merges it into that branch when the work is complete, and removes it. A base branch moves outside the application, so a stream is brought up to it on the author's request, in the direction opposite to a merge. A merge is an operation the author starts: Git merges first, and a clean merge completes with no run and no record. A merge Git cannot resolve becomes a **merge run**, a Graduation Run named `Merge <stream name>` that is the one durable record of that merge. An update runs for minutes and may stop to ask the author a question, so each stream holds a durable record of what its last update did, and the author drives it to rest. A direct run that targets a stream's working copy (per `GRD-graduation.md` GRD-FR-ZVNO) is a run of that stream for its queue, its busy state and its deletion. Out of scope: what a run does, which is `GRD-graduation.md`'s; the reconciliation a merge and an update perform, which is `GRB-graduation-rebase.md`'s; and the author's own worktrees, which are `WTC-worktree-context.md`'s.

## Functional requirements
1. **WKS-FR-QMTV** A work stream holds a stable id, an author-given name, the branch it owns, the absolute path of its working copy, the branch it was created from, and the revision of that branch at creation.
2. **WKS-FR-ZBHL** A stream exists only in a project whose content root is inside a Git repository, resolved as `GTC-git.md` GTC-FR-02 resolves it. A project that is not returns the typed `not_a_git_repository` from every operation here.
3. **WKS-FR-KDXF** `create_work_stream(name, base_branch)` creates the branch `synthesis/stream/<slug>` from `base_branch`, creates a linked worktree on that branch, and records the stream. `slug` is the name reduced to path-safe characters and cut to 48 characters.
4. **WKS-FR-PWNR** A `base_branch` the request does not name defaults to the branch checked out in the project's active worktree. A detached active worktree with no `base_branch` returns the typed `base_branch_required`.
5. **WKS-FR-JGCA** The working copy stands at `short_data_dir()/w/<stream-id>/`, under `FSA-filesystem-access.md`'s short root and nowhere else. An execution turn stands in that path and the container names the same path the host names (per `../tools/EAC-execute-agent-cli.md` EAC-FR-ZKMR).
6. **WKS-FR-VTEY** Creation is one all-or-nothing operation. A failure removes the branch and the working copy it had made and returns the typed `stream_creation_failed`; a cleanup that itself fails returns `stream_cleanup_failed` naming the branch and the path it could not reclaim.
7. **WKS-FR-BSLO** A stream records the store it shares as a path relative to its own Git directory, on exactly the terms `WTC-worktree-context.md` WTC-FR-QVNL sets, because an agent turn reads the repository through a container that mounts that store at a path of the executor's choosing.
8. **WKS-FR-HRUZ** A name whose **slug** no other live stream of the project holds is accepted. Two names that reduce to one slug would own one branch, so the second returns the typed `stream_name_taken`; a name that reduces to an empty slug returns `stream_name_invalid`.
9. **WKS-FR-SGCM** `list_work_streams()` returns every live stream of the project with its record, how far ahead of its base it stands, how far behind it stands, its queue depth, its merge run link (`mergeRun`), and its update record. `get_work_stream(stream_id)` returns one, or the typed `unknown_stream`.
10. **WKS-FR-YBST** A stream's **queue depth** counts its stream runs and the direct runs that target its working copy. It is read from the project's run order index, which names each run's stream and state, and never by opening run records. A depth the index cannot answer is reported as zero rather than by reading them.
11. **WKS-FR-MFDW** A stream is **visible** to the author: it appears in the worktree enumeration of `WTC-worktree-context.md` WTC-FR-04 and its branch appears in the branch enumerations of `GTC-git.md` GTC-FR-07, each flagged as a stream.
    - *Why:* A checkout the author can open, read and diff is the whole way they judge a stream before they merge it.
12. **WKS-FR-CYAG** A stream is **busy** while a run holds it, a stream run or a direct run that targets its working copy. `WorktreeEntry` carries that state (per `WTC-worktree-context.md` WTC-FR-QKZD), and `activate_worktree` and `check_out_branch_in_active_worktree` refuse a busy stream with the typed `stream_busy`.
13. **WKS-FR-JQJA** The busy state is **durable**. The stream record holds the id of the run holding it, written before that run's first turn starts and cleared the moment the run stops holding the stream on the terms `GRD-graduation.md` GRD-FR-BNTC sets.
    - *Why:* A run that pauses, is interrupted, or is discarded stops holding its stream at a state that is not terminal, and a mark that waited for a terminal state would strand the stream.
14. **WKS-FR-RQVM** A run's claim of a stream is refused with `stream_busy` while an update of that stream runs, or while the merge check of that stream runs, from its request until its outcome is written. The run stays queued, and the reconciliation alone writes the stream's branch and working copy (per `GRD-graduation.md` GRD-FR-BNTC).
15. **WKS-FR-LWEI** A stream the application finds busy at launch, with no loop behind the run it names, has that run marked abandoned on the terms `GRD-graduation.md` GRD-FR-XVUD sets, and the stream is released.
15. **WKS-FR-NDSV** Each stream holds **one queue** of its stream runs and of the direct runs that target its working copy, in one order, and one run of a stream works at a time. Queues work at the same time up to the one project-wide graduation concurrency limit, which no stream has of its own (per `PSS-project-settings-storage.md` PSS-FR-JRWC, `GRD-graduation.md` GRD-FR-KKKN).
16. **WKS-FR-GKPX** `merge_work_stream(stream_id, publication)` is an author-started async operation that merges the stream branch into the branch it was created from by Git's ordinary merge, under the repository update guard (WKS-FR-HLGN). It is refused with `stream_busy` while the stream holds a non-terminal run, a merge run included, and writes nothing when it refuses.
17. **WKS-FR-QNHF** `merge_work_stream` returns a `StreamMergeResult` once the check settles: `nothing_to_merge`, `merged` or `conflicted`. It does not wait for the turns of a merge run. The check holds no queue slot and no project slot, and it is in no queue.
18. **WKS-FR-PZKD** A merge that returns `nothing_to_merge` or `merged` creates no run and leaves no durable record: no run record, no run log, no statistics line. A stream holds no merge record. The merge run is the one durable record of a conflict merge.
19. **WKS-FR-TVBM** `merge_work_stream` returns `merged` after it applies a clean result on the publication the request names: `uncommitted` leaves the result unstaged in the base worktree, and `commit` makes one merge commit with the author's message. The result lists the merged paths and the commit id where committed.
20. **WKS-FR-FOTC** Where the stream holds nothing the base branch does not, `merge_work_stream` returns `nothing_to_merge` and writes nothing: no commit, no worktree change, no run.
21. **WKS-FR-MWYD** From the start of the check until it settles, the merge is one in-flight operation `Merging <stream name>`, reported through `PRG-progress-reporting.md` (PRG-FR-11). The operation ends on every outcome, a refusal included. No other progress event exists for a merge.
22. **WKS-FR-BPGM** Where a path conflicts, `merge_work_stream` runs the image preflight (per `GSU-graduation-start.md` GSU-FR-SQRC) before it builds anything. A refusal returns the typed refusal, writes nothing and creates no run.
23. **WKS-FR-ZLWT** Where a path conflicts and the preflight passes, `merge_work_stream` builds the merge snapshot, creates a merge run in the state `queued`, and returns `{ kind: "conflicted", runId, conflictedPaths }` (per `GRB-graduation-rebase.md` GRB-FR-FPYA). Both branches and both working copies are unchanged. The guard is then released and the queue is offered a dispatch.
24. **WKS-FR-UCMR** A conflict that a text turn cannot reconcile is refused at the check with the typed `unsupported_conflict`: a conflict that involves a symbolic link or a submodule, and a path that is a file on one side and a directory on the other. The refusal builds no snapshot and no run, and writes nothing to either branch or either working copy. The author settles such a conflict with Git.
24. **WKS-FR-VQDE** The merge run is a Graduation Run that carries merge data, named `Merge <stream name>`, assigned to the stream, with no draft. It is the **only durable record** of a conflict merge. It is a stream run for the stream's queue, its busy state, the project-wide concurrency limit and its deletion.
24. **WKS-FR-DAKP** A merge run the application finds `working` or `reviewing` at launch, with no loop behind it, is marked `interrupted` with the reason `execution_abandoned` on the first queue read, as WKS-FR-LWEI releases a stream a stopped application left busy. Continue resumes it. Neither branch moved.
27. **WKS-FR-RAOM** A merge that reaches a conflict Git cannot settle, and a merge run that is discarded, paused, failed or not yet approved, leave the stream, both branches and both working copies exactly as they were. Nothing of the base branch is changed by a merge that did not complete.
28. **WKS-FR-KHJS** A stream summary carries `mergeRun` as `{ runId, name, state }`: the stream's newest merge run that is not `discarded` and not archived, with `name` as `Merge <stream name>` and `state` as the run state. It is absent where the stream has none. It is read from the project's run order index alone.
29. **WKS-FR-ENRU** The application restarts before handoff with nothing durable. A merge the author started and did not see settle leaves no run, and the author starts Merge again. A merge run found at launch after handoff is on disk, resumes on Continue (WKS-FR-DAKP), and has its merge worktree, registration, scratch branch and snapshot ref reclaimed once it has ended.
28. **WKS-FR-UZHT** A merge refuses either side that is not clean: the stream's own working copy with the typed `stream_dirty`, and the base branch's worktree with `base_dirty`, each carrying the complete path set (WKS-FR-JVLM). The author commits their own work under a message they wrote; this module commits, stashes, resets and cleans nothing on their behalf.
28. **WKS-FR-JVLM** An untracked path inside the application-owned storage of `PST-project-storage.md` PST-FR-XKVD makes no side dirty, and no path of it makes dirty a side the operation does not check out. On the side it checks out, a tracked path of that storage that differs from `HEAD` makes the side dirty. The operation commits none of it.
29. **WKS-FR-ETKW** A stream whose working copy is gone is refused for a merge with the typed `stream_missing`. A tree nothing can open reports no uncommitted path, so a check that read it would call a missing stream clean and merge from nothing.
30. **WKS-FR-NRQT** `update_work_stream(stream_id, strategy, base_revision)` brings the stream branch up to the branch it was created from, through the reconciliation of `GRB-graduation-rebase.md`. `strategy` is `merge_source` or `rebase_source`. It, its answers and its retry are refused with `stream_busy` while the project holds a non-terminal run of the stream, and with `stream_missing` where the working copy is gone.
31. **WKS-FR-XDBM** An update takes the stream's recorded `base_branch` as its source, whatever branch the project's active worktree holds. It writes the stream branch and the stream working copy alone, and nothing of the base branch is changed by it.
32. **WKS-FR-RJPD** A stream stands **behind** its base branch by the commits that branch holds and the stream branch does not. An update is offered for a stream that stands behind and for no other, and a stream that stands behind by nothing rests at `nothing_to_update`.
33. **WKS-FR-UBGX** A summary names at most the first fifty commits the stream is missing and reports the whole number in `behind_base`. A stream far behind its base is described by that count rather than by a listing no reader can take in.
34. **WKS-FR-KFVJ** Before any clean check and before any write, an update verifies that `base_branch` still points to the `base_revision` the request carries. A branch that has moved returns the typed `stale_base_revision`, writes nothing, and the author takes a new revision from a new listing.
    - *Why:* The revision the author read beside the commit list is what they judged the request by, so a branch that moved since then is a different request.
35. **WKS-FR-PWZC** The accepted `base_revision` is what every later step of that update reads: the comparison, the merge or the replay, and each semantic turn. The update resolves `base_branch` no second time, so movement of that branch after the check changes nothing the update does.
36. **WKS-FR-HLGN** The **repository update guard** is one exclusion for each repository. For an update it covers the pinned-revision check of WKS-FR-KFVJ and every write after it. For a merge it covers the check with the application of a clean result, and the apply step of a merge run with its tip verification. The base branch cannot advance in between.
37. **WKS-FR-TSOA** `merge_work_stream` and `update_work_stream` both take the repository update guard, so one of them runs in a repository at a time. A request made while the guard is held is refused with `merge_in_progress` or `update_in_progress`, whichever operation holds it, and changes nothing. The apply step of a merge run takes it too (per `GRB-graduation-rebase.md` GRB-FR-CWLD).
38. **WKS-FR-MJEB** `update_work_stream` starts the update and returns the stream's update record at once, in the `running` state. The update runs to rest whether or not a surface watches it, and what it settled is read from the record rather than from the call.
39. **WKS-FR-ZKUP** Each stream holds one **update record**, at `short_data_dir()/wu/<stream-id>.toml`. It holds what the most recent update of that stream did, and a new update of the stream replaces it.
40. **WKS-FR-AMWE** An update record holds the strategy the author chose, the recorded `base_branch`, the pinned `base_revision` the request carried, and the commits the stream was missing. It keeps that revision for as long as the record exists, and a retry and an answer each run on it.
41. **WKS-FR-VQRD** An update record stands in one of seven states: `running`, `updated`, `nothing_to_update`, `conflicted`, `escalated`, `cancelled` and `failed`. An `escalated` update waits for the author. A `conflicted`, `cancelled` or `failed` update rests and permits a new one. The other states are settled.
42. **WKS-FR-GYHF** `get_work_stream_update(stream_id)` returns the stream's update record, or nothing where no update of that stream has run. It returns the typed `unknown_stream` for a stream the project does not hold.
43. **WKS-FR-CBXW** `answer_work_stream_update_escalation(stream_id, answers)` takes the whole ordered set of answers. A set that does not cover each recorded position, or that holds a blank answer, records nothing and starts nothing. An accepted set is recorded, the escalation is cleared, and a new update starts with it.
44. **WKS-FR-DPNM** `retry_work_stream_update(stream_id)` starts the update again from `conflicted`, `cancelled` or `failed`, under the recorded strategy and the recorded `base_revision`. It is refused with `update_in_progress` while an update runs, and with `update_state_not_permitted` on an `escalated` update.
45. **WKS-FR-LRAV** `clear_work_stream_update(stream_id)` removes the update record and the escalation it holds. It changes neither branch and neither working copy. It is refused with `update_in_progress` while an update runs.
46. **WKS-FR-WEJK** `cancel_work_stream_update(stream_id)` stops the update of that stream. The update is recorded `cancelled`, having written nothing to either branch and having left both working copies as they were (per `GRB-graduation-rebase.md` GRB-FR-TXVL). A stream with no update running is answered without error.
47. **WKS-FR-QFTH** An update record the application finds `running` at launch, with no update behind it, is recorded `failed` with the typed `update_interrupted`, as WKS-FR-LWEI releases a stream a stopped application left busy. Neither branch moved.
48. **WKS-FR-OKVB** An update refuses either side that is not clean: the stream's own working copy with the typed `stream_dirty`, and the worktree of the recorded `base_branch` with `base_dirty`, each carrying the complete path set (WKS-FR-JVLM). A `base_branch` no worktree holds is refused with `base_not_checked_out`.
49. **WKS-FR-ZHTC** An update that is cancelled, that conflicts, that escalates, that refuses a stale revision, or that fails leaves both branches and both working copies exactly as they were, and records what stopped it. Nothing of the base branch is changed by an update at all.
50. **WKS-FR-FQLS** `"work stream update progress"` is emitted as an update advances, carrying `{ project_key, stream_id, attempt_id, strategy, turn, turns_max, reconciling_paths }`. It is emitted when each semantic turn begins and when the update settles. A reader renders from the event and asks nothing further.
51. **WKS-FR-EIBC** `delete_work_stream(stream_id, force, discard_uncommitted)` removes the working copy, the branch, the stream record and the update record. Without `force`, a stream holding commits its base branch does not hold is refused with the typed `stream_unmerged` naming the count.
52. **WKS-FR-OVLQ** Deletion is refused with `stream_busy` while a run holds the stream, with `stream_has_runs` where the project holds a non-terminal run assigned to it, a merge run and a direct run that targets its working copy included, and with `stream_active` while its working copy is the project's active worktree. **Neither `force` nor `discard_uncommitted` reaches any of these.**
    - *Why:* These refusals protect a working copy an agent is writing, a run still answerable for its work, and the tree the author stands in; a flag that removed them would delete all three from under their owner.
53. **WKS-FR-FJYN** Deleting a stream keeps every run record the stream carried. A run's record names the stream it ran in for as long as the record exists, whether or not the stream still does (per `GRD-graduation.md` GRD-FR-PZAK).
54. **WKS-FR-AXRD** A stream whose working copy directory is gone is reported rather than removed, flagged missing like any other worktree (per `WTC-worktree-context.md` WTC-FR-05), rather than failing every read of the project's streams. This module offers no re-creation of its own: the surface composes one from `delete_work_stream` with `force` and `create_work_stream` (per `../ui/WSS-work-stream-selector.md` WSS-FR-OQYG).
55. **WKS-FR-SMKU** Nothing of a stream is written into a project's `.synthesis/`, none of it is committed, and none of it is reset by a change of project or of active worktree.
56. **WKS-FR-DHOP** Every operation emits through `LGC-logging.md`'s internal API under the `backend` domain: an `INFO` record when it begins and when it ends, and a `WARN` or `ERROR` record on every failure path. No record carries a file's content.
57. **WKS-FR-IPCE** Every filesystem read and write goes through `FSA-filesystem-access.md` rather than a bare path call, on that module's own terms: an instance method takes an absolute path (FSA-FR-19), and a project-relative path is resolved against the stream's working copy through `resolve_under`, which refuses one that escapes it.
58. **WKS-FR-YBGS** Every behaviour here is exercisable on a machine with no container runtime, no agentic CLI, no provider credential and no network. Creation, enumeration, the busy state, the merge refusals and the deletion refusals reach none of them.
60. **WKS-FR-YSUB** A direct run that targets a stream's working copy claims the stream as a stream run does. It is refused the claim with `stream_busy` while an update or the merge check of that stream runs, and it stays queued. A merge and an update are refused with `stream_busy` while the stream holds such a run.
59. **WKS-FR-WULF** `"work streams changed"` is emitted whenever the set of streams, a stream's busy state, a stream's queue depth, a stream's update record, or the state of a stream's merge run changes. Payload: `{ project_key }`. A reader reloads its own listing rather than reading a stream set off the event.
61. **WKS-FR-DBXN** Without `discard_uncommitted`, deletion of a stream whose working copy holds uncommitted paths is refused with the typed `stream_dirty` carrying the complete path set (WKS-FR-JVLM), and nothing is removed. With `discard_uncommitted` true those paths are discarded with the working copy. A working copy that is missing reports no uncommitted path (WKS-FR-ETKW).
62. **WKS-FR-GQSU** `get_work_stream_uncommitted_paths(stream_id)` returns the complete uncommitted path set of the stream's working copy on the terms of WKS-FR-JVLM, and an empty set where the working copy is missing. An unknown stream returns `unknown_stream`. It writes nothing, and it is the read a deletion confirmation names the discarded paths from.
63. **WKS-FR-HLYM** A branch deletion by `GTC-git.md` `delete_branch` never removes a stream's branch (per GTC-FR-JOWX). `delete_work_stream` is the only route that removes one, so every refusal of WKS-FR-EIBC, WKS-FR-OVLQ and WKS-FR-DBXN binds the Git panel and the Streams overlay alike.
64. **WKS-FR-SPVK** A `create_work_stream` or `delete_work_stream` that returns success or `stream_cleanup_failed` emits `"branches changed"` once (per `WTC-worktree-context.md` WTC-FR-25). A refusal emits nothing.

## Contract surface

### Tauri commands
Names match `../ui/WSS-work-stream-selector.md` byte-for-byte:

- `"create work stream (name, base branch)"` → `create_work_stream(name, base_branch?)` → `WorkStream`. Side effects: a branch, a working copy, and a stream record, and one `"branches changed"` (WKS-FR-SPVK).
- `"list work streams"` → `list_work_streams()` → `[WorkStreamSummary]`. No side effect.
- `"get work stream (id)"` → `get_work_stream(stream_id)` → `WorkStream`. No side effect.
- `"merge work stream (id, publication)"` → `merge_work_stream(stream_id, publication)` → `StreamMergeResult`. Async. Side effects: the base branch advances where a clean merge lands; where Git cannot resolve a path, a merge snapshot ref, a merge worktree and a merge run in the state `queued` are created, and no branch moves.
- `"update work stream (id, strategy, base revision)"` → `update_work_stream(stream_id, strategy, base_revision)` → `StreamUpdateRecord`. Side effects: the update record is written `running`, and the update runs on its own. The stream branch and its working copy advance where the update lands; the base branch does not move.
- `"get work stream update (id)"` → `get_work_stream_update(stream_id)` → `StreamUpdateRecord?`. No side effect.
- `"answer work stream update escalation (id, answers)"` → `answer_work_stream_update_escalation(stream_id, answers)` → `StreamUpdateRecord`. Side effects: the answers are recorded, the escalation is cleared, and a new update starts.
- `"retry work stream update (id)"` → `retry_work_stream_update(stream_id)` → `StreamUpdateRecord`. Side effect: a new update starts under the recorded strategy and revision.
- `"clear work stream update (id)"` → `clear_work_stream_update(stream_id)` → `()`. Side effect: the update record is removed.
- `"cancel work stream update (id)"` → `cancel_work_stream_update(stream_id)` → `()`. Side effect: a running update stops and reclaims its attempt.
- `"delete work stream (id, force, discard uncommitted)"` → `delete_work_stream(stream_id, force, discard_uncommitted)` → `()`. Side effects: the working copy, the branch and the records are removed, and one `"branches changed"` is emitted (WKS-FR-SPVK).
- `"get work stream uncommitted paths (id)"` → `get_work_stream_uncommitted_paths(stream_id)` → `[path]`. No side effect.

### Events (Tauri event bus)
- `"work streams changed"` — payload `{ project_key }` (WKS-FR-WULF). Emitted when a merge run changes state too.
- `"work stream update progress"` — payload `{ project_key, stream_id, attempt_id, strategy, turn, turns_max, reconciling_paths }` (WKS-FR-FQLS).

### Payload shapes
```
WorkStream {
  id,                       // stable, unique across projects
  name,                     // the author's own
  branch,                   // synthesis/stream/<slug>
  worktree_path,            // absolute; short_data_dir()/w/<id>/
  base_branch,              // the branch it was created from and merges into
  base_revision,            // that branch's revision at creation
  created_at,               // RFC 3339 UTC
  busy_run_id,              // the run holding it, or null
  is_missing                // its directory is gone (WKS-FR-AXRD)
}

WorkStreamSummary {
  stream,                   // the WorkStream
  queued_run_count,         // runs assigned to it and not terminal, read from
                            //   the run order index alone (WKS-FR-YBST)
  ahead_of_base,            // commits the base branch does not hold
  behind_base,              // commits base_branch holds and the stream does not
                            //   (WKS-FR-RJPD)
  base_tip_revision,        // base_branch's tip when the listing was read; the
                            //   revision an update pins (WKS-FR-KFVJ)
  missing_commits,          // [StreamUpdateCommit], the first of the commits
                            //   counted by behind_base (WKS-FR-UBGX)
  mergeRun,                 // { runId, name, state } of the newest live merge run,
                            //   or null (WKS-FR-KHJS)
  update                    // the StreamUpdateRecord, or null (WKS-FR-SGCM)
}

StreamUpdateCommit {
  revision,                 // full object id
  summary,                  // the commit's first line
  author,                   // the commit author's name
  committed_at              // RFC 3339 UTC
}

StreamMergePublication =
  | { kind: "uncommitted" }              // apply into the base worktree, no commit
  | { kind: "commit", message }          // apply and commit

StreamMergeResult =                     // camelCase on the wire
  | { kind: "nothing_to_merge" }
  | { kind: "merged", mergedPaths, commit? }
  | { kind: "conflicted", runId, conflictedPaths }

StreamMergeConflict {                   // no content and no container path
  path,                                 // project-relative
  base_change,                          // created | updated | deleted | unchanged
  stream_change                         // the same, on the stream branch
}

StreamMergeDecision {
  position,                             // the question's recorded position
  question,                             // the question, whole
  answer,                               // what the author answered
  summary                               // the proposed response's summary, or empty
}

StreamUpdateStrategy =
  | "merge_source"                      // merge the pinned base revision into
                                        //   the stream branch, one merge commit
  | "rebase_source"                     // replay the stream commits onto the
                                        //   pinned base revision

StreamUpdateState =
  | "running" | "updated" | "nothing_to_update"
  | "conflicted" | "escalated" | "cancelled" | "failed"

StreamUpdateRecord {                    // WKS-FR-ZKUP, one per stream
  stream_id,
  project_key,
  state,                                // StreamUpdateState (WKS-FR-VQRD)
  strategy,                             // StreamUpdateStrategy the author chose
  attempt_id,                           // the attempt the state belongs to; empty
                                        //   before the first attempt
  base_branch,                          // the stream's recorded base branch
  base_revision,                        // the pinned revision, retained for the
                                        //   life of the record (WKS-FR-AMWE)
  missing_commits,                      // [StreamUpdateCommit] the stream lacked
  semantic_turns,                       // agent turns a settled update spent
  reason,                               // the turn's own words, where escalated
  failure,                              // the typed error, where failed
  requested_at,                         // RFC 3339 UTC
  updated_at,                           // RFC 3339 UTC
  escalation,                           // GraduationEscalation with
                                        //   origin = semantic_merge, or null
  conflicts,                            // [StreamMergeConflict], where conflicted
  updated_paths,                        // project-relative, where updated
  decisions                             // [StreamMergeDecision] the author gave
}
```

Typed errors: `unsupported_conflict`, `not_a_git_repository`, `base_branch_required`, `stream_name_taken`, `stream_name_invalid`, `stream_creation_failed`, `stream_cleanup_failed`, `unknown_stream`, `stream_busy`, `stream_dirty`, `base_dirty`, `stream_missing`, `base_not_checked_out`, `merge_in_progress`, `merge_branch_moved`, `vendor_image_unconfigured`, `vendor_image_invalid`, `vendor_execution_unsupported`, `docker_backend_unverified`, `stale_base_revision`, `update_attempts_exhausted`, `update_in_progress`, `update_cancelled`, `update_interrupted`, `update_state_not_permitted`, `stream_unmerged`, `stream_has_runs`, `stream_active`.

### Internal (Rust API, not registered as Tauri commands)
- Claiming and releasing a stream for a run, which is what the busy state and the per-stream queue are read through.
- Resolution of a stream from the working copy an execution turn stands in.

## Non-functional requirements
- Enumerating the project's streams reads the stream records and the project's run order index. It opens no individual run record (WKS-FR-YBST).
- A stream's working copy is a linked worktree and holds its own index, so two streams work at the same time without sharing an index lock.
- The pre-conflict check of a merge holds the repository update guard for one in-memory merge and one application of its result, which is seconds rather than minutes. The turns of a merge run hold no guard. An update runs for as long as its semantic turns take. No other operation of this module waits on one, and a cancellation is answered without waiting for the turn's container to exit.
- The update record and a merge run each survive a window reload and a relaunch of the application, so work that stopped to ask the author is answerable at the next launch.
- One merge check, one apply step or one update runs in a repository at a time (WKS-FR-TSOA). A stream whose base branch is not being written is read, listed and opened while another stream reconciles.
- The store root is short, and every segment of it is short, because the path is what an agent turn stands in and repeats on each command.
