# Draft change proposals

**Spec code:** `DCP`

## Intent
The persistence home for the **changes an agent proposes to a draft's prompt**: an ordered set of anchored **hunks**, each one a replacement, an insertion, or a deletion in the one file a draft holds, offered by an agent in that draft's conversation (`../tools/PDC-propose-draft-changes-tool.md`) and held beside the draft until the author decides each of them (`../ui/DCR-draft-change-review.md`). It exists because a collaborator that can see how a passage would be better should show the author the change in the passage it belongs to, and because an agent that edits the author's working material without being asked is not a collaborator. A hunk names the text it changes rather than a position in the file, so the author edits the prompt while a proposal stands and each hunk still knows where it belongs. Each hunk is decided on its own: accepting one writes the prompt and records one version, rejecting one writes nothing, and neither touches the hunks beside it. A proposal is one transaction that closes only when no hunk is undecided, and until then a rejected hunk can still be accepted, and its agent can **update** the proposal: change hunks in place, add hunks, and withdraw hunks, in one call that posts one comment. A hunk's proposed text is the author's to correct while they read it, and the write that lands it in the draft is the one they perform by accepting. The author decides on the review, or tells an agent to decide, and the agent's tool (`../tools/DDC-decide-draft-changes-tool.md`) then runs the same operations on the same terms. A draft carries at most one undecided proposal at a time, so the author is never holding two competing rewrites of the same working material in their head. Every decided proposal is also **counted**: one line naming that proposal and the way it went is appended to the draft's statistics log (`DSS-draft-statistics-storage.md`), carrying the proposal's identity and the decision and nothing of the text either side of it. Out of scope: composing a proposal, which is the agent's and reaches this module through one tool (`../tools/PDC-propose-draft-changes-tool.md`); rendering the hunks, editing their text, and the accept and reject controls, all of which are `../ui/DCR-draft-change-review.md`'s; writing the prompt's contents, which stays `DRS-draft-storage.md`'s single write path (DRS-FR-12); the transaction an acceptance is performed as, which this module delegates whole to `DHS-draft-history.md` (DHS-FR-15) rather than performing itself; telling the agent what was decided, since the decision comment appended on this module's behalf is what a later turn reads and nothing here dispatches one (`AGC-agent-conversations.md` AGC-FR-29); expiring or retrying a proposal, and deciding one without the author's explicit instruction, none of which happens on any timer or at any threshold; and proposing to an artifact already in the project, there being no such thing — a proposal exists only inside a draft. The module also holds the **new-draft proposals** of a **decomposition**: an agent proposes, one at a time, the new drafts that replace a draft that grew too big (`../tools/PND-propose-new-draft-tool.md`), and the author accepts, rejects, or discusses each of them on its own card (`../ui/DCR-draft-change-review.md`). Nothing is created until the author accepts a proposal, and accepting one creates exactly one draft in the original draft's folder. Only the author ends a decomposition, on the card or by an instruction to an agent, whose tool then confirms it on the same terms (`../tools/DDC-decide-draft-changes-tool.md`), and confirming it archives the original draft with a `Superseded` marker and never edits or deletes it.

## Contract surface
The module owns the `proposals/` folder inside a draft's own directory and the Tauri commands and event below. Names match `../ui/DCR-draft-change-review.md` and `../ui/NAW-new-artifact.md` byte-for-byte. Every path this module resolves is resolved against the project's **active worktree** (per `WTC-worktree-context.md` WTC-FR-03), the same root `DRS-draft-storage.md` resolves against, and the draft's own directory beneath it is obtained from `DRS-draft-storage.md` (DRS-FR-36) rather than composed from the draft's id.

### Record shapes

```
ProposalState = "pending" | "accepted" | "rejected"
HunkState     = "pending" | "accepted" | "rejected" | "discussing" | "withdrawn"
HunkKind      = "add" | "del" | "replace"

DraftChangeProposal {
  id,                    // opaque, generated at creation, stable for its lifetime
  draft_id,
  path,                  // draft-relative path of the prompt these hunks change
  agent: Participant,    // the agent participant that proposed it (CMS's shape)
  rationale,             // what the agent wrote; also the body of its comment
  thread_id,             // the conversation the proposal was posted into
  comment_id,            // the comment carrying the reference to this proposal
  state: ProposalState,  // derived from the ledger below (DCP-FR-PWSF)
  legacy,                // true for a proposal held as a whole document (DCP-FR-QLMH)
  base_sha256,           // the prompt as the agent read it, at record time
  hunk_count,
  counts: { pending, accepted, rejected, discussing, withdrawn },
  candidate_edited,      // true while any hunk's text differs from the agent's own
  ledger: HunkLedgerRow[],   // in proposal order
  created_at,            // RFC 3339 UTC
  decided_at             // RFC 3339 UTC; null while the proposal is not resolved
}

HunkLedgerRow {
  id,                    // opaque, minted at recording, stable for the hunk's life
  kind: HunkKind,
  state: HunkState,
  edited,                // true while this hunk's text differs from the agent's own
  revision,              // bumped by an agent's `change` update; never by an author edit
  decided_at             // RFC 3339 UTC; null while undecided
}
```

A hunk carries no link to a discussion. A **hunk discussion** names the hunk instead, through the `proposal_hunk` attachment of its opening comment (DCP-FR-FHWN).

