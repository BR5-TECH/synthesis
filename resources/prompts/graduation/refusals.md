<!--
  Written from `specifications/ai/GRL-graduation-loop.md` GRL-FR-GADT, GRL-FR-TVXI,
  GRL-FR-MZHT and GRL-FR-XSFA.
  One section per refusal this loop answers. Each names what could not be read and what to
  send instead, so a turn is told the correction rather than the fault.
  Every comment here is stripped before an agent reads this file (GRL-FR-YIJG).
-->

## malformed_arguments

The arguments for that tool were not in the shape it expects. Check the tool's parameters and call it again.

## unknown_tool

There is no tool by that name available here. Use one of the tools you were given, or answer as you were asked to.

## review_result

The review you returned last time could not be read, so nothing was recorded from it and the implementation has not moved. Return a single JSON object as `result` holding exactly these fields and no others: `verdict`, which is `"ready"` or `"revise"`; `rationale`, a short non-empty explanation addressed to the person who started this run; and `findings`, a list which is empty for `ready` and holds at least one entry for `revise`. Each finding holds `severity`, which is `"critical"`, `"major"`, or `"minor"`; `description`, one problem stated once and never blank; `affected_files`, a list of project-relative paths inside the working copy you are standing in — the change set's own and any other, whether or not it exists yet — never an absolute path and never a path outside that copy, and empty only where the finding is about the repository as a whole; and `correction`, never blank, saying what the next implementation turn is to do about it. Return the findings ordered by severity from highest to lowest, then by their first affected path in ascending order with a finding that has none before those that do, then by description; a set in any other order is refused whole rather than tidied up. Where a decision turns on something only the author knows, return an `escalate_to_user` request in place of the verdict instead of guessing, with a `reason` and between one and eight `questions`, and give no verdict beside it.

## pr_summary_result

The summary you returned last time could not be read, so nothing was recorded from it. Change no file. Return a single JSON object as `result` holding exactly these fields and no others: `title`, one line that is never blank, and `description`, Markdown that is never blank. Ask the author nothing.

## answer_missing

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. No answer was found. End the turn with that one document, and with nothing after it.

## answer_malformed

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. It was not in the required shape: a field was missing, had the wrong type, or was not one the contract names. `protocol_version` is the integer `1`.

## answer_several

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. It held more than one document. Send one document and nothing else.

## answer_summary_empty

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. Its `summary` was empty. Write one line saying what the turn did.

## answer_outcome_mismatch

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. Its `outcome` did not match the fields it carried. `"success"` carries no `failure` and no `escalation`, `"failure"` carries `failure` alone, and `"escalation_required"` carries `escalation` alone.

## answer_escalation_shape

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. Its `escalation` broke a rule of its shape. Give a `reason` that is not blank and between one and eight `questions`. Each question has text that is not blank and at most three `options`, and each option has an `answer`, a `summary`, and a `description`, none of them blank.

## answer_too_large

Your answer last time could not be read. The work you did stands, so change no file and do not repeat the work. Send your answer again, as exactly one JSON document in the shape of the response contract. A value in it was longer than its limit. Keep each text inside the limit the contract states for it.
