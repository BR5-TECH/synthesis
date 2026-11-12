# Graduation loop

**Spec code:** `GRL`

## Intent
The agent loop a graduation run is driven by. It holds two phases and no more: a **work** turn that drives an execution agent against the run's work stream until the agent reports the task finished, and a **review** turn that judges what the work turn wrote. A `ready` review also gives the **run summary** of the work, a title and a description that a pull request can reuse, and a **PR summary run** (per `../core/GRD-graduation.md` GRD-FR-KAKU) is one turn that writes such a title and description for a whole head branch. Both follow the repository's own pull request template, skills and agent instructions, because the shape of a pull request is the project's decision. A **merge run** (per `../core/GRD-graduation.md` GRD-FR-MRNQ) drives the same two phases under its own instructions: a **merge work** turn that resolves a merge Git could not settle in an isolated merge worktree, and a fresh **merge review** turn that judges the result against both branches and the repository's specifications. An **OpenSpec change run** (per `../core/OCR-openspec-change-runs.md`) also drives the same two phases: an **apply** run's work turn implements the tasks of a change and its review verifies the code against the change, and a **rework** run's work turn revises a change on the author's instruction and its review judges the revision. A work turn runs the project's own checks and records their results in the **check folder** of its working copy. It ends with every required check passed or it asks the author, and a **check gate** on its answer holds it to that rule. A review turn runs no check: it reads that record and judges it, so a pass runs each check once. The loop enforces no shape on the work. It tells the agent what was asked for and where to find the project's own instructions; what a change must contain is the project's rule, not this module's. A review that answers `ready` ends the run; one that asks for revision starts one further work turn while the project's pass budget holds one, and a review that still asks for revision when the budget is spent stops the run for the author. A merge run has a pass budget of its own. Each turn is bounded by the project's stage time limit, and the whole run by its total time limit, so an author can give a long run more room or stop a run that costs too much. Out of scope: the container the turn runs in, which is `../tools/EAC-execute-agent-cli.md`'s; the dispatch seam, which is `../core/GXD-graduation-execution.md`'s; and the run's states and queues, which are `../core/GRD-graduation.md`'s.

## Functional requirements
1. **GRL-FR-EKXT** This loop is composed and driven through the `rig` framework, separate from `CVL-conversation-loop.md` in prompt, tool set, state and lifecycle. One instance runs per work stream at a time (per `../core/WKS-work-streams.md` WKS-FR-NDSV).
2. **GRL-FR-GADT** Every instruction this loop uses is a file under `resources/prompts/graduation/`, compiled into the binary at build time: `work.md`, `review.md`, `rebase.md`, `refusals.md`, `merge-work.md`, `merge-review.md`, `pr-summary.md`, `openspec-propose.md`, `openspec-apply.md`, and `openspec-rework.md`. `refusals.md` holds every correction text. `openspec-propose.md`, `openspec-apply.md`, and `openspec-rework.md` each hold the two blocks of one phase (GRL-FR-FYKL, GRL-FR-APHV, GRL-FR-EUSF, GRL-FR-NWSC, GRL-FR-CWSA). `rebase.md` serves the semantic rebase turn of a stream update alone. A merge run compiles `merge-work.md` and `merge-review.md`, and compiles none of `work.md`, `review.md` and `rebase.md`. A PR summary run compiles `pr-summary.md` alone. No run reads one from disk, and no operation accepts, returns or overrides one. `work.md`, `review.md`, `merge-work.md`, and `merge-review.md` each carry the milestone block of GRL-FR-KWPT.
3. **GRL-FR-YIJG** A prompt file's instruction text is its content with every authoring comment removed. The files carry comments naming the requirements they satisfy; an agent never sees them.
4. **GRL-FR-VYNX** One **pass** of a draft run is a work turn followed by a review turn. The loop dispatches the work turn through `../core/GXD-graduation-execution.md`'s `dispatch_graduation_turn`, and on a completed execution dispatches the review turn over what the working copy then holds.
5. **GRL-FR-ARPX** A run that is not a merge run takes at most the project's **pass budget** of passes, which is two where the project configures none (per `../core/PSS-project-settings-storage.md` PSS-FR-QBNE). A review that answers `ready` on any pass ends the loop. A review that answers `revise` starts the next pass while the budget holds one, and rests the run in `awaiting_author` carrying the findings where it does not.
   - *Why:* A loop that revises without a bound spends the author's time and tokens re-deciding work it has already been told about.
6. **GRL-FR-DXLU** `work.md` is the instruction every work turn carries, whole and unaltered, identical for every run and every project. The work turn of an OpenSpec run carries the work block of its phase after it (GRL-FR-FYKL, GRL-FR-EUSF, GRL-FR-CWSA). It names the captured prompt as the task and directs the agent to the repository's own instructions, contributor notes and skills for how work is done there. It carries the milestone block of GRL-FR-KWPT.
7. **GRL-FR-DAIB** Outside an OpenSpec run of any phase, the loop states no required deliverable. It does not ask for a specification, an implementation, a test, or a file in a named place, and it refuses no path the agent wrote.
   - *Why:* Every project shapes its own work through its own instructions, and a loop that imposed a shape would override the rule the project already states.
7. **GRL-FR-FYKL** A propose run's work turn carries `work.md` followed by the work block of `openspec-propose.md`, with the change name filled in. The block names the folder `openspec/changes/<name>/` as the deliverable, and tells the agent to take the instructions of each artifact from `openspec instructions <artifact> --change <name> --json`.
   - *Why:* The project chose OpenSpec, so the shape of the work is its rule; the CLI gives the shape of the project's own schema.
7. **GRL-FR-GWDB** The work block of `openspec-propose.md` and the work block of `openspec-rework.md` tell the agent to change no file outside the change folder and the check folder, and to run `openspec validate <name> --strict` before it reports. The apply block scopes its writes by GRL-FR-SZQK.
7. **GRL-FR-APHV** A propose run's review turn carries `review.md` followed by the review block of `openspec-propose.md`. The block tells the agent to judge the change against the prompt: whether its proposal, deltas, design, and tasks state what the prompt asks for, and nothing that it does not ask for.
7. **GRL-FR-BFPT** The review block tells the agent that a path changed outside the change folder and the check folder is a finding, and that `input.openspec_validation` is the validation of the tree it judges.
7. **GRL-FR-CDNI** Before each review turn of a propose run, the application runs `validate_change` on the review checkout (per `../core/OSM-openspec-mode.md` OSM-FR-APVF) and carries the result as `openspec_validation` in the task input. The review turn needs no CLI command for it (GRL-FR-RTNC).
7. **GRL-FR-HSZD** A propose review that answers `ready`, or that ends the loop as `ready` under GRL-FR-UQNV, while `openspec_validation` is not valid, is replaced with a `revise` verdict. That verdict holds one finding of severity `critical` that names the change, and the loop treats it as any `revise` verdict.
   - *Why:* A change that OpenSpec cannot validate cannot be applied or archived, so no reviewer may accept it.
