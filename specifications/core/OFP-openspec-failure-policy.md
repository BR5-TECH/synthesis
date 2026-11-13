# OpenSpec failure policy

**Spec code:** `OFP`

## Intent
Every OpenSpec CLI process that the application runs can fail, and a failure means one of three things: OpenSpec cannot run on this machine or in this agent image, the CLI did not finish, or the CLI answered that a change is wrong. This module sorts every OpenSpec failure into one of these three **failure classes** and gives each class one behaviour on every path: what the backend runs again, what a start, a merge, and a run do, what the error carries to the surfaces, and where a wrong change goes to be fixed. The author then sees one sentence and one route for one condition, a short CLI hiccup does not stop work, and a change that does not archive at a stream merge gets a repair proposal that the author decides. Before the first turn of an OpenSpec run, this module also checks that the agent image of the run holds a usable CLI, because the agent calls the CLI itself. Out of scope: the CLI seam and its reads, which are `OSM-openspec-mode.md`'s; the archive, which is `OAR-openspec-archive.md`'s; the words and the actions that a surface shows for each class, which are `../ui/OFN-openspec-failure-notices.md`'s; the fixed refusal texts of the agent tools, which `../tools/LOS-list-openspec-specs-tool.md`, `../tools/ROS-read-openspec-spec-tool.md`, and `../tools/LOC-list-openspec-changes-tool.md` keep; and a retry of a process that is not an OpenSpec CLI process.

## Functional requirements
1. **OFP-FR-PHOW** An OpenSpec failure is in exactly one **failure class**: `environment`, `transient`, or `content`. `classify_openspec_failure(code)` gives the class of a classifying code (OFP-FR-BYYT) from the class table of the contract surface. It reads no file, no setting, and no clock.
2. **OFP-FR-BYYT** The **classifying code** of a failure is its `cause` where it carries one, and its `code` otherwise (OFP-FR-ZDEE).
3. **OFP-FR-SQDK** The class `environment` holds the failures where OpenSpec cannot run: `openspec_mode_inactive`, `cli_unavailable` in each of its CLI states, `cli_output_unreadable`, and `openspec_image_cli_unavailable` (OFP-FR-XBNJ).
4. **OFP-FR-DGSC** The class `transient` holds the failures where the CLI did not finish: `cli_timed_out` (per `OSM-openspec-mode.md` OSM-FR-ZIMO), and `cli_failed`, which a process gets for an exit by a signal, for an exit status other than 0 and 1, for an exit status 1 whose output is not the JSON of the CLI (per OSM-FR-WWWK), and, for a read, for a process that does not start. The version check of `OSM-openspec-mode.md` OSM-FR-ZNVZ treats a process that does not start as the state `failed`, with no retry. Output is the JSON of the CLI where its standard output decodes as one JSON object.
5. **OFP-FR-RQXW** The class `content` holds the answers of the CLI that a change is wrong: `cli_rejected` (per `OSM-openspec-mode.md` OSM-FR-WWWK), `change_not_ready` (per `OCR-openspec-change-runs.md` OCR-FR-LNKQ), `tasks_incomplete` (per `OAR-openspec-archive.md` OAR-FR-QTBK), and each CLI `status` code of an archive (per OAR-FR-EUXZ).
6. **OFP-FR-FKRV** `classify_openspec_failure` returns `none` for a code that the class table does not name, such as `change_not_found`, `change_archived`, `invalid_change_name`, `proposal_pending`, and `commit_failed`. Such a code is not an OpenSpec failure, and its operation states it on its own terms.
7. **OFP-FR-UPDO** No step runs a process again by itself for a failure of the class `environment` or `content`. An exit status 1 whose output is the JSON of the CLI is an answer, and no step runs that process again for it.
8. **OFP-FR-PQNS** A CLI process whose outcome is of the class `transient` runs once more, after a wait of 2 seconds, before its read fails. The outcome of the second process is the outcome of the read. No process runs more than twice for one read.
   - *Why:* A slow Node start or a short machine stall is the usual cause, and one more try clears it without a surface that the author must act on.
9. **OFP-FR-AZIV** The retry of OFP-FR-PQNS belongs to the process seam of `OSM-openspec-mode.md` and applies to each process of a read on its own. A read that runs two subcommands runs again only the process whose outcome is transient.
10. **OFP-FR-MJZU** The wait of OFP-FR-PQNS goes through an injectable **wait seam** that receives the requested delay. The process gives back its slot of `OSM-openspec-mode.md` OSM-FR-QHRV before the wait and takes a slot again after it, so the wait holds no slot.
    - *Why:* Tests assert the requested delay and the count of process starts, never the passing of real time.
