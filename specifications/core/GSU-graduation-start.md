# Graduation start

**Spec code:** `GSU`

## Intent
What has to be true before a graduation run exists. `start_graduation` settles which work stream the run belongs to, takes an immutable copy of the prompt it answers, checks that the machine can actually execute an agent, and enqueues the run — all as one operation that either produces a run or produces nothing at all. `start_direct_graduation` does the same for a run that works directly in the worktree that is active when the author confirms: it pins that worktree and its branch, and it refuses a worktree that is detached or holds uncommitted work. Both refuse rather than repair: a missing vendor image is the author's to configure, and uncommitted work is the author's to commit. What a stream run does with work standing uncommitted in its stream, and under what message, is the author's choice, taken here and applied when the run's turn comes. The same checks stand between a restarted run and existence. Out of scope: everything the run then does, which is `GRD-graduation.md`'s; the stream itself, which is `WKS-work-streams.md`'s.

## Functional requirements
1. **GSU-FR-ELZO** `start_graduation(draft_id, stream_id, standing_work, standing_work_message)` captures the prompt, runs every check below, and enqueues the run as **one all-or-nothing operation**. A refusal leaves no run record, no queue entry, no draft lock and no dispatched turn.
2. **GSU-FR-IJEE** A draft that is not a single prompt file returns `draft_not_single_file` and enqueues nothing. A draft a non-terminal run already holds returns `draft_locked_by_graduation`.
3. **GSU-FR-TTAY** A GitHub-shadow draft (per `DRS-draft-storage.md` DRS-FR-XDWS) is admitted on the terms of every other draft. Its `github_shadow` status refuses no start, and a refused start leaves it `github_shadow` with nothing changed on GitHub.
3. **GSU-FR-JBKZ** A `stream_id` naming no live stream of the project returns `unknown_stream`. The run is assigned to that stream for its whole life (per `GRD-graduation.md` GRD-FR-PZAK).
4. **GSU-FR-GLVQ** The **image preflight** runs before a run exists and probes nothing. The project must commit an image for the resolved vendor and the machine's Docker backend must be verified, or the start is refused.
5. **GSU-FR-SQRC** The image preflight returns one of four typed refusals: `vendor_image_unconfigured`, `vendor_image_invalid`, `vendor_execution_unsupported`, or `docker_backend_unverified`. Each names what the author must configure.
6. **GSU-FR-MLEJ** The start records the author's `standing_work` choice on the run, one of `keep`, `commit` or `commit_and_push`, with the `standing_work_message` that commit takes (per `GRD-graduation.md` GRD-FR-RJFC). The run applies both when it is dispatched. The start reads no working copy, and work standing in the stream refuses no start.
7. **GSU-FR-RWYQ** The prompt is read through `DRS-draft-storage.md`'s `read_draft_prompt` and is captured whole, with the checksum of the captured bytes. It is captured once and never re-read for that run.
8. **GSU-FR-NCJN** A run is enqueued with `auto_start` enabled and at the latest end of the project's run order (per `GRD-graduation.md` GRD-FR-VLFO). Enqueuing does not dispatch: the stream's queue decides when the run starts.
8. **GSU-FR-RNOM** A start or a restart that enqueues a run asks for the graduation-start draft commit of the run's draft (per `PST-project-storage.md` PST-FR-DQZT). It asks after the enqueue, so a refused start commits nothing. The start does not wait for that commit and does not fail with it.
9. **GSU-FR-RJRF** The queue, every run record and every captured prompt live under `FSA-filesystem-access.md`'s `short_data_dir()` and nowhere else. Nothing of it is written into a project's `.synthesis/`, and none of it is committed.
10. **GSU-FR-IRAC** A restart takes these same checks afresh, at the moment it is taken, against the stream the restart names, and it records its own `standing_work` choice and message. It reads the discarded run's captured input and reads the draft's current prompt at no point.
11. **GSU-FR-DPSY** A failure after the capture but before the enqueue reclaims whatever it created before it reports, and returns the typed failure rather than a half-made run.
12. **GSU-FR-HIPF** Every check here is exercisable with no container runtime and no network: the image preflight reads configuration and verification state rather than contacting a daemon or a registry.
13. **GSU-FR-PVFP** `start_direct_graduation(draft_id, expected_worktree, expected_branch)` captures the prompt, runs the checks named here, and enqueues a **direct run** as one all-or-nothing operation. The run pins the worktree path and branch the author confirmed. A refusal leaves no run record, no queue entry, no draft lock and no dispatched turn.
14. **GSU-FR-MZGD** `preflight_direct_graduation()` is a read-only answer about the active worktree: its path, its name, its branch or its detached state, the work stream it is the working copy of, and the complete set of its uncommitted paths. It creates nothing, and the start uses the same reading.
15. **GSU-FR-SZTZ** A direct start checks, in this order and before the image preflight: the draft checks of GSU-FR-IJEE; `expected_worktree` is the active worktree, else `worktree_identity_changed` (per `GTC-git.md` GTC-FR-31); the worktree has a branch, else `direct_worktree_detached`; that branch is `expected_branch`, else `direct_branch_changed`; the worktree is clean, else `direct_worktree_dirty` with the complete path set.
16. **GSU-FR-ZTTS** A direct run records `standing_work` as `keep` and no commit message, and asks the author no standing-work choice. The clean check is made at the start and at no other moment: dispatch repeats nothing of it, so work the author adds afterwards is not refused.
17. **GSU-FR-GTRO** A refused start deletes no work stream. A stream that the author created before the start was refused remains a live stream of the project, whichever start refused.
18. **GSU-FR-NPFB** A restart of a discarded direct run takes the image preflight afresh and pins the worktree and branch the discarded run pinned. It makes no clean check and no active-worktree check, because the discarded run's work stays uncommitted in that worktree. A worktree whose directory is gone returns `direct_worktree_missing`.

## Contract surface

### The commands
```text
start_graduation(draft_id, stream_id, standing_work, standing_work_message) → GraduationRun
start_direct_graduation(draft_id, expected_worktree, expected_branch) → GraduationRun
preflight_direct_graduation() → DirectGraduationPreflight
```

`start_graduation` is registered and shaped by `GRD-graduation.md`; the checks it makes before it creates anything are this module's. `start_direct_graduation` and `preflight_direct_graduation` are registered by this module.

### The typed refusals
```text
draft_not_single_file        draft_locked_by_graduation
unknown_stream               vendor_image_unconfigured
vendor_image_invalid         vendor_execution_unsupported
docker_backend_unverified    worktree_identity_changed
direct_worktree_detached     direct_branch_changed
direct_worktree_dirty        direct_worktree_missing
not_a_git_repository
```

### Payload shapes
```text
DirectGraduationPreflight {
  worktree_path,        // absolute path of the active worktree
  worktree_name,        // the directory's basename
  branch?,              // absent when the worktree is detached
  is_detached,
  stream?,              // { stream_id, stream_name, busy_run_id? } where the worktree is a stream's
  dirty_paths           // every uncommitted path, complete
}
```

### Internal (Rust API, not registered as Tauri commands)
- The image preflight, which `AIC-agentic-integrations.md` and `PSS-project-settings-storage.md` are read through.
- The capture of `CapturedGraduationInput`, whose shape `GRD-graduation.md` holds.

## Non-functional requirements
- The preflight opens no container, contacts no registry and resolves no credential.
- A refusal costs no branch, no working copy and no draft lock.
- `preflight_direct_graduation` opens one repository and reads its status once. It contacts no remote.
