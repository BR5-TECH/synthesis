# Graduation loop

**Spec code:** `GRL`

## Intent
The agent loop a graduation run is driven by. It holds two phases and no more: a **work** turn that drives an execution agent against the run's work stream until the agent reports the task finished, and a **review** turn that judges what the work turn wrote. A **merge run** (per `../core/GRD-graduation.md` GRD-FR-MRNQ) drives the same two phases under its own instructions: a **merge work** turn that resolves a merge Git could not settle in an isolated merge worktree, and a fresh **merge review** turn that judges the result against both branches and the repository's specifications. The loop enforces no shape on the work. It tells the agent what was asked for and where to find the project's own instructions; what a change must contain is the project's rule, not this module's. A review that answers `ready` ends the run; one that asks for revision starts one further work turn while the project's pass budget holds one, and a review that still asks for revision when the budget is spent stops the run for the author. A merge run has a pass budget of its own. Out of scope: the container the turn runs in, which is `../tools/EAC-execute-agent-cli.md`'s; the dispatch seam, which is `../core/GXD-graduation-execution.md`'s; and the run's states and queues, which are `../core/GRD-graduation.md`'s.

## Functional requirements
1. **GRL-FR-EKXT** This loop is composed and driven through the `rig` framework, separate from `CVL-conversation-loop.md` in prompt, tool set, state and lifecycle. One instance runs per work stream at a time (per `../core/WKS-work-streams.md` WKS-FR-NDSV).
2. **GRL-FR-GADT** Every instruction this loop uses is a file under `resources/prompts/graduation/`, compiled into the binary at build time: `work.md`, `review.md`, `rebase.md`, `refusals.md`, `merge-work.md`, and `merge-review.md`. `rebase.md` serves the semantic rebase turn of a stream update alone. A merge run compiles `merge-work.md` and `merge-review.md`, and compiles none of `work.md`, `review.md` and `rebase.md`. No run reads one from disk, and no operation accepts, returns or overrides one.
3. **GRL-FR-YIJG** A prompt file's instruction text is its content with every authoring comment removed. The files carry comments naming the requirements they satisfy; an agent never sees them.
4. **GRL-FR-VYNX** One **pass** of a draft run is a work turn followed by a review turn. The loop dispatches the work turn through `../core/GXD-graduation-execution.md`'s `dispatch_graduation_turn`, and on a completed execution dispatches the review turn over what the working copy then holds.
5. **GRL-FR-ARPX** A run that is not a merge run takes at most the project's **pass budget** of passes, which is two where the project configures none (per `../core/PSS-project-settings-storage.md` PSS-FR-WPKS). A review that answers `ready` on any pass ends the loop. A review that answers `revise` starts the next pass while the budget holds one, and rests the run in `awaiting_author` carrying the findings where it does not.
   - *Why:* A loop that revises without a bound spends the author's time and tokens re-deciding work it has already been told about.
6. **GRL-FR-DXLU** `work.md` is the instruction every work turn carries, whole and unaltered, identical for every run and every project. It names the captured prompt as the task and directs the agent to the repository's own instructions, contributor notes and skills for how work is done there.
7. **GRL-FR-DAIB** The loop states no required deliverable. It does not ask for a specification, an implementation, a test, or a file in a named place, and it refuses no path the agent wrote.
   - *Why:* Every project shapes its own work through its own instructions, and a loop that imposed a shape would override the rule the project already states.
8. **GRL-FR-OTRH** `review.md` is the instruction every review turn carries. The turn is dispatched into a **fresh agent session every time**, carrying no session of any earlier turn.
9. **GRL-FR-YKRI** A review turn changes nothing. It stands in a **throwaway checkout** at `short_data_dir()/g/<run-id>/rv/`, detached at the run's base commit and holding the run's change set as uncommitted work. The checkout, the registration that names it, and the scratch branch it stands on all go when the turn ends.
   - *Why:* The review must run the project's own build and test commands, and those commands write files; a reviewer that shared the stream could not be told apart from one that tampered with it.