A **hunk document** carries the text and the anchor, and is read only when the review renders:

```
ProposalHunk {
  id, kind, revision,
  note,                  // optional, one sentence about this change alone
  before,                // the exact prompt text this hunk replaces or deletes;
                         //   absent for an "add"
  after,                 // the exact new text; absent for a "del"
  anchor: HunkAnchor
}

HunkAnchor {
  lead,                  // exact prompt text immediately before `before`, bounded
  trail,                 // exact prompt text immediately after `before`, bounded
  hint_start, hint_end   // character offsets into the base at record time; a hint
}

HunkResolution =
    { kind: "resolved", start, end }   // where this hunk applies in the prompt now
  | { kind: "lost" }                   // the text it changes is no longer there
```

### Tauri commands
- `"list draft change proposals"` → `list_draft_change_proposals(draft_id)` → `DraftChangeProposal[]`, every proposal that draft holds, most recently created first. It reads records alone and never a hunk document.
- `"load draft change proposal hunks"` → `load_draft_change_proposal_hunks(proposal_id)` → `{ hunks: ProposalHunk[], resolutions: HunkResolution[], checksum }`, in proposal order, each resolution taken against the prompt as it stands. The checksum is the baseline a hunk edit is checked against (DCP-FR-27).
- `"accept draft change hunk"` → `accept_draft_change_hunk(proposal_id, hunk_id, feedback?)` → a `DecisionOutcome`. Applies that hunk to the prompt and decides nothing else.
- `"reject draft change hunk"` → `reject_draft_change_hunk(proposal_id, hunk_id, feedback?)` → a `DecisionOutcome`. Writes nothing into the draft.
- `"hold draft change hunk"` → `set_draft_change_hunk_discussing(proposal_id, hunk_id, discussing)` → the `DraftChangeProposal` as it now stands.
- `"edit draft change hunk"` → `edit_draft_change_hunk(proposal_id, hunk_id, after, baseline_checksum)` → `{ checksum }` of the hunk document written. Overwrites one undecided hunk's proposed text and writes nothing else anywhere (DCP-FR-25).
- `"decline draft change proposal"` → `decline_draft_change_proposal(proposal_id, feedback?)` → a `DecisionOutcome`. Rejects every undecided hunk at once.
- `"list new draft proposals"` → `list_new_draft_proposals(draft_id)` → `NewDraftProposal[]`, every new-draft proposal of that draft, most recently created first. It reads records alone and never a body.
- `"load new draft proposal body"` → `load_new_draft_proposal_body(proposal_id)` → `{ body }`, the proposed prompt as it stands.
- `"accept new draft proposal"` → `accept_new_draft_proposal(proposal_id, feedback?)` → a `NewDraftDecisionOutcome`. Creates the new draft (DCP-FR-KOOA).
- `"reject new draft proposal"` → `reject_new_draft_proposal(proposal_id, feedback?)` → a `NewDraftDecisionOutcome`. Creates nothing (DCP-FR-FZEP).
- `"confirm draft decomposition"` → `confirm_draft_decomposition(draft_id)` → the original `Draft` as it now stands (DCP-FR-VLLP).

```
DecisionOutcome {
  proposal:    DraftChangeProposal,   // as it now stands
  resolved,    // true where this decision left the proposal with nothing undecided
  comment_id?, // the resolving decision's own comment; absent otherwise, and
               //   absent where it could not be appended
  origin_kind  // "draft_discussion" | "draft_comment" (AGC-FR-05)
}
```

The last two are carried because `../ui/DCR-draft-change-review.md` (DCR-FR-15) dispatches one fresh turn naming that comment as its trigger, and re-folding the thread to find what was just appended is both wasteful and ambiguous under a concurrent append.

Errors are typed: `no_project_open`, `draft_not_found`, `draft_not_single_file`, `proposal_not_found`, `hunk_not_found`, `already_decided`, `hunk_already_decided`, `hunk_withdrawn`, `anchor_lost`, `candidate_stale`, `path_missing`, `discussion_resolved`, `write_failed`, and the identity refusals `identity_selection_required`, `github_unreachable`, `keychain_unavailable`, which a decision inherits from the comment it appends (per `CMS-comments-storage.md` CMS-FR-12). An acceptance additionally passes through the typed `acceptance_in_progress`, `acceptance_incomplete`, and `history_recovery_failed` of `DHS-draft-history.md` (DHS-FR-17, DHS-FR-21, DHS-FR-23) unchanged, those being answers about the transaction it delegated rather than about the proposal.

### Events
- `"draft change proposals changed"` — carries `{ draft_id, proposal, cause }`, emitted when a proposal is recorded, when it is updated, and when a hunk is decided or held (DCP-FR-16).
- `"new draft proposals changed"` — carries a `NewDraftProposalsChanged`, emitted on a recording, an update, a decision, and a confirmation (DCP-FR-QINH).

```
ProposalChangeCause = "recorded" | "updated" | "decided" | "held"
```

### The statistics line
Beside the event above, a proposal that reaches a **final decision** contributes one `proposal_decision` line to the draft's statistics log through `DSS-draft-statistics-storage.md`'s `record_statistics_event` (DSS-FR-EKZI):

