# OpenSpec change runs

**Spec code:** `OCR`

## Intent
An **OpenSpec change run** is a graduation run on a change that already stands on a work stream, with no draft behind it. An **apply run** implements the tasks of the change in the stream's working copy and lands the work with one commit on the stream branch, and a fresh review verifies the work against the change. A **rework run** revises the planning files of the change on the author's instruction, in a worktree that the run owns, and gives its result to the author as one change edit proposal, so no run writes the change before the author accepts each hunk. Both use the state machine, the queues, the observability, and the loop of graduation, and this module states where they differ from a draft run. The author starts a run from the change workspace or by confirming the request of an agent. Out of scope: the archive of a change; the plan update of an apply run, which is `OPU-openspec-plan-updates.md`'s; the request that an agent makes, which is `CAR-change-action-requests.md`'s; the turn instructions, which are `../ai/GRL-graduation-loop.md`'s; and the surfaces, which are `../ui/OCW-openspec-change-workspace.md`'s and `../ui/GRU-graduation-runs.md`'s.

## Functional requirements
1. **OCR-FR-VHQW** An **OpenSpec change run** is a stream run whose record carries `openspec` data with the phase `apply` or `rework` and the name of a change on its stream. An **apply run** has the phase `apply`, and a **rework run** has the phase `rework`. A run with the phase `propose` is a propose run (per `GRD-graduation.md` GRD-FR-LKGC).
2. **OCR-FR-CXPK** The phase `apply` of `openspec` data names the kind of an OpenSpec change run. It is not the resume phase `apply` of a merge run's checkpoint (per `GXD-graduation-execution.md` GXD-FR-GMDI), and no rule reads one for the other.
3. **OCR-FR-NIOZ** An OpenSpec change run has **no draft**. Its `input.draft_id` and `input.draft_name` are empty, its `input.prompt` is the fixed sentence of its phase in the contract surface with the change name filled in, and its `input.prompt_checksum` is the sha256 of that sentence.
4. **OCR-FR-RYKA** An OpenSpec change run takes no draft lock, makes no draft report `graduated`, asks for no draft-storage commit (per `GSU-graduation-start.md` GSU-FR-RNOM), and has no direct target.
5. **OCR-FR-FZVF** The title of a run is `Apply <change>` for an apply run and `Rework <change>` for a rework run. It is the label of the run's in-flight operation (per `GRD-graduation.md` GRD-FR-ZHNV), and every listing of runs carries it as the run's name.
6. **OCR-FR-TBGA** A rework run also records the author's instruction, the id of its **request thread**, and its **requester**, the participant that its finish writes as. `start_change_rework` records the Synthesis participant (per `CMS-comments-storage.md` CMS-FR-MKEV), and a confirmed action request records the agent of the request (per `CAR-change-action-requests.md` CAR-FR-MMPV).
7. **OCR-FR-IDFC** `start_change_apply(stream_id, change)` and `start_change_rework(stream_id, change, instruction)` each capture the input, run every check of this spec, and enqueue the run as one all-or-nothing operation, on the terms of `GSU-graduation-start.md` GSU-FR-ELZO and GSU-FR-DPSY. A refusal leaves no run record, no queue entry, and no dispatched turn.
8. **OCR-FR-GTOM** A start checks in this order and is refused by the first check that fails: `no_project_open`; `unknown_stream` where the stream is not live; `openspec_mode_inactive`; `openspec_cli_unavailable`; `invalid_change_name`; `change_not_found` where `openspec/changes/<change>/` is not a folder in the stream's working copy; the checks of its phase (OCR-FR-MGPU through OCR-FR-TCFQ); the image preflight; and the write check.
9. **OCR-FR-MGPU** A start is refused with `openspec_run_active` where a non-terminal OpenSpec run of any phase on the same stream names the same change. The refusal carries the id of that run.
10. **OCR-FR-LNKQ** An apply start reads `apply_instructions` in the stream's working copy (per `OSM-openspec-mode.md` OSM-FR-QLRW). It is refused with `change_not_ready`, which carries the missing artifacts, where the state is `blocked`, and with `apply_progress_unavailable`, which carries the read's error code, where the read fails. The states `ready` and `all_done` admit the start. An apply start is also refused with `proposal_pending` where the change holds a pending change edit proposal, or a closed one whose commit is still owed (per `CEP-change-edit-proposals.md` CEP-FR-ZOIG, CEP-FR-IYHP).
   - *Why:* The accepted files of such a proposal stand uncommitted in the working copy, enter the run's change set, and make the planning gate of `../ai/GRL-graduation-loop.md` GRL-FR-EQDL refuse every `ready`.
