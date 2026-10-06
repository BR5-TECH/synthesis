# Draft change proposals

**Spec code:** `DCP`

## Intent
The persistence home for the **changes an agent proposes to a draft's prompt**: an ordered set of anchored **hunks**, each one a replacement, an insertion, or a deletion in the one file a draft holds, offered by an agent in that draft's conversation (`../tools/PDC-propose-draft-changes-tool.md`) and held beside the draft until the author decides each of them (`../ui/DCR-draft-change-review.md`). It exists because a collaborator that can see how a passage would be better should show the author the change in the passage it belongs to, and because an agent that edits the author's working material without being asked is not a collaborator. A hunk names the text it changes rather than a position in the file, so the author edits the prompt while a proposal stands and each hunk still knows where it belongs. Each hunk is decided on its own: accepting one writes the prompt and records one version, rejecting one writes nothing, and neither touches the hunks beside it. A hunk's proposed text is the author's to correct while they read it, and the write that lands it in the draft is the one they perform by accepting. A draft carries at most one undecided proposal at a time, so the author is never holding two competing rewrites of the same working material in their head. Every decided proposal is also **counted**: one line naming that proposal and the way it went is appended to the draft's statistics log (`DSS-draft-statistics-storage.md`), carrying the proposal's identity and the decision and nothing of the text either side of it. Out of scope: composing a proposal, which is the agent's and reaches this module through one tool (`../tools/PDC-propose-draft-changes-tool.md`); rendering the hunks, editing their text, and the accept and reject controls, all of which are `../ui/DCR-draft-change-review.md`'s; writing the prompt's contents, which stays `DRS-draft-storage.md`'s single write path (DRS-FR-12); the transaction an acceptance is performed as, which this module delegates whole to `DHS-draft-history.md` (DHS-FR-15) rather than performing itself; telling the agent what was decided, since the decision comment appended on this module's behalf is what a later turn reads and nothing here dispatches one (`AGC-agent-conversations.md` AGC-FR-29); expiring, retrying, or deciding a proposal on the author's behalf, none of which happens on any timer or at any threshold; and proposing to an artifact already in the project, there being no such thing — a proposal exists only inside a draft.

## Contract surface
The module owns the `proposals/` folder inside a draft's own directory and the Tauri commands and event below. Names match `../ui/DCR-draft-change-review.md` and `../ui/NAW-new-artifact.md` byte-for-byte. Every path this module resolves is resolved against the project's **active worktree** (per `WTC-worktree-context.md` WTC-FR-03), the same root `DRS-draft-storage.md` resolves against, and the draft's own directory beneath it is obtained from `DRS-draft-storage.md` (DRS-FR-36) rather than composed from the draft's id.

### Record shapes

```
ProposalState = "pending" | "accepted" | "rejected"
HunkState     = "pending" | "accepted" | "rejected" | "discussing"
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
  counts: { pending, accepted, rejected, discussing },
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
  revision,              // bumped by an agent revision; never by an author edit
  decided_at             // RFC 3339 UTC; null while undecided
}
```

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

Errors are typed: `no_project_open`, `draft_not_found`, `draft_not_single_file`, `proposal_not_found`, `hunk_not_found`, `already_decided`, `hunk_already_decided`, `anchor_lost`, `candidate_stale`, `path_missing`, `thread_locked`, `write_failed`, and the identity refusals `identity_selection_required`, `github_unreachable`, `keychain_unavailable`, which a decision inherits from the comment it appends (per `CMS-comments-storage.md` CMS-FR-12). An acceptance additionally passes through the typed `acceptance_in_progress`, `acceptance_incomplete`, and `history_recovery_failed` of `DHS-draft-history.md` (DHS-FR-17, DHS-FR-21, DHS-FR-23) unchanged, those being answers about the transaction it delegated rather than about the proposal.

