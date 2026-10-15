## Intent

If a project User is working on has a GitHub projects integration, user should have an option to convert any Note into a GitHub issue. Publishing **moves** the note: after the issue exists, the note is removed from the project. 

## User journey

- User clicks a … button on the Note.
- The overflow menu of the note row (NTS-FR-18) gets a sixth entry, **Publish to GitHub**, placed between the reminder entry and **Delete**. 
- IF the metadata run condition fails (GHP-FR-HGVL) — no GitHub Project is selected in Project settings, or the publication remote or token does not resolve — the entry is disabled, and a tooltip states which reason applies.
- User clicks “Publish to GitHub” button, and overlay modal window pops up.
- The modal is titled **Publish to GitHub** and names the repository. It reuses the dialog rules of the draft chooser (NAW-FR-PSMO) and the list states of NAW-FR-WMEP (loading, empty, unavailable, loaded). It holds these controls:
  - **Parent**: the author chooses **Publish as a sub-issue of** (filterable parent combobox, as NAW-FR-YLKD, NAW-FR-XTGP, NAW-FR-YELA) or **Publish as a root issue**. **Root issue is selected when the modal opens** (no parent by default). Publish is disabled while the sub-issue option is selected and no parent is chosen. While the parent list is loading, empty or unavailable, the sub-issue option is disabled with its notice.
  - **Type**: one control for root and sub-issue alike. It offers **No Type** and each available Type. It opens on the new project setting **Note issue Type** (default `Feature`). The author can change it in the modal. Where the stored Type is not in the Type list, the control opens on **No Type** and a notice says so.
  - **Milestone**: always shown for root and sub-issue, whatever the sub-issue milestone policy says (SET-FR-ZHCT does not apply to notes). It offers **No milestone** and each open milestone. It always opens on **No milestone**, including when the chosen parent has a milestone.
  - **Labels**: a multi-select of the repository labels, for root and sub-issue alike. No label is selected when the modal opens.
  - An empty or unavailable Type, milestone or label list renders a notice in place of its control. A notice blocks nothing.
- **Title**: an empty field. The author must type it. A note has no title, so nothing is prefilled. The maximum is 256 characters. Publish is disabled while the trimmed title is empty.
- **Description**: a multi-line field prefilled with the Markdown source of the note body. The author can edit it freely. A body over 65,536 characters is refused inline. The body carries no publication marker.
- When the note has a discussion, the modal shows a warning that the discussion is deleted with the note. When the note has a reminder, the modal states that the reminder is removed too.
- The author clicks **Publish**. The modal creates the issue with one create call, plus one sub-issue link call when a parent is chosen. It writes no attempt record, no marker, and no retry or recovery state.
- On success the modal closes and the note is deleted through the existing note deletion (NTC-FR-21), so its reminder and its discussion go with it. An Info toast shows the issue number and a link that opens the issue in the system browser. The toast has no other address.
- On failure of the create call or the sub-issue link call, the modal stays open with every entered value, shows the displayable error inline, and raises an Error toast. The author retries by hand. The note is not deleted.
- If the issue exists but the note deletion fails, the note stays, the modal closes, and an Error toast says that the issue was created (with its number and link) and that the note could not be removed.
- Publish is disabled while a publish call runs, so one click creates one issue. Escape and the close control close the modal and change nothing.

## Requirements

- Reuse the existing GitHub token, publication-remote resolution, REST calls, sub-issue call, chooser components and metadata cache. Create no second token, remote or publication flow.
- **Cache**: the labels of the publication repository become a fourth list of the publication metadata cache. The existing background refresher reads it with the parent, Type and milestone lists (startup, every 30 minutes, on each change of its inputs). It has the same four states, merge rules, change event, and failure rules as the other lists. The draft chooser does not show labels.
- **Setting**: the Publication group of GitHub Project settings gets a **Note issue Type** selector (default `Feature`; it offers **No Type** and each available Type). It persists at once, like the other publication settings, and does not change the root issue Type or the sub-issue Type used by drafts.
- **Backend**: a new operation publishes a note: it takes the note id, title, body, optional parent, Type, optional milestone and label names, creates the issue, links the sub-issue, then deletes the note. The frontend sends no token. Draft publication, GitHub polling and their settings do not change.
- An operation that opens an issue URL in the system browser must work without a draft id, for the toast link. It accepts only an HTTPS issue address of the publication repository.

## Specifications this work changes

- `specifications/ui/NTS-notes.md`: NTS-FR-19 (five entries become six; add **Publish to GitHub**), NTS-FR-24 (the modal is a floating surface of the same mutual exclusion), NTS-FR-21 and NTS-FR-30 (a published note is deleted with its discussion); add the entry, its disabled rule and tooltip.
- `specifications/core/NTC-notes-storage.md`: NTC-FR-21 (the publish operation reuses the deletion transaction).
- `specifications/core/GHP-github-publication.md`: Intent (labels out of scope applies to draft publication only; add note publication), GHP-FR-AWEZ and the cache requirements (fourth list, labels), the metadata types, the new note-publish operation and its typed errors, and the rule that note publication writes no attempt record.
- `specifications/ui/NAW-new-artifact.md`: NAW-FR-WMEP (labels list state), and a reference from the note modal to NAW-FR-YLKD, NAW-FR-XTGP, NAW-FR-YELA and NAW-FR-PSMO. The draft chooser stays unchanged. A new UI specification for the note publish modal is the likely home of its own rules.
- `specifications/ui/SET-project-settings.md`: **Note issue Type** selector (new requirement near SET-FR-ZNLF), SET-FR-MQUE (the settings write command gains this value), SET-FR-XFNA and SET-FR-LJYT (Type states apply to it).
- `specifications/core/PSS-project-settings-storage.md`: PSS-FR-HWBG and PSS-FR-RDXE (the note issue Type is stored in the publication settings table; default `Feature`).
- `specifications/ui/NTF-notifications.md`: NTF-FR-24 lists the raises as a closed set. Add the note-publication Info toast (issue number, link) and Error toast.
- Add frontend and backend tests for: entry enablement and tooltip, defaults (root selected, No milestone, no labels, Note issue Type), empty title, the discussion warning, success (issue, note, reminder and discussion removed, toast), create failure (modal stays, inline error, Error toast), sub-issue link failure, deletion failure after creation, and labels in the cache.