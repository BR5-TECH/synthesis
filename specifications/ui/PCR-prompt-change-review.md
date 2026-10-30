# Prompt change review

**Spec code:** `PCR`

## Intent
The surface on which an author decides the changes an agent has proposed to a **file the project already holds** — a published prompt artifact or a project document (a file of the Document type). The changes are decided **in the document**, in the document column of that file's Editor tab, with the review that `DCR-draft-change-review.md` defines for a draft: each hunk is decorated in the text it changes, and the author accepts, rejects, corrects, or discusses each on its own. It exists because a proposed change is only reviewable in the text it belongs to, and because an author who learns one review for drafts must not learn a second one for files. Accepting one hunk writes the file through the file's own write path; rejecting one writes nothing. Because the work that produced a proposal outlives the glance that started it, this surface also **calls the author back to it**: a proposal recorded while they look at something else raises a notification that lands them on the file at the first undecided hunk. Out of scope: the presentation of the review — the decorations, the action chip, the review bar, the gutter map, the keys, and the states of a hunk — which is `DCR-draft-change-review.md`'s and is applied here; the two-column shape of the Editor tab and the discussion column, which are `DDS-draft-discussion.md`'s; the proposal's storage, its hunks, its one-pending-per-artifact rule, and the acceptance transaction, all owned by `../core/PCP-prompt-change-proposals.md`; composing a proposal, which reaches the author through `../tools/PPC-propose-prompt-changes-tool.md`; the comment the proposal and the decision each take in the conversation, which `CMT-comments.md` renders and `../core/CMS-comments-storage.md` holds; the artifact's version history, which is Git's; and deciding what is worth interrupting for, which is `NTF-notifications.md`'s. This surface opens no modal, no scrim, and no second layout.

## Functional requirements
1. **PCR-FR-QHZN** A pending proposal renders **in place** in the document column of its file's Editor tab, as `DCR-draft-change-review.md` renders a draft proposal: the same decorations, action chip, review bar, gutter map, keys, lost and not-drawn states, and hunk edits (DCR-FR-05 to DCR-FR-33, DCR-FR-QPRV, DCR-FR-BQNL, DCR-FR-CXZG, DCR-FR-LGHZ, DCR-FR-TSNW), with the operations of this spec. It opens no modal and no scrim.
   - *Why:* One review for drafts and for files means the author learns how to decide a change once.
2. **PCR-FR-WLTE** The review shows on the **rich** surface of a Markdown file only. A proposal that arrives while the raw surface shows switches the tab to rich (per `EDT-editor.md` EDT-FR-RCHS), and while a proposal is pending the raw toggle is disabled and states why (per `EDT-editor.md` EDT-FR-RWTG).
3. **PCR-FR-SKRV** A file with no rich surface shows the same review on its **source surface** (per `ESH-editor-source-files.md` ESH-FR-SRVW): the file text with the decorations, the action chip, the review bar, and the gutter map. The file text is read-only while the proposal is pending, and the surface says so. A hunk's proposed text stays editable as plain text.
   - *Why:* The source text field cannot carry decorations inside its own text, so typing in it would move text under the hunks it shows.