```
ProposalDecisionStatisticsEvent {
  proposal_id,   // the DraftChangeProposal's own id; the stable identity of the line
  decision       // "accepted" | "rejected"
}
```

It carries no hunk text, no rationale, no feedback, no path, and no participant (DCP-FR-WTKA).

### Internal (Rust API, not registered as a Tauri command and unreachable from the frontend)
- `record_decision_statistics(draft_id, proposal)` — appends the statistics line of the Contract surface for one settled proposal (DCP-FR-WTKA). It is called from the same point the resolving decision's own event is emitted from, is asynchronous, and cannot fail the decision.
- `record_proposal(draft_id, path, hunks, rationale, agent, origin)` — the single path by which a proposal comes into existence (DCP-FR-05). Called by `../tools/PDC-propose-draft-changes-tool.md` and by nothing else. Each hunk of `hunks` carries its optional `lead` and `trail` beside `before`, `after`, and `after_text`. Refuses `proposal_pending`, `path_missing`, `no_change`, `discussion_resolved`, `no_hunks`, `hunk_anchor_lost`, `hunk_ambiguous`, `hunk_context_too_long`, and `hunk_overlap`, which are the refusals that tool reports as its own.
- `update_proposal(draft_id, updates, rationale, agent, origin)` — applies an ordered list of updates to the draft's open proposal and appends one comment, as one act (DCP-FR-XDRV). Called by `../tools/PDC-propose-draft-changes-tool.md` and by nothing else. Refuses `no_open_proposal`, `no_updates`, `discussion_resolved`, and `path_missing`, and, each with the update's place, `hunk_not_found`, `hunk_not_updatable`, `hunk_named_twice`, `no_change`, `hunk_anchor_lost`, `hunk_ambiguous`, `hunk_context_too_long`, and `hunk_overlap`.

```
ProposalUpdate =
    { action: "change", id, hunk: ProposedHunk }
  | { action: "add", hunk: ProposedHunk }
  | { action: "withdraw", id }
```
- `accept_draft_change_hunk`, `reject_draft_change_hunk`, `set_draft_change_hunk_discussing`, and `load_draft_change_proposal_hunks` — the functions behind the Tauri commands above, also called in-process by `../tools/DDC-decide-draft-changes-tool.md` with the same arguments and the same results (DCP-FR-LAWR).
- `read_proposal_changes(draft_id)` — returns every proposal the draft holds, each with its path and its changes, for a turn that is assembling its input (DCP-FR-HNWD). Called by `AGC-agent-conversations.md` (AGC-FR-RVQP) and by nothing else. It runs no reconciliation and writes nothing.

### New-draft proposals
A **decomposition** is the set of new-draft proposals of one draft, the **original draft**. Every proposal of the set carries the same `set_id`.

```
NewDraftProposalState = "pending" | "accepted" | "rejected" | "withdrawn"
NewDraftUpdateAction  = "change" | "withdraw"

NewDraftProposal {
  id,                    // opaque, generated at creation, stable for its lifetime
  set_id,                // the decomposition this proposal belongs to (DCP-FR-CKWC)
  draft_id,              // the original draft
  name,                  // the proposed name of the new draft
  position,              // N, counted from 1, as the agent gave it
  total,                 // M, as the agent gave it
  agent: Participant,    // the agent participant that proposed it (CMS's shape)
  rationale,             // what the agent wrote; also the body of its comment
  thread_id,             // the conversation the proposal was posted into
  comment_id,            // the comment that recorded it
  state: NewDraftProposalState,
  revision,              // 1 at recording; bumped by each `change` update
  created_draft_id,      // the draft an acceptance created; null otherwise
  created_at,            // RFC 3339 UTC
  decided_at             // RFC 3339 UTC; null while pending
}

NewDraftDecisionOutcome {
  proposal: NewDraftProposal,  // as it now stands
  created_draft?,              // the Draft an acceptance created (DRS's shape)
  comment_id?,                 // the decision comment; absent where it could not be appended
  origin_kind                  // "draft_discussion" | "draft_comment" (AGC-FR-05)
}

NewDraftProposalsChanged {
  draft_id,
  proposal?,             // absent for a confirmation
  cause                  // "recorded" | "updated" | "decided" | "confirmed"
}
```

Errors are typed: `no_project_open`, `draft_not_found`, `proposal_not_found`, `already_decided`, `draft_archived`, `draft_graduated`, `draft_locked_by_graduation`, `draft_github_shadow`, `decomposition_confirmed`, `new_draft_proposal_pending`, `nothing_accepted`, `discussion_resolved`, `write_failed`, and the identity refusals `identity_selection_required`, `github_unreachable`, `keychain_unavailable` that a decision inherits from the comment it appends (per `CMS-comments-storage.md` CMS-FR-12).

The new-draft commands and the `"new draft proposals changed"` event are listed with the others above. Internal (Rust API, not registered as a Tauri command):

