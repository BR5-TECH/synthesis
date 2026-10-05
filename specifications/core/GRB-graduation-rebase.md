# Graduation rebase

**Spec code:** `GRB`

## Intent
Reconciling a work stream with the branch it was created from, in both directions. Runs on one stream stack on each other and never diverge, so a stream never diverges from itself. Its base branch, however, moves outside the application, so the author brings the stream up to that branch when it has moved — an **update** — and merges the stream back into it when the work is complete — a **merge**. This module performs both. **Git settles what Git can settle.** A merge Git settles completes at once and leaves no record. A merge Git cannot settle is handed to a **merge run**, a Graduation Run that resolves the conflicts and reviews the whole result in repeated passes, and its result reaches the base branch only after a `ready` review. An update path Git cannot merge reaches an agent as one semantic turn over the conflicts alone. An update is slow work that runs on its own, so it reports what it is doing while it runs, stops when the author asks, and records what it settled where they can act on it. A turn that cannot choose between two contradictory requirements asks the author, and their answer reaches the turn that follows. Out of scope: the stream, which is `WKS-work-streams.md`'s; the run and its controls, which are `GRD-graduation.md`'s; the work and review passes of a merge run, which are `../ai/GRL-graduation-loop.md`'s; the surfaces that show and control this work, which are `../ui/WSS-work-stream-selector.md`'s and `../ui/RUN-runs.md`'s; and the turn's container, which is `../tools/EAC-execute-agent-cli.md`'s.

## Functional requirements
1. **GRB-FR-CYWM** The operations here are an internal Rust API taking typed requests and returning typed results or typed failures. None is registered as a `#[tauri::command]`, none is reachable from `src/**`, and none is a `rig` portable tool, so no agent can merge a stream. The one Tauri command that starts a merge is `merge_work_stream` of `WKS-work-streams.md` WKS-FR-GKPX, which calls these operations.
2. **GRB-FR-PADR** A merge starts for **one stream, at the author's request** through `merge_work_stream`, and at no other moment. No run triggers one, no publication boundary exists, and nothing polls for a merge that has become possible. Only a merge run the author's request created can apply a conflicted merge.
3. **GRB-FR-HRTB** The **pre-conflict check** computes Git's ordinary merge of the stream branch into the base branch in memory, under the repository update guard, and writes nothing until it settles. It is in no queue, holds no queue slot and no project slot, and leaves no durable record.
4. **GRB-FR-QIHE** Before anything is compared, the operation verifies that the stream's working copy holds no uncommitted path and that the base branch's worktree is clean. Application-owned storage is read on the terms of `WKS-work-streams.md` WKS-FR-JVLM. A dirty side returns the typed `stream_dirty` or `base_dirty` carrying the complete path set, and a base branch no worktree holds returns `base_not_checked_out`. Nothing is written.
5. **GRB-FR-YDTX** The merge base is the revision the two branches last shared. The three sides of every path are its content at that revision, its content on the base branch, and its content on the stream branch.
6. **GRB-FR-EPYG** **A merge Git settles is settled.** A path that merges cleanly is written and **no question is raised about it**, whatever the path is and whichever side changed it. Nothing classifies a path, and no path is treated as carrying intent a text merge cannot see.
   - *Why:* Asking an agent to re-decide a clean merge is what made reconciliation cost a container and minutes for work Git had already finished correctly.
