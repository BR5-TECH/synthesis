# Prompt change proposals

**Spec code:** `PCP`

## Intent
The persistence home for the **changes an agent proposes to a file the project already holds** whose resolved artifact type is `prompt` or `doc`: an ordered set of anchored **hunks**, each one a replacement, an insertion, or a deletion in that file, offered by an agent in a conversation about the file (`../tools/PPC-propose-prompt-changes-tool.md`) and held beside the project until the author decides each of them (`../ui/PCR-prompt-change-review.md`). The hunk model is the model of a draft's proposals (`DCP-draft-change-proposals.md`), so an author decides a change to a published file as they decide a change to a draft: one hunk at a time, in the text it changes. A hunk names the text it changes rather than a position in the file, so the author edits the file while a proposal stands and each hunk still knows where it belongs. Accepting one hunk writes the file through the project's own write path, rejecting one writes nothing, and neither touches the hunks beside it. A proposal closes only when no hunk is undecided, and until then a rejected hunk can still be accepted or revised by its agent. A hunk's proposed text is the author's to correct before they accept it, and that correction is not a change to the file. One artifact carries at most one undecided proposal at a time. Out of scope: composing a proposal, which is the agent's and reaches this module through one tool (`../tools/PPC-propose-prompt-changes-tool.md`); rendering the hunks, the decision controls, and the notification that calls the author to them, all of which are `../ui/PCR-prompt-change-review.md`'s; writing an artifact's contents, which stays `PST-project-storage.md`'s single write path (PST-FR-09); the artifact's own version history, which is read from Git by `HIS-history.md` and which this module neither writes nor consults; telling the agent what was decided, since the decision comment is what a later turn reads and nothing here dispatches one (`AGC-agent-conversations.md` AGC-FR-29); expiring, retrying, or deciding a proposal on the author's behalf; creating a file; and proposing to a draft, which is `DCP-draft-change-proposals.md`'s and which keeps its own storage, its own commands, and its own acceptance transaction.

## Functional requirements
 1. **PCP-FR-01** Proposals live in `.synthesis/proposals/` in the active worktree. A proposal at rest is two files: `<proposal-id>.toml`, which holds the record and its ledger, and `<proposal-id>.hunks`, which holds the ordered hunk documents. An acceptance in progress adds `<proposal-id>.prior` and `<proposal-id>.journal` (PCP-FR-14). The folder is created by the first write that needs it.
 2. **PCP-FR-02** The whole folder is gitignored — registered through `FSA-filesystem-access.md::ensure_gitignored` when it is created — and it is skipped by the project scan on the ordinary terms `.synthesis/` content is (per `ASC-artifact-scanning.md` ASC-FR-09). A proposal is never committed, never reaches anyone who clones the repository, and is never itself an artifact the Project panel shows or the Editor opens.
 3. **PCP-FR-03** A proposal's `id` and each hunk's `id` are opaque, generated at creation, and stable for their lifetime. Neither is derived from the artifact, the path, or the content, so two proposals of identical text against one file are two proposals.
 4. **PCP-FR-04** One artifact carries **at most one proposal in state `pending`** at any moment. `record_prompt_proposal` refuses while one stands, whichever agent made it. A proposal that is `accepted` or `rejected` frees the artifact for the next one. The check and the writes that follow it are **serialised per artifact**.
    - *Why:* Several turns are in flight at once (per `AGC-agent-conversations.md` AGC-FR-03), and two agents asked about one file can otherwise both pass the check.
 5. **PCP-FR-05** `record_prompt_proposal` is the **only** path by which a proposal comes into existence, and it is internal: no Tauri command creates one. It performs two writes and one append as **one act**: the `<proposal-id>.hunks` file, the `<proposal-id>.toml` record, and one comment appended through `CMS-comments-storage.md`'s agent write path (CMS-FR-41) carrying `rationale` and one `prompt_proposal` attachment (CMS-FR-66).
 6. **PCP-FR-06** The three writes of PCP-FR-05 succeed together or leave nothing behind. The hunks and the record reach disk before the comment is appended, and a failure to append the comment removes both files before returning. The comment's id is minted before either write. An append that succeeds announces its conversation (per `CMS-comments-storage.md` CMS-FR-51).
 7. **PCP-FR-07** `record_prompt_proposal` refuses, appending and writing nothing, with its own typed error for each case: `artifact_not_found` for a file the active worktree does not hold; `not_a_prompt_artifact` for a file whose resolved type (per `ASC-artifact-scanning.md` ASC-FR-06) is neither `prompt` nor `doc`; `proposal_pending` (PCP-FR-04); `no_change` where the change yields no hunk (PCP-FR-CNVT); and `discussion_resolved` (per `CMS-comments-storage.md` CMS-FR-17).
 8. **PCP-FR-CNVT** A `hunks` change is resolved against the file on disk by the rule of `../tools/PPC-propose-prompt-changes-tool.md` PPC-FR-RJCA and recorded as hunk documents with their anchors, as `DCP-draft-change-proposals.md` records a draft's hunks. A `content` change is converted into hunks against the file on disk at record time, and those hunks are recorded. The file and the `content` are compared line by line, and each run of changed lines becomes one hunk, so the author decides each changed passage separately. Where those hunks cannot each be found once in the file, the change is recorded as one hunk over the whole file.
    - *Why:* The author decides hunks however the agent composed the change, so a full text and a list of changes reach the same review.
 9. **PCP-FR-CUTC** A `hunks` change that cannot be recorded reliably is refused, and nothing is written or appended: `no_hunks` for an empty list, `hunk_anchor_lost` for text not found, `hunk_ambiguous` for text found more than once, `hunk_overlap` for two overlapping hunks, and `hunk_context_too_long` for a `lead` or `trail` over its bound. Each hunk refusal carries the hunk's place in the list.