7. **GRL-FR-KDYL** A validation or an apply-progress read that cannot run before a review, because OpenSpec mode is inactive, the CLI is not usable, or the read failed, dispatches no review turn. The run rests `blocked` with the code `openspec_validation_unavailable` (per `../core/GRD-graduation.md` GRD-FR-UJII).
7. **GRL-FR-MXXA** Every turn of an OpenSpec run carries `openspec` in its task input, with its phase (`propose`, `apply`, or `rework`), the change name, and, for a rework run, the author's instruction as `instruction`. `graduation_input_version` stays `1`.
7. **GRL-FR-EUSF** An apply run's work turn carries `work.md` followed by the work block of `openspec-apply.md`, with the change name filled in. The block names the tasks of the change as the task. It tells the agent to read them with `openspec instructions apply --change <name> --json`, to implement them in order, and to mark each one done in `tasks.md`.
7. **GRL-FR-SZQK** The apply work block tells the agent to change no file of the change folder other than `tasks.md`. Where the proposal, the deltas, the design, or the tasks are wrong, or cannot be built as they are written, it tells the agent to give a plan update (per `../core/OPU-openspec-plan-updates.md` OPU-FR-YBLE) and not to edit them.
7. **GRL-FR-OEFM** The apply work block tells the agent how to give a plan update: write each planning file it proposes, whole, at `.synthesis-plan/openspec/changes/<name>/<file>`, then answer `escalation_required` with `kind: "plan_update"`, its reason, and 0 to 8 questions. It says that a question not about the plan is an ordinary escalation.
7. **GRL-FR-NZMK** The apply review block tells the agent to give a finding `kind: "plan"` where the proposal, the deltas, the design, or the tasks are wrong or cannot be built as written, and `kind: "work"` otherwise (per `../core/OPU-openspec-plan-updates.md` OPU-FR-EPXL).
7. **GRL-FR-NWSC** An apply run's review turn carries `review.md` followed by the review block of `openspec-apply.md`. The block tells the agent to verify the work against the change for **completeness** (every task done and every delta requirement implemented), **correctness** (the code does what each requirement and scenario states), and **coherence** (the code follows the design).
7. **GRL-FR-YFGA** The apply review block maps the levels of a verification issue to finding severities: CRITICAL to `critical`, WARNING to `major`, and SUGGESTION to `minor`. It tells the agent that `input.openspec_apply_progress` and `input.openspec_validation` describe the tree it judges, and that it runs no check (GRL-FR-RTNC).
7. **GRL-FR-BHGJ** Before each review turn of an apply run, the application runs `apply_instructions` (per `../core/OSM-openspec-mode.md` OSM-FR-QLRW) and `validate_change` on the review checkout, and carries the results as `openspec_apply_progress` and `openspec_validation` in the task input. A read that cannot run rests the run by GRL-FR-KDYL.
7. **GRL-FR-EQDL** An apply review that answers `ready`, or that ends the loop as `ready` under GRL-FR-UQNV, is replaced with a `revise` verdict of one `critical` finding that names the cause where `openspec_apply_progress` holds a task that is not done, where `openspec_validation` is not valid, or where the change set changes a file of the change folder other than `tasks.md`.
7. **GRL-FR-CWSA** A rework run's work turn carries `work.md` followed by the work block of `openspec-rework.md`, and its review turn carries `review.md` followed by the review block of `openspec-rework.md`. The blocks state what the propose blocks state (GRL-FR-FYKL, GRL-FR-APHV, GRL-FR-BFPT), with the change as existing work to revise and `input.openspec.instruction` as the task.
7. **GRL-FR-HSNY** Before each review turn of a rework run, the application runs `validate_change` on the review checkout and carries the result as `openspec_validation`, on the terms of GRL-FR-CDNI. The gate of GRL-FR-HSZD and the rest of GRL-FR-KDYL apply to a rework run as to a propose run.
7. **GRL-FR-XNBQ** A rework review that answers `ready`, or that ends the loop as `ready` under GRL-FR-UQNV, is replaced with a `revise` verdict of one `critical` finding that names the cause where the change set changes a path outside the change folder and the check folder, or deletes a file of the change folder.
   - *Why:* The finish turns the change folder alone into a proposal (per `../core/OCR-openspec-change-runs.md` OCR-FR-ZNJT), so any other write would be lost and a deletion has no hunk.
8. **GRL-FR-OTRH** `review.md` is the instruction every review turn carries. The turn is dispatched into a **fresh agent session every time**, carrying no session of any earlier turn. A correction turn of GRL-FR-QKDC alone resumes the session of the review turn it corrects. `review.md` carries the milestone block of GRL-FR-KWPT.
9. **GRL-FR-YKRI** A review turn changes nothing. It stands in a **throwaway checkout** at `short_data_dir()/g/<run-id>/rv-<nonce>/`, detached at the run's base commit and holding the run's change set as uncommitted work. Git registers it as `<run-id>-rv-<nonce>` on the scratch branch `synthesis/review/<run-id>`. The checkout, its registration, and the scratch branch all go when the turn ends.
9. **GRL-FR-BIQX** Each review and `merge_review` turn draws a new `<nonce>` of 8 random lowercase hexadecimal digits for its checkout. A nonce whose directory or registration stands already is drawn again. So no turn stands in a path that an earlier turn of the run used.
   - *Why:* Docker Desktop can keep a deleted host path as "does not exist" for minutes after the path is made again, and every launch that mounts it fails.
9. **GRL-FR-PQLM** Before it makes a review checkout, and when the turn ends, the loop removes every review checkout of the run: each registration named `<run-id>-rv` or `<run-id>-rv-<nonce>`, and each directory `rv` or `rv-<nonce>` under the run home. A registration whose recorded path is not directly under the run home is pruned, and its directory stays.
9. **GRL-FR-LYXG** A review checkout that the loop cannot remove before a turn is logged as a warning. The loop makes the new checkout all the same.
9. **GRL-FR-XJJU** At the first read of a project's queue, the application removes the review checkouts of GRL-FR-PQLM for each run that no process drives and that is not in state `reviewing`.
9. **GRL-FR-QEUP** A correction turn of GRL-FR-QKDC within one dispatch stands in the checkout of the turn it corrects. A correction that is pending across Continue (GRL-FR-HWLD) stands in a new checkout. Where the vendor then does not honour the resume, GRL-FR-PTCE settles the first turn.
   - *Why:* A reviewer that shared the stream could change it, and that change could not be told apart from the work it judges.
