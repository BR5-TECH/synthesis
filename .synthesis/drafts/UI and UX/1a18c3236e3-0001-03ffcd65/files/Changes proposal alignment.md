## Intent

We’ve got misalignment between how we visualized the changes proposed by the agents. in Draft editor we have a modern in-place interface. but when I work on the document (i.e. document artifact type), I see legacy change visualization with an overlay modal window. This must be aligned, the experience across the app should be algined. Draft editor has the right way to reflect proposed changes.

## User journey

- The author discusses a prompt artifact or a DOC file in its Editor tab. An agent proposes changes. The proposal enters review in place in the document column, with no modal. A notification, or the reference in the agent's comment, lands the author on the first undecided hunk.
- The author reads each hunk in the text it changes, edits its proposed text if needed, and accepts, rejects, or discusses it. The author can also use **Accept all** or **Reject all** in the review bar.
- The author starts a discussion on one hunk. The agent answers in that discussion. The agent may revise the hunk, and the author decides it.
- The proposal closes when no hunk is undecided. The decoration and the review bar go. The hunk discussions stay readable.
- The author has the same journey for a plain text (`.txt`) DOC file, on its source surface.

## Requirements

- Look and feel of visualization of the proposed changed must be aligned to the Draft editor visualization.
- Comments can be triggered from the documents, and start a discussion.

**Scope and order**
- Scope: proposals to a published prompt artifact and to a DOC file, reviewed in the Editor tab. The draft review stays the reference. Every behaviour below follows it.
- This work goes after the `Partial changes` draft. It supersedes that draft's rule that the compare modal of `PCR-prompt-change-review.md` does not change. It also follows the `Partial rejections` draft for hunk discussions and the single **Resolved** state.

**In-place review**
- A proposal to a prompt or DOC file is decided **per hunk**. It reuses the draft hunk model: anchored `add`, `del`, and `replace` hunks, hunk states `pending`, `accepted`, `rejected`, and `discussing`, resolution against the text as it stands, and the `lost` state. The author can edit a pending hunk's proposed text. Edits are held per hunk and are not a change to the file.
- The review renders inside the document column of the file's Editor tab, with the draft's decorations, hunk chips (**Accept**, **Reject**, **Discuss**), accessible names, review bar (previous, next, **Reject all**, **Accept all**, counter), keyboard rules, and "lost" and "not drawn" states. It is no modal and no scrim. Accepting a hunk writes the file through the file's existing write path. Rejecting writes nothing. A rejected hunk stays decorated and can be accepted while the proposal is open.
- A proposal that arrives while the Editor tab is open enters review with nothing to activate. The action row shows the indication of DCR-FR-03. The notification still lands the author on the file at the first undecided hunk.
- The review shows on the **rich** surface only. When a proposal arrives while the raw surface is showing, the tab switches to rich. While a proposal is pending, the raw toggle is disabled and states why. A file with no rich surface (plain text `.txt`) shows the same in-place review on its source surface, and has no toggle to disable.
- Remove the compare modal. Remove its three visualizations, its whole-candidate edit buffer, and its whole-candidate **Accept** and **Reject** with the feedback field. The review has no second layout.
- One undecided proposal per artifact stays. The agent's `rationale` is the body of its comment in the discussion, as for drafts. The decision comment counts the final state of each hunk.

**Hunk discussion**
- **Discuss** on a hunk in the Editor tab follows the Partial rejections model: it creates a new discussion record with its own thread, linked to that hunk through a `proposal_hunk`-type attachment. A hunk discussion works in the Editor column as in the draft's column. This covers the chooser, the subject strip, the composer, resolve and reopen, and the agent revising a hunk in place.
- Discussion capability is the same in the Editor tab and in the draft tab. A hunk discussion is not a fragment discussion and not a whole-target discussion.
- The comment reference of a prompt proposal scrolls the document column to the first undecided hunk. It no longer opens a modal. The reference of a hunk scrolls to that hunk. It opens or focuses the Editor tab first when the tab is not open.

**Data and tool**
- `propose_prompt_changes` records hunks on the draft model. When a call gives hunks, the backend resolves them against the file on disk and records them as hunks. When a call gives full `text`, the backend converts it to hunks at record time, and the author reviews those hunks. Refusal rules for text not found, found more than once, or overlapping stay as in `Partial changes`.
- A prompt or DOC proposal that is pending when this work ships has no hunks. Read it as a legacy proposal on the terms of DCP-FR-QLMH. Do not lose it.

**Accessibility and states**
- Every state a hunk carries is conveyed by more than colour. Every control is operable by keyboard alone. The author learns one review for drafts and for files.

## Specification impact

This work changes, and must update in the same change:
- `specifications/ui/PCR-prompt-change-review.md`: replace the modal with the in-place hunk review in the Editor column. Remove the modal-only requirements (comparison modes, whole-candidate edit buffer and write, whole-candidate decisions). Keep the notification and the landing on the file.
- `specifications/ui/DCR-draft-change-review.md`: generalise the review surface to the Editor tab of a prompt or DOC file, or reference it from PCR. Remove the rule that PCR adopts it by comparison.
- `specifications/ui/DDS-draft-discussion.md`: DDS-FR-MXHD says the Editor tab carries no review. Replace it. Add hunk discussions to the Editor tab.
- `specifications/ui/EDT-editor.md`: the raw toggle is disabled while a proposal is pending. The review switches the tab to rich. Add the source-surface case. Check `specifications/ui/ESH-editor-source-files.md` for plain text files.
- `specifications/ui/CMT-comments.md` (CMT-FR-48): the `prompt_proposal` reference scrolls the document column. It no longer opens the PCR modal.
- `specifications/ui/DFV-diff-viewer.md`: remove the statements that the prompt change review reads its comparison in the DFV modes and shares the user-global choice of mode.
- `specifications/core/PCP-prompt-change-proposals.md`: per-hunk record and states, per-hunk acceptance, hunk edits, legacy whole-candidate proposals.
- `specifications/tools/PPC-propose-prompt-changes-tool.md`: the author decides hunks one by one. Full text converts to hunks.
- `specifications/core/CMS-comments-storage.md`: the attachments of prompt proposals and prompt hunks, and the hunk discussion record for artifacts.
- `specifications/ui/NTF-notifications.md`: the landing target only, if its text names the modal.