10. **PCP-FR-08** No route through this module **creates a file**. `record_prompt_proposal` refuses a target that does not already exist, an acceptance writes an existing artifact and never restores a missing one, and a rejection writes into the project not at all. The one file set this module adds to is its own `.synthesis/proposals/` folder.
11. **PCP-FR-09** `<proposal-id>.hunks` holds the hunk text as the proposing agent composed it, or as the author or a revision has since rewritten it through the paths that may (PCP-FR-EDTW, PCP-FR-RVSN). Nothing else touches it: it is never re-indented, re-encoded, or trimmed, and the line-ending normalisation an acceptance applies is applied by the write rather than to this file.
12. **PCP-FR-HKLD** A record carries its ledger of `HunkLedgerRow`s in proposal order, `counts`, `hunk_count`, `legacy`, and `base_sha256`, the checksum of the file as the agent read it. Its `state` and `counts` are derived from the ledger exactly as `DCP-draft-change-proposals.md` DCP-FR-PWSF derives a draft proposal's.
13. **PCP-FR-10** `list_prompt_change_proposals(artifact_id)` returns every proposal that artifact holds — pending, accepted, and rejected alike — most recently created first. It reads the records and never a `.hunks` or a `.content` file.
14. **PCP-FR-LDHK** `load_prompt_change_proposal_hunks(proposal_id)` returns the hunks of any proposal in proposal order, the checksum of the hunk document that is the baseline of the next hunk edit, and `legacy`. Each hunk carries its resolution against the file as it stands, taken by the rule of `DCP-draft-change-proposals.md` DCP-FR-VZTK, and is **lost** on the terms of DCP-FR-BMLX.
16. **PCP-FR-12** `accept_prompt_change_hunk(proposal_id, hunk_id, feedback?)` accepts one hunk that is undecided or rejected, of a proposal that is `pending`. It checks that state, that `artifact_id` still names a file of type `prompt` or `doc`, that an identity resolves, and that the conversation is not resolved. The write goes through `PST-project-storage.md`'s own write path (`save_artifact_contents`, PST-FR-09).
17. **PCP-FR-ZQAH** An acceptance resolves the hunk against the file **as it stands on disk at that moment**, replaces the resolved range of that hunk alone, and writes the whole file inside the transaction of PCP-FR-14 for that one hunk. A lost hunk is the typed `anchor_lost` and nothing is written. No baseline checksum is compared, and every other hunk is untouched.
    - *Why:* The author edits the file while a proposal stands, so a checksum check would refuse the author for having worked.
