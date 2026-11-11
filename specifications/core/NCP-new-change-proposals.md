# New change proposals

**Spec code:** `NCP`

## Intent
The persistence home for a **split** of an OpenSpec change: an agent proposes, one at a time, the new changes that take over parts of a change that grew too big, and the author accepts, rejects, or discusses each of them. It is the decomposition of a draft (per `DCP-draft-change-proposals.md` DCP-FR-VQWK through DCP-FR-TRPW) for a change: one proposal at a time, nothing created until the author accepts, and only the author ends the split. A new change lives on the same work stream as the change it comes from, so the changes apply and merge together: accepting a proposal writes the new change folder in the stream's working copy and commits it on the stream branch. The original change is not edited, deleted, or archived by a split. The author trims it through edit proposals (per `CEP-change-edit-proposals.md`), and confirming the split records which changes took over and closes the split. Out of scope: composing a proposal, which is `../tools/PNC-propose-new-change-tool.md`'s; the card and its controls, which are `../ui/OCW-openspec-change-workspace.md`'s; deciding by tool, which is `../tools/DCE-decide-change-edits-tool.md`'s; and a split on a worktree that is not a stream, which holds no proposal.

## Functional requirements
1. **NCP-FR-AZON** A change's new-change proposals live in the repository machine store, in `changes/<stream-id>/<change>/new-changes/`. Each proposal is two files: `<proposal-id>.toml` holding the record and `<proposal-id>.files` holding the proposed files. Nothing of it is committed.
2. **NCP-FR-MTCW** A proposal names the new change by an OpenSpec change name (per `OSM-openspec-mode.md` OSM-FR-IHUO) and holds every file of the new change folder: each a change-relative path (per `CEP-change-edit-proposals.md` CEP-FR-WJKY) and its full text. It also holds `position`, `total`, and `rationale`, on the terms of DCP-FR-OBAW.
3. **NCP-FR-VTPG** `record_new_change_proposal` is the only path by which a proposal comes into existence. It writes the files, then the record, then appends one agent comment carrying `rationale` and one `new_change_proposal` attachment (per `CMS-comments-storage.md` CMS-FR-QYGP). A failed append removes both files.
4. **NCP-FR-RPGN** A recording refuses, writing nothing: `new_change_proposal_pending` while the change holds a pending new-change proposal; `openspec_change_exists` where the name names a change folder of the stream's working copy or a pending or accepted proposal of the split; `invalid_change_name` for a bad name; `missing_proposal_file` where the files hold no `proposal.md`; `path_outside_change` for a path that fails CEP-FR-WJKY; `split_confirmed` where the split is confirmed; `discussion_resolved` where the thread is resolved; and the validation errors of DCP-FR-OBAW for `position` and `total`.
5. **NCP-FR-NDHQ** The first recording on a change mints the split's `set_id`, and every later recording on that change takes it. A change has at most one split.
6. **NCP-FR-QBIF** A pending new-change proposal and a pending change edit proposal of the same change stand side by side. Neither refuses the recording of the other.
7. **NCP-FR-SNMA** `update_new_change_proposal` changes or withdraws the pending proposal, on the terms of DCP-FR-GDTC and DCP-FR-ZJRW. A `change` replaces the name, the files, the position, or the total it carries, and the rules of NCP-FR-RPGN apply to the fields that result.
8. **NCP-FR-UWOR** `accept_new_change_proposal(proposal_id, feedback?)` writes each proposed file into `openspec/changes/<name>/` of the stream's working copy, through `FSA-filesystem-access.md`, and then makes one commit on the stream branch through `GTC-git.md` that holds exactly those files, with the message `Add OpenSpec change <name>`. It never pushes.
9. **NCP-FR-BOVN** An acceptance refuses, writing nothing: `stream_busy` on the terms of CEP-FR-ESQR, `unknown_stream` and `change_not_found` on the terms of CEP-FR-LYSS, and `openspec_change_exists` where the folder of the name exists at the moment of the acceptance.
10. **NCP-FR-KRSA** After the files are written, the acceptance writes the record as `accepted` with `created_change` and `decided_at`, then appends the decision comment. A failure to write a file removes the files it wrote and changes nothing else. A failure of the commit leaves the files written and marks the record `commit_owed`, and `complete_new_change_decision` makes the owed commit and clears the mark.
11. **NCP-FR-TWDN** `reject_new_change_proposal(proposal_id, feedback?)` writes the record as `rejected` with `decided_at`, appends the decision comment, and writes nothing into the working copy.
12. **NCP-FR-EBSA** A decision appends one comment into the proposal's own thread through the human write path, stating the decision, the proposed name, the position and the total, and carrying any feedback, on the terms of DCP-FR-PDCP. The outcome carries that comment's id, the origin kind, and whether a commit landed.
13. **NCP-FR-WYZI** A decision against a proposal that is not `pending` is `already_decided`, and one naming no proposal is `proposal_not_found`. Nothing is written.
14. **NCP-FR-MQCS** `confirm_change_split(stream_id, change)` refuses `new_change_proposal_pending` while a proposal of the split is pending, `nothing_accepted` where none is accepted, and `split_confirmed` where the split is confirmed. Otherwise it writes `changes/<stream-id>/<change>/split.toml` holding the `set_id`, the accepted change names oldest first, and the confirmation time.
15. **NCP-FR-MHBS** A confirmation never edits, deletes, or archives the original change, and never changes a new change. It appends one human comment into the thread of the newest decided proposal that states the split is confirmed and names the accepted changes. It dispatches no agent turn.
16. **NCP-FR-AFZP** After a confirmation, recording, updating, accepting, rejecting, and confirming on that change each refuse `split_confirmed`.
17. **NCP-FR-VLOO** `"new change proposals changed"` is emitted on a recording, on an update after its comment, on a decision after its comment, on a commit that clears `commit_owed`, and on a confirmation. It carries the stream id, the change name, the proposal where one is concerned, and the cause.
18. **NCP-FR-CEIA** A new-change proposal is decided only by the author, on the card or by `../tools/DCE-decide-change-edits-tool.md` on the author's explicit instruction, which calls the functions behind the commands in-process with no feedback. No proposal expires or is decided on a timer, and no new-change decision contributes a statistics line.
19. **NCP-FR-LQJK** A discussion about a new-change proposal is linked to it by the `new_change_subject` attachment of its opening comment (per `CMS-comments-storage.md` CMS-FR-MTUL). Opening, resolving, and reopening it change no proposal state.
20. **NCP-FR-EZTC** `read_new_change_proposals(stream_id, change)` returns the split's `set_id`, whether it is confirmed, every proposal oldest first, and the files of the pending one, for a turn that assembles its input. It reconciles nothing and writes nothing.
21. **NCP-FR-OLPB** Every command errors with `no_project_open` when no project is open. Every operation logs through `LGC-logging.md` under the `backend` domain: an `INFO` record on a recording, an update, a decision, a commit, and a confirmation, and a `WARN` record on every refusal, each naming the stream id, the change name, the proposal id, the proposed name, and counts, and none holding file text or a rationale.
22. **NCP-FR-BFCK** This module, `CEP-change-edit-proposals.md`, and `DCP-draft-change-proposals.md` share no state and no operation. Nothing in this module reaches the network.

