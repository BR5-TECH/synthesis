# Change edit proposals

**Spec code:** `CEP`

## Intent
The persistence home for the **edits an agent proposes to an OpenSpec change**: an ordered set of anchored hunks across the files of one change, offered in that change's discussion and held until the author decides each of them. An OpenSpec change is several files that must agree with one another, so one revision can touch the proposal, a delta, and the tasks together. One proposal holds all of its hunks, so that the author confirms every edit as OpenSpec's own update does, but in the text each edit belongs to. A change lives on a work stream (per `OSM-openspec-mode.md` OSM-FR-MFVN), so an accepted hunk writes its file in the stream's working copy at once, and one commit on the stream branch holds the accepted hunks of a proposal when the proposal closes. The hunk model, the decisions, the updates, and the decision comment are those of `PCP-prompt-change-proposals.md`, applied to a change in place of one file. Out of scope: composing a proposal, which is `../tools/PCE-propose-change-edits-tool.md`'s, for the result of a rework run `OCR-openspec-change-runs.md`'s, and for a plan update of an apply run `OPU-openspec-plan-updates.md`'s; deciding by tool, which is `../tools/DCE-decide-change-edits-tool.md`'s; the review surface, which is `../ui/OCW-openspec-change-workspace.md`'s; and a change in a worktree that is not a stream, which holds no proposal.

## Functional requirements
1. **CEP-FR-HXCC** A **change edit proposal** belongs to one change on one stream, named by the stream id and the change name. It holds one or more hunks in proposal order. Each hunk names a change-relative path, and its anchoring, its kinds, and its resolution are those of `DCP-draft-change-proposals.md` DCP-FR-HRQN and DCP-FR-VZTK.
2. **CEP-FR-ZOIG** A change holds at most one proposal that is `pending`. A recording against a change that holds one is refused with `proposal_pending` (on the terms of `PCP-prompt-change-proposals.md` PCP-FR-04).
3. **CEP-FR-OIZT** A recording on a change for which a rework run is not terminal (per `OCR-openspec-change-runs.md` OCR-FR-OXVH) is refused with `rework_running`, and nothing is recorded. The recording of that run's own finish is not refused for it.
3. **CEP-FR-YYZZ** A recording on a change for which an apply run is not terminal is refused with `apply_running`, and nothing is recorded (per `OPU-openspec-plan-updates.md` OPU-FR-DEEB). The recording of that run's plan proposal (OPU-FR-OQRA, OPU-FR-WOKJ) and an update of the pending proposal are not refused for it.
3. **CEP-FR-IISM** Proposals live in the repository machine store, in `changes/<stream-id>/<change>/proposals/`, outside every worktree and never committed (per `RMS-repository-machine-storage.md`). A record and a hunk document have the shapes of `PCP-prompt-change-proposals.md` PCP-FR-HKLD and PCP-FR-09, with a path on each hunk.
4. **CEP-FR-WJKY** A hunk path is a relative path inside `openspec/changes/<change>/` of the stream's working copy. A path that is absolute, holds a `..` segment, or leaves that folder is refused with `path_outside_change`, and nothing is recorded.
5. **CEP-FR-HRCX** A hunk whose path names no file of the change is a **new file**. Its kind is `add` with no anchor, and its text is the whole file. Accepting it creates the file. A new-file hunk on a path that holds a file at acceptance is refused with `file_exists`.
6. **CEP-FR-AYVS** A recording, an update, and a hunk resolution read each file of the stream's working copy through `FSA-filesystem-access.md`. They refuse on the codes of `PCP-prompt-change-proposals.md` PCP-FR-05 and PCP-FR-RVSN, with `change_not_found` in place of `artifact_not_found`.
7. **CEP-FR-QDJH** `record_change_proposal(stream_id, change, hunks, rationale, agent, origin)` is the one path that creates a proposal. As one act, on the terms of `PCP-prompt-change-proposals.md` PCP-FR-05, it records the proposal and appends one agent comment with the `change_proposal` attachment (`CMS-comments-storage.md` CMS-FR-QLMV) into its origin: a turn's conversation, a rework run's request thread (`OCR-openspec-change-runs.md` OCR-FR-WBZX), or a plan thread (`OPU-openspec-plan-updates.md` OPU-FR-OQRA).
8. **CEP-FR-AXEV** `update_change_proposal(stream_id, change, updates, rationale, agent, origin)` changes, adds, and withdraws hunks of the pending proposal, with the semantics and the refusals of `PCP-prompt-change-proposals.md` PCP-FR-RVSN and PCP-FR-RKJI. An added hunk names its path. Its comment carries the `change_proposal` attachment with the counts of the update (per `CMS-comments-storage.md` CMS-FR-BSQB).
9. **CEP-FR-OPGW** `accept_change_edit_hunk(proposal_id, hunk_id, feedback?)` resolves one hunk against its file as it stands in the stream's working copy and writes that whole file, in a transaction with the commit point of `PCP-prompt-change-proposals.md` PCP-FR-14.
10. **CEP-FR-ESQR** An acceptance is refused with `stream_busy` while a graduation run of the stream is `working` or `reviewing` (per `GRD-graduation.md`). It writes nothing, and the hunk stays undecided.
    - *Why:* A working run writes the same working copy, so a file written under it would reach its commit as work it did not do, while a run at rest writes nothing.