18. **PCP-FR-14** The acceptance of one hunk is a **transaction with one commit point**, and that commit point is **the artifact write landing**. The artifact write is the last fallible step and is atomic (per `FSA-filesystem-access.md` FSA-FR-04), so the artifact is never restored on any path: a failure before it means the file was never written, and a failure of it means the write did not land.
    The order is: the pre-checks of PCP-FR-12 and PCP-FR-16, which write nothing; where this decision resolves the proposal (PCP-FR-RSLV), the decision comment's identity and `event_id` are minted; `<proposal-id>.prior` is written with the artifact's whole current bytes and read back; `<proposal-id>.journal` is written, naming the proposal, the hunk and its prior ledger row, the artifact and its path, the checksum of `.prior`, the checksum of the bytes the artifact write puts on disk, and any minted comment identity; the **record** is written with that hunk's row `accepted`, the derived state, and `comment_owed` set where a comment is owed; then the **commit point**, the artifact write. Then the event is emitted (PCP-FR-17) and any owed comment is appended. `comment_owed` is cleared only after that append succeeds, and the journal is removed last of all, so a journal on disk always means something is still to finish.
    Every failure at or before the commit point ends with the artifact untouched and the hunk in its prior state, verified, before anything is returned. A failure of the artifact write writes the record back to the prior ledger row with `comment_owed` cleared, reads that rollback back, removes the journal and `.prior`, and returns the typed `write_failed`. Where the rollback cannot complete within the call, the journal stays and `reconcile(proposal_id)` completes it before any command of this module answers.
    A failure after the commit point is not a failed acceptance. A cleanup failure is reported to nobody and the next reconciliation sweeps it. A comment append failure leaves the acceptance landed and the comment owed, and is the typed `acceptance_incomplete`. The owed comment is settled by `reconcile(proposal_id)` and by `complete_prompt_change_decision(proposal_id)`, which runs that reconciliation and returns the resulting `PromptDecisionOutcome`; the append is idempotent by the pre-minted `event_id` (per `CMS-comments-storage.md` CMS-FR-06).
    `reconcile(proposal_id)` runs before `list_prompt_change_proposals`, before every decision command, inside `complete_prompt_change_decision`, and at project open. A journal present is read against the artifact's own checksum. Where it equals the journal's **written** checksum, the commit point was passed: the hunk's row is written `accepted` where it does not already read so, the owed comment is appended, the event is emitted, and the journal and `.prior` are removed once the comment is settled. Where it equals the **`.prior`** checksum, the write never landed: the hunk's row is written back to its prior state and the journal and `.prior` are removed. Where it matches **neither**, the file is left exactly as it is, the hunk's row is written back to its prior state, and one `WARN` record names the artifact and the proposal (PCP-FR-28). A reconciliation whose own writes fail leaves the journal in place, and the command answers `write_failed`. A decision attempted while another acceptance of the same proposal is in flight returns the typed `acceptance_in_progress` and writes nothing.
19. **PCP-FR-RJHN** `reject_prompt_change_hunk(proposal_id, hunk_id, feedback?)` moves one undecided hunk to `rejected` and **writes nothing into the project**. A rejected hunk stays in the `.hunks` file and can be accepted while the proposal is `pending`.
20. **PCP-FR-15** `decline_prompt_change_proposal(proposal_id, feedback?)` moves every undecided hunk to `rejected` at once and **writes nothing into the project**. It begins no journal and removes no hunk document.
21. **PCP-FR-HLDQ** `set_prompt_change_hunk_discussing(proposal_id, hunk_id, discussing)` marks one undecided hunk `discussing`, or returns it to `pending`, and returns the record as it now stands. It decides nothing, writes nothing into the project, and appends no comment.
22. **PCP-FR-16** A decision comment is appended into the proposal's own thread through the ordinary human write path (per `CMS-comments-storage.md` CMS-FR-11), so it carries the acting author's identity. It carries no attachment. An identity that does not resolve (per `CMS-comments-storage.md` CMS-FR-12) and a resolved conversation are each refused with their own typed error before anything is written.
23. **PCP-FR-RSLV** Only the decision that **resolves** the proposal appends the one decision comment. Its body counts the hunks accepted and rejected by the state each finally holds, states whether the author rewrote any hunk, and carries `feedback` where any was supplied. A decision that resolves nothing appends no comment and returns no `comment_id`.
    - *Why:* One comment per hunk would bury the conversation and give an agent a turn for each hunk where one will do.
