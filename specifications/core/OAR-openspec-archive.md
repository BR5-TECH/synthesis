# OpenSpec archive

**Spec code:** `OAR`

## Intent
When a work stream merges into its base branch, the OpenSpec changes that the stream built must leave the active changes and become part of the live specifications. OpenSpec does this with `openspec archive`: it merges the deltas of a change into `openspec/specs/` and moves the change folder to `openspec/changes/archive/<YYYY-MM-DD>-<name>/`. This module runs that archive on the stream branch just before the Git merge of `WKS-work-streams.md` WKS-FR-GKPX, with one commit for each change, so the base branch gets the built code, the updated live specifications, and the archived change in one merge. The application runs the archive, not an agent, and only for a change whose tasks are all done and whose discussion holds nothing that the author did not decide. An archive commit stays on the stream also when the merge does not complete, and the next merge carries it. The records that the application keeps about a change move with it, so a later change of the same name starts with none. Out of scope: the Git merge and the merge run, which are `WKS-work-streams.md`'s and `GRD-graduation.md`'s; the reads of OpenSpec, which are `OSM-openspec-mode.md`'s; the classes of a failure, the retry of an archive, and the merge notice that a refused archive gives, which are `OFP-openspec-failure-policy.md`'s; the archive of a change that a direct run built, because no direct apply run exists (per `OCR-openspec-change-runs.md` OCR-FR-RYKA); an archive outside a stream merge; and a lifecycle view of a change.

## Functional requirements
1. **OAR-FR-KGDP** A **change to archive** of a stream merge is a change that a completed apply run of the stream names (per `OCR-openspec-change-runs.md` OCR-FR-QMCG), where that run has no revert on the stream branch (per `GRD-graduation.md` GRD-FR-RVXK) and no landing on the base branch, and where `openspec/changes/<change>/` is a folder in the stream's working copy.
2. **OAR-FR-SURB** The changes to archive are found from the run records of the stream and from the working copy alone, while OpenSpec mode is active and while it is not. They are archived in the order in which the first such apply run of each change completed, oldest first. A run completes at its last transition to the stage `done`, or at its last update where it records no such transition. Times are compared as instants, and a time that does not parse sorts last. One merge archives a change once.
3. **OAR-FR-LZLW** A stream merge with no change to archive starts no CLI process, makes no archive commit, and merges on the terms of `WKS-work-streams.md` alone.
4. **OAR-FR-SEQU** The **archive step** runs inside `merge_work_stream`, under its repository update guard (per `WKS-work-streams.md` WKS-FR-HLGN), after the clean check of both sides (WKS-FR-UZHT), after the merge finds that the stream holds commits that its base branch does not (WKS-FR-FOTC), and before the merge is prepared. It works in the stream's working copy.
5. **OAR-FR-CCIN** The archive step checks every change to archive, in archive order, before it archives any. A check that fails refuses the merge with its typed refusal. Nothing is then archived or committed, and nothing is written to either branch, to either working copy, or to the store, apart from the merge notice of `OFP-openspec-failure-policy.md` OFP-FR-ETEN.
6. **OAR-FR-GJVM** The step checks the pending items of a change (OAR-FR-BEIK) before its archivability (OAR-FR-QTBK). The merge is refused with the first refusal in archive order.
7. **OAR-FR-BEIK** A change that holds a pending item is refused with `change_has_pending: <change>: <kinds>`. `<kinds>` names, separated by commas, each that applies: `edit_proposal` for a pending change edit proposal or one with `commit_owed`, `new_change_proposal` for a pending new-change proposal or one with `commit_owed`, and `action_request` for a pending change action request.
   - *Why:* An archive moves the records of the change (OAR-FR-MSOM), so an item that the author did not decide would be lost with them.
8. **OAR-FR-GTGQ** The pending items are read from `CEP-change-edit-proposals.md` (CEP-FR-ZOIG, CEP-FR-IYHP), `NCP-new-change-proposals.md` (NCP-FR-KRSA), and `CAR-change-action-requests.md` (CAR-FR-KYJU). A plan proposal is a change edit proposal. A plan rest (per `OPU-openspec-plan-updates.md` OPU-FR-RMZN) holds the stream, so WKS-FR-GKPX refuses the merge with `stream_busy` before this step runs.
9. **OAR-FR-QTBK** A change with no pending item is refused with `change_not_archivable: <change>: <reason>` where it cannot be archived. The reason is `openspec_mode_inactive` where OpenSpec mode is not active, `cli_unavailable` where the CLI is not usable, the code of the read where `apply_instructions` fails, such as `cli_timed_out` or `cli_rejected`, and `tasks_incomplete` where its state is not `all_done`.
10. **OAR-FR-AEOK** The check of OAR-FR-QTBK reads `apply_instructions` in the stream's location (per `OSM-openspec-mode.md` OSM-FR-QLRW) once for each change to archive, and runs no other CLI process.
    - *Why:* `openspec archive --yes` does not check that the tasks are done, so the application makes that check itself.
