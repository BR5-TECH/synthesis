# Note issue publication

**Spec code:** `NIP`

## Intent
The modal that **moves** a note out of the project and into a GitHub issue. A note is often a task that has outgrown the project: a gap, a follow-up, or a bug that somebody else must pick up. In a project with a GitHub Project integration, the author publishes the note as an issue of the publication repository, as a root issue or as a sub-issue of an open parent, with a Type, a milestone, and labels. When the issue exists, the note is deleted with its reminder and its discussion, so one piece of work has one home. The modal opens from the **Publish to GitHub** entry of a note row (`NTS-notes.md` NTS-FR-FYUJ) and reuses the dialog, list, and parent rules of the draft publication chooser (`NAW-new-artifact.md`). Out of scope: an attempt record, a publication marker, a retry or recovery state, and any history of the publication, which draft publication alone keeps; assignees, GitHub Project fields, and issue state; publishing the note's discussion; keeping the note after publication; and the token, the remote, and the metadata lists, which `../core/GHP-github-publication.md` owns.

## Functional requirements
1. **NIP-FR-TVNN** Choosing **Publish to GitHub** on a note row opens the **note publish modal**. The modal is titled **Publish to GitHub** and names the publication repository as `owner/name`, from the answer of `"get note publication availability"` that enabled the entry (per `NTS-notes.md` NTS-FR-FYUJ).
2. **NIP-FR-YQFL** The modal follows the dialog rules of `NAW-new-artifact.md` NAW-FR-PSMO: it is a modal dialog with an accessible name, focus moves to the first enabled control when it opens, Tab stays inside it, the two parent options form one labelled radio group, each notice is announced as a status, and every control works by keyboard alone.
3. **NIP-FR-HYUJ** When it opens, the modal invokes `"get publication metadata cache (remote name)"` once and `"refresh publication metadata"` once, for the remote that the availability answer names. Cached lists render at once and stay usable while the refresh runs.
4. **NIP-FR-GPEO** The parent list, the Type list, the milestone list, and the label list each render one of the four states of `NAW-new-artifact.md` NAW-FR-WMEP: loading, empty, unavailable with the displayable error, or loaded. Where the cache holds no list and the refresh answers `failed`, each list is unavailable with the displayable error of the refresh.
5. **NIP-FR-PWYE** While it is open, the modal follows `"publication metadata changed"` for its remote and the answer of its refresh, and renders the new lists. An open parent dropdown keeps its filter text and its scroll position, on the terms of `NAW-new-artifact.md` NAW-FR-HRXE.
6. **NIP-FR-DZWQ** The **Parent** group offers **Publish as a sub-issue of**, with its parent combobox, and then **Publish as a root issue**. The root option is selected when the modal opens. A list that loads, refreshes, or changes never selects the sub-issue option.
7. **NIP-FR-WVQP** The parent combobox follows `NAW-new-artifact.md` NAW-FR-YLKD, NAW-FR-XTGP, NAW-FR-YELA, NAW-FR-VJRW, and NAW-FR-PDXJ: one filterable list of the cached parent issues, a bounded overlay that stays inside the window, a filter on the number and the title, and the WAI-ARIA combobox pattern.
8. **NIP-FR-YEKZ** While the parent list is loading, empty, or unavailable, the sub-issue option is disabled and its notice shows. Choosing a parent selects the sub-issue option. Selecting the root option keeps the chosen parent but sends none.
9. **NIP-FR-ONHC** Where a new parent list no longer holds the chosen parent, the modal clears the choice. With the sub-issue option selected, **Publish** then stays disabled until the author chooses a parent or selects the root option.
10. **NIP-FR-NWGA** One **Type** control serves the root option and the sub-issue option alike. It offers **No Type** and each Type of the Type list, and sends the chosen name, or no Type for **No Type**. No sub-issue Type of the draft publication settings applies to it.
11. **NIP-FR-UMKW** The Type control opens on the resolved **Note issue Type** of the metadata (per `../core/GHP-github-publication.md` GHP-FR-JGXW). Where a Note issue Type is stored that the Type list does not hold, the control opens on **No Type** and a notice names the stored Type as unavailable. Where No Type is stored, it opens on **No Type** with no notice.
12. **NIP-FR-PWAB** The Type control takes its opening value when the Type list first renders loaded, and a later list never changes a value the author chose. Where a new list no longer holds the chosen Type, milestone, or label, that control resets to **No Type**, to **No milestone**, or drops that label.
13. **NIP-FR-SHMC** A **Milestone** control shows for the root option and the sub-issue option alike. It offers **No milestone** and each open milestone, and sends the chosen milestone number. It opens on **No milestone**, also where the chosen parent has a milestone. The sub-issue milestone policy (per `SET-project-settings.md` SET-FR-ZHCT) does not apply.
14. **NIP-FR-DKXP** A **Labels** control is a multi-select of the labels of the publication repository, for the root option and the sub-issue option alike. No label is selected when the modal opens. It sends the names of the selected labels, or an empty list.
15. **NIP-FR-VLFR** An empty or unavailable Type, milestone, or label list renders a notice in place of its control, which states that the issue publishes without a Type, a milestone, or labels. A notice blocks nothing.
16. **NIP-FR-FDAN** A **Title** field is empty when the modal opens, as a note has no title. It accepts at most 256 characters. **Publish** is disabled while the trimmed title is empty.
17. **NIP-FR-DIXC** A multi-line **Description** field opens on the Markdown source of the note body, and the author can edit it freely. A description over 65,536 characters is refused inline with the limit stated, and **Publish** is disabled until it fits.
18. **NIP-FR-ITUR** Where the note carries a discussion, the modal shows a warning that the discussion is deleted with the note. Where the note carries a reminder, the modal states that the reminder is removed too. A note with neither shows neither statement.
19. **NIP-FR-IJAL** **Publish** is disabled while the title or the description is refused (NIP-FR-FDAN, NIP-FR-DIXC), while the sub-issue option is selected with no parent chosen, while a publish call is in flight, and while every list is loading with no cached list. Otherwise it is enabled, whatever state the lists are in.
20. **NIP-FR-SJGY** **Publish** invokes `"publish note to github (note id, title, body, note publication choice)"` once, with the note id, the trimmed title, the description, and the choice: the chosen parent's issue number or null, the Type name or null, the milestone number or null, and the selected label names. The modal sends no token.
21. **NIP-FR-MTGO** An answer of `published` closes the modal, removes the note's row from the Notes panel, and closes the conversation tab of its discussion, on the terms of `NTS-notes.md` NTS-FR-30. It raises the Info toast of `NTF-notifications.md` NTF-FR-AGRZ with the issue number and its link.
22. **NIP-FR-ISQT** A typed error keeps the modal open with every entered value, renders the displayable error inline, and raises the Error toast of `NTF-notifications.md` NTF-FR-KBBO. The note stays. The author retries by hand, with **Publish**.
23. **NIP-FR-SNLZ** An answer of `sub_issue_link_failed` keeps the modal open with every entered value, and renders inline that the issue was created with its number but was not linked to its parent. It raises the Error toast of `NTF-notifications.md` NTF-FR-KBBO. The note stays.
    - *Why:* The issue exists on GitHub, so the author must know that a second **Publish** creates a second issue.