4. **PCR-FR-AXPW** **Accept** on one hunk flushes the session's scheduled write, quiesces the session (per `EDT-editor.md` EDT-FR-81), dismisses a standing external-change modal unanswered (per `EXC-editor-external-change.md` EXC-FR-AEQZ), and invokes `"accept prompt change hunk"`. Success resets the session to the file (per EXC-FR-JAWT). Failure lifts the quiesce, keeps the session, and renders the typed error beside the hunk.
5. **PCR-FR-HDMC** **Discuss** on a hunk opens the composer of a **new** discussion of the artifact in the discussion column (per `DDS-draft-discussion.md` DDS-FR-EHDK), seeded with the agent's tag. Posting invokes `"open discussion (target, fragment target, body, attachments)"` with one `proposal_hunk` naming the hunk and the artifact, then `"hold prompt change hunk"` for an undecided hunk. An existing hunk discussion is shown instead.
6. **PCR-FR-LGVY** A proposal that `"load prompt change proposal hunks"` reports as **legacy** renders as one hunk covering the whole file and accepts no edit to that hunk. It is accepted and rejected on the ordinary terms.
7. **PCR-FR-RFJC** The `prompt_proposal` reference on the agent's comment scrolls the document column to the first undecided hunk, and a `proposal_hunk` reference that names an artifact scrolls it to that hunk (per `CMT-comments.md` CMT-FR-48). Each opens or focuses the file's Editor tab first where that tab is not the active one.
8. **PCR-FR-14** After the decision that **resolves** the proposal, the surface invokes `"dispatch agent turn"` once for each distinct agent of the conversation's active set, and not at all where that set is empty (per `CTA-comment-agent-turns.md` CTA-FR-LCFU, CTA-FR-QUXJ). Each dispatch names the outcome's `comment_id` and `origin_kind`. An unresolving decision dispatches nothing, and a dispatch that fails does not undo the decision.
9. **PCR-FR-15** An `acceptance_incomplete` is not a refusal. The surface states, at the foot of the document column, that the change is applied and the conversation is not yet told, with a **Try again** control that invokes `"complete prompt change decision"` and nothing else. On success it dispatches on the terms of PCR-FR-14. A `write_failed` states that the file has not changed.
10. **PCR-FR-16** A proposal that arrives while its file's Editor tab is open enters review in place, with nothing for the author to activate. An Editor tab whose artifact holds a `pending` proposal also renders an indication in its action cluster (per `EDT-editor.md` EDT-FR-16), which scrolls the document column to the first undecided hunk. The indication clears when the proposal closes.
11. **PCR-FR-17** This surface mounts a subscriber to `"prompt change proposals changed"` at the **shell**, and raises through `NTF-notifications.md` (NTF-FR-01) when a proposal is recorded. The raise carries a key derived from the proposal's own id, the level Info, a title naming the agent and the file, and the project-file address of NTF-FR-03. Nothing is raised when a hunk is decided.
    - *Why:* A tab-scoped listener hears nothing for a file whose tab is closed, which is the case the author most needs telling about.
12. **PCR-FR-18** Whether the raise becomes a notification is `NTF-notifications.md`'s policy (NTF-FR-02, NTF-FR-08, NTF-FR-26). Activating it opens or focuses that artifact's Editor tab (per `NTF-notifications.md` NTF-FR-17) and scrolls the document column to the first undecided hunk. A proposal closed before the activation shows the file with no review.
13. **PCR-FR-26** When `"prompt change proposals changed"` carries a proposal in state `accepted` or `rejected`, the shell subscriber **retracts** the raise for that proposal with the key it raised under (per `NTF-notifications.md` NTF-FR-38), and clears the proposal from the reading of PCR-FR-27. The retraction is unconditional, and an acceptance whose comment is still owed retracts like any other.
14. **PCR-FR-27** Everything this surface says about a proposal it says from **one reading per artifact**, taken with `"list prompt change proposals"` when a surface first needs it and kept current from `"prompt change proposals changed"` thereafter. A failed reading is asked for again on the terms of `CTA-comment-agent-turns.md` CTA-FR-LOOE and is never reported as an absence.
15. **PCR-FR-28** The readings of PCR-FR-27 belong to the content root they were taken from, and are discarded when the project closes and when the active worktree changes.
16. **PCR-FR-29** This surface and `DCR-draft-change-review.md` share presentation and no state. They read different stores, invoke different operations, and follow different events, and a draft review behaves the same whether or not a prompt proposal stands anywhere in the project.

## User stories
- As an author, I want to read a proposed change to a published file in the passage it changes, so that accepting is a judgement rather than a hope.
- As an author, I want to decide a change to a file the way I decide a change to a draft, so that I learn one review.
- As an author, I want to accept the changes I agree with and keep discussing the rest, so that one objection does not block the others.
- As an author looking at a change that is right but for one word, I want to fix that word and accept it.
- As an author working in another tab, I want to be told when an agent has something for me to look at.

## Wireframes

An Editor tab on `prompts/review.md` with a proposal of three hunks, the second under review:

```
│ Prompt · review.md                       [ ◉ 3 ] [ ☰ ] [ ◧ ⊘ ] [ 70|30 ] [ 💬 ] │
├──────────────────────────────────────────────────┬─┬────────────────────────────┤
│  ## Steps                                       ▍│┃│ Review prompt              │
│                                                  │┃│ @arch · 4m ago             │
│  ┃−  Read the file, summarise it and file a note.▍│┃│ The instructions ask for   │
│    ┌─[ 2/3  [ Accept ] [ Reject ]  Discuss → ]─┐  │┃│ three things in one        │
│  ┃+  Read the file.                            │  │┃│ sentence. I have split     │
│  ┃+  Summarise it, then file a note.           │  │┃│ them.                      │
│    └───────────────────────────────────────────┘ ▍│┃│ [ 3 changes  +2 −1 ]       │
│                                                  │┃│                            │
│  ┃+  ## Output                          (pending) ▍│┃│                            │
├──────────────────────────────────────────────────┤┃├────────────────────────────┤
│  ‹ ⌥↑   Change 2 of 3   ⌥↓ ›    [ Reject all ] [ Accept all ] │ Write a comment…   │
└──────────────────────────────────────────────────┴─┴────────────────────────────┘
```

- Layout notes: the review is drawn by `DCR-draft-change-review.md`'s rules, in the document column below the Editor's toolbar. The review bar stands at the foot of the document column and holds its place while the document scrolls. The gutter map rides the outer edge of the scroller. The indication (◉) stands at the leading end of the action cluster. The raw toggle (◧) is disabled while the proposal is pending and carries its reason as its tooltip. On the source surface of a file with no rich surface, the hunk blocks stand between the lines of the file text, and a statement above the page says that the file text is read-only until the proposal is decided.

## UI contract boundary
- **Owned by the UI**: the placement of the review in the Editor tab's document column, on the rich surface or on the source surface; the switch to rich and the disabled raw toggle; the read-only file text of the source surface while a proposal is pending; the order of flush, quiesce, decide, and reset around each acceptance; the indication in the action cluster; the scroll to the first undecided hunk from each route; the hunk discussion Discuss opens and the reveal of one that exists; the statement and the retry of an `acceptance_incomplete`; the one reading per artifact; and the raise and the retraction, with the key they carry. The decorations, the action chip, the review bar, the gutter map, the keys, the hunk edit buffer and its schedule, and the states of a hunk are `DCR-draft-change-review.md`'s, applied with the operations below in place of the draft's.
- **Delegated to backend (abstract)**:
  - `"list prompt change proposals"`, `"load prompt change proposal hunks"`, `"accept prompt change hunk"`, `"reject prompt change hunk"`, `"hold prompt change hunk"`, `"edit prompt change hunk"`, `"decline prompt change proposal"`, and `"complete prompt change decision"` — owned by `../core/PCP-prompt-change-proposals.md`. **Reject all** invokes `"decline prompt change proposal"` once, and **Accept all** invokes `"accept prompt change hunk"` once for each undecided hunk on the terms of `DCR-draft-change-review.md` DCR-FR-CXZG.
  - `"open discussion (target, fragment target, body, attachments)"` — the hunk discussion, owned by `../core/CMS-comments-storage.md`; it carries one `proposal_hunk` attachment that names the artifact (CMS-FR-TQVB, CMS-FR-AHDQ).
  - `"load artifact contents by id"` — the file text the hunks are drawn against, read through the artifact's editing session on the terms an Editor tab reads it. Owned by `../core/PST-project-storage.md`.
  - `"dispatch agent turn"` — owned by `../core/AGC-agent-conversations.md`; invoked after the decision that resolves the proposal (PCR-FR-14).
  - The events `"prompt change proposals changed"` (`../core/PCP-prompt-change-proposals.md` PCP-FR-17) and `"artifact changed externally"` (`../core/PST-project-storage.md` PST-FR-16) keep the review, the indication, and the reading current.
  - The raise and the retraction are `NTF-notifications.md`'s facility (NTF-FR-01, NTF-FR-38) and introduce no operation here.

## Non-functional requirements
- The review stays responsive on a file of several thousand lines and a proposal of up to 50 hunks.
- Resolving the hunks against the file is settled rather than run per keystroke, and moves neither the caret nor the scroll position.
- Every state a hunk carries is conveyed by more than colour, and every control of the review is operable by keyboard alone.
- Nothing in the review scrolls horizontally, and it stays usable at the minimum window size the shell supports (≥1280×800).
- Editing a hunk costs one call per settled burst of typing and never a write to the file.
- A raise costs the shell one subscription and nothing per artifact: a project with no proposals anywhere raises nothing and reads nothing.
- Nothing here requires network access beyond what the decision's own comment resolves.