7. **GRB-FR-SRVN** A path Git **cannot** merge is an unresolved path. An update's semantic turn is asked about unresolved paths alone, each with `semantic_reason = "unresolved_conflict"`. A merge hands them to its merge run as `unresolvedPaths`. A deletion on one side of a path the other side changed is one of those.
8. **GRB-FR-FLIB** Where every path merged cleanly, the check applies the result on the request's publication and returns `merged`, and **no run and no agent turn exists**. Where the stream holds nothing the base does not, the check returns `nothing_to_merge` and writes nothing.
9. **GRB-FR-NNLS** Where at least one path conflicts, the merge is handed to **one merge run** for the whole attempt (GRB-FR-FPYA) and not to a semantic turn. No turn is dispatched for a single path, and the run's work turns see every unresolved path together.
10. **GRB-FR-UHFE** The check and the handoff are one unit. A failure at any point leaves the base branch, the stream branch and both working copies byte-identical to what they were, index entries included, and creates no run, no snapshot ref and no merge worktree.
11. **GRB-FR-MSNP** Where a path conflicts, the check runs the image preflight before it builds anything, with the same four refusals as `start_graduation` (per `GSU-graduation-start.md` GSU-FR-SQRC). A refusal returns the typed refusal, writes nothing and creates no run.
12. **GRB-FR-DZQE** At handoff the application builds the **merge snapshot**, a Git commit object. Its tree is the Git merge of the two pinned tips with diff3 conflict markers written into every path Git could not merge. Its parents are the pinned base tip and the pinned stream tip. Neither branch moves when it is built.
13. **GRB-FR-RMQC** The application keeps the snapshot reachable under the private ref `refs/synthesis/merge/<run-id>` and removes that ref when the run ends. Until a merge is applied, no branch, no live worktree and no ref other than the private ref and the merge worktree's scratch branch is written.
14. **GRB-FR-FPYA** The handoff creates the **merge run** in the state `queued` and returns `conflicted` with the run id and the conflicted paths. The run records the merge data of `GRD-graduation.md`: stream and base branch, pinned tips, merge base, snapshot commit, publication, changed paths, unresolved paths, conflicts. The guard is then released and the queue is offered a dispatch.
15. **GRB-FR-BXEJ** The **pinned tips** are the base tip and the stream tip at handoff. Before each dispatch the application verifies that both equal the branch tips. A moved tip ends the run `failed` with the typed `merge_branch_moved` and writes no branch and no worktree.
16. **GRB-FR-VNKX** Continue and the answer to an escalation of a merge run verify both pinned tips first. A moved tip refuses the request with `merge_branch_moved`; nothing changes, and no branch and no worktree is written.
17. **GRB-FR-KOGU** The **merge worktree** is a linked worktree the run owns, at `short_data_dir()/g/<run-id>/mw/`, registered under the name `<run-id>-mw`, on the scratch branch `synthesis/merge-run/<run-id>`, seeded from the merge snapshot. Work turns run there. Neither live branch and neither live worktree is written before approval.
18. **GRB-FR-LTVI** A `merge_work` turn runs in the merge worktree with `merge-work.md`. It resolves the unresolved paths and may change any other path needed to reconcile the merge. The repository is mounted read-only as for every work turn (per `../tools/EAC-execute-agent-cli.md` EAC-FR-FNFV), and no `/rebase` mount is supplied.
19. **GRB-FR-SRNW** A `merge_review` turn runs in a fresh session in a throwaway review checkout derived from the merge worktree, with the snapshot commit as base commit (per `../ai/GRL-graduation-loop.md` GRL-FR-YKRI), and with `merge-review.md`. It is read-only except build and test output.
20. **GRB-FR-YCMH** A `merge_review` checks the final result against both pinned branch versions and the repository specifications, for every path the merge changed, Git-clean paths included, and every path changed in reconciliation. It runs the project-defined build and test commands and invents none. It answers `ready` only when those checks and commands pass.
21. **GRB-FR-QPLA** One **pass** is one `merge_work` turn then one fresh `merge_review` turn. A `revise` verdict carries findings that go whole into the next `merge_work` turn. The `ReviewVerdict` rules hold (per `../ai/GRL-graduation-loop.md` GRL-FR-VIAT). The advisory-minor rule of GRL-FR-UQNV does not apply to a merge review.
22. **GRB-FR-TGAZ** A merge run has its own **pass budget of two passes**, a constant of this module and no project setting. When the budget is spent and the review still says `revise`, the run rests `awaiting_author` with the reason `pass_budget_exhausted` and releases the stream and the project slot.
23. **GRB-FR-JQFO** Continue after `pass_budget_exhausted` adds two passes, keeps the merge worktree and the snapshot, and puts the run at the front of the stream's queue. A merge turn that escalates pauses the run in `awaiting_author` on the terms of GRB-FR-LADU, and the answers reach the phase that asked.
24. **GRB-FR-CWLD** The **apply step** runs only after a `ready` review. It takes the repository update guard and then performs these steps in this order: it verifies both pinned tips, derives the result tree from the merge worktree, checks that no conflict marker stands in an unresolved path, verifies that both sides are clean, and applies the tree on the recorded publication.
25. **GRB-FR-ZEPB** A successful apply makes the run `completed` and records the merge result: `published` as `uncommitted` or `commit`, the merge commit id where committed, and the merged paths. It writes one `commit` record to the run log. Moved tips at apply end the run `failed` with `merge_branch_moved` and write nothing.
26. **GRB-FR-HUTP** An apply that finds a side dirty, the guard held, or fails rests the run `blocked` with the code `merge_dirty_side`, `merge_guard_held` or `merge_apply_failed`. The run resumes at the phase `apply` and spends no new turn. Continue retries the apply.
27. **GRB-FR-NSAY** Conflict markers left in an unresolved path make the apply step handle the result as a `revise` verdict with one `critical` finding. Discard, pause, failure and a non-ready review leave both branches and both worktrees unchanged.
28. **GRB-FR-ASWC** The result of a merge reaches the base branch on the publication the request names, for a clean merge and for a ready merge run alike: `{ kind: "uncommitted" }` leaves the result in the base worktree unstaged, and `{ kind: "commit", message }` creates one commit holding it. There is no default and no implicit commit.
29. **GRB-FR-WDHU** When a merge run ends, the application removes its merge worktree, its registration, its scratch branch and its snapshot ref. A run that rests in `awaiting_author` or `interrupted` keeps them. The first queue read after launch removes them for every run already ended.
30. **GRB-FR-YPEX** The application generates **one rebase artifact** for an update attempt that needs a semantic turn, under the run store at `short_data_dir()/m/<attempt-id>/`. It holds three mirrors — `base/`, `stream/`, `merged/` — for the **conflicting paths alone**.
31. **GRB-FR-AAVK** Beside those mirrors the artifact holds one **index document** naming every other path the update wrote and which side changed it, so the turn knows what moved around it without three renderings of each.
32. **GRB-FR-FMCU** Each artifact file is cut to one mebibyte. A path whose content passes that bound is truncated with the truncation stated in the file, and the turn is told the file is partial.
33. **GRB-FR-TOOU** The artifact is mounted by the application and never chosen by the model: it is supplied as `../tools/EAC-execute-agent-cli.md`'s typed semantic-rebase mount, read-only, at the fixed container root `/rebase`, for that one turn.
34. **GRB-FR-VZHV** The semantic turn reaches **no Git capability of any kind**, and the guarantee is enforced rather than argued from: every repository-metadata path inside its execution directory is masked, and the executor refuses to launch the turn otherwise (per `../tools/EAC-execute-agent-cli.md` EAC-FR-41).
35. **GRB-FR-LADU** A semantic turn that cannot choose between contradictory requirements escalates, carrying its reason and its questions to the author on exactly the terms an execution turn escalates (per `GXD-graduation-execution.md` GXD-FR-HGSU).
36. **GRB-FR-YRHM** An update spends at most **three semantic turns** for one request. A request whose turns ran and still settled nothing answers `conflicted`, naming the paths that are still unsettled. A request where no turn ran at all rests with the typed `update_attempts_exhausted`.
    - *Why:* A failure to launch a container and a conflict an agent could not resolve ask opposite things of the author, so they are answered apart.