10. **GRL-FR-CKBL** The review turn judges four things on every pass: whether what the prompt asked for is delivered, `hidden_paths` included; whether the check report of GRL-FR-XQMT shows that every check the project requires passed (GRL-FR-PNLQ); whether the work follows the project's conventions and test practice; and whether it introduces a security problem. The **delivery** decision is made against the work turn's own account as well as against the change set: `review.md` names the account among the material the turn is given, says it is the work turn's words rather than evidence, and requires what it claims to be checked against what the change set actually holds. An account that reports part of the prompt left undone is therefore read by the one turn whose decision it settles.
    - *Why:* Delivery is the decision that turns on what a reader knows. A turn that says it stopped early is the strongest evidence a review can have that delivery is incomplete, and it costs nothing to read — checked rather than believed, because the account is a claim and the working copy is what says whether it is true (GRL-FR-NVBZ), so an agent that under-reports its own work has no finding raised against work it actually did. A file an ignore rule hides is the same decision from the other side: it is absent from the change set, so no review reads it and no commit writes it, and only a reader tells a build directory from an authored module.
11. **GRL-FR-XNQU** The tree the review turn stands in is the tree a `ready` verdict commits. It is derived once, and both use that one derivation.
    - *Why:* Two derivations let a reviewer judge one tree while another lands, and nothing downstream can detect that.
12. **GRL-FR-VIAT** The review returns a `ReviewVerdict`: `verdict` of exactly `ready` or `revise`, a non-empty `rationale`, and `findings`. A `ready` verdict carries no finding and a `revise` verdict carries at least one.
12. **GRL-FR-FRNJ** A `ready` verdict carries an optional `summary`, the run summary: a `title` of one non-blank line and a non-blank Markdown `description` of the run's work. `review.md` asks for it and directs the agent to the repository's pull request template, skills, and agent instructions for its shape. A `revise` verdict carries no summary.
12. **GRL-FR-ECES** The loop reads the verdict without the summary and then reads the summary on its own. A summary that is absent, blank in a field, or of another shape is dropped, and the verdict, its correction count, and the run's next step do not change.
13. **GRL-FR-TVXI** A verdict the application cannot read is refused whole, and a further **review** turn is asked **once**, carrying the `review_result` correction of `refusals.md`. No work turn is composed. A second consecutive unreadable verdict rests the run `blocked` with the code `review_verdict_invalid`, and the author's Continue resets the count.
13. **GRL-FR-QKDC** A work, review, `merge_work`, `merge_review`, or `pr_summary` turn that ends in `invalid_structured_output` with a correctable refusal and a reported session, on a vendor that resumes, is asked **once** more in that session and execution directory, carrying the `refusals.md` correction for that refusal. This **answer correction** does not settle the first turn.
   - *Why:* The agent finished its work and only the answer failed, so the run must not stop on a fault the agent can repair.
13. **GRL-FR-MZHT** The correctable refusals, each with its `refusals.md` section, are `not_found` (`answer_missing`), `malformed` and `protocol_version` (`answer_malformed`), `multiple_documents` (`answer_several`), `summary_empty` (`answer_summary_empty`), `exclusivity` (`answer_outcome_mismatch`), `escalation_shape` (`answer_escalation_shape`), and `too_large` (`answer_too_large`). No other refusal is correctable.
13. **GRL-FR-XSFA** An answer correction says that the work stands and that the turn changes no file and sends its answer again; a check correction lets the turn change files. Each states its rules, names no failed value, and holds no agent text. `work.md`, `review.md`, `merge-work.md` and `merge-review.md` tell a turn to act on `correction`, and the two work files on `check_correction`.
13. **GRL-FR-JVNB** A turn's answer is corrected at most once. A second refused answer rests the run on `unreadable_answer`, and that stop commits the work of both turns in one commit.
13. **GRL-FR-PTCE** No answer correction turn is dispatched where the run is cancelled, a merge branch has moved, or no total time is left: the first turn then settles as it ended. Where the vendor does not honour the resume, the first turn also settles as it ended, and the answer of the correction turn is discarded.
13. **GRL-FR-HWLD** The loop saves the pending correction in the checkpoint's `answer_correction`, and clears it only when a correction turn has ended with an answer. Otherwise it stays, also after a launch error, a stop, or no dispatch, so the next turn of that part after Continue is the correction. A work turn that runs first drops a pending review correction.
13. **GRL-FR-NSEY** A correction turn counts as one turn of its part and is charged to the time limits. It changes neither the pass nor the pass history, and it does not raise the count of GRL-FR-TVXI.
14. **GRL-FR-UQNV** A `revise` verdict of two findings or fewer, all of severity `minor`, ends the loop as `ready` does. The findings are recorded as advisory remarks and no further work turn is composed. This rule does not apply to a merge review (GRL-FR-MRVK), or to a verdict that holds a `plan` finding (per `../core/OPU-openspec-plan-updates.md` OPU-FR-FDCW).
15. **GRL-FR-GQAB** A `revise` verdict that starts another pass carries its findings whole into the next work turn's instruction, in the order the review returned them.
16. **GRL-FR-NVBZ** The loop never treats an agent response envelope as evidence about the filesystem. It reads from the envelope the outcome the agent reported, the account of GRL-FR-DNKA, and the check record of GRL-FR-IJTT, and it reads what changed from the working copy. The account reaches the review as a **claim**, never as a finding of fact.
17. **GRL-FR-DNKA** A work turn's own **account** of what it did and did not do, its success summary, goes to the review that judges what it wrote, on every turn the check gate accepts (GRL-FR-XWNQ). The review turn is told that the account is there and whose words it is (GRL-FR-CKBL). No work turn is given an account, in any pass.
   - *Why:* The account describes the tree as its own turn left it, so the review checks it against the change set, and a later work turn would misread it as the tree in front of it.