11. **OFP-FR-FETF** The version check of `OSM-openspec-mode.md` OSM-FR-ZNVZ follows OFP-FR-PQNS. A check whose process times out, ends by a signal, or exits with a status other than zero runs once more before the check records `failed`.
12. **OFP-FR-IJYA** An archive of `OAR-openspec-archive.md` OAR-FR-IAKL whose outcome is of the class `transient` runs once more on the terms of OFP-FR-PQNS only after its restore of OAR-FR-KBDD is complete. Where the restore fails, the archive ends with its first outcome and does not run again.
13. **OFP-FR-ZQNU** Each retry emits through `LGC-logging.md` under the `backend` domain one `WARN` record with the subcommand, the code of the first outcome, the attempt number, and the delay in milliseconds. The record holds no CLI output and no diagnostic (per `OSM-openspec-mode.md` OSM-FR-IFWM).
14. **OFP-FR-ITUD** An explicit act of the author clears a kept `failed` CLI check (per `OSM-openspec-mode.md` OSM-FR-ZNVZ) before the next check: `get_openspec_status` with `recheck` (OSM-FR-YXUC), and `continue_graduation_run` against a run that rests on an OpenSpec failure (OFP-FR-TBHX). A kept `available` or `unsupported` result stays kept.
15. **OFP-FR-OFLU** A failure of the class `transient` or `content` carries its **diagnostics**: a list of `CliDiagnostic` entries, each with a message and an optional severity, code, path, and fix. A failure whose process gave no diagnostic carries an empty list. A failure of the class `environment` carries an empty list.
16. **OFP-FR-NJSO** The diagnostics of an exit status 1 whose output is the JSON of the CLI are its `status` entries, in the order of the CLI, each with its `severity`, `code`, `message`, and `fix`. The rule applies also where the output holds the expected shape beside the `status` list.
17. **OFP-FR-RUOP** The diagnostics of a validation that answers not valid (per `OSM-openspec-mode.md` OSM-FR-APVF, OSM-FR-BXIF) are its issues, in order, each with its level as the severity, its path, and its message.
18. **OFP-FR-BWML** The diagnostics of `change_not_ready` come from `instructions apply`: one entry of severity `error` for each missing artifact, which names the artifact, one entry of severity `warning` for each entry of `warnings`, and one entry of severity `info` with the `instruction` text.
19. **OFP-FR-WNVO** A failure of the class `transient` carries one diagnostic with the last 4 KiB of the standard error of its last process, decoded as UTF-8 with each sequence that is not valid replaced, and no severity. A process whose standard error is empty adds no entry. The seam keeps at most 4 KiB of standard error for each process. This diagnostic is the one exception to the 2 KiB cut of OFP-FR-QOET.
20. **OFP-FR-QOET** A list of diagnostics holds at most 50 entries, the first 50 in the order of the CLI. Each message and each fix is cut to its first 2 KiB.
21. **OFP-FR-WRDX** Diagnostics reach the author alone: in the `OpenSpecFailure` of a Tauri command (OFP-FR-QLQJ), in the blocker of a run (OFP-FR-TBHX), and in the comment and the instruction of a merge notice (OFP-FR-EJPE). No log record and no result of an agent tool holds a diagnostic. An event payload holds a diagnostic only where it carries one of those records whole: the run whose blocker holds it, or the discussion whose comment holds it. No other event payload holds one.
22. **OFP-FR-QLQJ** The Tauri commands of the contract surface reject an OpenSpec failure with one `OpenSpecFailure` object, camelCase on the wire. `read_openspec_change`, `read_apply_progress`, and `load_specification_map` reject every failure with that object. A refusal of another command that is not an OpenSpec failure keeps its own form.
23. **OFP-FR-ZDEE** `code` of an `OpenSpecFailure` is the refusal code of the operation. `cause` is the OpenSpec code under a refusal that wraps one: the read code of `apply_progress_unavailable` and `openspec_validation_unavailable`, the reason of `change_not_archivable`, and the code of `archive_failed`. `change` names the change where the refusal names one.
24. **OFP-FR-EOAD** `detail` of an `OpenSpecFailure` is the CLI state of `cli_unavailable` and of `openspec_image_cli_unavailable` (`not_found`, `unsupported`, or `failed`), the missing artifacts of `change_not_ready` separated by commas, the exit status of `cli_failed`, empty for an exit by a signal, and the archive folder name of `change_archived`.
25. **OFP-FR-GPCO** `cli_unavailable` is the one code for an OpenSpec CLI that is not usable on this machine, at every layer and in every refusal, with the CLI state in `detail`. No operation refuses with another code for that condition.
26. **OFP-FR-TBHX** A CLI step of a run whose failure is of the class `environment` or `transient` dispatches no turn and rests the run `blocked`, with its queue, its stream, and its project slot held (per `GRD-graduation.md` GRD-FR-UJII). The blocker holds the code, the cause, and the diagnostics. `continue_graduation_run` clears a kept `failed` CLI check and runs the step again.
27. **OFP-FR-UVPV** Inside a run, a CLI answer that the change is wrong reaches the agent as a finding or a plan update (per `../ai/GRL-graduation-loop.md` GRL-FR-HSZD, GRL-FR-EQDL, GRL-FR-ZFMZ). A gate read that fails with `cli_rejected` rests the run on the terms of OFP-FR-TBHX with the cause `cli_rejected`.
28. **OFP-FR-NQPY** A run that is not terminal continues when OpenSpec mode becomes inactive. Each later CLI step of the run, the review gates and the image probe included, rests the run on the terms of OFP-FR-TBHX with the cause `openspec_mode_inactive`, and the run does not skip the step.
29. **OFP-FR-AQDH** A merge run records `openspecGate: true` in its merge data where OpenSpec mode is active when the merge run is created, and `openspecGate: false` otherwise. The specs gate of `../ai/GRL-graduation-loop.md` GRL-FR-ZDNL runs for a merge run whose merge data holds `openspecGate: true` alone.
    - *Why:* A project whose mode was off when the merge began has no OpenSpec work to guard, and a mode that turns off during the run must not skip the guard.