11. **OCR-FR-TCFQ** A rework start is refused with `blank_instruction` where the instruction is empty once trimmed, with `proposal_pending` where the change holds a pending change edit proposal (per `CEP-change-edit-proposals.md` CEP-FR-ZOIG), and with `discussion_resolved` where its request thread is resolved.
12. **OCR-FR-QBIF** The image preflight is the one of `GSU-graduation-start.md` GSU-FR-GLVQ and GSU-FR-RQDS, with its refusals. The write check is the main branch write check of `GTC-git.md` GTC-FR-JWOL on the stream branch (per `GRD-graduation.md` GRD-FR-WJLO).
13. **OCR-FR-VUXZ** An OpenSpec change run records `standing_work` as `keep` with no message, is enqueued with `auto_start` enabled at the latest end of the run order (per `GSU-graduation-start.md` GSU-FR-NCJN), and belongs to its stream's queue on the terms of every stream run. No start asks the author a standing-work choice.
14. **OCR-FR-UAWO** `start_change_rework` makes its own request thread. After every check passes, it opens a new whole-change discussion of the change whose first comment is the instruction, appended in the author's name through the human write path (per `CMS-comments-storage.md` CMS-FR-11). Then it enqueues the run with that thread and returns the run and the thread id.
15. **OCR-FR-SFDE** A discussion that cannot be opened refuses the start with its typed error, and nothing is enqueued. An enqueue that fails after the discussion is open leaves the discussion as it is, and the start returns the failure with the suffix `; thread: <thread-id>` after its code and detail, for example `write_failed; thread: <thread-id>`.
16. **OCR-FR-TWDK** `start_change_rework_in_thread(stream_id, change, instruction, thread_id, requester)` starts a rework run in an existing thread. It runs the checks of OCR-FR-GTOM, opens no discussion, and is called by `CAR-change-action-requests.md` alone. A thread that is not a discussion of the change refuses the start with `discussion_not_found`. `start_change_apply` is also called in-process by that module.
17. **OCR-FR-XKTV** An apply run works in the stream's working copy. Its work turns, its check gate, its review turns, and its pass budget are those of every run that is not a merge run, with the instructions of an apply run (per `../ai/GRL-graduation-loop.md` GRL-FR-EUSF, GRL-FR-NWSC).
18. **OCR-FR-UZNC** A `ready` review of an apply run commits the work onto the stream branch on the terms of `GRD-graduation.md` GRD-FR-ARLT. The commit message is `Apply OpenSpec change <change>`, on one line with no body.
19. **OCR-FR-QMCG** A completed apply run records the run summary of its `ready` review (per `GRD-graduation.md` GRD-FR-UUGP). When its stream merges into the base branch, it records a landing on the terms of GRD-FR-UXXF, and `list_branch_completed_runs` returns it with the kind `change_apply`.
20. **OCR-FR-IWOW** An apply run that stops without a review commits its abandoned turn on the terms of `GRD-graduation.md` GRD-FR-SWOJ. It is reverted and discarded on the terms of every stream run (GRD-FR-BLCR, GRD-FR-EWTN).
21. **OCR-FR-QUSP** A rework run owns a **rework worktree** at `short_data_dir()/g/<run-id>/cw/`, registered as `<run-id>-cw` on the scratch branch `synthesis/rework-run/<run-id>`. The run creates it at its first dispatch, before its first turn. Its work turns run in it, and no turn of the run writes the stream's working copy or the stream branch.
22. **OCR-FR-SSAI** The rework worktree is seeded from the stream branch head, and each file of `openspec/changes/<change>/` is then copied into it as the file stands in the stream's working copy, so the change folder of the seed equals the folder in the working copy, and a file that the working copy does not hold is not in the seed. The application commits that tree on the scratch branch as the **seed commit**, which is the run's base commit (per `GRD-graduation.md` GRD-FR-YBUM).
    - *Why:* An accepted hunk can stand uncommitted in the working copy (CEP-FR-IYHP), and a proposal made against the branch head would undo it.
