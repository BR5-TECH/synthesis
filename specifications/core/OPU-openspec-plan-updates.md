# OpenSpec plan updates

**Spec code:** `OPU`

## Intent
A **plan update** is how an apply run (per `OCR-openspec-change-runs.md`) asks the author to change the plan of its change: the proposal, the deltas, the design, or the tasks. The apply agent must not edit those files in place, because the author owns the plan. An apply agent that finds the plan wrong, or a review that finds it wrong, gives the author a reason, questions, and edits of the planning files. The author answers and decides them in the change discussion, where voice, personas, and the decide tool already work, and the run continues on the plan the author accepted. The run's own work stays apart from the author's plan commit, so the review and the commit of the run see only what the run did. Out of scope: an ordinary escalation of an apply run, which stays the escalation of `GXD-graduation-execution.md`; the change discussion surface, which is `../ui/OCW-openspec-change-workspace.md`'s; the question block, which is `../ui/DQA-discussion-question-answering.md`'s; and the turn instructions, which are `../ai/GRL-graduation-loop.md`'s.

## Functional requirements
1. **OPU-FR-RMZN** A **plan update** is a request of an apply run to change the planning files of its change. It holds a reason, 0 to 8 questions, and edits of files of the change folder. It ends the turn without a review, and the run rests `awaiting_author` with the reason `plan_update` once the plan update is recorded (OPU-FR-QXUE).
2. **OPU-FR-UNLM** What the turn wrote outside the scratch folder stays in the stream's working copy, uncommitted. The turn that takes the plan update's decisions continues on it.
3. **OPU-FR-UOYJ** The rest of a plan update holds the origin, `work` or `review`, the plan thread id, the plan proposal id where edits were recorded, the set id where questions were recorded, the questions as the agent gave them, the agent's reason, when the run rested, and, for the origin `review`, every finding of the verdict.
4. **OPU-FR-QUST** A plan update holds no `GraduationEscalation`. `answer_graduation_escalation` against a run at rest on `plan_update`, `plan_findings`, or `plan_update_invalid` is refused with `run_state_not_permitted` and records nothing.
5. **OPU-FR-EUZU** A run at rest on `plan_update`, `plan_findings`, or `plan_update_invalid` keeps its queue, its project slot, and its checkpoint (per `GRD-graduation.md` GRD-FR-BNTC). Continue from such a rest does not reset the pass budget.
6. **OPU-FR-BNFL** The **scratch folder** is `.synthesis-plan/` at the root of the stream's working copy. A file at `.synthesis-plan/<path>` holds the whole new text that the agent proposes for the project file at `<path>`. The scratch folder is not a file of the project.
7. **OPU-FR-WINO** Before the first turn of an apply run, the application makes sure that the repository's `info/exclude` file holds the line `/.synthesis-plan/`, on the terms of `../ai/GRL-graduation-loop.md` GRL-FR-LXWU. The change set, `hidden_paths`, every commit, and every review checkout leave out the scratch folder, also where that line is absent.
8. **OPU-FR-WJKF** Before it dispatches a work turn of an apply run, the application removes the scratch folder, except before a plan update correction turn (OPU-FR-XSRD). It also removes the folder after a plan update is recorded (OPU-FR-FOPN).
9. **OPU-FR-YBLE** The work turn of an apply run gives a plan update with an `escalation_required` answer whose escalation carries `kind: "plan_update"`, the reason, and 0 to 8 questions (per `../tools/ESU-escalate-to-user-tool.md` ESU-FR-VTAZ). The edits it proposes are the files of the scratch folder.
10. **OPU-FR-RFFH** Only the work turn of an apply run, its plan update correction turn, and its plan correction turn may give a plan update. Another turn that answers with `kind: "plan_update"` has its answer refused as `escalation_shape`, and the answer correction of `../ai/GRL-graduation-loop.md` GRL-FR-QKDC follows.
11. **OPU-FR-MBIY** The application reads a plan update in place of the check gate (per `../ai/GRL-graduation-loop.md` GRL-FR-XWNQ). It reads each entry below the scratch folder through `FSA-filesystem-access.md`. A file of the change folder that the scratch folder does not hold gets no hunk.
12. **OPU-FR-XLVF** A plan update is refused with the first cause that applies: `plan_path_outside_change` where an entry is not a regular file or its path is not below `openspec/changes/<change>/`; `plan_file_not_text` where a file is not UTF-8 text; `plan_deletes_file` where a file is empty and the working copy holds that path not empty; `plan_update_empty` where it gives no hunk and no question.
    - *Why:* A change edit proposal holds no hunk that deletes a file, so a deletion cannot reach the author as an edit.
