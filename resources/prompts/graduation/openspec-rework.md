<!--
  Written from `specifications/ai/GRL-graduation-loop.md` GRL-FR-CWSA, GRL-FR-GWDB,
  GRL-FR-HSNY and GRL-FR-XNBQ. It holds the two blocks of a rework run
  (`specifications/core/OCR-openspec-change-runs.md` OCR-FR-QUSP): the work block comes
  after `work.md`, and the review block comes after `review.md`. The application fills in
  `{{ change-name }}` before the turn is dispatched.

  The blocks state what the propose blocks state (GRL-FR-FYKL, GRL-FR-APHV, GRL-FR-BFPT),
  with the change as existing work to revise and the author's instruction as the task. The
  instruction travels in `input.openspec.instruction` and is never written into this text.

  The finish gives the author the change folder alone, as one proposal (OCR-FR-ZNJT). So a
  write outside the folder would be lost, and a deleted file has no hunk: both are refused.

  Every comment here is stripped before an agent reads this file (GRL-FR-YIJG).
-->

## Rework work

## This run revises an OpenSpec change

This project uses OpenSpec. This run implements nothing. It revises the OpenSpec change that already stands in `openspec/changes/{{ change-name }}/`, on the instruction of the author.

- The task is `input.openspec.instruction`. The captured prompt only names the change. Do what the instruction asks, and nothing that it does not ask for.
- Read the whole change first: the proposal, the deltas below `specs/`, the design, and the tasks. Keep them in agreement with one another after your edits.
- Revise each artifact as this project's OpenSpec schema defines it. Before you change an artifact, run `openspec instructions <artifact> --change {{ change-name }} --json` and follow what it returns.
- Read the live specs in `openspec/specs/` before you change a delta, and cite each requirement you modify or remove by its exact name.
- Change no file outside `openspec/changes/{{ change-name }}/` and the check folder, and delete no file of the change folder. Write no code. The author decides each of your edits before it reaches the work stream.
- Before you report, run `openspec validate {{ change-name }} --strict` and fix every issue it reports. Record it in the check report as one of your checks.

## Rework review

## This review judges a revised OpenSpec change

This run revises the OpenSpec change `openspec/changes/{{ change-name }}/` on the instruction of the author. It implements nothing, so judge the change, not code.

- Judge the revision against `input.openspec.instruction`: whether the edits do what the instruction asks, and nothing that it does not ask for, and whether the proposal, the deltas, the design, and the tasks still agree with one another.
- Each delta must cite the live requirement it modifies or removes by its exact name in `openspec/specs/`.
- A path changed outside `openspec/changes/{{ change-name }}/` and the check folder is a finding, and so is a deleted file of the change folder.
- `input.openspec_validation` is the result of `openspec validate {{ change-name }} --strict` on the tree you judge. Do not run it yourself. A change that does not validate is not ready, and each issue it names is a finding.
