<!--
  Written from `specifications/ai/GRL-graduation-loop.md` GRL-FR-FYKL, GRL-FR-GWDB,
  GRL-FR-APHV and GRL-FR-BFPT. It holds the two blocks of a propose run
  (`specifications/core/GRD-graduation.md` GRD-FR-LKGC): the work block comes after
  `work.md`, and the review block comes after `review.md`. The application fills in
  `{{ change-name }}` before the turn is dispatched.

  The blocks name the change folder as the deliverable because the project chose
  OpenSpec: the shape of the work is the project's rule here (GRL-FR-DAIB). The CLI
  gives each artifact's instructions, so a project's own schema shapes the change.

  Every comment here is stripped before an agent reads this file (GRL-FR-YIJG).
-->

## Propose work

## This run proposes an OpenSpec change

This project uses OpenSpec. This run does not implement the prompt. It writes the OpenSpec change that proposes it: the folder `openspec/changes/{{ change-name }}/`.

- Write the change as this project's OpenSpec schema defines it. Before you write each artifact, run `openspec instructions <artifact> --change {{ change-name }} --json` and follow what it returns. Run `openspec status --change {{ change-name }} --json` to see which artifact comes next.
- If the folder does not exist yet, create it with `openspec new change {{ change-name }}`.
- State in the deltas what the prompt asks for, and nothing that it does not ask for. Read the live specs in `openspec/specs/` first, and cite each requirement you modify or remove by its exact name.
- Change no file outside `openspec/changes/{{ change-name }}/` and the check folder. Write no code.
- Before you report, run `openspec validate {{ change-name }} --strict` and fix every issue it reports. Record it in the check report as one of your checks.

## Propose review

## This review judges a proposed OpenSpec change

This run proposes an OpenSpec change: the folder `openspec/changes/{{ change-name }}/`. It implements nothing, so judge the change, not code.

- Judge the change against the prompt: whether its proposal, its deltas, its design, and its tasks state what the prompt asks for, and nothing that it does not ask for.
- Each delta must cite the live requirement it modifies or removes by its exact name in `openspec/specs/`.
- A path changed outside `openspec/changes/{{ change-name }}/` and the check folder is a finding.
- `input.openspec_validation` is the result of `openspec validate {{ change-name }} --strict` on the tree you judge. Do not run it yourself. A change that does not validate is not ready, and each issue it names is a finding.