10. **GRL-FR-CKBL** The review turn judges four things on every pass: whether what the prompt asked for is delivered, `hidden_paths` included; whether the project's own definition of done passes when its commands are run; whether the work follows the project's conventions and test practice; and whether it introduces a security problem. The **delivery** decision is made against the work turn's own account as well as against the change set: `review.md` names the account among the material the turn is given, says it is the work turn's words rather than evidence, and requires what it claims to be checked against what the change set actually holds. An account that reports part of the prompt left undone is therefore read by the one turn whose decision it settles.
    - *Why:* Delivery is the decision that turns on what a reader knows. A turn that says it stopped early is the strongest evidence a review can have that delivery is incomplete, and it costs nothing to read — checked rather than believed, because the account is a claim and the working copy is what says whether it is true (GRL-FR-NVBZ), so an agent that under-reports its own work has no finding raised against work it actually did. A file an ignore rule hides is the same decision from the other side: it is absent from the change set, so no review reads it and no commit writes it, and only a reader tells a build directory from an authored module.
11. **GRL-FR-XNQU** The tree the review turn stands in is the tree a `ready` verdict commits. It is derived once, and both use that one derivation.
    - *Why:* Two derivations let a reviewer judge one tree while another lands, and nothing downstream can detect that.
12. **GRL-FR-VIAT** The review returns a `ReviewVerdict`: `verdict` of exactly `ready` or `revise`, a non-empty `rationale`, and `findings`. A `ready` verdict carries no finding and a `revise` verdict carries at least one.
13. **GRL-FR-TVXI** A verdict the application cannot read is refused whole, and a further **review** turn is asked **once**, carrying the `review_result` correction of `refusals.md`. No work turn is composed. A second consecutive unreadable verdict rests the run `blocked` with the code `review_verdict_invalid`, and the author's Continue resets the count.
14. **GRL-FR-UQNV** A `revise` verdict of two findings or fewer, all of severity `minor`, ends the loop as `ready` does. The findings are recorded as advisory remarks and no further work turn is composed. This rule does not apply to a merge review (GRL-FR-MRVK).
15. **GRL-FR-GQAB** A `revise` verdict that starts another pass carries its findings whole into the next work turn's instruction, in the order the review returned them.
16. **GRL-FR-NVBZ** The loop never treats an agent response envelope as evidence about the filesystem. It reads the envelope for two things — whether the agent reported success, a failure or an escalation, and the account the agent gave of its own turn (GRL-FR-DNKA) — and reads what changed from the working copy itself. The account is carried on to the review as a **claim**, never as a finding of fact: what the change set holds is read from the working copy there too.
17. **GRL-FR-DNKA** A work turn's own **account** of what it did and did not do is carried to the review that judges what it wrote, on **every** turn that completed — whichever outcome it reported. An agent-reported failure is material rather than an outcome, so a turn that reported one goes to review like any other and its account is that failure's message; a turn that reported success carries its summary on exactly the same terms. The review turn is told the account is there and told whose words it is (GRL-FR-CKBL). It reaches **that turn and no other**: no work turn is given one, whichever pass it belongs to.
    - *Why (the review alone):* A work turn's instruction names no such field, so an account reaching one is an input nothing explains. Worse, the account describes the tree as it stood when the turn before it finished, and a work turn is about to change that tree — a turn reading its predecessor's words as if they described what is in front of it is the confusion this field exists to prevent.
    - *Why:* The case that most needs a reader is a turn that wrote real work and knows it left some of what was asked undone. Such a turn reports success, that being the only outcome that does not misdescribe what it wrote, and says the rest in its summary. An account carried only from a failed turn is an account withheld from exactly the turn whose delivery is in question, and the review then judges a change set with no knowledge that its author said it was incomplete.