### Events
- `"draft change proposals changed"` — carries `{ draft_id, proposal }`, emitted when a proposal is recorded, when a hunk is decided or held, and when a hunk is revised (DCP-FR-16).

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
- `record_proposal(draft_id, path, hunks, rationale, agent, origin)` — the single path by which a proposal comes into existence (DCP-FR-05). Called by `../tools/PDC-propose-draft-changes-tool.md` and by nothing else. Refuses `proposal_pending`, `path_missing`, `no_change`, `thread_locked`, `no_hunks`, `hunk_anchor_lost`, `hunk_ambiguous`, and `hunk_overlap`, which are the refusals that tool reports as its own.
- `revise_draft_change_hunk(proposal_id, hunk_id, kind, before?, after?, note?)` — replaces one undecided hunk in place, keeping its id and its position (DCP-FR-XDRV). Called by `../tools/PDC-propose-draft-changes-tool.md` and by nothing else.
- `read_proposal_changes(draft_id)` — returns every proposal the draft holds, each with its path and its changes, for a turn that is assembling its input (DCP-FR-HNWD). Called by `AGC-agent-conversations.md` (AGC-FR-RVQP) and by nothing else. It runs no reconciliation and writes nothing.

## Functional requirements
1. **DCP-FR-01** A draft's proposals live in the `proposals/` folder of that draft's own directory, alongside the `draft.toml`, `files/`, `comments/`, `history/`, and `conversation.jsonl` it already holds (per `DRS-draft-storage.md` DRS-FR-01). Each proposal is two files: `<proposal-id>.toml` holding the record and the ledger, and `<proposal-id>.hunks` holding the ordered hunk documents.
2. **DCP-FR-02** The whole folder is **private draft storage** (per `DRS-draft-storage.md` DRS-FR-ZIVL, DRS-FR-WYIN): Git ignores it, no commit names it, the project scan skips it, and it is deleted with the draft (DRS-FR-21). A proposal stays in the checkout that recorded it and does not survive the draft it was made against. Graduating one takes nothing.
3. **DCP-FR-03** A proposal's `id` and each hunk's `id` are opaque, generated at creation, and stable for their lifetime. Neither is derived from the draft, the path, or the content, so two proposals of identical text are two proposals.
4. **DCP-FR-04** A draft carries **at most one proposal that is not resolved** at any moment. `record_proposal` refuses while one stands, whichever agent made it. The check and the writes that follow it are serialised per draft.
   - *Why:* Several turns are in flight at once, so two agents asked about one draft can otherwise both pass the check and leave the draft holding two undecided proposals.
5. **DCP-FR-05** `record_proposal` is the **only** path by which a proposal comes into existence, and it is internal: no Tauri command creates one. It performs two writes and one append as one act: the `<proposal-id>.hunks` file, the `<proposal-id>.toml` record, and one comment appended through `CMS-comments-storage.md`'s agent write path (CMS-FR-41) carrying `rationale` and one `proposal` attachment (CMS-FR-60).
6. **DCP-FR-06** The three writes of DCP-FR-05 succeed together or leave nothing behind. The hunks and the record reach disk before the comment is appended, and a failure to append the comment removes both files before returning. The comment's id is minted before either write, which is what makes this order possible.
   - *Why:* A record whose comment could not be appended is a proposal the author is never told about and the agent believes was made.
7. **DCP-FR-07** `record_proposal` refuses, appending and writing nothing, when the draft holds a proposal that is not resolved (DCP-FR-04), when `path` names anything but the draft's own prompt (per `DRS-draft-storage.md` DRS-FR-11), when the hunk list is empty, and when the thread is locked (per `CMS-comments-storage.md` CMS-FR-17). Each refusal is distinct.
8. **DCP-FR-JGCD** `record_proposal` resolves every hunk's anchor against the prompt before it writes. It refuses `hunk_anchor_lost` where a `before` is not in the prompt, `hunk_ambiguous` where a `before` occurs more than once, and `hunk_overlap` where two hunks resolve to ranges that overlap.
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
15. **DCP-FR-PWSF** A proposal's `state` is derived from its ledger: `pending` while it holds a hunk that is `pending` or `discussing`, `accepted` once every hunk is decided and at least one was accepted, and `rejected` once every hunk is decided and none was. `counts` is derived with it.
    - *Why:* Every surface that reads a proposal keys off these three spellings, and a fourth state would carry information `counts` already carries.