18. **GRL-FR-VBCL** An `escalation_required` response ends the turn without a review, and a merge turn's escalation is handled on the same terms (GRL-FR-MWPQ). The run rests in `awaiting_author` carrying the agent's reason and its ordered questions. The answers go into the phase that asked: a work turn's answers into the next work turn, and a review turn's answers into a fresh review turn, with no work turn composed. Each answer also enters the run's decision ledger (GRL-FR-BNDD).
19. **GRL-FR-ISIL** A run handed back after an interruption dispatches its next turn with `purpose = "resume"`, carrying the persisted checkpoint. Resuming is the loop's ordinary path rather than a special one.
20. **GRL-FR-ZDKP** Cancellation reaches every turn through **one path**: the token `../tools/EAC-execute-agent-cli.md` EAC-FR-25 honours. Nothing is judged, and what the turn already wrote is left in the working copy. `../core/GRD-graduation.md` GRD-FR-SWOJ commits it, except for a run that was discarded (GRD-FR-EWTN).
21. **GRL-FR-BRJO** The loop writes no file of the project, of the stream or of the draft, and it creates no commit. What a turn wrote is committed by `../core/GRD-graduation.md` on the host, after the turn ends.
22. **GRL-FR-REPL** The pass budget, the stage time limit and the total time limit are **validated project settings** (per `../core/PSS-project-settings-storage.md` PSS-FR-QBNE, PSS-FR-HVSM, PSS-FR-RKTW). An unset one takes the default of two passes, three hours or five hours. A merge run's pass budget is the constant of GRL-FR-CZBT. The blocker and verdict-refusal bounds are constants of this module.
    - *Why:* No phase here reaches a model, so the shared provider-call deadline bounds nothing of this loop and is the conversation loop's alone.
22. **GRL-FR-NUDC** The loop does not read the shared execution timeout or the shared retry budget (`../core/PSS-project-settings-storage.md` PSS-FR-TQMV, PSS-FR-WPKS). A value of either that the project stores changes no graduation turn and no graduation run.
23. **GRL-FR-KWNP** The three settings are read **before every dispatch**, and the turn runs under the values read. A merge run turn reads the two time limits alone (GRL-FR-CZBT). The semantic rebase turn of `../core/GRB-graduation-rebase.md` reads the stage time limit alone. A run resumed after an interruption therefore uses the values active at each resumed dispatch rather than the values its earlier turns ran under.
    - *Why:* A value cached at the start of a run makes an author's correction take effect only for the next run, which is the run they were not waiting on.
24. **GRL-FR-XBUE** A run whose pass budget is spent rests in `awaiting_author` carrying the reason `pass_budget_exhausted`, the budget it spent, and the review findings that stand. The run keeps its queue and its project slot, the checkpoint is kept whole, and the author's Continue resets that budget alone and resumes without repeating a completed pass (per `../core/GRD-graduation.md` GRD-FR-CYIB). This requirement states the pass budget of a run that is not a merge run; GRL-FR-CZBT states a merge run's own.
24. **GRL-FR-EFTM** Each turn of a run is dispatched with an **effective timeout**: the smaller of the stage time limit and the run's **remaining total time**. The remaining total time is the total time limit less the used total time of `../core/GRD-graduation.md` GRD-FR-UTTX, and it is never below zero.
24. **GRL-FR-WZTE** A turn that reaches the stage time limit while the remaining total time is larger rests the run `interrupted` with `execution_timeout`. A turn that reaches its effective timeout while the remaining total time is not larger than the stage time limit is stopped at once. The run then rests in `awaiting_author` with the reason `total_time_limit`.
24. **GRL-FR-WOBK** A run whose remaining total time is zero before a dispatch, for example after the author lowered the total time limit, dispatches no turn and rests in `awaiting_author` with the reason `total_time_limit`.
24. **GRL-FR-JNMI** A run at rest on `total_time_limit` keeps its queue and its project slot, keeps its checkpoint whole, and carries the limit and the used total time. What its stopped turn wrote is committed as for any cancelled turn (per `../core/GRD-graduation.md` GRD-FR-SWOJ). A run stopped in its review resumes at the review.
24. **GRL-FR-SPRE** The author's Continue from `total_time_limit` resets the used total time alone and resumes without repeating a completed pass (per `../core/GRD-graduation.md` GRD-FR-SKLO). It does not reset the pass budget. Continue from `pass_budget_exhausted` does not reset the used total time.
24. **GRL-FR-NVGQ** An answer to an escalation that the backend accepts resets the used total time (per `../core/GRD-graduation.md` GRD-FR-UTTX) and clears any rest. The turn that takes the answers is dispatched under the whole total time limit. The answer does not reset the pass budget.
   - *Why:* a run that asks late in its total time would otherwise stop again almost at once after the author answers.
25. **GRL-FR-UVJP** Every phase emits through `../core/LGC-logging.md`'s internal API under the `ai` and `backend` domains together, correlated by the run id, so one run's whole loop reads back as one sequence. Neither the assembled request nor the exchange it grew into is persisted.
26. **GRL-FR-GIMN** Every requirement here is exercisable on a machine with no network, no provider credential and no container runtime, through the dispatch seam and the completion seam alone.
27. **GRL-FR-QZFB** A second consecutive blocker with the same code rests the run `awaiting_author` and keeps the blocker on the run. A blocker with a different code rests the run `blocked` and starts the count again. Continue from `blocked` keeps the count; Continue from `awaiting_author` starts it again.
    - *Why:* Continue from `blocked` is the retry the count counts, so a reset there would make the bound unreachable and leave the run continuing into the same fault for ever.