13. **OPU-FR-MCEZ** A scratch file that differs from the same path in the stream's working copy becomes hunks on the terms of `OCR-openspec-change-runs.md` OCR-FR-UTFI: line hunks, one whole-file hunk, or one new-file hunk where the working copy holds no file at that path. An equal file becomes no hunk. The hunks follow the order of their paths.
14. **OPU-FR-XSRD** A refused plan update makes the loop dispatch one **plan update correction** turn of part `work`, on the session terms of `../ai/GRL-graduation-loop.md` GRL-FR-SMQK. It carries the `plan_update_invalid` text of `refusals.md` as `correction`. The turn changes no file outside the scratch folder, and answers a plan update or an escalation.
15. **OPU-FR-CLYW** A plan update correction turn whose answer is not a valid plan update and not a recorded escalation rests the run `awaiting_author` with the reason `plan_update_invalid` and the cause of the last refusal. Nothing is recorded in a discussion, and nothing is committed.
16. **OPU-FR-VKBO** A plan update correction turn adds one to `work_turns`, is charged to the time limits, and changes neither the pass nor the pass history. `continue_graduation_run` against a run at rest on `plan_update_invalid` clears the rest and dispatches a fresh work turn of the same pass with `purpose = "resume"`. It does not reset the used total time.
17. **OPU-FR-EPXL** `ReviewFinding.kind` is `work` or `plan`, and is `work` where it is absent. Only the review of an apply run may give `plan`, and the loop reads a `plan` finding of any other review as `work`. A finding that the application writes in place of a verdict (`../ai/GRL-graduation-loop.md` GRL-FR-HSZD, GRL-FR-EQDL, GRL-FR-XNBQ, GRL-FR-NDHW) has the kind `work`.
18. **OPU-FR-FDCW** A `revise` verdict of an apply review that holds at least one `plan` finding is never advisory (`../ai/GRL-graduation-loop.md` GRL-FR-UQNV does not apply to it) and starts no pass. The loop appends its findings to the review history (GRL-FR-RHST) and dispatches one **plan correction turn**.
    - *Why:* A work turn cannot correct a plan that it must not edit, so another pass on the same plan spends the budget on work that the review will refuse again.
19. **OPU-FR-DTYE** A plan correction turn is a turn of part `work` in the stream's working copy. It carries the `plan_correction` text of `refusals.md` as `correction`, and the `plan` findings of the verdict, in their order, as `loop_instruction`. It resumes the session of the run's last work turn where the vendor resumes one, and runs fresh where it does not.
20. **OPU-FR-FPRX** A plan correction turn changes no file outside the scratch folder and must answer a plan update. It adds one to `work_turns`, is charged to the time limits, changes neither the pass nor the pass history, and carries no milestone block. The check gate does not read its answer.
21. **OPU-FR-QKLX** A plan correction turn whose answer is a valid plan update records it with the origin `review`. Every other answer, a success, a failure, an ordinary escalation, and a refused plan update included, rests the run `awaiting_author` with the reason `plan_findings`, which holds every finding of the verdict. No review runs, and nothing is committed.
22. **OPU-FR-MCSG** When a run rests on `plan_findings`, the application opens a new whole-change discussion of the change, the **plan thread**, as the Synthesis participant (per `CMS-comments-storage.md` CMS-FR-MKEV). Its one comment is the fixed text of the contract surface, followed by the description and the correction of each `plan` finding, in order. The rest holds the thread id.
23. **OPU-FR-WOKJ** While a run rests on `plan_findings` and its rest holds no plan proposal, a recording of a change edit proposal into its plan thread is admitted as the run's **plan proposal**. The run writes its id on the rest in the same act. Every other recording on the change stays refused (OPU-FR-DEEB).
    - *Why:* The author settles a plan finding by asking an agent in the plan thread, and the run's Continue must take that proposal as the plan.