16. **DCP-FR-11** `accept_draft_change_hunk(proposal_id, hunk_id, feedback?)` accepts one undecided hunk. It checks that the hunk is undecided, that `path` still names the draft's prompt, that the anchor resolves, that an identity resolves, and that the conversation is unlocked, and then hands the acceptance to `DHS-draft-history.md`'s transaction (DHS-FR-15).
17. **DCP-FR-12** An acceptance replaces **the resolved range of that hunk alone** with the hunk's text, and leaves every other byte of the prompt as it was. The write settles the line-ending convention, which is the project's and is applied to every draft write alike (per `DRS-draft-storage.md` DRS-FR-12). The draft's file set is unchanged.
18. **DCP-FR-NKTB** Every hunk decision is serialised per draft, so two acceptances never read the prompt, splice their own hunk, and write it back over one another.
19. **DCP-FR-13** The write is atomic on the terms every draft write is (per `FSA-filesystem-access.md` FSA-FR-04). A failure anywhere before the transaction's commit point rolls that one hunk's acceptance back (per `DHS-draft-history.md` DHS-FR-19) — the prompt is byte-for-byte as it was, no history entry exists, and the hunk is undecided again — and returns a typed `write_failed`.
20. **DCP-FR-14** `reject_draft_change_hunk(proposal_id, hunk_id, feedback?)` moves one undecided hunk to `rejected` and **writes nothing into the draft**. `decline_draft_change_proposal(proposal_id, feedback?)` does the same for every undecided hunk at once. Neither removes a hunk document, so a rejected hunk is still read back afterwards.
21. **DCP-FR-15** The decision that **resolves** a proposal appends **one comment** into the proposal's own thread, through the ordinary human write path (per `CMS-comments-storage.md` CMS-FR-11). Its body states how many hunks were accepted and how many rejected, states whether any hunk was rewritten by the author, and carries `feedback` where any was supplied. A decision that resolves nothing appends none.
    - *Why:* One comment per hunk would bury the conversation and give an agent seven turns to answer where one will do.
22. **DCP-FR-16** `"draft change proposals changed"` is emitted when a proposal is recorded, when a hunk is decided, held, or revised, carrying the draft and the whole proposal. An acceptance emits it after its transaction has committed and never before, and after `DHS-draft-history.md`'s own `"draft history changed"` (DHS-FR-22). The `"drafts changed"` of the prompt's own write precedes both.
23. **DCP-FR-17** A decision against a hunk that is already `accepted` or `rejected` is a typed `hunk_already_decided` error, and a decision against a proposal that is already resolved is a typed `already_decided` error. Nothing is written, no comment is appended, and no event is emitted.
24. **DCP-FR-18** An acceptance whose `path` no longer names the draft's prompt is a typed `path_missing` error, and one whose hunk no longer resolves (DCP-FR-BMLX) is a typed `anchor_lost` error. Neither writes anything and both leave the hunk undecided. Rejecting refuses for neither reason, which is how such a hunk is cleared.
25. **DCP-FR-19** This module decides nothing on its own. No proposal expires, is collected, is retried, is auto-accepted, or is auto-declined, at any age or under any load. A proposal stands until the author decides it or until the draft it belongs to is deleted (DCP-FR-02).
26. **DCP-FR-20** This module dispatches no agent turn and reaches no model. The decision comment is a human comment like any other, and the turns it dispatches are the surface's to dispatch (per `../ui/DCR-draft-change-review.md` DCR-FR-15) and `AGC-agent-conversations.md`'s to run (AGC-FR-29).
27. **DCP-FR-21** Every write this module performs goes through `FSA-filesystem-access.md` primitives — `write_text_atomic` for a `.hunks` file, `write_toml_atomic` for a record, `delete_path` for the cleanup of DCP-FR-06 — and is subject to FSA's path-escape rejection (FSA-FR-10). It writes nothing outside the draft's own `proposals/` folder, and neither into `files/` nor into `history/`.
28. **DCP-FR-22** Every command errors with a typed `no_project_open` when no project is open, `draft_not_found` when `draft_id` names no draft in the active worktree, `draft_not_single_file` when the draft it names is inconsistent (per `DRS-draft-storage.md` DRS-FR-15), `proposal_not_found` when `proposal_id` names no proposal, and `hunk_not_found` when `hunk_id` names no hunk of it.
29. **DCP-FR-23** Nothing in this module reaches the network, contacts an AI integration, or touches a credential, save for the GitHub identity the decision comment's append resolves through `CMS-comments-storage.md` (CMS-FR-12).
30. **DCP-FR-24** Every operation emits through `LGC-logging.md`'s internal API under the `backend` domain: an `INFO` record when a proposal is recorded and when a hunk is decided, a `DEBUG` record when a hunk is edited or revised, and a `WARN` record when an operation refuses. **No record carries any proposed text, prompt text, `rationale`, or `feedback`**.
31. **DCP-FR-25** `edit_draft_change_hunk(proposal_id, hunk_id, after, baseline_checksum)` writes the author's own text over one undecided hunk, and is the only path but a recording and a revision that changes a `.hunks` file. It sets that hunk's `edited` whenever the text differs from what the agent composed, and clears it again where an edit restores that text exactly.
32. **DCP-FR-26** A hunk edit against a hunk that is already decided is a typed `hunk_already_decided` error and writes nothing. The command emits no `"draft change proposals changed"` event when it succeeds, the proposal's state being exactly what it was.
33. **DCP-FR-27** A hunk edit whose `baseline_checksum` does not match the checksum of the hunk document as it stands is a typed `candidate_stale` error and writes nothing, so a review editing a hunk another window has since rewritten cannot overwrite it unknowingly. A failed write leaves the hunk document byte-for-byte as it was and returns `write_failed`.
34. **DCP-FR-XDRV** `revise_draft_change_hunk` replaces one undecided hunk's kind, text, note, and anchor **in place**, keeping its `id` and its position in the order and bumping its `revision`. The hunk stays undecided and its `edited` is cleared, the agent's text being the new origin.
    - *Why:* The id is what every reply addressed to that hunk names, so a revision that minted a new one would orphan the discussion about it.