- `record_new_draft_proposal(draft_id, name, body, position, total, rationale, agent, origin)` — the single path by which a new-draft proposal comes into existence (DCP-FR-TGRN). Called by `../tools/PND-propose-new-draft-tool.md` alone.
- `update_new_draft_proposal(draft_id, action, fields, rationale, agent, origin)` — changes or withdraws the draft's open new-draft proposal and appends one comment, as one act (DCP-FR-GDTC). Called by `../tools/PND-propose-new-draft-tool.md` alone.
- `accept_new_draft_proposal` and `reject_new_draft_proposal` — the functions behind the commands above, also called in-process by `../tools/DDC-decide-draft-changes-tool.md` with no feedback (DCP-FR-EJWZ).
- `confirm_draft_decomposition` — the function behind the command above, also called in-process by `../tools/DDC-decide-draft-changes-tool.md` (DCP-FR-IKTU).
- `read_new_draft_proposals(draft_id)` — every new-draft proposal of the draft, oldest first, and the body of the open one, for a turn that is assembling its input (DCP-FR-FKMT). Called by `AGC-agent-conversations.md` (AGC-FR-TIGM) alone.

## Functional requirements
1. **DCP-FR-01** A draft's proposals live in the `proposals/` folder of that draft's own directory, alongside the `draft.toml`, `files/`, `comments/`, `history/`, and `conversation.jsonl` it already holds (per `DRS-draft-storage.md` DRS-FR-01). Each proposal is two files: `<proposal-id>.toml` holding the record and the ledger, and `<proposal-id>.hunks` holding the ordered hunk documents.
2. **DCP-FR-02** The whole folder is **private draft storage** (per `DRS-draft-storage.md` DRS-FR-ZIVL, DRS-FR-WYIN): Git ignores it, no commit names it, the project scan skips it, and it is deleted with the draft (DRS-FR-21). A proposal stays in the checkout that recorded it and does not survive the draft it was made against. Graduating one takes nothing.
3. **DCP-FR-03** A proposal's `id` and each hunk's `id` are opaque, generated at creation, and stable for their lifetime. Neither is derived from the draft, the path, or the content, so two proposals of identical text are two proposals.
4. **DCP-FR-04** A draft carries **at most one draft change proposal that is not resolved** at any moment. `record_proposal` refuses while one stands, whichever agent made it. The check and the writes that follow it are serialised per draft.
   - *Why:* Several turns are in flight at once, so two agents asked about one draft can otherwise both pass the check and leave the draft holding two undecided proposals.
5. **DCP-FR-05** `record_proposal` is the **only** path by which a proposal comes into existence, and it is internal: no Tauri command creates one. It performs two writes and one append as one act: the `<proposal-id>.hunks` file, the `<proposal-id>.toml` record, and one comment appended through `CMS-comments-storage.md`'s agent write path (CMS-FR-41) carrying `rationale` and one `proposal` attachment (CMS-FR-60).
6. **DCP-FR-06** The three writes of DCP-FR-05 succeed together or leave nothing behind. The hunks and the record reach disk before the comment is appended, and a failure to append the comment removes both files before returning. The comment's id is minted before either write, which is what makes this order possible.
   - *Why:* A record whose comment could not be appended is a proposal the author is never told about and the agent believes was made.
7. **DCP-FR-07** `record_proposal` refuses, appending and writing nothing, when the draft holds a proposal that is not resolved (DCP-FR-04), when `path` names anything but the draft's own prompt (per `DRS-draft-storage.md` DRS-FR-11), when the hunk list is empty, and when the thread is resolved (per `CMS-comments-storage.md` CMS-FR-17). Each refusal is distinct.
8. **DCP-FR-JGCD** `record_proposal` finds every hunk in the prompt before it writes, by the text `lead`+`before`+`trail` (for an `add`, `lead`+`after_text`+`trail`; per `../tools/PDC-propose-draft-changes-tool.md` PDC-FR-WCXD). It refuses `hunk_anchor_lost` where that text is not in the prompt, `hunk_ambiguous` where it occurs more than once, `hunk_context_too_long` where `lead` or `trail` exceeds its bound, and `hunk_overlap` where two hunks overlap.
   - *Why:* This catches a model that invented the text it claims to be changing, before the author is ever offered it.
9. **DCP-FR-HRQN** A hunk names the text it changes rather than a position in the file. It carries the bounded `lead` and `trail` text either side of it and a positional hint. Its kind and its text agree: a replacement carries `before` and `after`, a deletion carries `before` alone, and an insertion carries `after` alone.
   - *Why:* A record holds nothing acceptance cannot use, so what the review draws is what accepting writes.
10. **DCP-FR-VZTK** A hunk resolves against the prompt as it stands, in this order: at the hint where the text there still matches; else at the occurrence of `lead`+`before`+`trail` nearest the hint; else at the occurrence of `before` nearest the hint where `lead` or `trail` still abuts it. Both texts are line-ending normalised first.
11. **DCP-FR-BMLX** A hunk that resolves by none of the steps of DCP-FR-VZTK is **lost**. Lost is derived on every read and never stored, so a hunk lost by an edit resolves again when the author undoes that edit.
12. **DCP-FR-08** `<proposal-id>.hunks` holds the hunk text as the proposing agent composed it, or as the author has since rewritten it through the one path that may (DCP-FR-25). Nothing else touches it: it is never re-indented, re-encoded, or trimmed, and the line-ending normalisation an acceptance applies is applied by the write rather than to this file.
13. **DCP-FR-09** `list_draft_change_proposals(draft_id)` returns every proposal the draft holds — resolved and unresolved alike — most recently created first. It reads the records and never a `.hunks` file, so a draft carrying a long history of proposals costs a list no more than a draft carrying one.
14. **DCP-FR-10** `load_draft_change_proposal_hunks(proposal_id)` returns the hunks of any proposal in proposal order, each with its resolution against the prompt as it stands, together with the checksum that is the baseline the next hunk edit is checked against (DCP-FR-27). It is invoked when a review renders rather than when proposals are listed.
15. **DCP-FR-HNWD** `read_proposal_changes(draft_id)` returns every proposal of that draft, most recently created first, each carrying its path and its changes in proposal order. Each change carries its kind, its state, and the text it names. It runs no reconciliation, takes no resolution against the prompt, and writes nothing.
    - *Why:* A turn assembling its input must read a draft without changing one (per `../ai/CVL-conversation-loop.md` CVL-FR-09), which the listing of DCP-FR-28 cannot promise.