24. **OPU-FR-QXUE** A valid plan update is **recorded** in three steps, in this order. The application opens a new whole-change discussion of the change, the plan thread, whose first comment is the reason, appended as the Synthesis participant through `append_as` (CMS-FR-26). It reserves the question set where there are questions. It records the plan proposal where there are hunks.
25. **OPU-FR-VXVO** The question set is reserved through `reserve_discussion_question_set` (CMS-FR-QLDW) with a fresh `set_id`, the plan thread, the Synthesis participant as `asked_by`, and each question in order. Each option takes the escalation option's `summary` as its `value` and its `description` as its `description`.
    - *Why:* The question block shows the summary of a run's option as its label (`../ui/DQA-discussion-question-answering.md` DQA-FR-QWTB), and the `answer` of the option reaches the agent from the rest (OPU-FR-CAGI).
26. **OPU-FR-OQRA** The plan proposal is recorded through `record_change_proposal` (`CEP-change-edit-proposals.md` CEP-FR-QDJH) into the plan thread, as the Synthesis participant, with the reason as its rationale. This recording is not refused with `apply_running`.
27. **OPU-FR-FOPN** When every step has succeeded, the application writes the rest of OPU-FR-UOYJ with the checkpoint durably and moves the run to `awaiting_author`. It then removes the scratch folder.
28. **OPU-FR-BKZX** The checkpoint records the id of each step when it completes. A step that is refused rests the run `blocked` with the code of the refusal and the resume phase `record` (per `GXD-graduation-execution.md` GXD-FR-GMDI). `continue_graduation_run` then takes the first step that is not done, dispatches no turn, and spends no pass, on the terms of `OCR-openspec-change-runs.md` OCR-FR-PBKH.
29. **OPU-FR-TPYT** A submission through `submit_discussion_question_answers` that names the set of a plan update rest first writes the answers on that rest, durably (per `CMS-comments-storage.md` CMS-FR-TXRB). A write that fails refuses the submission with `write_failed`, and nothing is appended. A later submission of the same set replaces the answers written before.
30. **OPU-FR-GYHC** The submission then appends its comments and deletes the set on the terms of CMS-FR-TXRB, and the surface dispatches on the terms of `../ui/DQA-discussion-question-answering.md` DQA-FR-CIRK. A plan thread in which the author addressed no agent has no active agent, so its submission dispatches no turn.
31. **OPU-FR-EVBZ** A decision on a hunk of the plan proposal is refused with `plan_questions_pending` while the question set of its plan update stands (per `CEP-change-edit-proposals.md` CEP-FR-KGOL). After the set is gone, the plan proposal is decided on the terms of every change edit proposal, by chip and by `decide_change_edits`.
32. **OPU-FR-DMCX** A hunk of the plan proposal may edit `tasks.md`. The close commit of the plan proposal holds each accepted file as it stands in the working copy (CEP-FR-XVXQ), so the tasks that the run marked done before the plan update land in that commit.
33. **OPU-FR-THOZ** `continue_graduation_run` against a run at rest on `plan_update` is refused with `plan_questions_pending` while the rest holds a set id and no answers. Against a rest on `plan_update` or `plan_findings`, it is refused with `proposal_pending` while the plan proposal is pending or holds `commit_owed` (CEP-FR-IYHP). A refused Continue changes nothing.
34. **OPU-FR-PXDY** Before it dispatches, Continue from `plan_update` or `plan_findings` runs `validate_change` and `apply_instructions` on the stream's working copy (per `OSM-openspec-mode.md` OSM-FR-APVF, OSM-FR-QLRW). A read that cannot run refuses Continue with `openspec_validation_unavailable` and the read's code, and the run stays at rest.
35. **OPU-FR-CAGI** Continue from `plan_update` appends one ledger entry for each question, in position order: the question, the chosen option's `answer` or the author's own words as the answer, the chosen option's `summary` or the own words as the summary, the note, the turn `work`, and the pass (per `GXD-graduation-execution.md` GXD-FR-NWSL).
36. **OPU-FR-ZNRR** Continue then appends one ledger entry for each hunk of the plan proposal that is not withdrawn, in proposal order. Its question is `<path> edit <n>`, where `n` is the 1-based position of the hunk among the hunks of its path. Its answer and its summary are `accepted`, `edited`, or `rejected`, and its note is the feedback of the decision.
37. **OPU-FR-NQBK** Where the plan proposal closed with a commit, Continue moves the run's base commit to a **plan base** before it dispatches (per `GRD-graduation.md` GRD-FR-YBUM).
38. **OPU-FR-YNXV** A plan base is a commit whose tree is the tree of the run's base commit, with each path that the plan proposal's close commit changed set as that commit holds it. Its one parent is the run's base commit. The application keeps it reachable under the private ref `refs/synthesis/plan-base/<run-id>`.
    - *Why:* The plan commit is the author's work, so the change set, the review, and the commit of the run must not hold it.
