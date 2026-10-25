## Intent

Markdown files now support visualization of mermaid diagrams. It would be a great user experience to provide a way to edit mermaid code blocks and view the edited diagrams at the same time in a dedicated overlay.

## User journey

- User is working on draft or any other markdown artifact.
- User sees a markdown diagram as a part of artifact.
- User clicks on an “edit" button in the top right corner of the mermaid visualization, next to the source/diagram toggle.
- A large modal overlay opens, split in two halves: the left pane is the editable Mermaid source, the right pane is the live rendering of the diagram. The caret is in the source pane.
- The right pane re-renders while the user types. When the source is invalid, it keeps the last valid diagram dimmed with the error message under it. When no valid diagram exists yet (empty source, or invalid when the overlay opened), it shows the statement "No diagram to show" and the error message when there is one.
- The user can resize the two panes with a divider. The overlay always opens with a 50/50 split; the ratio is not remembered.
- The overlay opens centered in the window at 90% of its width and 95% of its height. The user can move and resize the overlay. It reopens at 90% × 95% centered each time.
- The user can undo and redo inside the overlay and in the document. Both use the one undo history of the file.
- The user closes the overlay with the Close button or Escape. A click on the scrim does nothing. Closing discards nothing. Focus returns to the diagram fence that opened the overlay, and the user continues work on the document.

## Requirements

- The same visualization of the mermaid diagrams is expected to be reused. 
- Regular save rules apply: markdown should be saved dynamically including when the editor is open.
- **Edit button.** It is an icon button with the accessible name and tooltip "Edit diagram", reachable and operable by keyboard. It sits in the top-right corner of the diagram fence, next to the view toggle. It shows on every editable diagram fence in both views, including a fence with empty source, a fence with a diagram error, and a fence shown as a Find peek. In those states the view toggle is absent (EDT-FR-BQIU), and the Edit button takes the corner alone. It does not show on a read-only fence (an original block in the Diff tab, a Documents reader).
- **Surfaces.** The Edit button shows on every editable rich surface: the Editor tab, the New Artifact draft, the Diff Rich target, and the Settings Draft template. It does not show in the review-modal candidate, because an overlay cannot open over another.
- **Overlay scope.** The overlay is modal over the window that owns the surface, drawn inside that window with a scrim (like About). While it is open, that window accepts no interaction outside it. In the main window it is a floating overlay under SNV-FR-56: opening it closes any other overlay, and opening another overlay closes it. In the Settings window, SNV-FR-56 does not apply and the overlay covers that window. Closing by any of these ways discards nothing. The overlay also closes when its window is replaced.
- **Modal behaviour.** It has the role of a modal dialog with an accessible name (for example "Edit Mermaid diagram"). Focus moves into the source pane on open and stays inside while open. The divider can be operated by keyboard. Each pane has an accessible name.
- **Source pane.** It is a plain-text editor in the Source code role. It edits the fence's source in the same buffer as the document. An edit is an ordinary edit: it changes the buffer, marks the artifact dirty, and is one step of the single undo history (EDT-FR-MIQE, EDT-FR-22). The fence behind the scrim shows the new diagram as the user types.
- **Rendering.** The right pane reuses the same Mermaid render and the same error messages as the document fence (EDT-FR-XTEB, EDT-FR-XKMP, EDT-FR-VIOP). A diagram error never blocks an edit, the write schedule, or a save. Opening, moving, resizing, and closing the overlay are view state only: they leave the buffer unchanged, do not mark the artifact dirty, and add no undo step (EDT-FR-CQBG).
- **Save.** The overlay adds no Save control. The surface's own write schedule applies while the overlay is open (EDT-FR-70, NAW-FR-13, SET-FR-17).
- **Edge cases.** If an undo, an external change, or a rollback removes the fence while the overlay is open, the overlay closes and focus goes to the surface. The overlay holds no state after it closes.
- **Specifications this work changes:** `specifications/ui/EDT-editor.md` (new requirements for the Edit button and the overlay; relation to EDT-FR-USWS, EDT-FR-BQIU, EDT-FR-CQBG), `specifications/ui/SNV-shell-navigation.md` (SNV-FR-56 list of floating overlays), `specifications/ui/DFV-diff-viewer.md` (DFV-FR-EGFA), `specifications/ui/NAW-new-artifact.md` (NAW-FR-11), and `specifications/ui/SET-project-settings.md` (SET-FR-16).