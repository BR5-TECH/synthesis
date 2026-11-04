## Intent

Markdown is missing certain things I badly need: vertical breaks of different size and horizontal alignment: left/right/center.

I want to extend our rich editor for all artifacts to support visualization of this. The author writes both features as plain HTML comments in the Markdown file, so the file stays valid Markdown. Alignment applies to one block. A vertical break is a fixed-size empty space. The rich surfaces render both. Every other HTML comment stays in the file and shows as a small chip.

## User journey

### Toolbar

- User works on a Markdown artifact (document, draft, prompt, template) on a rich surface that shows the formatting toolbar.
- The toolbar shows three dropdown buttons: **Hx** (heading), **Vx** (vertical break), and **Ax** (alignment). Hx replaces the four H1–H4 buttons of the heading group (EDT-FR-VUGQ) and keeps their behaviour. Each dropdown holds icon toggle buttons: H1–H4 in Hx, V1–V4 in Vx, and Left, Center, Right in Ax, using the standard alignment icons.
- Each dropdown is keyboard-operable, closes on Escape, and has an accessible name and tooltip on its trigger and on each item. The trigger shows the active choice of the caret block where one exists.

### Vertical break

- A break is one line, directly above a block: `<!-- % -->` (V1), `<!-- %% -->` (V2), `<!-- %%% -->` (V3), `<!-- %%%% -->` (V4). The number of `%` is the level. V1 is the biggest: V1 = 12, V2 = 8, V3 = 5, V4 = 3 body line heights. Five or more `%` show as V4.
- The rich surface shows the break as an empty spacer of that height. The spacer is selectable as one unit, and Backspace or Delete removes it.
- Vx inserts the break directly above the caret block, so a V1 above an H1 moves the H1 down the page. With a selection over several blocks, Vx inserts one break above the first selected block. When a spacer is selected, a Vx choice resizes that spacer and inserts nothing.
- A comment such as `<!-- %x -->` or a multi-line comment that holds `%` is not a break. It is an ordinary comment.

### Alignment

- A marker is one line, directly above a block: `<!-- center -->` or `<!-- right -->`. It aligns that block only. It has no closing marker and cannot nest. Left is the default and has no marker.
- Ax with a selection over several blocks writes one marker above each selected top-level block and replaces any marker already there. **Left** removes the marker.
- Every top-level block type can be aligned: paragraph, heading, list, quote, image, table, code block, diagram. A list or a quote aligns as a whole. Blocks inside a list item or a quote are not aligned alone.
- The rich surface does not show the marker. A marker with no block directly below it (end of file, or another comment below it) is an ordinary comment.

### Ordinary HTML comments

- Every other HTML comment, one-line or multi-line, shows as one small chip on every rich surface. A comment inside a paragraph shows as an inline chip.
- On an editable surface the chip is selectable and keyboard-focusable. Backspace or Delete removes the comment. Hover and keyboard focus show an overlay tooltip with the comment text as plain text.
- Read-only rich views (Document tab, Diff original side) show the same chip and the same breaks and alignment. They offer no removal.

## Requirements

- **Scope:** all rich surfaces: Editor tab, New Artifact tab, Settings draft template, Diff Rich, and the Document tab rich view. The toolbar stays only where it exists now. Raw-text and source surfaces show the markers as plain text and do not change.
- **File:** markers are written in the canonical form above (lowercase, single spaces, ASCII `-->`). A reader accepts extra spaces inside the comment. Opening, rendering, and saving without an edit leave the file bytes unchanged (EDT-FR-66 to EDT-FR-69). Insert, resize, align, and remove are normal edits: they mark the file dirty and enter the undo history.
- **Safety:** comment text is never rendered as HTML and is never interpreted (DTV-FR-SSQI).
- **Size:** a break is a multiple of the body line height, so it follows the Rich Markdown size and line-height settings.
- **Multi-line comments:** one comment is one chip, whatever its line count.
- **Diff Rich:** a changed marker line marks its spacer or aligned block as a changed block (DFV-FR-19).
- **Comments on text:** a fragment across a break or a marker keeps its source range with the marker lines included (CMT-FR-06).
- **Accessibility:** a spacer has the accessible name "Vertical break V2" (level as set). A chip has the accessible name "HTML comment".

## Expected deliverables

- Rich editor formatting panel is extended with new buttons for alignment and vertical breaks.
- Rich editor renders breaks as spacers, applies alignment markers, and shows other comments as chips.
- Specifications updated: `EDT-editor.md` (the heading group in the toolbar and wireframes, and EDT-FR-68, which says an HTML comment is never rendered), `CMT-comments.md` (toolbar wireframe), `DTV-document-text-viewer.md` (DTV-FR-SSQI), and `DFV-diff-viewer.md` (Rich marking).
- Documentation and any skill that describes the supported Markdown list the new markers.