27. **GRL-FR-UNKD** A PR summary run has one turn of part `pr_summary` and turn kind `pr_maintenance`, dispatched through `../core/GXD-graduation-execution.md`'s `dispatch_graduation_turn` with the result contract `pull_request_summary`. It has no review, no pass budget, and no escalation answer. It reads the two time limits alone, on the terms of a merge run turn (GRL-FR-KWNP).
27. **GRL-FR-ZZGO** The answer of a `pr_summary` turn is a `PullRequestSummary`: a `title` of one non-blank line and a non-blank Markdown `description`. An unreadable answer is corrected once (GRL-FR-QKDC). A second unreadable answer, a failure, or an escalation ends the turn without a result (per `../core/GRD-graduation.md` GRD-FR-SWHW).
27. **GRL-FR-TZKR** `pr-summary.md` directs the agent to the repository's pull request template, skills, and agent instructions for the title and the description, and states that the turn changes no file. In `diff` mode the task is the change from the merge base to the head tip. In `combine` mode the task is one description made from the run summaries the input holds.
27. **GRL-FR-SNHR** The task input of a `pr_summary` turn carries `pr_summary` with the mode, the head and the base, the pinned head tip and merge base, and, in `combine` mode, the captured run summaries in landing order. Its `draft_name`, `prompt`, and `stream_name` are empty.
28. **GRL-FR-MWPQ** One **merge pass** is a `merge_work` turn followed by a fresh `merge_review` turn. The loop dispatches each through `../core/GXD-graduation-execution.md`'s `dispatch_graduation_turn` with turn kinds `merge_work` and `merge_review`, and dispatches the review turn on a completed work turn over what the merge worktree then holds. A `ready` merge review ends the loop and hands the run to the apply of `../core/GRD-graduation.md` GRD-FR-AQNW. A `revise` merge review starts the next merge pass while the budget holds one (GRL-FR-CZBT). A merge turn that escalates ends the turn without a review, and the answers go into the phase that asked (GRL-FR-VBCL). A merge run composes no `work` turn, no `review` turn and no `semantic_rebase` turn. The rules of GRL-FR-VIAT, GRL-FR-TVXI, GRL-FR-GQAB, GRL-FR-NVBZ, GRL-FR-ZDKP and GRL-FR-BRJO hold for merge turns, with the merge review as the review turn and the merge work turn as the work turn.
29. **GRL-FR-SLQF** `merge-work.md` is the instruction every `merge_work` turn carries, whole and unaltered, identical for every merge run and every project. It names the unresolved paths and the conflicts of the task's `merge` context as the task, directs the agent to read both pinned branch versions and the repository's own instructions and specifications, and permits the agent to change any other path that the reconciliation of the merge needs. It requires that no conflict marker stand in a resolved path. It states that the review findings of the earlier pass arrive in the task's `loop_instruction`. It carries the milestone block of GRL-FR-KWPT.
30. **GRL-FR-MRVK** `merge-review.md` is the instruction every `merge_review` turn carries. The turn is dispatched into a **fresh agent session every time**, a correction turn of GRL-FR-QKDC excepted, and stands in a throwaway review checkout (per GRL-FR-YKRI) derived from the merge worktree, with the merge snapshot commit as its base commit. The merge review judges the final result against both pinned branch versions and against the repository's specifications, for every path in `changed_paths` of the merge context, Git-clean paths included, and for every path in `reconciled_paths`. It runs no check command (GRL-FR-RTNC) and changes nothing. It answers `ready` only when its merge-intent checks and its specification checks pass and the check report of the `merge_work` turn shows that every required check passed (GRL-FR-PNLQ). It answers `revise`, with actionable findings, otherwise. The advisory-minor rule of GRL-FR-UQNV does not apply to it: every `revise` verdict of a merge review starts another merge pass or rests the run for the author. It carries the milestone block of GRL-FR-KWPT.
31. **GRL-FR-CZBT** A merge run has its own pass budget of **2** passes. The budget is a constant of this module and is not read from the project's settings. A merge review that answers `revise` when the budget is spent rests the run in `awaiting_author` carrying the reason `pass_budget_exhausted`, the budget it spent, and the review findings that stand, and keeps the stream and the project slot (per `../core/GRD-graduation.md` GRD-FR-BNTC). `continue_graduation_run` adds 2 passes, keeps the merge worktree and the merge snapshot, and resumes without repeating a completed pass (per `../core/GRD-graduation.md` GRD-FR-CYIB and `../core/GXD-graduation-execution.md` GXD-FR-BQLN). Each turn of a merge run is bounded by the project's stage time limit and total time limit, read before every dispatch (GRL-FR-KWNP, GRL-FR-EFTM), and a merge run rests on `total_time_limit` on the terms of every run (GRL-FR-WZTE).
32. **GRL-FR-NDHW** A `ready` merge review whose merge worktree still holds a conflict marker in a path Git could not merge is not applied. The application replaces the verdict with a `revise` verdict that holds one finding of severity `critical`, which names that path, and the loop treats it as any `revise` verdict of a merge review (GRL-FR-MRVK).
32. **GRL-FR-ZDNL** Before each `merge_review` turn of a merge run, where OpenSpec mode is active, the application runs `validate_project` with the scope `specs` on the review checkout (per `../core/OSM-openspec-mode.md` OSM-FR-BXIF) and carries the result as `openspec_specs_validation` in the task input. Where the mode is not active, the input carries none.
32. **GRL-FR-ZXOE** `merge-review.md` tells the agent that `input.openspec_specs_validation`, where the input carries it, is the strict validation of the OpenSpec specifications of the tree it judges, and that a specification that is not valid is a finding.
32. **GRL-FR-ZFMZ** A `ready` merge review while `openspec_specs_validation` is not valid is not applied. The application replaces the verdict with a `revise` verdict of one `critical` finding that names the first capability that is not valid, and the loop treats it as any `revise` verdict of a merge review (GRL-FR-MRVK).
    - *Why:* The archive of `../core/OAR-openspec-archive.md` merges deltas into the live specifications on the stream, so a conflict merge can join two edits of one capability into a specification that OpenSpec refuses.
32. **GRL-FR-AGOA** A specs validation that cannot run before a merge review while OpenSpec mode is active, because the CLI is not usable or the read failed, dispatches no review turn. The run rests `blocked` with the code `openspec_validation_unavailable` (per `../core/GRD-graduation.md` GRD-FR-UJII).
33. **GRL-FR-TXEB** Every turn of a merge run carries the `merge` context of `MergeTaskContext` in its task input, and its `part` is `merge_work` or `merge_review`. The context holds the stream branch, the base branch, the pinned tips, the merge base, the snapshot commit, the changed paths, the unresolved paths and the conflicts of the run's merge data. A `merge_review` turn's context also holds `reconciled_paths`, the paths that the work turns changed beyond the unresolved paths. `graduation_input_version` stays `1`. The `draft_name` of a merge run's task is empty, and its `prompt` is the one-sentence statement of the merge.
35. **GRL-FR-BNDD** Every turn of every kind carries the run's decision ledger as `decisions` (per `../core/GXD-graduation-execution.md` GXD-FR-NWSL). The instruction of each kind (`work.md`, `review.md`, `merge-work.md`, `merge-review.md` and `rebase.md`) says that these decisions are binding. The agent must not contradict a recorded decision and must not ask the author again about one. The application does not refuse an escalation that repeats a question.
36. **GRL-FR-VHIS** The instruction of a work turn (`work.md`, `merge-work.md`) says that the findings in `input.review_history` come from earlier passes and are probably already fixed. They are context and not new work: the agent must not redo a fix and must not undo it. A review turn gets no review history and judges the working copy as it is.
37. **GRL-FR-RHST** The loop appends the findings of every `revise` verdict to the run's review history (per `../core/GXD-graduation-execution.md` GXD-FR-WRVH), in the order the review returned them, each with the pass of its review. A merge review adds its findings on the same terms, and so does the `revise` verdict of GRL-FR-NDHW.
38. **GRL-FR-RTNC** A `review` or `merge_review` turn runs no check command: no test, build, lint, type check, specification check, or dependency install. `review.md` and `merge-review.md` say this. It is an instruction only, and the application does not inspect or refuse the commands the agent runs.
   - *Why:* The work turn already ran every check, and a review checkout has no build cache, so each check in a review costs a cold build and repeats a result the report holds.