30. **OFP-FR-YKGQ** A start or a stream merge that meets an OpenSpec failure of any class is refused with it and changes no state. The merge notice of OFP-FR-ETEN is the one exception.
31. **OFP-FR-HQQN** A **repair cause** is one of the CLI `status` codes `archive_spec_update_failed`, `archive_spec_validation_failed`, and `archive_validation_failed`: the deltas of the change conflict with the live specifications, or the change or the result does not validate.
32. **OFP-FR-ETEN** A stream merge refused with `archive_failed` or `change_not_archivable` whose classifying code is of the class `content` gives one **merge notice** for its change. The notice comes after the merge has released the repository update guard (per `WKS-work-streams.md` WKS-FR-HLGN) and every other guard that it holds, and before `merge_work_stream` returns.
    - *Why:* A run cannot claim the stream under the guard (WKS-FR-RQVM), and the graduation write guard that a start takes is not reentrant.
33. **OFP-FR-EJPE** A merge notice opens a new whole-change discussion of the change on the stream, as the Synthesis participant (per `CMS-comments-storage.md` CMS-FR-MKEV), through `append_as` (CMS-FR-26). Its first comment is the notice text of the contract surface for its cause, followed by the diagnostics, one line for each.
34. **OFP-FR-XQDY** For a repair cause, the merge notice also starts a **repair run**: `start_change_rework_in_thread(stream_id, change, instruction, thread_id, requester)` (per `OCR-openspec-change-runs.md` OCR-FR-TWDK), with the text of the first comment as the instruction, the notice's thread, and the Synthesis participant as the requester.
35. **OFP-FR-FAHB** A repair run is a rework run on every term of `OCR-openspec-change-runs.md`. Its result is a change edit proposal that the author decides hunk by hunk (OCR-FR-WBZX), and no merge, archive, or file write follows from it without the author.
36. **OFP-FR-HVHV** After the notice, the application writes the **notice record** of the change (per the storage of the contract surface): the digest of the change folder (OFP-FR-KYXI), the cause, the thread id, and the repair run id or the repair refusal.
37. **OFP-FR-KYXI** The **digest** of a change folder is the sha256 over the change-relative path and the bytes of each regular file below `openspec/changes/<change>/` of the stream's working copy, in path order.
37. **OFP-FR-PSVR** Where the notice record of a change holds the digest that the change folder has at the refusal, the merge gives no notice: it opens no discussion, posts no comment, and starts no run. The refusal carries the thread id, and the repair run id or the repair refusal, of the record.
    - *Why:* At most one automatic repair runs for one state of the change files, so a repair that does not fix the change cannot start a loop.