11. **OAR-FR-IAKL** After every check passes, the application runs `openspec archive <change> --json --yes` for each change, in archive order, in the root of the stream's working copy. It uses the CLI that OSM-FR-EZKW resolves, on the process terms of OSM-FR-HHUD, OSM-FR-ITZI, OSM-FR-ZIMO, and OSM-FR-QHRV (per OSM-FR-XKQB).
12. **OAR-FR-VAGH** An archive **succeeds** where the process exits with status zero and its output decodes with an `archive` object whose `change` is the change name and whose `archivedAs` is valid (OAR-FR-XTLB). The **archive folder** is `openspec/changes/archive/<archivedAs>/`.
13. **OAR-FR-XTLB** `archivedAs` is valid only where it is one path segment of the form `<YYYY-MM-DD>-<change>`. An archive whose `archivedAs` is not valid does not succeed, with the code `cli_output_unreadable`.
14. **OAR-FR-LZLS** After an archive that succeeds, the application makes one commit on the stream branch in its working copy through `GTC-git.md`, with the message `Archive OpenSpec change <change>` on one line with no body. The commit never pushes.
15. **OAR-FR-BLBJ** The archive commit holds exactly the paths below `openspec/changes/<change>/`, the archive folder, and `openspec/specs/` that differ from the stream branch head. It stages no other path. A stream working copy that holds any other uncommitted path after the commit refuses the merge with `stream_dirty: <paths>` (per `WKS-work-streams.md` WKS-FR-UZHT), the commit stays, and the other paths stay as the CLI left them.
    - *Why:* The working copy was clean at the check (WKS-FR-UZHT), so each such path is a write of the archive.
16. **OAR-FR-EUXZ** An archive that does not succeed, or whose commit fails, refuses the merge with `archive_failed: <change>: <code>`. The code is the `code` of the first `status` entry where the output decodes with a null `archive`, `commit_failed` where the commit fails, and otherwise the code of the read: `cli_timed_out`, `cli_failed`, `cli_rejected`, or `cli_output_unreadable`. A first `status` entry with no `code` is read on the exit-status rules below. Where OpenSpec mode is not active between the check and the archive, the code is `openspec_mode_inactive`, and where the CLI is not usable, `cli_unavailable`; no archive process then starts. Only exit statuses 0 and 1 are read. Any other status, an exit by a signal, and exit 1 with output that is not CLI JSON is `cli_failed`. Exit 1 with CLI JSON and no `status` entry is `cli_rejected`, and exit 0 with a null `archive` and no `status` entry is `cli_output_unreadable`.
17. **OAR-FR-KBDD** After an archive that fails, the application restores each path below the three folders of OAR-FR-BLBJ that differs from the stream branch head in the stream's working copy. A tracked path gets its content at the head, and an untracked path is removed. A folder below the three folders that the restore empties is removed, and the three folders themselves stay. The restore is judged by the working copy afterwards, not by the write of the Git index. No other path is changed.
    - *Why:* The CLI undoes its own writes on most failures but not on all of them, and the merge must leave the stream clean.
18. **OAR-FR-OQVF** An archive commit stays on the stream branch on every later outcome of its merge: a later archive of the same merge that fails, a refusal, a conflict, and a merge run that is discarded, fails, or does not complete. The next merge of the stream carries it and does not archive that change again (OAR-FR-KGDP).
19. **OAR-FR-SOFQ** The merge result names each change that the merge archived, in archive order, as `{ change, archivedAs }` in `archived` of the `merged` and the `conflicted` kinds (per `WKS-work-streams.md` WKS-FR-QNHF). A refusal after an archive commit carries its own code alone.
20. **OAR-FR-MSOM** In the same operation as its commit, the archive moves the change's folder of the repository machine store from `changes/<stream-id>/<change>/` to `changes/<stream-id>/archive/<archivedAs>/` (per `RMS-repository-machine-storage.md`). The proposals, the action requests, the new-change proposals, the split record, the notice record of `OFP-openspec-failure-policy.md` OFP-FR-HVHV, the conversation logs, and the statistics log move with it.
    - *Why:* `archive` is never a change name (per `OSM-openspec-mode.md` OSM-FR-IHUO), so the archive folder cannot be the folder of a change.