10. **CEP-FR-KGOL** An acceptance, a rejection, an edit, a hold, and a decline of the plan proposal of a plan update are refused with `plan_questions_pending` while the question set of that plan update stands (per `OPU-openspec-plan-updates.md` OPU-FR-EVBZ). They write nothing, and the hunk stays as it was.
11. **CEP-FR-LYSS** An acceptance is refused with `unknown_stream` where the stream is not a live stream of the project, and with `change_not_found` where `openspec/changes/<change>/` is not a folder in its working copy.
12. **CEP-FR-UCVO** `reject_change_edit_hunk`, `set_change_edit_hunk_discussing`, `edit_change_edit_hunk`, and `decline_change_edit_proposal` have the semantics of the PCP operations of the same names (PCP-FR-RJHN, PCP-FR-HLDQ, PCP-FR-EDTW, PCP-FR-15). They write nothing into the working copy.
13. **CEP-FR-RWTQ** A proposal **closes** when no hunk of it is undecided. Only the decision that closes it appends the one decision comment, on the terms of `PCP-prompt-change-proposals.md` PCP-FR-16 and PCP-FR-RSLV.
14. **CEP-FR-FOJW** When a proposal closes with at least one accepted hunk, the application makes one commit on the stream branch in its working copy. The commit holds exactly the files that the accepted hunks of that proposal wrote, and its message is `Accept edits to OpenSpec change <change>`.
15. **CEP-FR-XVXQ** The commit of CEP-FR-FOJW is made through `GTC-git.md` and never pushes. A path that another writer also changed is committed as it stands on disk, because the commit holds files and not hunks.
15. **CEP-FR-DQAJ** A proposal whose close commit lands records the revision of that commit as `commit`. A proposal that closes with no accepted hunk, or whose commit is owed, records none until its commit lands.
16. **CEP-FR-IYHP** A commit that fails marks the proposal `commit_owed`. The files stay written. The next close of a proposal of the same change, and `complete_change_edit_decision`, commit every file still owed, and clear the mark.
17. **CEP-FR-OAIW** A proposal that closes with no accepted hunk makes no commit.
18. **CEP-FR-HUFE** `list_change_edit_proposals(stream_id, change)` returns every proposal of the change, most recently created first. `load_change_edit_proposal_hunks(proposal_id)` returns its hunks, each with its path, on the terms of `PCP-prompt-change-proposals.md` PCP-FR-LDHK.
19. **CEP-FR-XXMW** `read_change_proposal_changes(proposal_id)` returns one proposal's change, counts, and hunks in proposal order, each with its path, for a turn that assembles its input (per `AGC-agent-conversations.md` AGC-FR-EPNB).
20. **CEP-FR-SQCG** `"change edit proposals changed"` is emitted on a recording, a hunk decision, an update, a hold, a commit, and a cleared `commit_owed`. It carries the stream id, the change name, the proposal, and the cause.
21. **CEP-FR-KLWZ** A decision by `decide_change_edits` (`../tools/DCE-decide-change-edits-tool.md`) on the author's explicit instruction is a decision of the author, and calls the functions behind the Tauri commands in-process, on the terms of `PCP-prompt-change-proposals.md` PCP-FR-VCBX and PCP-FR-MXSB.
22. **CEP-FR-FQMZ** This module, `PCP-prompt-change-proposals.md`, and `DCP-draft-change-proposals.md` share no state and no operation. All three use the one pure hunk module of PCP-FR-29.
23. **CEP-FR-GGDV** No proposal expires, is collected, is retried, is auto-accepted, or is auto-declined. This module dispatches no agent turn and reaches no model.
24. **CEP-FR-TFOY** Every command errors with `no_project_open` when no project is open, and with `proposal_not_found` when `proposal_id` names no proposal of the open project.
25. **CEP-FR-MZFF** Every operation logs through `LGC-logging.md` under the `backend` domain: an `INFO` record when a proposal is recorded, updated, or decided, or a commit is made, and a `WARN` record on every refusal and failed commit. A record names the stream id, the change name, the proposal id, and counts. It holds no hunk text and no file text.
26. **CEP-FR-EODY** Every filesystem operation goes through `FSA-filesystem-access.md`, on the terms of `PCP-prompt-change-proposals.md` PCP-FR-25. Nothing in this module reaches the network.
27. **CEP-FR-FBVC** A decision's outcome states whether the decision closed the proposal, the id of the decision comment it appended where it appended one, and the origin kind of the proposal's conversation, so the surface dispatches the decision's own turns on the terms of `../ui/PCR-prompt-change-review.md` PCR-FR-14.
27. **CEP-FR-QVLO** A proposal that closes contributes exactly one `proposal_decision` event to its change's statistics log (per `CST-change-statistics.md` CST-FR-OXGK), after the closing decision has committed and never before: `accepted` where at least one hunk is accepted, `rejected` otherwise. A recording, an update, a hold, an edit, and a decision that leaves the proposal open contribute none.