35. **DCP-FR-28** `list_draft_change_proposals`, `accept_draft_change_hunk`, `reject_draft_change_hunk`, and `decline_draft_change_proposal` each run `DHS-draft-history.md`'s reconciliation for the draft before they answer (per `DHS-draft-history.md` DHS-FR-18). A reconciliation that cannot complete returns the typed `history_recovery_failed` in place of the command's own answer.
36. **DCP-FR-QLMH** A proposal whose folder holds a whole-document candidate and no `.hunks` file is **legacy**. It is served as one `replace` hunk covering the whole prompt, with `before` read from the prompt as it stands, and is decided on the ordinary terms. Nothing rewrites it, and `edit_draft_change_hunk` refuses it.
    - *Why:* `proposals/` travels through Git, so such a proposal can arrive on a pull long after this module stopped writing them.
37. **DCP-FR-WTKA** A proposal that reaches a **final decision** contributes exactly one `proposal_decision` line to that draft's statistics log, carrying its own id and the decision it now holds. It is contributed after the resolving decision has committed and never before. An unresolved proposal, a recording, an edit, and a revision each contribute none.

## Non-functional requirements
- A proposal's text is a file on disk in an ordinary folder, so an author can read or recover one with a file manager if the application will not open.
- Listing proposals reads records alone and never a hunk document, so opening a draft that has accumulated a long proposal history costs one directory walk.
- Resolving a proposal's anchors is bounded by the size of the prompt and the number of hunks, and a hunk that has not moved costs one comparison at its hint.
- An acceptance costs the transaction of `DHS-draft-history.md` for one hunk, which is bounded by the size of the prompt and independent of how many proposals or versions the draft has accumulated.
- An author who revises a hunk pays one atomic write per settled burst of their typing and nothing else — no record rewrite beyond the `edited` flag when it turns over, no event, and no work anywhere in the draft.
- Nothing here holds a proposal in memory between calls, watches the folder, or maintains a queue: the folder is the whole of the state, so a relaunch mid-review finds every undecided hunk exactly where it was.
- The walking-skeleton implementation may keep records and hunk documents in memory rather than on disk, provided the record shapes, the one-undecided rule, the sort order, the emitted event, the anchor resolution, the checksum semantics, and the error types above are the ones returned to the UI.
- No operation here contacts the network or any AI integration.
