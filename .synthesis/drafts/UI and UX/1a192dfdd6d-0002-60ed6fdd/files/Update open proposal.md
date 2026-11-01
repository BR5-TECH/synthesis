## Intent

If there’s an open proposal, agent should have an ability to update it if that’s required due to discussion in the ongoing conversation.

An update can change several hunks, add new hunks, and withdraw hunks, in one call. It works for every artifact that can hold a change proposal: a draft's prompt, a published prompt, and a project document.

## User journey

- User has an active discussion  with an agent over a draft or some other artifact.
- Agent makes a change proposal.
- User wants to discuss one of the changes and eventually asks agent to change the change a bit. The request can need more than one change: edit some hunks, add a hunk, drop a hunk.
- Agent sends one update to the open proposal. It posts one comment with its rationale.
- User gets the same notification, and the same toast, as for a new proposal. It lands the user on the first undecided hunk.
- User sees the changed and added hunks undecided in the document. Withdrawn hunks are gone from the review.
- User is able to accept or reject or discuss updated changes. When the update leaves no undecided hunk, the proposal closes.

## Requirements

- Look and feel must stay as is: the decorations, chips, review bar, and keys of `DCR-draft-change-review.md` do not change. The only visible differences are the ones below: added hunks appear, withdrawn hunks do not, and the update comment.

**Scope**
- Applies to `propose_draft_changes` (draft prompt) and `propose_prompt_changes` (published prompt or project document). Any later proposal tool follows the same rules.
- Only the one open (pending) proposal of the artifact can be updated. A closed proposal is not updated.

**Tool contract**
- Remove the `revises` argument from both tools. Add an optional `updates` list in its place. `updates` can not be sent with `hunks` or `content`. Each entry is one of:
  - `change`: the id of an existing hunk, and the new hunk fields (`before`, `after`, `after_text`, `lead`, `trail`, with the same rules as a new hunk). It replaces that hunk in place.
  - `add`: the fields of a new hunk, and no id.
  - `withdraw`: the id of an existing hunk.
- `rationale` stays required and has the same meaning as for a new proposal.
- The call is all or nothing. Every `change` and `add` is placed against the text as it stands, by the rules of PDC-FR-TZKQ and PDC-FR-WCXD, and must not overlap another live hunk (not withdrawn, not accepted) or another entry of the call.
- A refusal is the retryable `InvalidArgs` of the tool contract. It names the entry by its place (*Update 2 of 4*). Refuse: an unknown hunk id; a `change` or `withdraw` of an accepted or withdrawn hunk; one id named twice; an empty list; `updates` together with `hunks` or `content`; and `updates` where no proposal is open (the agent then sends a new proposal).
- Update the parameter descriptions (TLC-FR-06) and the declared side effects (TLC-FR-17) of both tools.

**Hunk behaviour**
- `change` keeps the hunk's id and position, bumps its `revision`, and makes it `pending`. `decided_at` and `edited` are cleared, so an author's edit of that hunk is lost. A rejected or discussing hunk is `pending` again.
- `add` mints a new id and places the hunk after the existing hunks, in the order of the call. It is `pending`.
- `withdraw` sets a new hunk state `withdrawn` on a pending, discussing, or rejected hunk. The record keeps the hunk. Ledger counts gain `withdrawn`.
- A withdrawn hunk is hidden in review: no decoration, no gutter mark, no chip, and it is not in the review bar counter. Its hunk discussions stay readable. The author can not accept, reject, edit, or hold it; such a call is a typed error and writes nothing.

**Proposal state**
- When an update leaves no undecided hunk, the proposal closes. Its final state follows today's rules (DCP-FR-PWSF, PCP-FR-HKLD) with withdrawn hunks counting for neither state. When no hunk is accepted, the proposal is `rejected`. No new proposal state is added.
- The decision comment counts only accepted and rejected hunks. An update that closes the proposal appends no decision comment; its own comment (below) is the only one.

**Comment and event**
- One update appends exactly one agent comment: the `rationale` as body, the proposal reference as for a recording, a one-line summary (hunks changed, added, withdrawn), and the action that scrolls to the first undecided hunk (none when the proposal closed). This replaces the rule that a revision appends no comment.
- The `"draft change proposals changed"` and `"prompt change proposals changed"` events are emitted once per update, after the comment is appended. The payload lets a subscriber tell an update from a decision, a hold, or an edit.

**Notification**
- An update raises the same notification as a recording (PCR-FR-17 and its draft counterpart): the same key derived from the proposal id, so it replaces an earlier raise of that proposal; level Info; the same address, landing on the first undecided hunk. The toast follows the same policy (NTF-FR-02, NTF-FR-08, NTF-FR-26), so nothing posts while the author looks at that artifact.
- An update that closes the proposal retracts the raise (PCR-FR-26) and raises nothing.

## Specification impact

This work changes, and must update in the same change:
- `specifications/tools/PDC-propose-draft-changes-tool.md`: arguments, PDC-FR-GMWR (replace `revises` with `updates`), declared side effects.
- `specifications/tools/PPC-propose-prompt-changes-tool.md`: arguments, PPC-FR-RVKS, declared side effects.
- `specifications/core/DCP-draft-change-proposals.md`: `HunkState` and counts gain `withdrawn`; DCP-FR-XDRV becomes the update operation; DCP-FR-PWSF; DCP-FR-16 and DCP-FR-24; the update comment.
- `specifications/core/PCP-prompt-change-proposals.md`: the same, for PCP-FR-RVSN and PCP-FR-HKLD, and the comment of PCP-FR-05.
- `specifications/ui/DCR-draft-change-review.md`: the hidden `withdrawn` state, the counter, the gutter map, DCR-FR-WRJP, DCR-FR-HVXK for the update comment, and the raise on an update.
- `specifications/ui/PCR-prompt-change-review.md`: PCR-FR-17 raises on an update as well as on a recording; PCR-FR-26.
- `specifications/ui/NTF-notifications.md`: the proposal raises (NTF-FR-24) also happen on an update.
- The drafts `Partial rejections` and `Changes proposal alignment` name `revises`. `updates` replaces it there.