## Contract surface

### Tauri commands
- `"list change edit proposals"` → `list_change_edit_proposals(stream_id, change)` → `ChangeEditProposal[]` (CEP-FR-HUFE).
- `"load change edit proposal hunks"` → `load_change_edit_proposal_hunks(proposal_id)` → `{ hunks: ChangeEditHunk[], resolutions: HunkResolution[], checksum }` (CEP-FR-HUFE).
- `"accept change edit hunk"` → `accept_change_edit_hunk(proposal_id, hunk_id, feedback?)` → `ChangeDecisionOutcome` (CEP-FR-OPGW).
- `"reject change edit hunk"` → `reject_change_edit_hunk(proposal_id, hunk_id, feedback?)` → `ChangeDecisionOutcome` (CEP-FR-UCVO).
- `"hold change edit hunk"` → `set_change_edit_hunk_discussing(proposal_id, hunk_id, discussing)` → `ChangeEditProposal` (CEP-FR-UCVO).
- `"edit change edit hunk"` → `edit_change_edit_hunk(proposal_id, hunk_id, after, baseline_checksum)` → `{ checksum }` (CEP-FR-UCVO).
- `"decline change edit proposal"` → `decline_change_edit_proposal(proposal_id, feedback?)` → `ChangeDecisionOutcome` (CEP-FR-UCVO).
- `"complete change edit decision"` → `complete_change_edit_decision(proposal_id)` → `ChangeDecisionOutcome` (CEP-FR-IYHP).

### Events
- `"change edit proposals changed"` — carries `{ stream_id, change, proposal, cause }` (CEP-FR-SQCG).

### Payload shapes
```
ChangeEditProposal {
  id, stream_id, change,
  state,                // "pending" | "accepted" | "rejected", derived as PCP-FR-HKLD derives it
  hunks: HunkLedgerRow[],   // PCP's ledger row, with `path` on each row, and `feedback?`: the author's feedback of the decision that settled the row (per `OPU-openspec-plan-updates.md` OPU-FR-ZNRR)
  counts, hunk_count,
  comment_owed: boolean,
  commit_owed: boolean, // CEP-FR-IYHP
  commit?,              // CEP-FR-DQAJ; the revision of its close commit
  created_at, decided_at?
}

ChangeEditHunk {
  id, path,             // change-relative (CEP-FR-WJKY)
  new_file: boolean,    // CEP-FR-HRCX
  kind, before, after, after_text?, lead?, trail?,
  state                 // PCP's hunk states
}

ChangeDecisionOutcome {
  proposal: ChangeEditProposal,
  committed: boolean,   // true where this decision closed the proposal and its commit landed
  resolved: boolean,    // true where this decision left no hunk undecided (CEP-FR-RWTQ)
  comment_id?,          // the decision comment; absent where none was appended
  origin_kind           // "change_discussion" | "change_comment" (AGC-FR-05)
}
```

### Internal (Rust API)
- `record_change_proposal(stream_id, change, hunks, rationale, agent, origin)` — called by `../tools/PCE-propose-change-edits-tool.md`, which also serves the synthesis turn of an agent group on a change (per `AGC-agent-conversations.md` AGC-FR-WPAP), by the finish of a rework run (per `OCR-openspec-change-runs.md` OCR-FR-WBZX), and by the recording of a plan update (per `OPU-openspec-plan-updates.md` OPU-FR-OQRA) (CEP-FR-QDJH, CEP-FR-OIZT, CEP-FR-YYZZ).
- `update_change_proposal(stream_id, change, updates, rationale, agent, origin)` — called by `../tools/PCE-propose-change-edits-tool.md` alone (CEP-FR-AXEV).
- `read_change_proposal_changes(proposal_id)` — called by `AGC-agent-conversations.md` alone (CEP-FR-XXMW).
- The functions behind the Tauri commands, called in-process by `../tools/DCE-decide-change-edits-tool.md` (CEP-FR-KLWZ).

## Non-functional requirements
- A decision costs one file write and, at a close, one commit. Nothing grows with the number of earlier proposals of the change.
- The store is private to this machine, like the comment logs of `CMS-comments-storage.md`, so an author's proposals never reach the repository.
