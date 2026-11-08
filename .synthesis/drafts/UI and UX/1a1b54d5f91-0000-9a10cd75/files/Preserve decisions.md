## Intent

During graduation runs, all the User’s responses that were given to agent (e.g. Claude Code) questions - must be preserved and passed down to every later work, review, merge and rebase turn of the same run or update.

The intent is: if some decision was made by User - it must be respected through the whole run, regardless how many iterations/passes we have.

The same applies to review findings: what a review found in an earlier pass (and the worker fixed) must stay in the context of later work turns of the same run.

A second goal of this draft: a `review` turn must not run the project's tests. The work turn follows the project's own rules, tests included, so a test run in review repeats the work turn. The review judges the working copy by reading it and may run build and lint commands. The `merge_review` turn is not part of this goal and does not change.

## User journey

1. An agent turn of a graduation run stops and asks the author questions. The run rests, waiting for the author.
2. The author answers every question and submits. The answers are recorded in the run's decision ledger.
3. The next turn starts. Its input holds the whole ledger, and its instruction says that the recorded decisions are binding.
4. Later turns (work, review, more passes, resumes after an interruption) get the same ledger plus any new decisions. The agents respect the decisions in every pass. Work turns also get the findings of all earlier reviews, marked as probably fixed already.
5. At any time the author opens the **Escalations log** in the run region of the Runs panel, expands it and reads the decisions. The author cannot change them.
6. The ledger ends when the run completes or is discarded.

## Requirements

**Ledger**
- Each run (draft run or merge run) and each stream update has one **decision ledger**: the ordered list of the author's answers to its escalations. It is append-only.
- One entry per answered question. It holds the question text, the author's answer, its summary, the author's note (where there is one), the turn that asked (work, review, merge, semantic rebase) and the pass.
- Only the author's answers are recorded. An answer from the clarification judgement (ESU-FR-18) is not a decision of the author and is not in the ledger.
- An entry is never replaced, edited, merged or removed. If an agent asks a question the author already answered and the author answers it again, the new answer is added as a new entry. The old entry stays.
- The ledger is stored durably with the run's checkpoint (or the update record). It survives an application restart, an interruption, a resume and a Continue after `pass_budget_exhausted`.
- The ledger ends when the run ends (completes) or is discarded. For an update, it ends when the update settles or its record is cleared. The author cannot withdraw a decision.

**Turn input**
- Every turn of every kind (`work`, `review`, `merge_work`, `merge_review`, `semantic_rebase`) gets the full ledger in its structured input, in order, in every pass and after every resume. The ledger is empty before the first answer.
- The instruction of every turn kind says that the recorded decisions are binding: the agent must not contradict them and must not ask the author again about a decision already recorded. This is an instruction only. The application does not refuse an escalation that repeats a question, and the author is shown the new question as it is.
- The ledger is never truncated, summarised or reordered. It is small in practice (typically under 10 questions). No new size limit is added, and the existing bound of the turn input (EAC-FR-08) is unchanged.

**Review history (backend only)**
- Each draft run and each merge run keeps a **review history**: the findings of every review verdict of the run, in order, each with its pass. A pass with a `ready` verdict and no findings adds nothing.
- Only work turns (`work`, `merge_work`) get the review history, in their structured input, in every pass and after every resume. Review turns (`review`, `merge_review`) do not get it. The latest findings keep their existing place as the instruction of a revise pass; the history holds the findings of the passes before it.
- The instruction of a work turn says that the findings of earlier passes are probably already fixed. They are context, not new work: the agent must not redo a fix and must not undo it. The instruction of a review turn is unchanged, so a review judges the working copy as it is and flags what is wrong now.
- The history is stored durably with the run's checkpoint, survives restart, interruption, resume and Continue, and ends with the ledger. It is never truncated, summarised or reordered. No new size limit is added, and the bound of the turn input (EAC-FR-08) is unchanged.
- The Escalations log does not show the review history. The UI has no new element for it.

**Review turn commands (backend only)**
- A `review` turn runs **no test command**. It must not run the project's tests, test suites or test runners. It may run build and lint commands.
- The instruction of the `review` turn says this. It is an instruction only: the application does not inspect or refuse the commands the agent runs.
- A build or lint command that cannot run, fails, times out or gives an unstable result is still a finding, as before. A test that is absent or not run is not a finding by itself; the review judges test coverage by reading the change set.
- The `merge_review` turn is unchanged: its instruction and its commands stay as they are.

**Escalations log (UI)**
- The run region of the graduation section of the Runs panel shows one read-only list called **Escalations log**. It is not repeated inside the pass entries.
- The list is collapsible and expandable with a control on its header. It is collapsed by default. The header shows the number of decisions.
- Each row shows the question, the author's answer and note, the turn that asked and the pass, in the order they were recorded. Rows are plain text. A repeated question shows both of its entries.
- The list is shown only when the ledger is not empty. It has no control that changes a decision. It updates when the run state is published.
- The ledger of a stream update is backend only. The update window shows no log.

**Out of scope**
- Withdrawing or editing a decision; truncating the ledger; the application refusing a repeated question; showing decisions inside pass entries (a possible later way to pass information); showing the review history in the UI; giving the review history to review turns or to stream update turns; enforcing the review rule on test commands in the application; any change to the `merge_review` turn or its instruction.

## Specification impact

Update: `core/GXD-graduation-execution.md` (checkpoint holds the ledger and the review history; every turn input carries the ledger, work turns also the review history), `core/GRD-graduation.md` (record the answered set into the ledger; ledger lifetime; the run record read by the UI), `ai/GRL-graduation-loop.md` (GRL-FR-VBCL and the turn composition for every turn kind; a review history field in `GraduationTaskInput` for work turns, next to `loop_instruction`; GRL-FR-CKBL, and the *Why* line of GRL-FR-QEUP, which say that the review runs the project's own build and test commands: the `review` turn runs no tests, and may run build and lint commands; the `merge_review` turn is not changed), `core/GRB-graduation-rebase.md` and `core/WKS-work-streams.md` (update record holds the ledger; semantic turns receive it), `tools/ESU-escalate-to-user-tool.md` (ESU-FR-13: answers also stay in the ledger), `tools/EAC-execute-agent-cli.md` (EAC-FR-08: the ledger counts in the bounded input; no change to the bound), the graduation turn instructions in `resources/prompts/graduation/` (binding decisions for every turn kind; earlier findings are probably fixed, for work turns; `review.md`: no test commands, optional build and lint, and the removal of the rule that a check which could not run is a finding where the check is a test command; the `merge_review` instruction is not changed), and `ui/GRU-graduation-runs.md` (Escalations log in the run region). `ui/GEA-graduation-escalation-answering.md` is not changed: the answer submission stays as it is.