37. **GRB-FR-PBJX** The author's own retry of an update grants three more semantic turns, and nothing else does. A retry re-plans the update from the pinned revision and from what the stream holds at that moment.
38. **GRB-FR-ZQNW** An update that escalates records the reason and the questions on the stream's update record (per `WKS-work-streams.md` WKS-FR-ZKUP), as an escalation with `origin = "semantic_merge"`, on the terms `GXD-graduation-execution.md` GXD-FR-HGSU sets. It carries no resumable session, an update turn being re-planned rather than resumed.
39. **GRB-FR-KMXT** The author's answers reach the next update's semantic turns as structured input alone, each carrying the question asked and the answer given. None reaches the turn in an argument, an environment variable, or a composed instruction.
40. **GRB-FR-WCHL** An answered escalation re-plans the update from the pinned revision and from what the stream holds at that moment, under a new attempt and a new artifact. The answers stand for every turn of that update.
41. **GRB-FR-JIRD** **One merge check, one apply step or one update runs in a repository at a time**, under the repository update guard (per `WKS-work-streams.md` WKS-FR-HLGN). A check or an update request made while the guard is held is refused with `merge_in_progress` or `update_in_progress` and changes nothing. The hold carries the update's cancellation (GRB-FR-MWTC, GRB-FR-TXVL).
    - *Why:* A merge writes the base branch and an update reads a pinned revision of it, so two of them at once in one repository would write one worktree from two directions.