24. **NIP-FR-GQVQ** An answer of `note_not_removed` closes the modal and keeps the note in the Notes panel. It raises the Error toast of `NTF-notifications.md` NTF-FR-IWAR, which states that the issue was created, with its number and its link, and that the note could not be removed.
25. **NIP-FR-WUID** **Cancel**, the close control, and Escape close the modal. While the parent dropdown is open, Escape closes the dropdown only. An outside click does not close it. A dismissal invokes nothing, changes no note and no setting, and returns focus to the `[⋯]` button of the row. While a publish call is in flight, no dismissal closes the modal.
26. **NIP-FR-DQQN** The modal keeps no state after it closes. Each opening starts from the defaults of NIP-FR-DZWQ, NIP-FR-UMKW, NIP-FR-SHMC, NIP-FR-DKXP, NIP-FR-FDAN, and NIP-FR-DIXC, also after a failed publication.

## User stories
- As an author, I want to turn a note that has grown into real work into a GitHub issue, so that somebody outside this application can pick it up.
- As an author, I want the note to go away when the issue exists, so that the work does not live in two places.
- As an author, I want to choose the parent, the Type, the milestone, and the labels when I publish, so that the issue lands where my team looks for it.
- As an author, I want to be told before I publish that the note's discussion goes with it, so that I do not lose a conversation by surprise.

