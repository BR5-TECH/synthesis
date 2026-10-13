## Intent

Sometimes the user does not want to run graduation in the IDE. The user wants to copy the draft prompt and use it outside of the IDE. Add a **Copy as Source** action to the action control (the floating button in the bottom-trailing corner) of the New Artifact tab. It sits between **Graduate** and **Publish to GitHub**. It is an action of a draft only.

## User journey

- User works on a draft in its New Artifact tab, in any status.
- User activates the action control in the bottom-trailing corner. The entries show in this order: **Discuss**, **Graduate**, **Copy as Source**, **Publish to GitHub**, **Archive**.
- User activates **Copy as Source**.
- IF the clipboard write succeeds: the entry label changes to **Copied** for about 1.5 s, then the control collapses. The user pastes the text in another tool.
- IF the clipboard write fails: the control collapses and an Error toast shows. The draft does not change.

## Requirements

### What is copied

- The clipboard receives the **live prompt text** of the draft as shown in the editor, including unsaved edits, as plain text with no change (no line-ending conversion, no formatting).
- Only the prompt text is copied. The discussion column, comments, and attachments are not part of the copy.
- An empty prompt copies empty text.

### Action behaviour

- **Copy as Source** shows for every draft and for no other item. It has an icon and a label, like the other entries.
- It is enabled in every draft status (`active`, `archived`, `published`, `graduated`, `github_shadow`), while a graduation run holds the draft, and while the prompt is empty. The one exception is ACT-FR-07: where the draft no longer resolves in the project, every entry is disabled, this one too.
- The copy changes no status, writes no file, creates no version, marks nothing dirty, saves nothing, invokes no backend operation, and dispatches no agent turn.
- While the clipboard write runs, the entry ignores further activations.

### Feedback

- On success the entry label reads **Copied** for about 1.5 s, announced politely to assistive technology. Then the control collapses. Escape or a pointer-down outside the control collapses it at once. This is an exception to ACT-FR-03, which collapses the control the moment an action is activated.
- On failure the control collapses at once and an **Error** toast shows through the notification facility (`NTF-notifications.md`): a fixed key, a title and a body that say the draft was not copied, and no address. A click on it only dismisses it. No other message shows.

## Specifications this work must change

- `specifications/ui/ACT-action-control.md`: Intent and ACT-FR-04 (a draft also affords **Copy as Source**); ACT-FR-05 and the layout notes and wireframes (new order: Discuss, Graduate, Copy as Source, Publish to GitHub, Archive); ACT-FR-03 (the **Copied** interval before the collapse); ACT-FR-07 (the entry is disabled with the others).
- `specifications/ui/NAW-new-artifact.md`: the draft actions of the New Artifact tab (NAW-FR-17, NAW-FR-28, NAW-FR-44): add **Copy as Source** with its enablement in every status. Add the live-prompt copy and its failure toast.
- `specifications/ui/NTF-notifications.md`: NTF-FR-24 lists the raises. Add this Error raise, with no address. Note that the Toasts draft states that no new raise is added. That work must land first or include this raise.
- Add frontend tests for: entry order, the copied text (unsaved edits, empty prompt), enablement in every status, the **Copied** label and the collapse, the failure toast, and no change to the draft.