15. **DCP-FR-PWSF** A proposal's `state` is derived from its ledger: `pending` while it holds a hunk that is `pending` or `discussing`, else `accepted` where at least one hunk is accepted, else `rejected`. A `withdrawn` hunk counts for neither final state. `counts` is derived with it.
    - *Why:* Every surface that reads a proposal keys off these three spellings, and a fourth state would carry information `counts` already carries.
16. **DCP-FR-11** `accept_draft_change_hunk(proposal_id, hunk_id, feedback?)` accepts one hunk that is undecided or rejected, of a proposal that is not resolved. It checks that state, that `path` still names the draft's prompt, that the anchor resolves, that an identity resolves, and that the conversation is not resolved. It then hands the acceptance to `DHS-draft-history.md`'s transaction (DHS-FR-15).
17. **DCP-FR-12** An acceptance replaces **the resolved range of that hunk alone** with the hunk's text, and leaves every other byte of the prompt as it was. The write settles the line-ending convention, which is the project's and is applied to every draft write alike (per `DRS-draft-storage.md` DRS-FR-12). The draft's file set is unchanged.
18. **DCP-FR-NKTB** Every hunk decision is serialised per draft, so two acceptances never read the prompt, splice their own hunk, and write it back over one another.
19. **DCP-FR-13** The write is atomic on the terms every draft write is (per `FSA-filesystem-access.md` FSA-FR-04). A failure anywhere before the transaction's commit point rolls that one hunk's acceptance back (per `DHS-draft-history.md` DHS-FR-19) — the prompt is byte-for-byte as it was, no history entry exists, and the hunk holds the state it held before — and returns a typed `write_failed`.
20. **DCP-FR-14** `reject_draft_change_hunk(proposal_id, hunk_id, feedback?)` moves one undecided hunk to `rejected` and **writes nothing into the draft**. `decline_draft_change_proposal(proposal_id, feedback?)` does the same for every undecided hunk at once. Neither removes a hunk document, so a rejected hunk is still read back afterwards.
21. **DCP-FR-15** The decision that **resolves** a proposal appends **one comment** into the proposal's own thread through the human write path (per `CMS-comments-storage.md` CMS-FR-11). Its body counts the accepted and the rejected hunks, states whether the author rewrote any hunk, and carries any `feedback`. A decision that resolves nothing appends none, and an update that closes the proposal appends none.
    - *Why:* One comment per hunk would bury the conversation and give an agent seven turns to answer where one will do.