39. **OPU-FR-FSCV** The private ref moves to each newer plan base of the run, and is removed when the run reaches a terminal state. The change set, every review checkout (`../ai/GRL-graduation-loop.md` GRL-FR-YKRI), the planning gate of GRL-FR-EQDL, and the run's `ready` commit (GRD-FR-ARLT) measure from the plan base.
40. **OPU-FR-YTKN** Continue writes the ledger entries, the plan base, and the cleared rest with the checkpoint in one durable write, resets the used total time (per `../ai/GRL-graduation-loop.md` GRL-FR-NVGQ), and dispatches. A plan base that cannot be made refuses Continue with `write_failed` and changes nothing.
41. **OPU-FR-WPSR** Continue then dispatches a work turn of the same pass with `purpose = "escalation_answer"`, and the pass does not advance. Where the vendor resumes a session, the turn resumes the session of the turn that gave the plan update (per GXD-FR-XPUR): the work turn for the origin `work`, and the plan correction turn for the origin `review`.
42. **OPU-FR-SKTV** The turn carries `plan_update` in its task input: the origin, the reason where there is one, and each hunk of the plan proposal by path, position, and outcome, with no file text. It carries the results of OPU-FR-PXDY as `openspec_validation` and `openspec_apply_progress`. For the origin `review`, it carries the `work` findings of the verdict as `loop_instruction`.
43. **OPU-FR-TZBD** Continue against a `plan_findings` rest that holds no plan proposal dispatches a work turn of the same pass with `purpose = "resume"`. It carries every finding of the verdict as `loop_instruction`, and no `plan_update`. It makes no plan base, appends no ledger entry, and resets the used total time.
44. **OPU-FR-DEEB** While an apply run of a change is not terminal, a recording of a change edit proposal on that change is refused with `apply_running` (per `CEP-change-edit-proposals.md` CEP-FR-YYZZ). The recording of OPU-FR-OQRA and the admitted recording of OPU-FR-WOKJ are not refused for it, and an update of the pending plan proposal (CEP-FR-AXEV) is not refused for it.
    - *Why:* An accepted proposal of another agent writes the change folder under the run, enters its change set, and makes the planning gate refuse every `ready`.
45. **OPU-FR-ZBXF** A discard or a revert of a run at rest on `plan_update` deletes the question set of its plan update where the set stands (per `CMS-comments-storage.md` CMS-FR-ECWC). The plan thread and its comments stay. The plan proposal stays and stays decidable, and its close commit, once made, stays on the stream branch.
46. **OPU-FR-VGFJ** Every recording, refusal, hand-over of answers, and Continue logs through `LGC-logging.md` under the `backend` domain: an `INFO` record with the run id, the change name, the origin, the counts, and the ids, or a `WARN` record with the code or the cause. No record holds a reason, a question, an answer, a note, a hunk, or file text.
47. **OPU-FR-IBWG** Every behaviour of this module is exercisable with no container runtime, no OpenSpec CLI, and no network, through the dispatch seam of `GXD-graduation-execution.md` and the process seam of `OSM-openspec-mode.md`.