39. **GRL-FR-NTFD** A check report that is `missing`, lacks a check the project requires, records a failed, timed-out, or skipped check that no author decision permits (GRL-FR-HLNB), or disagrees with its logs is a finding of a `review` or `merge_review` turn. A test that is absent is not a finding by itself: the review judges test coverage by reading the change set.
40. **GRL-FR-XQMT** The **check folder** is `.synthesis-checks/` at the root of the working copy a `work` or `merge_work` turn stands in. It holds the **check report** `report.md` and the directory `logs/`. The check folder is not a file of the project.
41. **GRL-FR-LXWU** Before the first turn of a run, the application makes sure that the repository's `info/exclude` file holds the line `/.synthesis-checks/`. It adds the line where the file does not hold it, and it adds no second copy. It writes no `.gitignore` and no tracked file.
   - *Why:* `info/exclude` is never committed or pushed, so no step has to remove it before a push.
42. **GRL-FR-DCKG** The change set, `hidden_paths`, and every commit a run makes leave out the check folder, also where the line of GRL-FR-LXWU is absent.
43. **GRL-FR-KYYW** Before it dispatches a `work` or `merge_work` turn, the application removes the check folder of that turn's working copy, so the folder holds no result of an earlier turn. It keeps the folder before a turn that carries `correction` or `check_correction`, which continues the record of the turn before it.
44. **GRL-FR-AMQE** Before it dispatches a `review` or `merge_review` turn, the application copies the check folder of the working copy into the same path of the review checkout (GRL-FR-YKRI). Where the working copy has no check folder, the review checkout has none.
45. **GRL-FR-FESZ** The task input of a `review` or `merge_review` turn carries `checks`: the path `.synthesis-checks/report.md` where the review checkout holds that file, and the value `missing` where it does not. No other turn carries `checks`.
46. **GRL-FR-VDCP** `work.md` and `merge-work.md` tell the agent to run narrow checks while it works, and the project's full checks once after its last edit, each full check as a command of its own. The agent writes the output of each full check to `.synthesis-checks/logs/<name>.log`.
46. **GRL-FR-CNMP** As soon as a full check ends, the agent adds its entry to `.synthesis-checks/report.md`: command, exit status, start and end time, and summary line. The entry of a check that did not end records it as not finished, with its start time.
   - *Why:* a turn that stops before its last check still leaves the entry of every check that ended, so the review can name the one check that is missing.
47. **GRL-FR-MQFB** `work.md` and `merge-work.md` tell the agent to run a long command in the foreground with the longest timeout its tool allows, and never to wait for a background job with a sleep or poll loop. They say that the turn ends at the answer, and that nothing reports the end of a background job after it.
   - *Why:* a job that still runs at the answer stops with the turn, so its result is lost and the review finds no record of it.
47. **GRL-FR-ZHHX** The agent does not wait for a command that its tool moved to the background. It stops that command, records its check as not finished (GRL-FR-CNMP), and names that check in its account.
48. **GRL-FR-PNLQ** `review.md` and `merge-review.md` tell the agent to read the check report named by `input.checks` and its logs, and to verify that the report names every check the project's own instructions require for the change set, that each exit status is zero or is waived by the author decision its entry names, and that each summary line agrees with its log.
49. **GRL-FR-LKLE** No instruction file names a command of one project, such as a task name or a test runner. Each turn finds the project's checks in the project's own instructions.
50. **GRL-FR-QEOT** `review.md` and `merge-review.md` tell the agent that a correction about a check asks for that check to run in the foreground, and never asks the work turn to wait for a background job.
50. **GRL-FR-IJTT** Every `work` and `merge_work` turn is dispatched with the result contract `work_result`. Its success answer carries a `WorkResult`: `checks`, one `CheckRecord` for each full check the turn ran, with `name`, `command`, `finished`, `exit_status` and an optional `waived_by`; and `no_checks_reason`, the reason where the project requires no check.
50. **GRL-FR-JGBX** `work.md` and `merge-work.md` tell the agent that a turn ends with every check the project requires passed, or with an escalation to the author, and that a cause outside the change, such as the environment, is no exception. A turn that cannot finish answers `escalation_required`. A success lists each full check in `result.checks` as the check report records it.
50. **GRL-FR-CPXI** The **check gate** refuses the success answer of a `work` or `merge_work` turn with the first cause that applies: `record_missing` where `result` is not a valid `WorkResult`, or holds no check and no non-blank `no_checks_reason`; `checks_unfinished` where a check is not finished; `checks_red` where a check has an exit status other than 0 and no valid `waived_by`.
   - *Why:* Only the author can accept work whose checks did not pass, so no fault outside the change, such as the environment, lets a turn report success over a red check.
50. **GRL-FR-TYRV** The check gate refuses a `failure` answer of a `work` or `merge_work` turn with the cause `work_failed`. Such a turn goes to no review.
   - *Why:* A work turn that cannot finish asks the author through an escalation, and a failure that went to review would end a pass on work that nobody can accept.
