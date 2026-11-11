<!--
  Written from `specifications/ai/GRL-graduation-loop.md` GRL-FR-GADT, GRL-FR-TVXI,
  GRL-FR-MZHT and GRL-FR-XSFA.
  GRL-FR-CHDT, GRL-FR-SUBP and GRL-FR-XSFA: the check corrections `checks_not_passing`,
  `checks_record_missing` and `work_failed` let the turn change files, and the escalate
  correction `checks_escalate` does not. None names a failed value or holds agent text.
  OPU-FR-XSRD, OPU-FR-DTYE and GRL-FR-QSMY: `plan_update_invalid` and `plan_correction` let
  the turn write the scratch folder alone, and ask for a plan update.
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

## checks_not_passing

The application did not accept your last answer: its check record shows a check that did not pass or did not finish. A turn ends with every check the project requires passed, or with an escalation to the author, and a cause outside your change, such as the environment, is no exception. You may change files. Fix what the checks report, run the full checks again in the foreground, and record each one in `.synthesis-checks/` as before. Then answer `success` with `result.checks` as the report records them. Set `waived_by` only where a decision in `input.decisions` lets that check fail. Where you cannot make every required check pass, answer `escalation_required` and ask the author what to do.

## checks_record_missing

The application did not accept your last answer: it carried no valid check record. A success carries `result` with `checks`, one entry for each full check, each with `name`, `command`, `finished`, `exit_status`, and `waived_by` only where a decision in `input.decisions` lets that check fail. Where the project requires no check for the change, `checks` is empty and `no_checks_reason` says why. You may change files. Run the project's full checks in the foreground if you did not, record each one in `.synthesis-checks/`, and answer again with that record. Where you cannot make every required check pass, answer `escalation_required` and ask the author what to do.

## work_failed

The application did not accept your last answer: it reported `failure`. A work turn does not end on failure. It ends with every check the project requires passed, or with an escalation to the author. You may change files. Finish the work, run the full checks in the foreground, record each one in `.synthesis-checks/`, and answer `success` with `result.checks` as the report records them. Where you cannot finish, answer `escalation_required` with your reason and your questions for the author.

## checks_escalate

The application did not accept your answers: your work did not end with every check the project requires passed, and a correction did not change that. You may not report success, and you may not report failure. Change no file. Answer `escalation_required`, with a `reason` that says which checks did not pass and why, and between one and eight `questions` that ask the author what to do, for example whether a check may fail or what to change.

## plan_update_invalid

The application did not accept your plan update. Change no file outside `.synthesis-plan/`. Give the plan update again: write each planning file that you propose, whole, at `.synthesis-plan/openspec/changes/<change>/<file>`, then answer `escalation_required` with `kind: "plan_update"`, a `reason`, and between 0 and 8 `questions`. Each entry below `.synthesis-plan/` must be a regular file below `openspec/changes/<change>/`, and must be UTF-8 text. A file must not be empty where the change holds that file, because a plan update cannot delete a file. The plan update must propose at least one change to a file or ask at least one question. Where you have a question that is not about the plan, answer `escalation_required` with no `kind` instead.

## plan_correction

The review found that the plan of the change is wrong. `input.loop_instruction` holds the findings about the plan, in order. Change no file outside `.synthesis-plan/`, and do not continue the work. Answer with a plan update that corrects the plan: write each planning file that you propose, whole, at `.synthesis-plan/openspec/changes/<change>/<file>`, then answer `escalation_required` with `kind: "plan_update"`, a `reason`, and between 0 and 8 `questions`. Each entry below `.synthesis-plan/` must be a UTF-8 text file below `openspec/changes/<change>/`, and must not be empty where the change holds that file. No other answer is accepted.