22. **DCP-FR-16** `"draft change proposals changed"` is emitted on a recording, a hunk decision, a hold, and once per update after its comment. It carries the draft, the whole proposal, and the `cause` naming which of the four happened. An acceptance emits it after its transaction commits and after `DHS-draft-history.md`'s `"draft history changed"` (DHS-FR-22), which the prompt write's `"drafts changed"` precedes.
23. **DCP-FR-17** An acceptance of an `accepted` hunk, and a rejection of an `accepted` or a `rejected` hunk, is a typed `hunk_already_decided` error. A decision against a proposal that is already resolved is a typed `already_decided` error. Nothing is written, no comment is appended, and no event is emitted.
24. **DCP-FR-18** An acceptance whose `path` no longer names the draft's prompt is a typed `path_missing` error, and one whose hunk no longer resolves (DCP-FR-BMLX) is a typed `anchor_lost` error. Neither writes anything and both leave the hunk in the state it held. Rejecting refuses for neither reason, which is how such a hunk is cleared.
25. **DCP-FR-19** This module decides nothing on its own. No proposal expires, is collected, is retried, is auto-accepted, or is auto-declined, at any age or under any load. A proposal stands until the author decides it (DCP-FR-CIPW) or until the draft it belongs to is deleted (DCP-FR-02).
26. **DCP-FR-20** This module dispatches no agent turn and reaches no model. The decision comment is a human comment like any other, and the turns it dispatches are the surface's to dispatch (per `../ui/DCR-draft-change-review.md` DCR-FR-15) and `AGC-agent-conversations.md`'s to run (AGC-FR-29). A decision by tool dispatches none (per `../tools/DDC-decide-draft-changes-tool.md` DDC-FR-KQZN).
27. **DCP-FR-21** Every write this module performs goes through `FSA-filesystem-access.md` primitives — `write_text_atomic` for a `.hunks` file, `write_toml_atomic` for a record, `delete_path` for the cleanup of DCP-FR-06 — and is subject to FSA's path-escape rejection (FSA-FR-10). It writes nothing outside the draft's own `proposals/` folder, and neither into `files/` nor into `history/`.
28. **DCP-FR-22** Every command errors with a typed `no_project_open` when no project is open, `draft_not_found` when `draft_id` names no draft in the active worktree, `draft_not_single_file` when the draft it names is inconsistent (per `DRS-draft-storage.md` DRS-FR-15), `proposal_not_found` when `proposal_id` names no proposal, and `hunk_not_found` when `hunk_id` names no hunk of it.
29. **DCP-FR-23** Nothing in this module reaches the network, contacts an AI integration, or touches a credential, save for the GitHub identity the decision comment's append resolves through `CMS-comments-storage.md` (CMS-FR-12).
30. **DCP-FR-24** Every operation emits through `LGC-logging.md`'s internal API under the `backend` domain: an `INFO` record when a proposal is recorded or updated and when a hunk is decided, a `DEBUG` record when a hunk is edited, and a `WARN` record when an operation refuses. **No record carries any proposed text, prompt text, `rationale`, or `feedback`**.
31. **DCP-FR-25** `edit_draft_change_hunk(proposal_id, hunk_id, after, baseline_checksum)` writes the author's own text over one undecided hunk, and is the only path but a recording and an update that changes a `.hunks` file. It sets that hunk's `edited` whenever the text differs from what the agent composed, and clears it again where an edit restores that text exactly.
32. **DCP-FR-26** A hunk edit against a hunk that is already decided is a typed `hunk_already_decided` error and writes nothing. The command emits no `"draft change proposals changed"` event when it succeeds, the proposal's state being exactly what it was.
33. **DCP-FR-27** A hunk edit whose `baseline_checksum` does not match the checksum of the hunk document as it stands is a typed `candidate_stale` error and writes nothing, so a review editing a hunk another window has since rewritten cannot overwrite it unknowingly. A failed write leaves the hunk document byte-for-byte as it was and returns `write_failed`.
34. **DCP-FR-XDRV** `update_proposal` applies an ordered list of updates to the draft's **open** proposal: `change` replaces a hunk in place, `add` appends a hunk, and `withdraw` withdraws a hunk. It refuses `no_open_proposal` where the draft holds no open proposal and `no_updates` for an empty list. A resolved proposal is never updated.
35. **DCP-FR-TQBO** A `change` replaces the kind, text, note, and anchor of a hunk that is `pending`, `discussing`, or `rejected`, keeping its `id` and its position and bumping its `revision`. The hunk is `pending` after it, with `decided_at` and `edited` cleared, so an author's edit of that hunk is lost.
    - *Why:* The id is what every reply addressed to that hunk names, so a change that minted a new one would orphan the discussion about it.
35. **DCP-FR-IAFV** An `add` mints a new hunk id and places the hunk after the existing hunks, in the order of the call. A `withdraw` sets `withdrawn` on a hunk that is `pending`, `discussing`, or `rejected`. The record and the `.hunks` file keep a withdrawn hunk.
35. **DCP-FR-BFDR** An update names an unknown hunk id `hunk_not_found`, a `change` or `withdraw` of an `accepted` or `withdrawn` hunk `hunk_not_updatable`, and an id named twice `hunk_named_twice`. Each `change` and `add` is placed by DCP-FR-JGCD and must not overlap another update or a hunk that is neither `withdrawn`, `accepted`, nor the hunk it changes.
35. **DCP-FR-FKMH** An update refusal carries the place of the update it is about. One refused update leaves the proposal byte-for-byte as it was and appends no comment.
35. **DCP-FR-PHUB** An update writes the `.hunks` file and the record, then appends one agent comment through `CMS-comments-storage.md`'s agent write path (CMS-FR-41) into the turn's conversation. Its body is `rationale`, and its one `proposal` attachment carries `update`, the counts of hunks changed, added, and withdrawn (CMS-FR-BSQB).
35. **DCP-FR-VTYX** A failure to append the update's comment restores the `.hunks` file and the record to the bytes they held before the update. An update is serialised per draft with every hunk decision (DCP-FR-NKTB).
35. **DCP-FR-MKIM** An acceptance, a rejection, a hold, and an edit of a `withdrawn` hunk are each the typed `hunk_withdrawn`, and write nothing. `decline_draft_change_proposal` leaves a withdrawn hunk as it is.
35. **DCP-FR-FHWN** A hunk discussion is linked to its hunk by the `proposal_hunk` attachment of the discussion's opening comment (per `CMS-comments-storage.md` CMS-FR-TQVB). Opening, resolving, and reopening it change no hunk state, and `"hold draft change hunk"` is the one operation that marks a hunk as discussed.
35. **DCP-FR-28** `list_draft_change_proposals`, `accept_draft_change_hunk`, `reject_draft_change_hunk`, and `decline_draft_change_proposal` each run `DHS-draft-history.md`'s reconciliation for the draft before they answer (per `DHS-draft-history.md` DHS-FR-18). A reconciliation that cannot complete returns the typed `history_recovery_failed` in place of the command's own answer.
36. **DCP-FR-QLMH** A proposal whose folder holds a whole-document candidate and no `.hunks` file is **legacy**. It is served as one `replace` hunk covering the whole prompt, with `before` read from the prompt as it stands, and is decided on the ordinary terms. Nothing rewrites it, and `edit_draft_change_hunk` refuses it.
    - *Why:* `proposals/` travels through Git, so such a proposal can arrive on a pull long after this module stopped writing them.