50. **GRL-FR-XWNQ** The check gate reads the answer of every completed `work` and `merge_work` turn, on every vendor. It never refuses an `escalation_required` answer whose escalation is recorded, and it refuses one whose escalation is not recorded with the cause `record_missing`. It reads the record as the agent's claim and runs no check. An accepted success goes to the review turn.
50. **GRL-FR-HLNB** `waived_by` is the 1-based position of an entry in the turn's input `decisions`. The check gate accepts a red check that carries `waived_by` only where that entry exists. The review judges whether the author's answer in that entry permits the check to fail (GRL-FR-NTFD).
50. **GRL-FR-WRHT** `work.md` and `merge-work.md` tell the agent to set `waived_by` only to cite a decision of `input.decisions` in which the author lets that check fail, and to name the same position in the entry of that check in the check report.
50. **GRL-FR-CHDT** Where the check gate refuses an answer, the loop dispatches one **check correction** turn of the same part. It carries `check_correction`, the fixed `refusals.md` text of the cause: `checks_not_passing` for `checks_unfinished` and `checks_red`, `checks_record_missing` for `record_missing`, and `work_failed` for `work_failed`. The turn may change files.
50. **GRL-FR-SMQK** A check correction turn resumes the session that the refused turn reported, in the same working copy. Where no session was reported, or the vendor does not resume, it runs as a fresh turn in that working copy. Its effective timeout is read before its dispatch (GRL-FR-EFTM).
50. **GRL-FR-IPNO** A task input carries `check_correction` only on the part `work` or `merge_work`, and never beside `correction`. An input that breaks this rule is invalid before dispatch.
50. **GRL-FR-LAVF** The loop dispatches at most one check correction in each **gate episode**. An episode starts with each `work` or `merge_work` turn that the gate did not ask for: the first work turn of a pass, the turn that takes escalation answers or the decisions of a plan update, and the turn that Continue from `checks_not_passed` dispatches (GRL-FR-FGWT).
50. **GRL-FR-SUBP** Where the check gate refuses the answer of the check correction, the loop dispatches one **escalate correction** turn of the same part, on the session terms of GRL-FR-SMQK. It carries the `checks_escalate` text of `refusals.md` as `correction`: the turn changes no file, may not report success, and answers `escalation_required` with its questions for the author.
50. **GRL-FR-BOHV** A check correction turn and an escalate correction turn each add one to `work_turns` and are charged to the time limits. Neither changes the pass or the pass history, and neither counts against the bound of GRL-FR-JVNB.
50. **GRL-FR-FGWT** An escalate correction turn whose answer is not a recorded escalation, a green success included, rests the run in `awaiting_author` with the reason `checks_not_passed`, the cause of the last refusal, and each check of the last record that did not pass (per `../core/GRD-graduation.md` GRD-FR-PYJN). No review runs, and nothing is committed.
50. **GRL-FR-RXJF** An apply run's plan update and plan findings follow `../core/OPU-openspec-plan-updates.md`. A plan update rests the run by OPU-FR-RMZN in place of GRL-FR-VBCL, and the check gate does not read it (OPU-FR-MBIY). A verdict that holds a `plan` finding is routed by OPU-FR-FDCW in place of GRL-FR-ARPX and GRL-FR-GQAB.
50. **GRL-FR-QSMY** `refusals.md` holds the `plan_update_invalid` and `plan_correction` texts (per `../core/OPU-openspec-plan-updates.md` OPU-FR-XSRD, OPU-FR-DTYE). Each tells the turn to change no file outside `.synthesis-plan/` and to answer a plan update. `plan_correction` names `loop_instruction` as the plan findings; `plan_update_invalid` states the rules of OPU-FR-XLVF.
50. **GRL-FR-BVTS** The loop saves `check_gate` in the checkpoint, with the cause, the next step (`correct` or `escalate`) and the session to resume, before it dispatches that step. Continue after a pause, a shutdown or an interruption dispatches the saved step. Where no total time is left, the run rests on `total_time_limit` and keeps `check_gate`.
50. **GRL-FR-LNIZ** The loop clears `check_gate` when the check gate accepts an answer, when the backend accepts the answers to an escalation, and when Continue from `checks_not_passed` dispatches its turn. An escalation raised while the checkpoint holds `check_gate` goes to the author whole (per `../tools/ESU-escalate-to-user-tool.md` ESU-FR-18).
34. **GRL-FR-OPRK** A model downgrade is checked only on a call of this loop to an AI API provider through the completion seam. Such a call compares the two ids as `../core/AGC-agent-conversations.md` AGC-FR-BXOS says, and the run emits `"model downgrade detected"` once for each pair, with `source` `graduation` and the run id. A turn of an agentic CLI is not checked.
50. **GRL-FR-KWPT** `work.md`, `review.md`, `merge-work.md`, and `merge-review.md` each carry one **milestone block**. It tells the agent to write the plan marker and the start marker of milestone 1 in its first text message, before any other work. It tells the agent to write the done marker of the last milestone before it reports.
50. **GRL-FR-LCBJ** The milestone block tells the agent that each move to the next milestone is one text message with the done marker of the finished milestone and the start marker of the next one. The agent writes it before the work of the next milestone starts. Before each progress message, the agent makes sure the markers show its current milestone.
50. **GRL-FR-PCRD** The milestone block tells the agent that a start marker marks every earlier milestone done. Where earlier work is already done, for example by an earlier call of the same pass, the agent writes the start marker of the milestone it is in.
50. **GRL-FR-BZMH** The milestone block asks for a plan of 3 to 5 high-level milestones. It tells the agent to write each marker on its own line in a text message and never inside the final result document.
50. **GRL-FR-DJPE** The milestone block gives the exact marker format of `../tools/EAC-execute-agent-cli.md` and the title limits of EAC-FR-VCBJ. Its one example is the full marker sequence of a plan of 3 milestones: the plan, then the start marker and the done marker of each milestone in plan order.
51. **GRL-FR-NDRY** The milestone block names no deliverable (GRL-FR-DAIB). It says that the author depends on the markers to see progress, and that the agent writes every marker. It says that markers are for display only, that the application ignores a bad marker, and that a marker changes no state, no stage, and no verdict.
52. **GRL-FR-LCSB** An answer correction turn (GRL-FR-QKDC), an escalate correction turn (GRL-FR-SUBP), a plan update correction turn, and a plan correction turn (per `../core/OPU-openspec-plan-updates.md` OPU-FR-XSRD, OPU-FR-DTYE) carry their instruction without the milestone block, and `refusals.md` carries none. A check correction turn carries the block. `rebase.md` and `pr-summary.md` carry no block, so a `semantic_rebase` turn and a `pr_maintenance` turn announce no milestones.

## Contract surface

### The phases
```text
1  work     execution agent   work.md     → WorkResult | escalation (GRL-FR-IJTT)
2  review   execution agent   review.md   → ReviewVerdict | escalation
```

A merge run holds two further phases in its own pass (GRL-FR-MWPQ):

```text
1  merge_work     execution agent   merge-work.md     → a resolved merge worktree; WorkResult | escalation
2  merge_review   execution agent   merge-review.md   → ReviewVerdict | escalation
```