24. **PCP-FR-17** `"prompt change proposals changed"` is emitted when a proposal is recorded, when a hunk is decided, held, or revised, and when a reconciliation settles a decision, carrying the artifact and the proposal. An acceptance emits it after its commit point, so a rollback emits nothing. A hunk edit emits nothing. The suppression of `PST-project-storage.md` PST-FR-16 holds for the acceptance's write.
25. **PCP-FR-18** An acceptance of an `accepted` hunk, and a rejection or a hold of an `accepted` or a `rejected` hunk, is the typed `hunk_already_decided`. A decision against a proposal that is already `accepted` or `rejected` is the typed `already_decided`. Nothing is written, no comment is appended, and no event is emitted.
26. **PCP-FR-19** An acceptance whose `artifact_id` no longer resolves to a file the project holds, or resolves to one whose type is no longer `prompt` or `doc`, is the typed `artifact_not_found` or `not_a_prompt_artifact`: nothing is written, no journal is begun, the hunk keeps its state, and no file is created. Rejecting and declining succeed on the ordinary terms.
27. **PCP-FR-EDTW** `edit_prompt_change_hunk(proposal_id, hunk_id, after, baseline_checksum)` overwrites one undecided hunk's `after` and sets or clears its row's `edited` by whether the text differs from the agent's own. A `baseline_checksum` that differs from the hunk document's is the typed `candidate_stale`, and a decided hunk the typed `hunk_already_decided`. It writes no file of the project and emits no event.
28. **PCP-FR-RVSN** `revise_prompt_change_hunk` replaces one undecided or rejected hunk's kind, text, and anchor **in place**, keeping its `id` and its position and bumping its `revision`. The hunk is `pending` after it, with `decided_at` and `edited` cleared. A proposal that is not `pending` is not revised, and the revision appends no comment.
29. **PCP-FR-LGXQ** A proposal whose folder holds a `<proposal-id>.content` file and no `.hunks` file is **legacy**. It is served as one `replace` hunk with the id `legacy`, covering the whole file, with `before` read from the file as it stands and `after` the `.content` text. Its acceptance replaces the whole file. It is never rewritten, never lost, and `edit_prompt_change_hunk` refuses it.
    - *Why:* Proposals recorded as whole documents stay pending on disk, and the author must still be able to decide them.