23. **OCR-FR-TQCW** A review turn of a rework run stands in a throwaway review checkout that is derived from the rework worktree, with the seed commit as its base commit, on the terms of `../ai/GRL-graduation-loop.md` GRL-FR-YKRI.
24. **OCR-FR-HFLI** A turn of a rework run that stops without a review commits nothing. What it wrote stays in the rework worktree, and the run rests `interrupted` (per `GRD-graduation.md` GRD-FR-SWOJ).
25. **OCR-FR-ZNJT** A `ready` review of a rework run makes no commit. In its place the run **finishes**: it compares each file below `openspec/changes/<change>/` of the reviewed tree (per `../ai/GRL-graduation-loop.md` GRL-FR-XNQU) with the same path in the stream's working copy, read through `FSA-filesystem-access.md`.
26. **OCR-FR-UTFI** A file that differs becomes line hunks on the terms of `PCP-prompt-change-proposals.md` PCP-FR-CNVT, or one hunk over the whole file where those hunks cannot each be found once. A file that the working copy does not hold becomes one new-file hunk (per `CEP-change-edit-proposals.md` CEP-FR-HRCX). A file that is not UTF-8 text in the run's tree or in the working copy becomes no hunk. The hunks follow the order of their paths.
27. **OCR-FR-WBZX** Where at least one hunk results, the finish records one change edit proposal through `record_change_proposal` (per `CEP-change-edit-proposals.md` CEP-FR-QDJH) into the request thread, as the requester. Its rationale is the description of the run summary, or the verdict's rationale where the review gave no summary.
28. **OCR-FR-KXJX** Where no file differs, the finish records no proposal. It appends one comment into the request thread as the requester, with the fixed text of the contract surface, which states that the rework pass found nothing to change.
29. **OCR-FR-GWGE** A finish that records its proposal or its comment records the proposal id, where there is one, and the run summary, and moves the run to `completed`. A rework run makes no commit and no landing, and `revert_graduation_run` against it is refused with `run_not_revertable`.
30. **OCR-FR-PBKH** A finish that is refused rests the run `blocked` with the code of the refusal: `proposal_pending`, `discussion_resolved`, or `write_failed`. The checkpoint records the resume phase `record` (per `GXD-graduation-execution.md` GXD-FR-GMDI). `continue_graduation_run` retries the finish alone: it dispatches no turn and spends no pass.
31. **OCR-FR-OXVH** While a rework run of a change is not terminal, every other recording of a change edit proposal on that change is refused with `rework_running` (per `CEP-change-edit-proposals.md` CEP-FR-OIZT). The finish of that run is not refused for it.
31. **OCR-FR-RRUU** An apply run asks the author to change the plan of its change through a plan update, and a review finds a wrong plan through a `plan` finding (per `OPU-openspec-plan-updates.md`). While an apply run of a change is not terminal, every other recording of a change edit proposal on that change is refused with `apply_running` (OPU-FR-DEEB).
32. **OCR-FR-FJFI** When a rework run reaches a terminal state, its rework worktree, its registration, and its scratch branch are removed. On the first queue read after a launch, each rework run that has ended has them removed, and a removal that fails is logged and does not stop the read.
33. **OCR-FR-AWDH** A discard of a rework run writes no branch, no file of the stream's working copy, and no comment, and removes what the run owns (OCR-FR-FJFI).
34. **OCR-FR-XIBE** Every agent operation of an OpenSpec change run is counted against its change. When one finishes, the run appends one `agent_operation` event and one `token_usage` event of scope `graduation` for each provider-reported usage record to the change's statistics log (per `CST-change-statistics.md` CST-FR-RJPT). It writes no line to the log of a draft.
35. **OCR-FR-SAVO** `restart_graduation_run` against a discarded OpenSpec change run creates a new run of the same phase, change, and stream over the discarded run's captured input, with the same instruction, request thread, and requester. It reads none of the `stream_id`, `standing_work`, and `standing_work_message` that it is given, and records `keep`.
36. **OCR-FR-NGGJ** A restart takes the checks of OCR-FR-GTOM afresh at the moment it is taken. The check of OCR-FR-MGPU does not count the discarded run, and the thread check of a rework run reads its request thread.
37. **OCR-FR-QDOZ** Every start, finish, and refusal of this module logs through `LGC-logging.md` under the `backend` domain: an `INFO` record with the run id, the stream id, the change name, the phase, and the hunk count of a finish, or a `WARN` record with the refusal code. No record holds the instruction, a hunk, or the text of a file.
38. **OCR-FR-HNXC** Every behaviour of this module is exercisable with no container runtime, no OpenSpec CLI, and no network, through the dispatch seam of `GXD-graduation-execution.md` and the process seam of `OSM-openspec-mode.md`.