37. **DCP-FR-CIPW** A decision by `decide_draft_changes` (`../tools/DDC-decide-draft-changes-tool.md`) on the author's explicit instruction is a decision of the author, on the terms of the chip it matches. Nothing else decides a hunk on the author's behalf.
37. **DCP-FR-LAWR** A decision by tool calls the functions behind `accept_draft_change_hunk`, `reject_draft_change_hunk`, and `set_draft_change_hunk_discussing` in-process. It passes the same checks, emits the same event, appends the same decision comment through the human write path (CMS-FR-11), and contributes the same statistics line as the command.
37. **DCP-FR-WTKA** A proposal that reaches a **final decision** contributes exactly one `proposal_decision` line to that draft's statistics log, carrying its own id and the decision it now holds. It is contributed after the resolving decision has committed and never before. An unresolved proposal, a recording, an edit, and an update that leaves the proposal open each contribute none. An update that closes the proposal contributes its line.

38. **DCP-FR-VQWK** A draft's new-draft proposals live in the `new-drafts/` folder inside its `proposals/` folder. Each proposal is two files: `<proposal-id>.toml` holding the record and `<proposal-id>.md` holding the proposed body. The folder is private draft storage on the terms of DCP-FR-02 and is deleted with the draft.
39. **DCP-FR-TGRN** `record_new_draft_proposal` is the **only** path by which a new-draft proposal comes into existence. It writes the body, then the record, then appends one agent comment through `CMS-comments-storage.md`'s agent write path (CMS-FR-41), carrying `rationale` and one `new_draft_proposal` attachment (CMS-FR-YKGI). A failed append removes both files.
40. **DCP-FR-LTEZ** Recording, updating, accepting, rejecting, and confirming each refuse, writing nothing, while the original draft is `archived` (`draft_archived`), `graduated` (`draft_graduated`), held by a non-terminal graduation run (`draft_locked_by_graduation`, per `DRS-draft-storage.md` DRS-FR-19), a GitHub-shadow draft (`draft_github_shadow`), or carries a `superseded` marker (`decomposition_confirmed`).
41. **DCP-FR-OBAW** A draft holds **at most one pending new-draft proposal**. `record_new_draft_proposal` refuses `new_draft_proposal_pending` while one stands. It also refuses `discussion_resolved` where the thread is resolved, and a blank name, a blank body, a `position` less than 1, and a `total` less than `position`, each as a typed validation error.
42. **DCP-FR-CKWC** The first recording on a draft mints the `set_id`, and every later recording on that draft takes the `set_id` of the records the draft already holds. A draft has at most one decomposition, and no `set_id` is ever reused for another draft.
43. **DCP-FR-KLPP** A pending new-draft proposal and a pending draft change proposal of the same draft stand side by side. Neither refuses the recording of the other, and the rule of DCP-FR-04 is about draft change proposals alone.
44. **DCP-FR-GDTC** `update_new_draft_proposal` acts on the draft's pending new-draft proposal. A `change` replaces the fields it carries, keeps the `id`, and bumps `revision`. A `withdraw` sets `withdrawn` and `decided_at`. Each writes the files, then appends one agent comment whose attachment carries the action, and restores both files where the append fails.
45. **DCP-FR-ZJRW** `update_new_draft_proposal` refuses `no_open_proposal` where the draft holds no pending new-draft proposal, `no_change` for a `change` that leaves every field as it is, and the validation errors of DCP-FR-OBAW for the fields that result. A refused update leaves both files byte-for-byte as they were.
46. **DCP-FR-KOOA** `accept_new_draft_proposal` creates the new draft through `DRS-draft-storage.md`'s `create_draft_with_prompt` (DRS-FR-EMKG), in the drafts folder that holds the original draft at that moment. The draft takes the proposal's name, its body as the prompt, and the sequence `{ set_id, position, total, original_draft_id, original_name }`.
47. **DCP-FR-AQDM** After the draft is created, the acceptance writes the record as `accepted`, with `created_draft_id` and `decided_at`, then appends the decision comment. A failure to create the draft changes nothing. A failure to write the record deletes the created draft. Either failure returns `write_failed`.
48. **DCP-FR-FZEP** `reject_new_draft_proposal` writes the record as `rejected` with `decided_at`, appends the decision comment, and creates nothing. A rejected proposal keeps its record and its body.
49. **DCP-FR-PDCP** A decision appends **one comment** into the proposal's own thread through the human write path (CMS-FR-11). Its body states the decision, the proposed name, the position and the total, and carries any `feedback`. The outcome carries that comment's id and the conversation's origin kind.
50. **DCP-FR-EEDK** A decision against a proposal that is not `pending` is a typed `already_decided` error, and one naming no proposal is `proposal_not_found`. Nothing is written, no comment is appended, and no event is emitted.
51. **DCP-FR-GJXE** The sequence that an acceptance gives the new draft is never rewritten. A later call that gives a different `total`, and a rejection of another proposal of the set, leave the stored `position` and `total` of every created draft as they are.
52. **DCP-FR-VLLP** `confirm_draft_decomposition(draft_id)` refuses `new_draft_proposal_pending` while a proposal of the set is pending and `nothing_accepted` where no proposal of the set is accepted. Otherwise it calls `DRS-draft-storage.md`'s `mark_draft_superseded` (DRS-FR-UBWN) with the `set_id` and the accepted drafts.
53. **DCP-FR-FAQQ** The accepted drafts of the marker are the `created_draft_id` of each accepted proposal, oldest first, each with the current name of that draft, or the proposed name where that draft no longer exists. Confirming never edits the original prompt and never deletes the original draft or its proposals.
54. **DCP-FR-ORTA** After a confirmation, the original draft's decomposition is closed. Recording, updating, accepting, rejecting, and confirming each refuse `decomposition_confirmed`, also after the original draft is restored to `active`.
55. **DCP-FR-QINH** A confirmation appends one human comment into the thread of the newest decided proposal, stating that the decomposition is confirmed and naming the accepted drafts. It dispatches no agent turn, and emits `"new draft proposals changed"` with cause `confirmed` after the `"drafts changed"` of the marker's write.
56. **DCP-FR-ZDSP** `"new draft proposals changed"` is emitted on a recording, once per update after its comment, on a decision after its comment, and on a confirmation. A decision that creates a draft emits it after the `"drafts changed"` of that creation.
57. **DCP-FR-LMJW** A new-draft proposal is decided only by the author, on the card of `../ui/DCR-draft-change-review.md` or by `../tools/DDC-decide-draft-changes-tool.md` on the author's explicit instruction. No proposal expires or is decided on a timer, and no new-draft decision contributes a statistics line.
58. **DCP-FR-EJWZ** A decision by tool calls the functions behind `accept_new_draft_proposal` and `reject_new_draft_proposal` in-process, with no feedback. It passes the same checks, emits the same event, and appends the same decision comment through the human write path as the command.
59. **DCP-FR-TRPW** A discussion about a new-draft proposal is linked to it by the `new_draft_subject` attachment of its opening comment (per `CMS-comments-storage.md` CMS-FR-KAXQ). Opening, resolving, and reopening it change no proposal state.
60. **DCP-FR-KUSI** Every recording, update, decision, and confirmation of a draft's new-draft proposals is serialised per draft, so two acceptances never create two drafts from one proposal.
61. **DCP-FR-FKMT** `read_new_draft_proposals(draft_id)` returns every new-draft proposal of the draft, oldest first, with its id, position, total, name, state, and created draft, and the body of the pending one. It writes nothing.
62. **DCP-FR-BZHR** A pending new-draft proposal makes the draft's `has_pending_proposal` true in `list_drafts` (per `DRS-draft-storage.md` DRS-FR-WMIK).
63. **DCP-FR-EEGX** New-draft operations emit through `LGC-logging.md` under the `backend` domain: `INFO` on a recording, an update, a decision, and a confirmation, and `WARN` on a refusal. **No record carries a name, a body, a `rationale`, or a `feedback`**.
64. **DCP-FR-DIHV** Every write of a new-draft operation goes through `FSA-filesystem-access.md` primitives. It writes nothing outside the draft's own `proposals/new-drafts/` folder, save the draft an acceptance creates and the marker a confirmation writes, both through `DRS-draft-storage.md`.
65. **DCP-FR-IKTU** A confirmation by tool calls the function behind `confirm_draft_decomposition` in-process. It passes the same checks, writes the same marker, emits the same events, and appends the same confirmation comment through the human write path as the command (DCP-FR-VLLP, DCP-FR-QINH). It dispatches no agent turn.
66. **DCP-FR-HMUG** A decision by tool (DCP-FR-LAWR, DCP-FR-EJWZ) and a confirmation by tool (DCP-FR-IKTU) take the agent's display name and the call's `instruction`. The decision comment or the confirmation comment they append ends with the instruction line of `../tools/DDC-decide-draft-changes-tool.md` (DDC-FR-JJHZ). A decision or a confirmation from a command takes neither and appends no such line.

## Non-functional requirements
- A proposal's text is a file on disk in an ordinary folder, so an author can read or recover one with a file manager if the application will not open.
- Listing proposals reads records alone and never a hunk document, so opening a draft that has accumulated a long proposal history costs one directory walk.
- Resolving a proposal's anchors is bounded by the size of the prompt and the number of hunks, and a hunk that has not moved costs one comparison at its hint.
- An acceptance costs the transaction of `DHS-draft-history.md` for one hunk, which is bounded by the size of the prompt and independent of how many proposals or versions the draft has accumulated.
- An author who edits a hunk pays one atomic write per settled burst of their typing and nothing else — no record rewrite beyond the `edited` flag when it turns over, no event, and no work anywhere in the draft.
- Nothing here holds a proposal in memory between calls, watches the folder, or maintains a queue: the folder is the whole of the state, so a relaunch mid-review finds every undecided hunk exactly where it was.
- The walking-skeleton implementation may keep records and hunk documents in memory rather than on disk, provided the record shapes, the one-undecided rule, the sort order, the emitted event, the anchor resolution, the checksum semantics, and the error types above are the ones returned to the UI.
- No operation here contacts the network or any AI integration.
