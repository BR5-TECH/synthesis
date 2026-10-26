## Intent

Two small layout changes. The author of Markdown files and drafts finds the Table of contents (ToC) control in a better place. The author of notes gets a `+ Note` button that matches the `+ Draft` button.

Users: anyone who edits a Markdown file in the Editor tab, writes a prompt in the New Artifact (Draft) tab, or adds notes in the Notes panel.

Scope: placement and alignment only. No behaviour changes. The ToC panel, its contents, and the conditions in which the ToC control shows stay as they are (Markdown files only; no ToC control over source files, Diff tabs, or a past version read in the New Artifact tab).

### Change 1

The ToC button must stand directly to the left of the toggle button that switches between source and rich editor modes. Today it stands next to the discussion controls, and this does not fit well.

- Rule: the ToC control always belongs to the group of the mode toggle. It stands immediately to the left of the toggle, with no other control between them. The comments control, or the discussion-column control, never stands between them. The discussion controls have no relation to ToC.
- Editor tab: the rule applies in WYSIWYG mode and in raw Markdown mode, whether or not the formatting toolbar or a Find panel is in the band. The floating discussions control is not part of the rule.
- New Artifact tab: the rule applies the same way while the live prompt shows. The ToC control stands immediately to the left of the same source/rich toggle, and the discussion-column control does not stand between them.
- Controls other than ToC keep their current order, look, size, tooltips, and accessible names. The ToC control keeps its icon, its accessible name and tooltip "Table of contents", its panel, and its keyboard behaviour.
- Where the existing wireframes disagree about the order of the comments control and the mode toggle, make them agree with the rule and change nothing else.

### Change 2.

Notes panel has `+ Note` button. Right now it is aligned to the trailing (right) edge. It should be aligned to center, like the `+ Draft` button in the Drafts panel.

- The pinned `+ Note` button (below the scrollable list) is centred across the panel's width. It stays pinned below the list and visible while the list scrolls.
- The `+ Note` button of the empty state is centred across the panel's width too. Check that it is, and fix it if it is not.
- The button keeps its size, style, label, and behaviour. A failed-create error that shows below the button keeps its place below the button.
- The result is the same in all three scope positions (This file, Project, All).

## Requirements

- Look and Feel must stay "as is". Only the position of the ToC control and the alignment of `+ Note` change.
- Both changes apply at every panel and tab width. At a narrow tab width, ToC and the mode toggle stay together and no control overlaps another.
- This draft replaces the placement sentence of Change 3 of the draft "Insert Mermaid and ToC" ("leading of the comments control"). Other parts of that draft stay.

## Specifications this work changes

- `specifications/ui/EDT-editor.md`: EDT-FR-16 (cluster order) and EDT-FR-LGQA (ToC "directly before the comments control" becomes "directly before the mode toggle"), and any wireframe of the action cluster.
- `specifications/ui/NAW-new-artifact.md`: NAW-FR-TFFX (ToC "directly before the control that hides and shows the discussion column") and the layout note that repeats it.
- `specifications/ui/DDS-draft-discussion.md`: DDS-FR-XQMF, only where it states the ToC position.
- `specifications/ui/CMT-comments.md` and `specifications/ui/EFR-editor-find-replace.md`: action-cluster wireframes, only to match the new order.
- `specifications/ui/NTS-notes.md`: the wireframe (`[ + Note ]` centred) and the layout note about the create affordance, including the empty state.