A PR summary run holds one phase and no pass (GRL-FR-UNKD):

```text
1  pr_summary     execution agent   pr-summary.md     → PullRequestSummary
```

The answer of a `work` or `merge_work` turn passes the check gate before any review (GRL-FR-CPXI):

```text
answer ─ accepted, or escalation ──────────────────────→ review, or awaiting_author
       └ refused → check correction ─ accepted ─────────→ review
                                     └ refused → escalate correction ─ escalation → awaiting_author
                                                                      └ other ─────→ awaiting_author, checks_not_passed
```

A turn exists outside both passes, at stream update alone: the **semantic rebase** turn under `rebase.md`, dispatched by `../core/GRB-graduation-rebase.md` for a conflict Git cannot settle.

### The task input
Every turn carries a `GraduationTaskInput`, strictly composed here and strictly validated before dispatch:

```
GraduationTaskInput {
  graduation_input_version,   // the integer 1
  run_id,
  purpose,                    // "generate" | "revise" | "resume" | "escalation_answer"
  draft_name,
  prompt, prompt_checksum,    // the captured prompt, whole
  stream_name,
  pass,                       // 1 .. the pass limit of the run's budget window
  part,                       // "work" | "review" | "semantic_merge"
                              //   | "merge_work" | "merge_review" | "pr_summary"
  loop_instruction?,          // the review findings a revise pass carries; the plan findings of a
                              //   plan correction turn (OPU-FR-DTYE)
  review_history?,            // work and merge_work turns alone (GRL-FR-VHIS): the findings of
                              //   the passes before the latest review, [{ pass, finding }],
                              //   probably fixed already; the latest findings are loop_instruction
  decisions,                  // every turn kind (GRL-FR-BNDD): the run's whole decision ledger,
                              //   [{ question, answer, summary, note?, turn, pass }], in order
  changed_paths?,             // what the working copy holds against the base commit
  hidden_paths,               // what the ignore rules kept out of those paths; an
                              //   ignored directory is one entry with a trailing
                              //   separator, and at most 200 are named
  hidden_paths_omitted,       // how many more there were than the input names
  correction?,                // GRL-FR-TVXI, GRL-FR-QKDC, GRL-FR-SUBP: what a turn asked again is to send
  check_correction?,          // GRL-FR-CHDT: work and merge_work alone, never beside correction;
                              //   the refusals.md text of the check gate's cause
  merge?,                     // MergeTaskContext; on every turn of a merge run (GRL-FR-TXEB)
  pr_summary?,                // PrSummaryTaskContext; on the turn of a PR summary run (GRL-FR-SNHR)
  openspec?,                  // { phase: "propose" | "apply" | "rework", change, instruction? }; on every
                              //   turn of an OpenSpec run; instruction on a rework run alone (GRL-FR-MXXA)
  openspec_validation?,       // ChangeValidation of OSM; review turns of an OpenSpec run, and the turn that
                              //   Continue from a plan rest dispatches (GRL-FR-CDNI, GRL-FR-BHGJ,
                              //   GRL-FR-HSNY, OPU-FR-SKTV)
  openspec_apply_progress?,   // ApplyInstructions of OSM; review turns of an apply run, and the turn that
                              //   Continue from a plan rest dispatches (GRL-FR-BHGJ, OPU-FR-SKTV)
  openspec_specs_validation?, // ProjectValidation of OSM; merge_review turns in OpenSpec mode alone
                              //   (GRL-FR-ZDNL)
  plan_update?,               // PlanUpdateTaskContext of OPU; the turn that takes the decisions of a
                              //   plan update alone (OPU-FR-SKTV)
  checks?,                    // GRL-FR-FESZ: review and merge_review alone; the path
                              //   ".synthesis-checks/report.md", or "missing"
  agent_account?,             // GRL-FR-DNKA: the work turn's own account of
                              //   what it did and did not do, carried to the
                              //   review that judges what it wrote. A claim
                              //   the review checks, never a finding of fact
  escalation_context?,
  escalation_answers?
}
```

```
MergeTaskContext {
  stream_branch, base_branch,
  base_tip, stream_tip, merge_base, snapshot_commit,
  changed_paths,              // every path the Git merge changes against base_tip,
                              //   Git-clean paths and unresolved paths included
  unresolved_paths,           // the paths Git could not merge
  conflicts,                  // [{ path, base_change, stream_change }], each change one of
                              //   created | updated | deleted | unchanged
  reconciled_paths?           // merge_review only: paths the work changed beyond the unresolved ones
}
```

```
PrSummaryTaskContext {
  mode,                       // "diff" | "combine"
  head, base,
  head_tip, merge_base,
  inputs                      // [{ title, description }]; combine mode only
}
```

### Result shapes
```
ReviewVerdict {
  verdict,                    // "ready" | "revise"
  rationale,                  // never blank
  findings: [ReviewFinding],
  summary?                    // RunSummary { title, description }; a ready verdict only (GRL-FR-FRNJ)
}

WorkResult {                  // GRL-FR-IJTT; the success answer of work and merge_work
  checks: [CheckRecord],      // at most 64
  no_checks_reason?           // non-blank, at most 1024; where the project requires no check
}

CheckRecord {
  name,                       // non-blank, at most 128
  command,                    // non-blank, at most 1024
  finished,                   // false where the check did not end
  exit_status,                // integer, or null where the check did not end
  waived_by?                  // GRL-FR-HLNB: 1-based position in input.decisions
}

PullRequestSummary {
  title,                      // one line, never blank
  description                 // Markdown, never blank
}

ReviewFinding {
  severity,                   // "critical" | "major" | "minor"
  description,                // one problem, once
  affected_files,             // project-relative; may name a path that does not exist yet
  correction,                 // directly actionable by the next work turn
  kind?                       // "work" | "plan"; "work" where absent; "plan" from an apply review alone (OPU-FR-EPXL)
}
```

### Seams
- `dispatch_graduation_turn` (`../core/GXD-graduation-execution.md` GXD-FR-DRFF) — the only route to an execution agent.
- `complete(exchange, endpoint)` — the only route to a model, used by no phase of this loop directly.

## Non-functional requirements
- A pass costs at most two containers, so a run costs at most twice its pass budget.
- A review turn runs no check, so its cost is the reading of the change set, the specifications, and the check report.
- The review's checkout is created from a commit, and the change set is materialized into it from the repository's own object database. It costs no tree copy, and Git settles file modes, symbolic links and the ignore rules.
- No instruction text is composed in Rust: everything that varies per run travels in the structured task input.
