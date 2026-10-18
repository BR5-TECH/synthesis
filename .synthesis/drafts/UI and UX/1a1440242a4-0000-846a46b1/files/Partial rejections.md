## Intent

When an author discusses a draft with agents, the author must be able to:

- decide each change of a proposal on its own: accept 3 out of 5, reject the other 2. This already ships (DCR-FR-12..14, DCR-FR-20). Keep it as context and check that it works during this work. Add no new behaviour for it.
- start a **separate discussion** about one change (hunk) of a proposal, also after the author rejected it.
- **resolve** a discussion, so that no new message can be added to it.
- **reopen** a resolved discussion.

Users: the author (a human) who reviews a draft in the New Artifact tab, and the agents that propose changes and answer in the discussions.

## User journey

1. The author starts a draft discussion.
2. After a few messages, an agent makes a change proposal.
3. The author accepts or rejects each change on its own, without deciding the whole proposal.
4. The author selects **Discuss** on one change, rejected or undecided. A new discussion opens, with its own thread, linked to that change. The agent can answer there and revise that change in place. A revised change is undecided again.
5. The author must decide every change: accept or reject. A discussion is not a decision. The proposal closes only when no change is undecided.
6. When the author is satisfied with a discussion, the author selects **Mark resolved** in the card menu. The card then takes no new message.
7. Later, the author selects **Reopen discussion**. Only the author can write in it again. Reopening dispatches no agent turn.

## Requirements

### Functional requirements

**Hunk discussion**
- **Discuss** on a hunk creates a **new discussion record** with its own thread, linked to that hunk. It no longer posts a reply in the current discussion. A draft already holds many discussions. Today's behaviour is DCR-FR-MTFD.
- Discussing a hunk that is already rejected does not change its state.

**Proposal "transaction"**
- A proposal closes when no hunk is undecided (DCP-FR-PWSF). Accepting 3 of 5 closes it only after the other 2 are decided.
- While the proposal is open, a **rejected** hunk stays decorated in the document. Its chip offers **Discuss** and **Accept**, so the author can reverse the rejection. An accepted hunk is not decorated.
- Accepting a rejected hunk is allowed while the proposal is open. When the proposal closes, all decoration and the review bar go (DCR-FR-17). Hunk discussions stay readable.
- An agent may revise a rejected hunk in place (`revises`). The hunk then becomes undecided. A closed proposal can not be revised. A new `revises` call on a closed proposal is an ordinary new proposal.
- The decision comment of DCP-FR-15 counts the final state of each hunk.

**One resolved state (all discussions, all surfaces)**
- **Lock** and **Resolve** merge into one state: **Resolved**. The card menu has one entry: **Mark resolved** / **Reopen discussion**. The **Lock discussion** / **Unlock discussion** entry is removed, and no surface invokes `"set discussion lock"`.
- A resolved discussion has no composer and no Quote action. The backend refuses a human post to it. It refuses an agent answer from a turn started before the resolution: the turn ends and appends no comment. Agent tools that post (for example `ask_user_comment`) are refused on it.
- A resolved discussion shows no Retry for a failed agent contribution.
- Reopen restores the composer and the Quote action and dispatches nothing.
- Existing discussions stored as locked read as **Resolved**, and Reopen restores them. Do not rewrite the logs. Read the legacy lock lines at read time, as CMS-FR-83 does for legacy lines.
- The Comments panel groups are **Active** and **Resolved**. The **Locked** group is removed.
- **Gap to close:** the discussion column beside the document in the draft's New Artifact tab has no way to resolve or reopen. Add the card menu there. Other surfaces already render it.

### Non-functional requirements

- Existing look-and-feel of the discussions must be preserved.
- Existing look-and-feel of the “change proposals” must be preserved. The chip on a rejected hunk reuses the existing chip and the existing `--warn` held-hunk style.
- A rejected hunk keeps an accessible name that states it is rejected (DCR-FR-10). The chip is reachable by keyboard.

### Specifications this work changes

- `specifications/ui/DCR-draft-change-review.md`: DCR-FR-MTFD (Discuss), DCR-FR-12 (chip), DCR-FR-17 (decoration until close).
- `specifications/core/DCP-draft-change-proposals.md`: DCP-FR-11 and DCP-FR-17 (accept a rejected hunk), DCP-FR-XDRV (revise a rejected hunk), DCP-FR-15, and the hunk-to-discussion link.
- `specifications/tools/PDC-propose-draft-changes-tool.md`: PDC-FR-GMWR (`revises` on a rejected hunk).
- `specifications/ui/CMT-comments.md`: CMT-FR-11, CMT-FR-15, CMT-FR-16 (one state, one menu entry).
- `specifications/ui/DDS-draft-discussion.md`: resolve and reopen in the draft discussion column.
- `specifications/core/CMS-comments-storage.md`: CMS-FR-17 and the lock/resolution fold (resolved refuses posts, legacy lock reads as resolved).
- `specifications/ui/CTA-comment-agent-turns.md`: CTA-FR-QTNB, CTA-FR-XZUO, CTA-FR-CSZT, CTA-FR-TJCA.
- `specifications/tools/AUC-ask-user-comment-tool.md`: AUC-FR-10.
- `specifications/ui/CVP-conversation-presentation.md`: CVP-FR-34 (states `Active` and `Resolved`).
- `specifications/ui/CMP-comments-panel.md`: remove the Locked group.