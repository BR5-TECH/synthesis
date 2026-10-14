# Intent

Integration with GitHub Projects works, but certain improvements are needed.

## Change 1.

The **publication chooser** (the **Publish to GitHub** modal) changes its option order and gets a default Type for root issues.

- The first option is **Publish as a sub-issue of** with its parent combobox. It is selected when the chooser opens.
- **Publish as a root issue** is listed under it, with its **Type** and **Milestone** controls.
- The root option stays enabled in every list state. When the parent list is loading, empty, or unavailable, the sub-issue option is disabled, its notice shows, and the root option is selected instead. A failed read never blocks root publication.
- This order replaces the root-first order of NAW-FR-TCQB.

The Project settings **Publication** group gets a new **Root issue Type** selector. It is separate from the existing **Sub-issue Type** (SET-FR-HPKV), which keeps its meaning and its `Task` default.

- Unset means **No Type**. The root issue has no default Type until the author selects one.
- The value persists at once in the project-public publication settings, as the other Publication settings do.
- When the chooser opens, the root **Type** control is pre-filled with the stored Type. The author can still change it for that publication.
- A stored Type that the Type list does not hold stays stored and shows as unavailable in settings. The chooser then pre-fills **No Type**, and the issue publishes without a Type.

## Change 2.

When the draft has a current publication record, the action control (the floating button) shows `Update on GitHub` in place of `Publish to GitHub`.  The button updates the issue in place. It creates no new issue.

- The update writes the issue title (the draft's current name) and the issue body (the draft's current content followed by the same publication marker line) only. Milestone, Type, parent, labels, and everything else on the issue stay as GitHub holds them. The update sends a partial change that names only the title and the body.
- Before it writes, the update reads the issue. If the issue is closed, a dialog asks the author to **Update closed issue** or **Cancel**. **Cancel**, Escape, and the close control write nothing. **Update closed issue** continues the update.
- If the issue is deleted or moved, or if GitHub rejects the request (for example, no access), an error toast shows the displayable error. Nothing is written locally.
- The button is disabled while a call is in flight, while a publication attempt stands, and while the publication eligibility check fails. It states the reason, as **Publish to GitHub** does.
- A successful update appends one history record. The record keeps the repository, issue number, issue URL, marker, and publication choice of the record before it, and adds an **updated** instant in RFC 3339 UTC. Earlier records stay unchanged. The new record becomes the current record. The draft status stays `published`.
- The publication group of the draft information modal shows the updated instant of a record that has one.

### User journey

- User publishes a draft as an issue on GitHub.
- The draft's publication record holds the repository and the issue number. The update reuses them.
- User updates a draft.
- User clicks `Update on GitHub` button
- IDE reads the issue. If the issue is open, IDE replaces the title and the body with the new content. If the issue is closed, IDE shows the dialog first. The update continues only if the user selects **Update closed issue**.
- IDE appends a history record with an updated instant and the `Published` tag keeps the same issue number.
- IF: the user cancels the closed-issue dialog THEN nothing is written and no toast shows.
- IF: update fails due to error (deleted, moved, no access, network) THEN error Toast is visualised and no record is appended.

## Requirements

- Existing GitHub integrations should be used.
- Draft meta information can’t persist any credentials.
- The update uses the existing token resolution, remote resolution, and REST client of the publication. It adds no token flow and no `gh` CLI dependency. It works on a GitHub Enterprise host as publication does.
- The update never edits an issue that a record other than the current record names.

## Specifications that change

- `specifications/ui/NAW-new-artifact.md`: NAW-FR-TCQB, NAW-FR-YLKD, NAW-FR-ASYN, NAW-FR-ZOAS, NAW-FR-RBTE, the chooser wireframes, and the draft action control rows. Add the `Update on GitHub` action, the closed-issue dialog, and the error toast.
- `specifications/ui/ACT-action-control.md`: the draft action list shows `Update on GitHub` instead of `Publish to GitHub` while a current record exists.
- `specifications/ui/SET-project-settings.md`: add the **Root issue Type** selector to the Publication group.
- `specifications/core/GHP-github-publication.md`: GHP-FR-KVRH, GHP-FR-NQWX, GHP-FR-MDLD (add the root Type setting); GHP-FR-QLDF, GHP-FR-UZMX, GHP-FR-JAWD (an update appends a record and edits the issue of the current record); add the update operation and its typed errors.
- `specifications/core/PSS-project-settings-storage.md`: PSS-FR-HWBG, PSS-FR-RDXE (store the root issue Type).
- `specifications/core/DRS-draft-storage.md`: DRS-FR-EJBM, DRS-FR-XNLP (the history record gets an optional updated instant).
- `specifications/ui/DFI-draft-information.md`: DFI-FR-BZQN (show the updated instant).