## Contract surface

### Internal (Rust API)
- The reading and the recording of a plan update (OPU-FR-MBIY, OPU-FR-QXUE), called by the loop of `../ai/GRL-graduation-loop.md` on the answer of an apply run's work turn, plan update correction turn, or plan correction turn, and by `continue_graduation_run` at the resume phase `record`.
- The hand-over of answers (OPU-FR-TPYT), called by `submit_discussion_question_answers` of `CMS-comments-storage.md` before its append.
- The Continue from `plan_update`, `plan_findings`, and `plan_update_invalid` (OPU-FR-THOZ through OPU-FR-TZBD, OPU-FR-VKBO), called by `continue_graduation_run` of `GRD-graduation.md`.
- The admission of a plan proposal at a `plan_findings` rest (OPU-FR-WOKJ), called by `record_change_proposal` of `CEP-change-edit-proposals.md`.

No Tauri command is registered by this module.

### Rest reasons
```text
plan_update            a recorded plan update (OPU-FR-RMZN)
plan_findings          a plan correction turn gave no valid plan update (OPU-FR-QKLX)
plan_update_invalid    a plan update correction turn gave no valid plan update (OPU-FR-CLYW)
```

### Refusal causes of a plan update
```text
plan_path_outside_change    plan_file_not_text
plan_deletes_file           plan_update_empty
```

### Typed refusals of Continue
```text
plan_questions_pending      proposal_pending
openspec_validation_unavailable    write_failed
```

`openspec_validation_unavailable` carries the read's error code after a colon. The blocker codes of a recording step are the codes of the store that refused it (OPU-FR-BKZX).

### Payload shapes
```text
GraduationRest (plan fields)   // GRD's GraduationRest, reason "plan_update" | "plan_findings" | "plan_update_invalid"
{
  origin?,          // plan_update: "work" | "review"; plan_findings: "review"
  thread_id?,       // plan_update, plan_findings: the plan thread
  proposal_id?,     // the plan proposal, where edits were recorded or admitted
  set_id?,          // plan_update: the question set, where questions were recorded
  questions?,       // plan_update: the questions as given, each with its options { answer, summary, description }
  answers?,         // plan_update: the submitted QuestionAnswer[] of CMS, once the set is submitted
  plan_reason?,     // plan_update: the agent's reason
  findings?,        // origin review: every ReviewFinding of the verdict, in order
  cause?            // plan_update_invalid: a refusal cause of a plan update
}

PlanUpdateTaskContext {   // GraduationTaskInput.plan_update (OPU-FR-SKTV)
  origin,           // "work" | "review"
  reason?,          // the agent's reason; absent after a plan_findings rest
  hunks: [{ path, position, outcome }]   // outcome: "accepted" | "edited" | "rejected"
}

ReviewFinding.kind? // "work" | "plan"; "work" where absent (OPU-FR-EPXL)

AgentResponseEnvelope.escalation.kind?   // "plan_update" (OPU-FR-YBLE)
```

### Fixed text
- The comment of a `plan_findings` rest (OPU-FR-MCSG): `The review of the apply run found that the plan of "<change>" is wrong. Ask an agent here to propose the edits, then use Continue.`

### Storage
```text
<working copy>/.synthesis-plan/            the scratch folder (OPU-FR-BNFL)
refs/synthesis/plan-base/<run-id>          the newest plan base of a run (OPU-FR-YNXV)
```

## Non-functional requirements
- A plan update costs one read of each scratch file and of its counterpart in the working copy, one discussion, at most one question set, and at most one proposal.
- A Continue from a plan rest costs one validation, one apply-progress read, and at most one commit object without a checkout.
- No log record, event, or task input carries a credential, a question's text outside the ledger, or the text of a project file.