## Wireframes
```
┌ Publish to GitHub ─────────────────────── acme/widgets ─ [×] ┐
│ Parent                                                       │
│ ( ) Publish as a sub-issue of                                │
│     ┌──────────────────────────────────────────────────┐     │
│     │ Choose a parent issue…                         ▾ │     │
│     └──────────────────────────────────────────────────┘     │
│ (•) Publish as a root issue                                  │
│                                                              │
│ Type        [ Feature              ▾ ]                       │
│ Milestone   [ No milestone         ▾ ]                       │
│ Labels      [ bug ×  ui ×        + ▾ ]                       │
│                                                              │
│ Title                                                        │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │                                                          │ │
│ └──────────────────────────────────────────────────────────┘ │
│ Description                                                  │
│ ┌──────────────────────────────────────────────────────────┐ │
│ │ Check the FR numbering before the handoff.               │ │
│ │                                                          │ │
│ └──────────────────────────────────────────────────────────┘ │
│ ⚠ This note has a discussion. It is deleted with the note.   │
│ ⓘ The reminder of this note is removed too.                  │
│ <inline error>                                               │
│                                         [Cancel] [Publish]   │
└──────────────────────────────────────────────────────────────┘
```
- Layout notes: the modal is a dialog over the main window, with the dimensions and the dropdown rules of the draft publication chooser (per `NAW-new-artifact.md` NAW-FR-XTGP). The Type, Milestone, and Labels controls sit below the Parent group, as they apply to both options. A notice of an empty or unavailable list takes the place of its control on the same row. The discussion warning, the reminder statement, and an inline error sit above the action row. **Cancel** leads **Publish**.

## UI contract boundary
- **Owned by the UI**: the note publish modal, its title and repository name, its Parent radio group and the option selected when it opens, the parent combobox and its dropdown, filter, and keyboard model, the Type, Milestone, and Labels controls with their opening values and their notices, the Title and Description fields with their limits, the discussion warning and the reminder statement, the enablement of **Publish**, the inline error, the dismissal and focus rules, and the toasts it raises.
- **Delegated to backend (abstract)**:
  - `"get publication metadata cache (remote name)"` and `"refresh publication metadata"` — owned by `../core/GHP-github-publication.md` (GHP-FR-JTYL, GHP-FR-SIMK). The event `"publication metadata changed"` (GHP-FR-JIAU) keeps the lists of an open modal current.
  - `"publish note to github (note id, title, body, note publication choice)"` — owned by `../core/GHP-github-publication.md` (GHP-FR-JGNI). It creates the issue, links the sub-issue, and deletes the note through the transaction of `../core/NTC-notes-storage.md` NTC-FR-21.
  - `"open publication repository issue (url)"` — owned by `../core/GHP-github-publication.md` (GHP-FR-MAMB). The link of the toasts of NIP-FR-MTGO and NIP-FR-GQVQ invokes it.
  - `"get note publication availability"` is invoked by `NTS-notes.md` before the modal opens, and the modal reads the remote and the repository from its answer.

## Non-functional requirements
- The modal opens with no wait where the cache holds lists: it renders the cached lists at once and reads GitHub only through the one refresh it starts.
- One publication is one create request, at most one sub-issue link request, and one note deletion, whatever the length of the description.