30. **PCP-FR-20** This module decides nothing on its own. No proposal expires, is collected, is retried, is auto-accepted, or is auto-declined. The one sweep this module performs runs at project open and removes the files of a proposal that is not `pending` and whose artifact the project no longer holds. A **pending** proposal is never swept.
31. **PCP-FR-21** This module dispatches no agent turn and reaches no model. The decision comment is a human comment like any other, and the turns it dispatches are the surface's to dispatch (per `../ui/PCR-prompt-change-review.md` PCR-FR-14, `../ui/CTA-comment-agent-turns.md` CTA-FR-LCFU, CTA-FR-QUXJ) and `AGC-agent-conversations.md`'s to run (AGC-FR-29).
32. **PCP-FR-25** Every filesystem operation this module performs goes through `FSA-filesystem-access.md` primitives: `write_text_atomic` for a `.hunks` file and a `.prior` file, `write_toml_atomic` for a record and a journal, `sha256_file` for every checksum, `delete_path` for every removal, and `ensure_gitignored` for the folder. It writes nothing outside `.synthesis/proposals/`, the one exception being the artifact bytes an acceptance lands through `PST-project-storage.md`'s write path (PCP-FR-12).
33. **PCP-FR-26** Every command errors with a typed `no_project_open` when no project is open, `artifact_not_found` when `artifact_id` names no file in the active worktree, `proposal_not_found` when `proposal_id` names no proposal, and `hunk_not_found` when `hunk_id` names no hunk of it. Changing the active worktree re-resolves the proposals folder against the new content root.
34. **PCP-FR-27** Nothing in this module reaches the network, contacts an AI integration, or touches a credential, save for the GitHub identity the decision comment's append resolves through `CMS-comments-storage.md` (CMS-FR-12).
35. **PCP-FR-28** Every operation logs through `LGC-logging.md` under the `backend` domain: an `INFO` record when a proposal is recorded or a hunk decided, a `DEBUG` record when a hunk is held, edited, or revised, and a `WARN` record when an operation refuses, a transaction rolls back, or a reconciliation settles one. **No record carries any proposed text, file text, `rationale`, or `feedback`**.
36. **PCP-FR-29** This module and `DCP-draft-change-proposals.md` share no state and no operation. Both use one neutral, pure-text module that resolves and applies anchored hunks; it holds no proposal, draft, or artifact state and names neither module. No operation of either reads or decides a proposal of the other, and the one-pending rule of PCP-FR-04 counts this module's proposals alone.

## Contract surface
The module owns the `.synthesis/proposals/` folder and the Tauri commands and event below. Names match `../ui/PCR-prompt-change-review.md` byte-for-byte. Every path this module resolves is resolved against the project's **active worktree** (per `WTC-worktree-context.md` WTC-FR-03), the same root `PST-project-storage.md` resolves against (PST-FR-21).

### Record shapes

```
PromptProposalState = "pending" | "accepted" | "rejected"

PromptChangeProposal {
  id,                    // opaque, generated at creation, stable for its lifetime
  artifact_id,           // the stable, path-derived node key of the target
                         //   (ASC-artifact-scanning.md ASC-FR-13)
  path,                  // project-relative path of the file
  agent: Participant,    // the agent participant that proposed it (CMS's shape)
  rationale,             // what the agent wrote; also the body of its comment
  thread_id,             // the conversation the proposal was posted into
  comment_id,            // the comment carrying the reference to this proposal
  state: PromptProposalState,   // derived from the ledger (PCP-FR-HKLD)
  legacy,                // true for a proposal held as a whole document (PCP-FR-LGXQ)
  base_sha256?,          // the file as the agent read it, at record time
  hunk_count,
  counts: { pending, accepted, rejected, discussing },
  ledger: HunkLedgerRow[],      // in proposal order
  candidate_edited,      // true while any hunk's text differs from the agent's own
  comment_owed,          // true between an acceptance landing and its decision
                         //   comment being appended (PCP-FR-14)
  origin_checksum?,      // kept for a legacy record
  created_at,            // RFC 3339 UTC
  decided_at             // RFC 3339 UTC; null while pending
}
```

`HunkLedgerRow`, `ProposalHunk`, `HunkAnchor`, and `HunkResolution` have the shapes `DCP-draft-change-proposals.md` declares for them. A legacy record, which holds no ledger on disk, is served with one row built from its `state`.

### Tauri commands
- `"list prompt change proposals"` → `list_prompt_change_proposals(artifact_id)` → `PromptChangeProposal[]`, most recently created first (PCP-FR-10).
- `"load prompt change proposal hunks"` → `load_prompt_change_proposal_hunks(proposal_id)` → `{ hunks: ProposalHunk[], resolutions: HunkResolution[], checksum, legacy }` (PCP-FR-LDHK).
- `"accept prompt change hunk"` → `accept_prompt_change_hunk(proposal_id, hunk_id, feedback?)` → a `PromptDecisionOutcome` (PCP-FR-12, PCP-FR-ZQAH).
- `"reject prompt change hunk"` → `reject_prompt_change_hunk(proposal_id, hunk_id, feedback?)` → a `PromptDecisionOutcome` (PCP-FR-RJHN).
- `"hold prompt change hunk"` → `set_prompt_change_hunk_discussing(proposal_id, hunk_id, discussing)` → the `PromptChangeProposal` as it now stands (PCP-FR-HLDQ).
- `"edit prompt change hunk"` → `edit_prompt_change_hunk(proposal_id, hunk_id, after, baseline_checksum)` → `{ checksum }` of the hunk document written (PCP-FR-EDTW).
- `"decline prompt change proposal"` → `decline_prompt_change_proposal(proposal_id, feedback?)` → a `PromptDecisionOutcome` (PCP-FR-15).
- `"complete prompt change decision"` → `complete_prompt_change_decision(proposal_id)` → a `PromptDecisionOutcome`. It finishes an acceptance whose decision comment is still owed (PCP-FR-14). Against a proposal that owes no comment it returns that proposal as it stands.

