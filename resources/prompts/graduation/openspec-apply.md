<!--
  Written from `specifications/ai/GRL-graduation-loop.md` GRL-FR-EUSF, GRL-FR-SZQK,
  GRL-FR-NWSC and GRL-FR-YFGA. It holds the two blocks of an apply run
  (`specifications/core/OCR-openspec-change-runs.md` OCR-FR-XKTV): the work block comes
  after `work.md`, and the review block comes after `review.md`. The application fills in
  `{{ change-name }}` before the turn is dispatched.

  The blocks name the tasks of the change as the deliverable because the project chose
  OpenSpec: the shape of the work is the project's rule here (GRL-FR-DAIB). The CLI parses
  `tasks.md`, so the block names no task format.

  The review block maps the levels of a verification issue to the finding severities of
  `review.md`, so a reviewer that verifies the way OpenSpec's own verify step does can
  answer in the verdict this loop reads.

  Every comment here is stripped before an agent reads this file (GRL-FR-YIJG).
-->

## Apply work

## This run implements an OpenSpec change

This project uses OpenSpec. The task of this run is the tasks of the change `openspec/changes/{{ change-name }}/`. The captured prompt names the change, and the change says what to build.

- Run `openspec instructions apply --change {{ change-name }} --json` to read the tasks, the files of the change to read first, and the progress. Read the proposal, the deltas below `specs/`, and the design before you write code.
- Implement the tasks in the order they are given. Each task that you complete, mark done in `tasks.md` at once: change its `- [ ]` to `- [x]`.
- Change no file of the change folder other than `tasks.md`. Do not edit the proposal, the deltas, or the design.
- Where the proposal, a delta, or the design is wrong, or cannot be built as it is written, do not edit it and do not work around it. Escalate to the author with the question, and say which requirement or task stops you.
- Run the project's own checks as the standing instruction tells you, and record them in the check report.

## Apply review

## This review verifies an implemented OpenSpec change

This run implements the OpenSpec change `openspec/changes/{{ change-name }}/`. Verify the work against the change, not against the captured prompt alone.

- **Completeness**: every task of the change is done, and every requirement of each delta below `specs/` is implemented.
- **Correctness**: the code does what each requirement and each of its scenarios states.
- **Coherence**: the code follows the design of the change, where the change has one.
- A file of the change folder that the work changed, other than `tasks.md`, is a finding.
- Give each issue you find a severity: a CRITICAL issue is a finding of severity `critical`, a WARNING is `major`, and a SUGGESTION is `minor`.
- `input.openspec_apply_progress` is the task progress that `openspec instructions apply --change {{ change-name }} --json` gives on the tree you judge, and `input.openspec_validation` is the result of `openspec validate {{ change-name }} --strict` on it. Do not run either yourself, and run no check. A task that is not done, and a change that does not validate, each mean the work is not ready.