18. **GRL-FR-VBCL** An `escalation_required` response ends the turn without a review, and a merge turn's escalation is handled on the same terms (GRL-FR-MWPQ). The run rests in `awaiting_author` carrying the agent's reason and its ordered questions. The answers go into the phase that asked: a work turn's answers into the next work turn, and a review turn's answers into a fresh review turn, with no work turn composed.
19. **GRL-FR-ISIL** A run handed back after an interruption dispatches its next turn with `purpose = "resume"`, carrying the persisted checkpoint. Resuming is the loop's ordinary path rather than a special one.
20. **GRL-FR-ZDKP** Cancellation reaches every turn through **one path**: the token `../tools/EAC-execute-agent-cli.md` EAC-FR-25 honours. Nothing is judged, and what the turn already wrote is left in the working copy. `../core/GRD-graduation.md` GRD-FR-SWOJ commits it, except for a run that was discarded (GRD-FR-EWTN).
21. **GRL-FR-BRJO** The loop writes no file of the project, of the stream or of the draft, and it creates no commit. What a turn wrote is committed by `../core/GRD-graduation.md` on the host, after the turn ends.
22. **GRL-FR-REPL** The execution timeout and the pass budget are **validated project settings** (per `../core/PSS-project-settings-storage.md` PSS-FR-TQMV and PSS-FR-WPKS), and an unset one leaves the loop at its own default of two hours or two passes. The pass budget setting governs a run that is not a merge run, and a merge run's budget is the constant of GRL-FR-CZBT. The blocker bound and the verdict-refusal bound stay constants of this module.
    - *Why:* No phase here reaches a model, so the shared provider-call deadline bounds nothing of this loop and is the conversation loop's alone.
23. **GRL-FR-KWNP** Both settings are read **before every dispatch**, the semantic rebase turn of `../core/GRB-graduation-rebase.md` included, except that the turns of a merge run read the execution timeout alone (GRL-FR-CZBT), and the values read are the ones that turn is dispatched under. A run resumed after an interruption therefore uses the values active at each resumed dispatch rather than the values its earlier turns ran under.
    - *Why:* A value cached at the start of a run makes an author's correction take effect only for the next run, which is the run they were not waiting on.
24. **GRL-FR-XBUE** A run whose pass budget is spent rests in `awaiting_author` carrying the reason `pass_budget_exhausted`, the budget it spent, and the review findings that stand. The stream is released, the checkpoint is kept whole, and the author's Continue resets that budget alone and resumes without repeating a completed pass (per `../core/GRD-graduation.md` GRD-FR-CYIB). This requirement states the pass budget of a run that is not a merge run; GRL-FR-CZBT states a merge run's own.
25. **GRL-FR-UVJP** Every phase emits through `../core/LGC-logging.md`'s internal API under the `ai` and `backend` domains together, correlated by the run id, so one run's whole loop reads back as one sequence. Neither the assembled request nor the exchange it grew into is persisted.
26. **GRL-FR-GIMN** Every requirement here is exercisable on a machine with no network, no provider credential and no container runtime, through the dispatch seam and the completion seam alone.
27. **GRL-FR-QZFB** A second consecutive blocker with the same code rests the run `awaiting_author` and keeps the blocker on the run. A blocker with a different code rests the run `blocked` and starts the count again. Continue from `blocked` keeps the count; Continue from `awaiting_author` starts it again.
    - *Why:* Continue from `blocked` is the retry the count counts, so a reset there would make the bound unreachable and leave the run continuing into the same fault for ever.