21. **OAR-FR-LUSO** After the move, a change of the same name on that stream starts with no record in the store. A store folder that does not exist moves nothing. Where the target folder exists, the folder moves to the target path with the suffix `-2`, or with the lowest free suffix above it.
22. **OAR-FR-KSZI** A store move that fails emits a `WARN` record with the code `store_move_failed` and does not refuse the merge. The folder stays where it is, and the archive commit stays.
23. **OAR-FR-BMBR** No operation of the application reads the records of an archived change at its archive folder. The change workspace states that the change is archived (per `../ui/OCW-openspec-change-workspace.md` OCW-FR-AKVG), and no surface shows an archived change beyond that.
24. **OAR-FR-VSSH** The archive step is a part of the in-flight operation `Merging <stream name>` of WKS-FR-MWYD, and it reports no operation of its own.
25. **OAR-FR-HQAI** Every check, archive, commit, restore, and move emits through `LGC-logging.md` under the `backend` domain: an `INFO` record with the stream id, the change name, `archivedAs`, the count of committed paths, and the duration, or a `WARN` record with the refusal or failure code.
26. **OAR-FR-NEEI** A log record may hold the `code` of a CLI `status` entry, which is a fixed identifier of the CLI. A `code` that is not 1 to 64 characters of `A-Z`, `a-z`, `0-9`, `_`, `.`, and `-` is read as `cli_output_unreadable` and reaches no refusal and no record. No record holds the `message` or the `fix` of a `status` entry, other CLI output, a project path other than the change name, or file text (on the terms of OSM-FR-IFWM).
27. **OAR-FR-EZNX** Every filesystem read and write goes through `FSA-filesystem-access.md`, and every Git read and write goes through `GTC-git.md`. Nothing in this module reaches the network.
28. **OAR-FR-NIIP** Every behaviour of this module is exercisable with no OpenSpec CLI, no container runtime, and no network, through the process seam of `OSM-openspec-mode.md`.

## Contract surface
This module registers no Tauri command. `WKS-work-streams.md`'s `merge_work_stream` is its one consumer.

### Internal (Rust API)
- `archive_stream_changes(stream_id, base_branch)` → `[ArchivedChange]`, or an `ArchiveRefusal` (OAR-FR-SEQU). Called by `merge_work_stream` under its guard (no UI consumer). `merge_work_stream` carries `change_not_archivable` and `archive_failed` on the wire as an `OpenSpecFailure` of `OFP-openspec-failure-policy.md`, with the reason or the code as `cause` and the `status` entries as `diagnostics`.

### Payload shapes
```text
ArchivedChange {                 // camelCase on the wire, in StreamMergeResult.archived
  change,                        // the change name
  archivedAs                     // "<YYYY-MM-DD>-<change>" (OAR-FR-XTLB)
}

ArchiveRefusal =
  | "change_has_pending: <change>: <kinds>"         // OAR-FR-BEIK
  | "change_not_archivable: <change>: <reason>"     // OAR-FR-QTBK
  | "archive_failed: <change>: <code>"              // OAR-FR-EUXZ

PendingKind = "edit_proposal" | "new_change_proposal" | "action_request"

NotArchivableReason =
  | "openspec_mode_inactive" | "cli_unavailable"
  | "cli_timed_out" | "cli_failed" | "cli_rejected" | "cli_output_unreadable"
  | "tasks_incomplete"
```

### CLI fields read
- `archive <change> --json --yes`: `archive.change`, `archive.archivedAs`, and `status[].code` where `archive` is null, with `status[].severity`, `status[].message`, and `status[].fix` as diagnostics. Every other field is ignored.

### Fixed text
- The archive commit message: `Archive OpenSpec change <change>`.

### Storage
```text
app_data_dir()/repositories/<slug>-<digest>/
  changes/<stream-id>/<change>/                  the records of an active change
  changes/<stream-id>/archive/<archivedAs>/      the same records after an archive (OAR-FR-MSOM)
```

## Non-functional requirements
- An archive step costs one CLI process for each change to archive at the check, and one CLI process and one commit for each change at the archive. A stream with no change to archive costs nothing beyond a read of its run records.
- Each CLI process is bounded by the 30 seconds of OSM-FR-ZIMO and runs at most twice (per `OFP-openspec-failure-policy.md` OFP-FR-PQNS). The guard of a merge that archives is held for seconds for each change in the usual case, and for about two minutes for each change in the worst case.
- The archive date of `archivedAs` is the local date of the machine, which the CLI decides.