```
PromptDecisionOutcome {
  proposal:    PromptChangeProposal,   // as it now stands
  resolved,    // true where this decision left no hunk undecided
  comment_id?, // the resolving decision's own comment; absent otherwise,
               //   and absent where it could not be appended
  origin_kind  // "artifact_discussion" | "artifact_comment" (AGC-FR-05)
}
```

Errors are typed: `no_project_open`, `artifact_not_found`, `not_a_prompt_artifact`, `proposal_not_found`, `hunk_not_found`, `already_decided`, `hunk_already_decided`, `anchor_lost`, `candidate_stale`, `discussion_resolved`, `write_failed`, `acceptance_in_progress`, `acceptance_incomplete`, and the identity refusals `identity_selection_required`, `github_unreachable`, `keychain_unavailable`, which a decision inherits from the comment it appends (per `CMS-comments-storage.md` CMS-FR-12).

### Events
- `"prompt change proposals changed"` — carries `{ artifact_id, proposal }` (PCP-FR-17).

### Internal (Rust API, not registered as a Tauri command and unreachable from the frontend)
- `record_prompt_proposal(artifact_id, change, rationale, agent, origin)` — the single path by which a proposal comes into existence (PCP-FR-05). Called by `../tools/PPC-propose-prompt-changes-tool.md` and by nothing else. Refuses `proposal_pending`, `artifact_not_found`, `not_a_prompt_artifact`, `no_change`, and `discussion_resolved`, and for a `hunks` change also `no_hunks`, `hunk_anchor_lost`, `hunk_ambiguous`, `hunk_overlap`, and `hunk_context_too_long` (PCP-FR-CUTC).
- `revise_prompt_change_hunk(proposal_id, hunk_id, hunk)` — replaces one undecided or rejected hunk in place (PCP-FR-RVSN). Called by `../tools/PPC-propose-prompt-changes-tool.md` and by nothing else. It refuses `hunk_not_found` for a hunk that is not undecided or rejected, and the hunk refusals of PCP-FR-CUTC for the new hunk.

```
ProposedChange =
    { kind: "content", content }          // the complete new text
  | { kind: "hunks", hunks: ProposedHunk[] }

ProposedHunk {
  before?, after?, after_text?,           // the shape of ../tools/PPC-propose-prompt-changes-tool.md
  lead?, trail?                           //   (PPC-FR-RJCA)
}
```

## Non-functional requirements
- A proposal's text is a file on disk in an ordinary folder, so an author can read or recover one with a file manager if the application will not open.
- Listing proposals reads records alone and never a hunk document, so opening an artifact that has accumulated a long proposal history costs one directory read.
- Resolving a proposal's anchors is bounded by the size of the file and the number of hunks, and a hunk that has not moved costs one comparison at its hint.
- An acceptance costs one read of the artifact, one `.prior` write of the same size, one journal write, one record write, one write through the artifact's own save path, at most one appended comment, and two removals.
- An author who corrects a hunk pays one atomic write per settled burst of typing and nothing else — no record rewrite beyond the `edited` flag when it turns over, no event, and no work anywhere in the project.
- Nothing here holds a proposal in memory between calls, watches the folder, or maintains a queue: the folder is the whole of the state, so a relaunch mid-review finds every undecided hunk exactly where it was.
- No operation here contacts the network or any AI integration.