## Contract surface

### Tauri commands
- `"start change apply"` → `start_change_apply(stream_id, change)` → `GraduationRun` (OCR-FR-IDFC).
- `"start change rework"` → `start_change_rework(stream_id, change, instruction)` → `ChangeReworkStart` (OCR-FR-IDFC, OCR-FR-UAWO).

Both are registered by this module. The run they return has the shape of `GRD-graduation.md`.

### Internal (Rust API)
- `start_change_apply(stream_id, change)` — also called in-process by `CAR-change-action-requests.md` (CAR-FR-MMPV).
- `start_change_rework_in_thread(stream_id, change, instruction, thread_id, requester)` — called by `CAR-change-action-requests.md` alone (OCR-FR-TWDK).
- The finish of a rework run (OCR-FR-ZNJT), called by the loop when a rework review answers `ready`, and by `continue_graduation_run` at the resume phase `record`.

### Typed refusals
```text
no_project_open              unknown_stream
openspec_mode_inactive       openspec_cli_unavailable
invalid_change_name          change_not_found
openspec_run_active          change_not_ready
apply_progress_unavailable   blank_instruction
proposal_pending             discussion_resolved
discussion_not_found
vendor_image_unconfigured    vendor_image_invalid
vendor_execution_unsupported docker_backend_unverified
docker_socket_unavailable    main_branch_write_unconfirmed
```

`openspec_run_active` carries the run id after a colon: `openspec_run_active: <run-id>`. `change_not_ready` carries the missing artifacts after a colon, separated by commas. `apply_progress_unavailable` carries the read's error code after a colon.

The blocker codes of a rework run's finish are `proposal_pending`, `discussion_resolved`, and `write_failed` (OCR-FR-PBKH).

### Payload shapes
```text
ChangeReworkStart {
  run,                // GraduationRun
  thread_id           // the request thread (OCR-FR-UAWO)
}

OpenSpecRunData {     // GraduationRun.openspec
  phase,              // "propose" | "apply" | "rework"
  change,             // the OpenSpec change name
  instruction?,       // rework: the author's instruction (OCR-FR-TBGA)
  thread_id?,         // rework: the request thread
  requester?,         // rework: Participant; the Synthesis participant or the requesting agent
  proposal_id?        // rework: the proposal its finish recorded (OCR-FR-GWGE)
}
```

### Fixed text
- The captured prompt of an apply run: `Apply the OpenSpec change "<change>": implement its tasks.`
- The captured prompt of a rework run: `Rework the OpenSpec change "<change>" on the instruction of the author.`
- The comment of a finish that found no difference (OCR-FR-KXJX): `The rework pass found nothing to change in "<change>".`

### Storage
```text
short_data_dir()/g/<run-id>/cw/    a rework run's rework worktree (OCR-FR-QUSP)
```

A rework run also owns, in the repository, the scratch branch `synthesis/rework-run/<run-id>` and the worktree registration `<run-id>-cw`. Nothing of the three outlives the run (OCR-FR-FJFI).

## Non-functional requirements
- A start costs one OpenSpec CLI process for an apply run, and none for a rework run, beside the checks of `GSU-graduation-start.md`.
- A finish costs one read of each file of the change in two trees and one recording. Nothing grows with the number of earlier runs of the change.
- No log record and no event carries a credential, the instruction, or a byte of a project file.