38. **OFP-FR-PCQY** A repair start that is refused, for example with `proposal_pending`, `openspec_run_active`, or an image preflight refusal, starts nothing. The discussion and its comment stay, the notice record holds the refusal code, and the merge refusal carries it as `repairRefusal`.
39. **OFP-FR-MQGG** The `OpenSpecFailure` of a merge refusal with a notice carries `threadId`, the notice's thread, and either `repairRunId`, the id of the repair run, or `repairRefusal`, the code that refused it. A notice of a cause that is not a repair cause carries `threadId` alone.
40. **OFP-FR-PWAV** A merge notice whose discussion cannot be opened writes no notice record and starts no run. The merge refusal then carries no `threadId`, no `repairRunId`, and no `repairRefusal`.
41. **OFP-FR-SJML** A start of an apply run refused with `change_not_ready` posts no comment and starts nothing. The refusal carries the diagnostics of OFP-FR-BWML to the surface where the author started it.
42. **OFP-FR-HMED** Each merge notice, notice record, repair start, and repair refusal emits through `LGC-logging.md` under the `backend` domain an `INFO` record with the stream id, the change name, the cause, the thread id, and the run id, or a `WARN` record with the failure code. No record holds a diagnostic or comment text.
43. **OFP-FR-BZDP** Before the first work turn of a propose run, an apply run, or a rework run, the application runs the **image probe**: `openspec --version` in a container of the run's image, through `probe_agent_image` of `../tools/EAC-execute-agent-cli.md` EAC-FR-WYYZ. The probe has the 30-second limit of `OSM-openspec-mode.md` OSM-FR-ZIMO and the retry of OFP-FR-PQNS.
44. **OFP-FR-XBNJ** The image's CLI is usable where the probe exits with status zero and prints a version whose major number is 1 or more (the rule of `OSM-openspec-mode.md` OSM-FR-ZNVZ). Otherwise the run rests by OFP-FR-TBHX with `openspec_image_cli_unavailable` and the state `not_found` where the probe exits with status 127, `unsupported` for a lower version, and `failed` for every other outcome of the CLI. A timeout, a signal, and a container that does not start are `transient` and get the retry. An error of the runtime or of the image setting is not a probe outcome: the probe records nothing, and the dispatch states that error on its own terms (per `../tools/EAC-execute-agent-cli.md`).
44. **OFP-FR-RKMB** Where the isolation of the run cannot run a command in place of the image's entry point (Microsandbox), the image probe is skipped: the dispatch goes on, records nothing, and emits a `WARN` record with the run id and the code `image_probe_skipped`. The agent's own `openspec` calls then report a missing CLI in its work.
45. **OFP-FR-WACS** A probe that passes records the version in the run's checkpoint as `openspec_image_cli` (per `GXD-graduation-execution.md` GXD-FR-GMDI), and no later dispatch of the run probes again. A probe that fails records nothing, and Continue probes again before it dispatches the turn.
46. **OFP-FR-JBSO** The image probe is not a turn. It spends no pass, no turn time, and no stage time, starts no agent CLI, mounts no credential, and emits no agent activity.
47. **OFP-FR-LXYP** Each image probe emits through `LGC-logging.md` under the `backend` domain an `INFO` record with the run id, the version, and the duration, or a `WARN` record with the run id, the code, and the state. No record holds the probe output.
48. **OFP-FR-XDXA** Every behaviour of this module is exercisable with no OpenSpec CLI, no container runtime, no network, and no real wait, through the process seam of `OSM-openspec-mode.md`, the wait seam (OFP-FR-MJZU), and the probe seam of `../tools/EAC-execute-agent-cli.md`.

## Contract surface
This module registers no Tauri command. It defines the failure classes, the `OpenSpecFailure` object that the commands below reject with, the merge notice, and the image probe.

### Commands that reject with `OpenSpecFailure` (OFP-FR-QLQJ)
- `read_openspec_change`, `read_apply_progress` (`OSM-openspec-mode.md`): every failure.
- `load_specification_map` (`OSI-openspec-spec-index.md`): every failure.
- `start_graduation` for a propose start (`GSU-graduation-start.md` GSU-FR-MTDA): `openspec_mode_inactive` and `cli_unavailable`.
- `start_change_apply`, `start_change_rework` (`OCR-openspec-change-runs.md`), and `confirm_change_action_request` (`CAR-change-action-requests.md`): `openspec_mode_inactive`, `cli_unavailable`, `change_not_ready`, and `apply_progress_unavailable`.
- `merge_work_stream` (`WKS-work-streams.md`): `change_not_archivable` and `archive_failed`.
- `continue_graduation_run` (`GRD-graduation.md`): `openspec_validation_unavailable` (per `OPU-openspec-plan-updates.md` OPU-FR-PXDY).

`get_openspec_status` fails for no CLI state (OSM-FR-YXUC). Its `recheck` input is the re-check of OFP-FR-ITUD.

