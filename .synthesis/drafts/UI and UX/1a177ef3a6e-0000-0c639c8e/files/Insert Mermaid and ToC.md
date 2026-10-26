## Intent

Authors of Markdown files need faster ways to structure and navigate a document in the rich editor: a fourth heading level, a one-click way to start a Mermaid diagram, and a table of contents to jump between headings.

Users: anyone who edits a Markdown file in the Editor tab or writes a prompt in the New Artifact (Draft) tab.

Scope: the formatting toolbar gets two controls (H4 and Mermaid). The tab's action cluster gets one control (ToC). The read-only Document tab, Diff tabs and source (non-`.md`) files get no ToC control. A past version read in the New Artifact tab (NAW-FR-10) gets no ToC control.

### Change 1

Top bar of the rich editor which holds formatting, needs an additional button for H4 style, directly after H1, H2 and H3. It behaves exactly as the H1, H2 and H3 buttons do: same size, same icon style, same active state when the caret is in an H4, same toggle back to a paragraph, same tooltip and accessible name pattern ("Heading 4"). The toolbar is one shared component, so H4 shows everywhere the toolbar shows: Editor tab, New Artifact tab, Settings draft template, Draft discussion, and the Diff Rich target and review-modal candidate where the toolbar is present.

### Change 2

Top bar of the rich editor which holds formatting, needs an additional button with a dropdown to insert a Mermaid diagram. The button has the same square size as the existing buttons, an icon and no text, an accessible name and tooltip "Insert diagram", and sits in the toolbar after the existing insert controls (quote, code block, link). Same surfaces as Change 1.

- Click opens a dropdown list of diagram types. It closes on selection, on Escape (focus returns to the button) and on a click outside. It is operable with the keyboard (arrow keys, Enter).
- Types: every diagram type that the bundled Mermaid version supports and renders today. The minimum set is Flowchart, Sequence, Class, State, ER, Gantt, Pie, Mindmap, Journey, Git graph, Timeline, Quadrant, Requirement and C4. The list comes from the bundled version, so a type the library supports is not left out.
- Selecting a type inserts a ```` ```mermaid ```` fence at the caret, filled with a short valid example for that type. Each example must render without a diagram error.
- WYSIWYG: if the caret is in an empty paragraph, the fence replaces it. Otherwise the fence is inserted as a new block after the current block. The new fence shows its source, with the caret inside it, so the author can edit at once; the author can switch to the diagram with the fence's view toggle.
- Raw Markdown mode: the fence text is inserted at the caret on its own lines.
- The insert is an ordinary edit: it changes the buffer, marks the file dirty, schedules the normal write, and is one undo step.
- Where the existing toolbar buttons are disabled (for example while the toolbar cannot act), this button is disabled too.

### Change 3

Top bar of the rich editor which holds the name of the draft and links (published etc), needs an additional dropdown button for the table of contents (ToC). It shows on the Editor tab and the New Artifact (Draft) tab, for any Markdown file. It joins the tab's action cluster (EDT-FR-16), as an icon button with accessible name and tooltip "Table of contents", leading of the comments control.

- Click opens a dropdown panel that lists every heading, H1 to H6, in document order, indented by level. An entry shows the heading text as plain text (inline formatting removed), truncated with an ellipsis where too long, with the full text in its tooltip. The panel scrolls within itself when long and is bounded in height.
- Click on an entry smooth-scrolls the document to that heading, places the caret at the start of the heading, moves focus to the editing surface, and closes the panel.
- WYSIWYG mode lists the headings of the rich body. The frontmatter region and the content of diagram fences or code blocks are never headings.
- Raw Markdown mode lists the `#` (ATX) headings of the source and scrolls the source to that line, with the caret at the start of the line. Lines inside the frontmatter and inside fenced code blocks are not headings.
- The list is built from the in-memory buffer when the panel opens and follows edits while it stays open. It follows a switch between WYSIWYG and raw mode.
- Empty state: when the document has no headings, the panel shows the text "No headings". The button stays enabled.
- A click on an entry changes no content: it does not mark the file dirty and adds no undo step.
- The panel is keyboard operable: arrow keys move through entries, Enter selects, Escape closes and returns focus to the button. The entry list has an accessible name, and the heading level is available to assistive technology.
- Open panel is part of the tab's own chrome behaviour: it closes on Escape, on selection and on a click outside, and it does not break the Find panel or the comment rail.

## User journey

1. The author opens a Markdown file in the Editor tab, or a draft in the New Artifact tab.
2. The author clicks H4 in the toolbar. The current block becomes a level 4 heading.
3. The author clicks the Insert diagram button, picks "Sequence". A sequence diagram fence with an example appears under the current block, with the caret in its source. The author edits the example.
4. The author clicks the Table of contents button, sees the indented heading list, and clicks "Prerequisites". The page scrolls smoothly to that heading and the caret stands at its start.
5. The author switches to raw Markdown mode. The Table of contents button now lists the `#` headings of the source. A click scrolls the source to that line.

## Requirements

- Look and feel must be aligned with the existing application style: same button size, theme colours in light and dark, same dropdown look as other dropdowns in the application. Use the `frontend-design` skill for UI development.
- New controls have accessible names, tooltips and keyboard operation. No new control uses text labels in the toolbar.
- Do not change the file format: a diagram stays a plain ```` ```mermaid ```` fence on disk (EDT-FR-MOYS, EDT-FR-67).
- Diagram examples must be bundled text. They need no network.
- The ToC must stay responsive on long documents (no visible delay when the panel opens on a document of several thousand lines).

## Specifications this work changes

- `specifications/ui/EDT-editor.md`: the formatting toolbar requirement and its wireframes (H4 and Insert diagram buttons), EDT-FR-16 (ToC control in the action cluster), and new requirements for the ToC panel.
- `specifications/ui/NAW-new-artifact.md`: NAW-FR-11 (same toolbar, action row now carries the ToC control) and its wireframe.
- `specifications/ui/SET-project-settings.md` (SET-FR-16) and `specifications/ui/DDS-draft-discussion.md`: toolbar wireframes only, where they list the heading buttons.
- `specifications/ui/EFR-editor-find-replace.md`: only if the band wording lists toolbar controls.