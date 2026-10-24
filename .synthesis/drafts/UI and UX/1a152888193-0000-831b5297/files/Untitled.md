## Intent

Rich editor we use for artifacts must also support rendering of inline mermaid diagrams when they are present in the markdown file.

## User journey

- User opens markdown file in editor
- IF: Markdown file has a ```mermaid code block THEN: on the WYSIWYG surface it is rendered as a diagram by default.
- User activates the small toggle button in the top-right corner of the diagram (or of its code block) to switch between the rendered diagram and the editable source. User can also click the diagram to edit its source in place.
- When the source changes, the diagram re-renders. The edit is a normal edit: it marks the file dirty and joins the single undo history.
- IF: The mermaid syntax is invalid THEN: an inline error message shows, and the source stays visible and editable.
- User switches the file to raw-text mode: the fence shows as plain Markdown text, with no diagram.

## Requirements

- Diagrams render on every surface that reuses the Editor's rich Markdown surface: Editor tab, New Artifact draft, Diff **Rich**, review modal, and Settings draft template. The read-only Documents viewer and source files (non-`.md`) are out of scope.
- The Markdown on disk stays a plain ```mermaid fence. A diagram never changes the file bytes by itself: opening, rendering, toggling the view and saving without an edit leave the buffer byte-for-byte unchanged, and fence contents are literal in both directions (EDT-FR-66, EDT-FR-67, EDT-FR-68).
- The toggle state is view state only. It does not mark the file dirty, does not enter the undo history, and is not saved.
- In Diff **Rich**, a diagram in the original side is inert: it renders but accepts no input and shows no edit toggle. A diagram in the editable target keeps the marking of the changed block.
- Rendering uses the bundled mermaid library, works offline, and follows the active light or dark theme. Diagram source is untrusted: use mermaid's strict security level and never insert unsanitised HTML.
- Comment fragments, Find and Find & Replace keep working on the fence source (CMT-FR-06). A diagram error or render failure never blocks editing or saving the rest of the file.
- The diagram and its toggle are keyboard-operable, and the toggle has an accessible name that states the mode it switches to.
- `tiptap-extension-mermaid` should be used. If it does not serialise to a plain ```mermaid fence, wrap or patch it so it does. Do not replace it with a custom node view.

## Specifications to change

- `specifications/ui/EDT-editor.md` — add the diagram rendering, toggle, invalid-syntax state and round-trip rule for the WYSIWYG surface (near EDT-FR-67, EDT-FR-68).
- `specifications/ui/ESH-editor-source-files.md` — ESH-FR-RKHL says fenced-code rendering is untouched. Make an exception for `mermaid` fences.
- `specifications/ui/DFV-diff-viewer.md` — DFV-FR-47: diagrams in Rich, with the original side inert.
- `specifications/ui/NAW-new-artifact.md` (NAW-FR-11) and `specifications/ui/SET-project-settings.md` (SET-FR-16) — cross-reference the new behaviour; both reuse the surface whole.