### Internal (Rust API)
- `classify_openspec_failure(code)` → `FailureClass` (OFP-FR-PHOW). Pure. Called by the backend and mirrored by the table of `../ui/OFN-openspec-failure-notices.md`.
- The wait seam (OFP-FR-MJZU): `wait(delay_ms)`, real in production and recorded in tests.
- `give_merge_notice(stream_id, change, failure)` → `MergeNotice` (OFP-FR-ETEN). Called by `merge_work_stream` alone, after its guards are released (no UI consumer).
- `probe_openspec_image(run_id)` (OFP-FR-BZDP). Called by the dispatch of a propose, apply, or rework run before its first work turn (no UI consumer).

### Payload shapes
```text
FailureClass = "environment" | "transient" | "content" | "none"

OpenSpecFailure {                // camelCase on the wire (OFP-FR-QLQJ)
  code,                          // the refusal code of the operation
  cause?,                        // the OpenSpec code under a wrapping refusal (OFP-FR-ZDEE)
  change?,                       // the change the refusal names
  detail?,                       // OFP-FR-EOAD
  threadId?,                     // a merge notice's thread (OFP-FR-MQGG)
  repairRunId?,                  // the repair run (OFP-FR-XQDY)
  repairRefusal?,                // the code that refused the repair start (OFP-FR-PCQY)
  diagnostics                    // [CliDiagnostic]; empty where there is none (OFP-FR-OFLU)
}

CliDiagnostic {
  severity?,                     // "error" | "warning" | "info", as the CLI gives it
  code?,                         // a CLI status code
  path?,                         // a validation issue's path
  message,                       // at most 2 KiB (OFP-FR-QOET)
  fix?                           // at most 2 KiB
}

MergeNotice {
  threadId?, repairRunId?, repairRefusal?
}
```

### Class table
```text
environment   openspec_mode_inactive
              cli_unavailable                    detail: not_found | unsupported | failed
              cli_output_unreadable
              openspec_image_cli_unavailable     detail: not_found | unsupported | failed
transient     cli_timed_out
              cli_failed                         detail: the exit status, empty for a signal
content       cli_rejected
              change_not_ready                   detail: the missing artifacts
              tasks_incomplete
              archive_spec_update_failed         repair cause (OFP-FR-HQQN)
              archive_spec_validation_failed     repair cause
              archive_validation_failed          repair cause
              archive_tasks_incomplete
              archive_target_exists
              archive_error
              every other CLI status code of an archive: a code that starts with
              `archive_`, other than `archive_failed`
none          every other code (OFP-FR-FKRV)
```

The wrapping refusals `apply_progress_unavailable`, `openspec_validation_unavailable`, `change_not_archivable`, and `archive_failed` take the class of their `cause` (OFP-FR-BYYT).

### Fixed text
- The notice text of a repair cause, which is also the instruction of the repair run (OFP-FR-EJPE, OFP-FR-XQDY): `The stream merge could not archive the OpenSpec change "<change>" (<cause>). Change the deltas of "<change>" so that they apply to the live specifications and the change validates with "openspec validate <change> --strict". The OpenSpec CLI reported:`
- The notice text of every other cause: `The stream merge could not archive the OpenSpec change "<change>" (<cause>). Nothing was merged. The OpenSpec CLI reported:`
- Each diagnostic line: `- <severity> <code> <path>: <message>`, each absent part left out, followed by ` Fix: <fix>` where the entry has a fix.

### Storage
```text
app_data_dir()/repositories/<slug>-<digest>/
  changes/<stream-id>/<change>/merge-notice.toml    // the notice record (OFP-FR-HVHV):
                                                    //   { digest, cause, thread_id, repair_run_id?,
                                                    //     repair_refusal?, recorded_at }
```

The record moves with the change's store folder at an archive (per `OAR-openspec-archive.md` OAR-FR-MSOM). A new notice replaces it.

### The image requirement
A project image that runs OpenSpec runs holds Node 22.12.0 or later and the OpenSpec CLI, version 1.0 or later, as `openspec` on the `PATH` of the launching user. The shipped images meet it (per `../infra/AVI-agent-vendor-images.md` AVI-FR-RZFR).

## Non-functional requirements
- A transient failure costs at most one more process and 2 seconds for each process of a read. A read of one subcommand ends in at most about 62 seconds.
- The archive step of a stream merge holds the repository update guard for about two minutes for each change in the worst case: one check read with its retry and one archive with its restore and its retry.
- A merge notice costs one discussion, one comment, one record write, and one start check. It runs outside every guard.
- The image probe costs one container start for each run, and none for a run whose checkpoint holds `openspec_image_cli`.