28. **GRL-FR-MWPQ** One **merge pass** is a `merge_work` turn followed by a fresh `merge_review` turn. The loop dispatches each through `../core/GXD-graduation-execution.md`'s `dispatch_graduation_turn` with turn kinds `merge_work` and `merge_review`, and dispatches the review turn on a completed work turn over what the merge worktree then holds. A `ready` merge review ends the loop and hands the run to the apply of `../core/GRD-graduation.md` GRD-FR-AQNW. A `revise` merge review starts the next merge pass while the budget holds one (GRL-FR-CZBT). A merge turn that escalates ends the turn without a review, and the answers go into the phase that asked (GRL-FR-VBCL). A merge run composes no `work` turn, no `review` turn and no `semantic_rebase` turn. The rules of GRL-FR-VIAT, GRL-FR-TVXI, GRL-FR-GQAB, GRL-FR-NVBZ, GRL-FR-ZDKP and GRL-FR-BRJO hold for merge turns, with the merge review as the review turn and the merge work turn as the work turn.
29. **GRL-FR-SLQF** `merge-work.md` is the instruction every `merge_work` turn carries, whole and unaltered, identical for every merge run and every project. It names the unresolved paths and the conflicts of the task's `merge` context as the task, directs the agent to read both pinned branch versions and the repository's own instructions and specifications, and permits the agent to change any other path that the reconciliation of the merge needs. It requires that no conflict marker stand in a resolved path. It states that the review findings of the earlier pass arrive in the task's `loop_instruction`.
30. **GRL-FR-MRVK** `merge-review.md` is the instruction every `merge_review` turn carries. The turn is dispatched into a **fresh agent session every time** and stands in a throwaway review checkout (per GRL-FR-YKRI) derived from the merge worktree, with the merge snapshot commit as its base commit. The merge review judges the final result against both pinned branch versions and against the repository's specifications, for every path in `changed_paths` of the merge context, Git-clean paths included, and for every path in `reconciled_paths`. It runs the project-defined build and test commands, `task specs` and `task tests` among them, and invents none. It changes nothing except the output that those commands write in its disposable checkout. It answers `ready` only when its merge-intent checks and its specification checks pass and the required commands pass. It answers `revise`, with actionable findings, otherwise. The advisory-minor rule of GRL-FR-UQNV does not apply to it: every `revise` verdict of a merge review starts another merge pass or rests the run for the author.
31. **GRL-FR-CZBT** A merge run has its own pass budget of **2** passes. The budget is a constant of this module and is not read from the project's settings. A merge review that answers `revise` when the budget is spent rests the run in `awaiting_author` carrying the reason `pass_budget_exhausted`, the budget it spent, and the review findings that stand, and releases the stream and the project slot. `continue_graduation_run` adds 2 passes, keeps the merge worktree and the merge snapshot, and resumes without repeating a completed pass (per `../core/GRD-graduation.md` GRD-FR-CYIB and `../core/GXD-graduation-execution.md` GXD-FR-BQLN). The execution timeout of each turn of a merge run is the project's setting, read before every dispatch (GRL-FR-KWNP).
32. **GRL-FR-NDHW** A `ready` merge review whose merge worktree still holds a conflict marker in a path Git could not merge is not applied. The application replaces the verdict with a `revise` verdict that holds one finding of severity `critical`, which names that path, and the loop treats it as any `revise` verdict of a merge review (GRL-FR-MRVK).
33. **GRL-FR-TXEB** Every turn of a merge run carries the `merge` context of `MergeTaskContext` in its task input, and its `part` is `merge_work` or `merge_review`. The context holds the stream branch, the base branch, the pinned tips, the merge base, the snapshot commit, the changed paths, the unresolved paths and the conflicts of the run's merge data. A `merge_review` turn's context also holds `reconciled_paths`, the paths that the work turns changed beyond the unresolved paths. `graduation_input_version` stays `1`. The `draft_name` of a merge run's task is empty, and its `prompt` is the one-sentence statement of the merge.

## Contract surface

### The phases
```text
1  work     execution agent   work.md     → whatever the project's rules require
2  review   execution agent   review.md   → ReviewVerdict | escalation
```

A merge run holds two further phases in its own pass (GRL-FR-MWPQ):

```text
1  merge_work     execution agent   merge-work.md     → a resolved merge worktree
2  merge_review   execution agent   merge-review.md   → ReviewVerdict | escalation
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
                              //   | "merge_work" | "merge_review"
  loop_instruction?,          // the review findings a revise pass carries
  changed_paths?,             // what the working copy holds against the base commit
  hidden_paths,               // what the ignore rules kept out of those paths; an
                              //   ignored directory is one entry with a trailing
                              //   separator, and at most 200 are named
  hidden_paths_omitted,       // how many more there were than the input names
  correction?,                // GRL-FR-TVXI: what a turn asked again is to send
  merge?,                     // MergeTaskContext; on every turn of a merge run (GRL-FR-TXEB)
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

### Result shapes
```
ReviewVerdict {
  verdict,                    // "ready" | "revise"
  rationale,                  // never blank
  findings: [ReviewFinding]
}

ReviewFinding {
  severity,                   // "critical" | "major" | "minor"
  description,                // one problem, once
  affected_files,             // project-relative; may name a path that does not exist yet
  correction                  // directly actionable by the next work turn
}
```

### Seams
- `dispatch_graduation_turn` (`../core/GXD-graduation-execution.md` GXD-FR-DRFF) — the only route to an execution agent.
- `complete(exchange, endpoint)` — the only route to a model, used by no phase of this loop directly.

## Non-functional requirements
- A pass costs at most two containers, so a run costs at most twice its pass budget.
- The review's checkout is created from a commit, and the change set is materialized into it from the repository's own object database. It costs no tree copy, and Git settles file modes, symbolic links and the ignore rules.
- No instruction text is composed in Rust: everything that varies per run travels in the structured task input.
