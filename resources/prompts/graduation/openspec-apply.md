<!--
  Written from `specifications/ai/GRL-graduation-loop.md` GRL-FR-EUSF, GRL-FR-SZQK,
  GRL-FR-OEFM, GRL-FR-NWSC, GRL-FR-NZMK and GRL-FR-YFGA. It holds the two blocks of an apply run
  (`specifications/core/OCR-openspec-change-runs.md` OCR-FR-XKTV): the work block comes
  after `work.md`, and the review block comes after `review.md`. The application fills in
  `{{ change-name }}` before the turn is dispatched.

  The blocks name the tasks of the change as the deliverable because the project chose
  OpenSpec: the shape of the work is the project's rule here (GRL-FR-DAIB). The CLI parses
  `tasks.md`, so the block names no task format.

  The review block maps the levels of a verification issue to the finding severities of
  `review.md`, so a reviewer that verifies the way OpenSpec's own verify step does can
  answer in the verdict this loop reads.

  The work block tells the agent how to give a plan update
  (`specifications/core/OPU-openspec-plan-updates.md` OPU-FR-YBLE): the planning files go
  whole into the scratch folder, and the answer names `kind: "plan_update"`. The review
  block tells the reviewer to mark a finding about the plan with `kind: "plan"`
  (OPU-FR-EPXL).

  Every comment here is stripped before an agent reads this file (GRL-FR-YIJG).
-->

## Apply work

## This run implements an OpenSpec change

This project uses OpenSpec. The task of this run is the tasks of the change `openspec/changes/{{ change-name }}/`. The captured prompt names the change, and the change says what to build.

- Run `openspec instructions apply --change {{ change-name }} --json` to read the tasks, the files of the change to read first, and the progress. Read the proposal, the deltas below `specs/`, and the design before you write code.
- Implement the tasks in the order they are given. Each task that you complete, mark done in `tasks.md` at once: change its `- [ ]` to `- [x]`.
- Change no file of the change folder other than `tasks.md`. Do not edit the proposal, the deltas, the design, or the tasks that are written there.
- Where the proposal, a delta, the design, or the tasks are wrong, or cannot be built as they are written, do not edit them and do not work around them. Give a plan update, and say which requirement or task stops you.

## How to give a plan update

A plan update asks the author to change the plan of the change. The author reads it, answers its questions, and accepts or rejects each edit you propose.

- Write each planning file that you propose to change, whole, at `.synthesis-plan/openspec/changes/{{ change-name }}/<file>`. For example, the new text of `openspec/changes/{{ change-name }}/design.md` goes to `.synthesis-plan/openspec/changes/{{ change-name }}/design.md`. A file that you do not write there stays as it is.
- Write only text files, and only below that folder. Do not write an empty file: a plan update cannot delete a file.
- Then answer `escalation_required` with `kind: "plan_update"`, a `reason` that says what is wrong in the plan and why, and between 0 and 8 `questions` for the author. Give at least one proposed file or one question.
- Change no other file for the plan update. The work you did before it stays in the working copy, and you continue from it once the author has decided.
- A question that is not about the plan is an ordinary escalation: answer `escalation_required` with no `kind`.
- Run the project's own checks as the standing instruction tells you, and record them in the check report.

## Apply review

## This review verifies an implemented OpenSpec change

This run implements the OpenSpec change `openspec/changes/{{ change-name }}/`. Verify the work against the change, not against the captured prompt alone.

- **Completeness**: every task of the change is done, and every requirement of each delta below `specs/` is implemented.
- **Correctness**: the code does what each requirement and each of its scenarios states.
- **Coherence**: the code follows the design of the change, where the change has one.
- A file of the change folder that the work changed, other than `tasks.md`, is a finding.
- Give each issue you find a severity: a CRITICAL issue is a finding of severity `critical`, a WARNING is `major`, and a SUGGESTION is `minor`.
- Give each finding a `kind`. Where the proposal, a delta, the design, or the tasks are wrong, or cannot be built as they are written, the finding is `kind: "plan"`, and its correction says how the plan must change. Every other finding is `kind: "work"`.
- `input.openspec_apply_progress` is the task progress that `openspec instructions apply --change {{ change-name }} --json` gives on the tree you judge, and `input.openspec_validation` is the result of `openspec validate {{ change-name }} --strict` on it. Do not run either yourself, and run no check. A task that is not done, and a change that does not validate, each mean the work is not ready.