42. **GRB-FR-MWTC** An update is **cancellable at any point**, including while a semantic turn is executing. A cancelled update answers `update_cancelled`, leaves both branches and both working copies byte-identical to what they were (GRB-FR-TXVL), and reclaims its attempt exactly as an exhausted one does. The cancellation reaches the turn's container through the executor rather than by abandoning the turn.
43. **GRB-FR-XPLV** An update reports its progress as it advances: which semantic turn of the bound it has begun, the attempt that turn belongs to, and the paths that turn is reconciling. The report reaches the author through `WKS-work-streams.md`'s `"work stream update progress"` (WKS-FR-FQLS) and, as one determinate in-flight operation, through `PRG-progress-reporting.md` (PRG-FR-11).
44. **GRB-FR-RNGX** The pre-conflict check reports as one in-flight operation `Merging <stream name>` through `PRG-progress-reporting.md` (PRG-FR-11), from its start until it settles. A merge run reports through the run's own progress, never through this operation.
45. **GRB-FR-KZUN** Every semantic turn of an update carries an activity sink bound to its **attempt**, so what the turn does reaches the surface watching it (per `AGV-agent-activity.md` AGV-FR-03 and `../ui/RUN-runs.md` RUN-FR-ZQWA). An update belongs to a stream and to no run, so the attempt is what its activity is keyed by.
46. **GRB-FR-OHWT** Every update attempt, every check, every handoff, dispatch, tip-check refusal, apply and cleanup of a merge emits through `LGC-logging.md`'s internal API under the `backend` domain, correlated by the stream id and the attempt id or run id, so one operation reads back as one sequence. The records a merge run writes at its dispatch, tip check, apply and cleanup carry the domains of `../ai/GLG-graduation-loop-logging.md` GLG-FR-QNLC. No record carries a file's content.
47. **GRB-FR-BQNF** An **update** reconciles in the opposite direction to a merge: it brings one stream branch up to a pinned revision of its base branch, at the author's request and at no other moment, under the strategy the request names. It writes the stream branch and the stream working copy alone.
48. **GRB-FR-DYUA** An update stands on the pinned revision the request carries and resolves the base branch no second time (per `WKS-work-streams.md` WKS-FR-PWZC). Every comparison, merge, replay and semantic turn of that update reads that one revision.
49. **GRB-FR-HJZC** `merge_source` merges the pinned base revision into the stream branch and creates the resulting merge commit. An update takes no publication: it commits on the stream branch, and it applies nothing to the base worktree.
50. **GRB-FR-RMKD** `rebase_source` replays the stream commits onto the pinned base revision and rewrites them. The stream branch then holds that revision as an ancestor and carries the replayed commits alone.
51. **GRB-FR-WGPS** A `rebase_source` update completes the whole replay comparison first, gathers the unresolved paths of every conflicting step into one set, and dispatches **one** semantic turn over that set. It dispatches no turn for a single replayed commit.
    - *Why:* A turn for each replayed commit would spend a container and minutes for each commit, and would ask the author one question several times over.
52. **GRB-FR-NFEB** Where the stream branch already holds the pinned revision, the outcome is `nothing_to_update` and nothing is written. Where every path reconciles cleanly, the outcome is `updated` and no agent turn is dispatched.
53. **GRB-FR-TXVL** An update is one unit. A cancellation, a conflict, an escalation, a stale-revision refusal and a failure each leave both branches and both working copies byte-identical to what they were, index entries included.
54. **GRB-FR-CLRO** An update takes the clean checks of GRB-FR-QIHE, the three-side comparison, and the settle-what-Git-settles rule of a merge, unchanged. Its artifact, three-turn bound, escalation and cancellation stand on GRB-FR-YPEX to GRB-FR-MWTC. It reports its progress through `WKS-work-streams.md`'s `\"work stream update progress\"` (WKS-FR-FQLS).
55. **GRB-FR-ITCJ** Every behaviour here except the semantic turn of an update and the turns of a merge run is exercisable on a machine with no container runtime, no agentic CLI, no credential and no network: the clean checks, the three-side comparison, the deterministic merge, the snapshot build, the artifact generation, the apply step and every refusal reach none of them.

## Contract surface

### Internal (Rust API, not registered as Tauri commands)
```text
check_merge(stream_id, publication)               → MergeCheckOutcome | MergeFailure
verify_merge_tips(run_id)                         → () | MergeBranchMoved
apply_merge(run_id)                               → MergeApplyOutcome | MergeApplyFailure
release_merge_run(run_id)                         → ()

release_rebase_artifact(attempt_id)               → ()
run_update(stream_id, strategy, base_revision, decisions, dispatch)
                                                  → UpdateOutcome | UpdateFailure
spawn_update(stream_id, strategy, base_revision, decisions)
                                                  → StreamUpdateRecord | UpdateFailure
cancel_update(stream_id)                          → bool
```