## Contract surface

### Tauri commands
- `"list new change proposals"` → `list_new_change_proposals(stream_id, change)` → `{ proposals: NewChangeProposal[], split: ChangeSplit? }`, most recently created first.
- `"load new change proposal files"` → `load_new_change_proposal_files(proposal_id)` → `{ files: { path, text }[] }`.
- `"accept new change proposal"` → `accept_new_change_proposal(proposal_id, feedback?)` → `NewChangeDecisionOutcome` (NCP-FR-UWOR).
- `"reject new change proposal"` → `reject_new_change_proposal(proposal_id, feedback?)` → `NewChangeDecisionOutcome` (NCP-FR-TWDN).
- `"confirm change split"` → `confirm_change_split(stream_id, change)` → `ChangeSplit` (NCP-FR-MQCS).
- `"complete new change decision"` → `complete_new_change_decision(proposal_id)` → `NewChangeDecisionOutcome` (NCP-FR-KRSA).

Errors are typed: `no_project_open`, `proposal_not_found`, `already_decided`, `unknown_stream`, `change_not_found`, `stream_busy`, `openspec_change_exists`, `invalid_change_name`, `missing_proposal_file`, `path_outside_change`, `new_change_proposal_pending`, `nothing_accepted`, `split_confirmed`, `discussion_resolved`, `write_failed`, and the identity refusals a decision inherits from its comment (per `CMS-comments-storage.md` CMS-FR-12).

### Events
- `"new change proposals changed"` — carries `{ stream_id, change, proposal?, cause }`, cause `"recorded" | "updated" | "decided" | "committed" | "confirmed"` (NCP-FR-VLOO).

### Payload shapes
```
NewChangeProposalState = "pending" | "accepted" | "rejected" | "withdrawn"

NewChangeProposal {
  id, set_id,
  stream_id, change,          // the original change
  name,                       // the proposed new change name
  paths: string[],            // change-relative paths of the proposed files
  position, total,
  agent: Participant,
  rationale, thread_id, comment_id,
  state: NewChangeProposalState,
  revision,
  created_change?,            // the name an acceptance created
  commit_owed: boolean,
  created_at, decided_at?
}

ChangeSplit { set_id, accepted: string[], confirmed_at? }

NewChangeDecisionOutcome {
  proposal: NewChangeProposal,
  committed: boolean,
  comment_id?,
  origin_kind                 // "change_discussion" | "change_comment"
}
```

### Internal (Rust API)
- `record_new_change_proposal(stream_id, change, name, files, position, total, rationale, agent, origin)` and `update_new_change_proposal(stream_id, change, action, fields, rationale, agent, origin)` — called by `../tools/PNC-propose-new-change-tool.md` alone.
- The functions behind the decision commands and `confirm_change_split`, called in-process by `../tools/DCE-decide-change-edits-tool.md` (NCP-FR-CEIA).
- `read_new_change_proposals(stream_id, change)` — called by `AGC-agent-conversations.md` (AGC-FR-JNOS) alone (NCP-FR-EZTC).

## Non-functional requirements
- A recording costs one write for the files, one for the record, and one comment line. An acceptance costs one write per file and one commit.
- The store is private to this machine, so a proposal that was never accepted never reaches the repository.