```
MergeCheckRequest {
  stream_id,
  stream_branch, stream_worktree_path,
  base_branch,  base_worktree_path,
  publication                  // StreamMergePublication, per WKS-work-streams.md
}

MergeCheckOutcome =
    { kind: "nothing_to_merge" }
  | { kind: "merged", merged_paths, commit? }
  | { kind: "conflicted", run_id, conflicted_paths }

MergeFailure =
    StreamDirty { paths }        | BaseDirty { paths }
  | BaseNotCheckedOut            | StreamMissing
  | MergeInProgress              | UpdateInProgress
  | ImagePreflightRefused { refusal }
  | SnapshotBuildFailed { reason }
  | MergeApplicationFailed { paths }

MergeSnapshot {                  // GRB-FR-DZQE, GRB-FR-RMQC
  commit,                        // the snapshot commit id
  ref_name,                      // refs/synthesis/merge/<run-id>
  base_tip, stream_tip,          // the two parents
  unresolved_paths               // paths carrying diff3 markers
}

MergeApplyOutcome =
    { kind: "applied", published, commit?, merged_paths }

MergeApplyFailure =
    MergeBranchMoved { branch, expected, actual }
  | MergeDirtySide { paths }     // run rests blocked, merge_dirty_side
  | MergeGuardHeld               // run rests blocked, merge_guard_held
  | MergeApplyFailed { paths }   // run rests blocked, merge_apply_failed
  | MarkersRemain { paths }      // handled as a revise verdict (GRB-FR-NSAY)

UpdateRequest {
  stream_id,
  stream_branch, stream_worktree_path,
  base_branch,  base_worktree_path,
  base_revision,               // pinned by the author's confirmation; the single
                               //   source every step reads (GRB-FR-DYUA)
  strategy,                    // "merge_source" | "rebase_source"
  decisions                    // [StreamMergeDecision] the author has already given
}

UpdateOutcome =
    { kind: "nothing_to_update" }
  | { kind: "updated", attempt_id, updated_paths, semantic_turns }
  | { kind: "conflicted", attempt_id, conflicted_paths }
  | { kind: "escalated", attempt_id, reason, questions }

UpdateFailure =
    StreamDirty { paths }        | BaseDirty { paths }
  | BaseNotCheckedOut
  | StaleBaseRevision { base_branch, expected, actual }
  | UpdateAttemptsExhausted     | UpdateInProgress | MergeInProgress
  | UpdateCancelled
  | ArtifactGenerationFailed { reason }
  | UpdateApplicationFailed { paths }

MergeConflict {                  // what one update semantic turn is asked about
  path,                          // project-relative
  base_change,                   // "created" | "updated" | "deleted" | "unchanged"
  stream_change,                 // the same four
  merged_file                    // container path of the conflicted merge, under /rebase/merged/
}
```

The merge run's own record, `GraduationMergeData`, and the merge turn input,
`MergeTaskContext`, are owned by `GRD-graduation.md` and
`GXD-graduation-execution.md`. The pass budget of two is a constant of this
module (GRB-FR-TGAZ).

`merged_file` names a path inside the attempt's mount, so it does not outlive the
attempt. The update record keeps `StreamMergeConflict` instead, which holds the
path and the two side changes and no container path (per `WKS-work-streams.md`
WKS-FR-ZKUP).

Typed errors surfaced to the author by `WKS-work-streams.md`, which holds the whole list: `unsupported_conflict`, `stream_dirty`, `base_dirty`, `draft_save_failed`, `base_not_checked_out`, `merge_in_progress`, `merge_branch_moved`, `stale_base_revision`, `update_attempts_exhausted`, `update_in_progress`, `update_cancelled`.

Block codes of a merge run at the apply step: `merge_dirty_side`, `merge_guard_held`, `merge_apply_failed` (GRB-FR-HUTP). Rest reason after the pass budget: `pass_budget_exhausted` (GRB-FR-TGAZ).

## Non-functional requirements
- The comparison reads path names and object ids rather than file contents, and reads a blob only for a path both sides changed.
- A clean merge of a stream holding many commits costs no container and no model call, and leaves no run.
- The pre-conflict check holds the repository update guard for the time of one in-memory merge and one application of its result. Nothing waits on it to answer a read of the project's streams.
- The artifact of an update is removed when its attempt completes, whatever the attempt decided, a cancelled attempt included. An escalated update keeps its questions on the update record rather than in the artifact.
- A `rebase_source` update costs one semantic turn at most for the whole replay, whatever number of stream commits it rewrites.
- A merge run costs two passes at most before it asks the author. An update takes as long as its turns take, and one turn is minutes. A cancellation is answered without waiting for a container to exit.
- The merge snapshot and the merge worktree are the only copies of a merge run's work. They live in application storage and are never inside the author's worktrees.
