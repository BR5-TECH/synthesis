we need to adjust our recent mermaid functionality.

# Change 1.

**Intent:** the author can see a diagram in a bigger window.

**Users and flow:** the author reads a Markdown file with a Mermaid fence on an editable rich surface. The author activates a third icon button on the fence. A viewer overlay opens with the diagram, scaled to fit. The author zooms and pans, then closes the overlay.

**Requirements:**

- The third button sits in the top-right corner of the fence, next to the view toggle and the Edit button. It has an icon, no text, and the keyboard can reach and operate it. Its accessible name and tooltip are "View diagram".
- The button shows only on a fence whose source renders as a valid diagram, in both views of the fence, and only on editable surfaces: the Editor tab, the New Artifact draft, the editable target of Diff Rich, and the Draft template of Project settings. It does not show on inert fences (Diff original side), on fences with empty source or a diagram error, or in the review modal.
- The **diagram viewer overlay** follows the rules of the diagram editor overlay: modal with a scrim, centred at 90% × 95%, moved by its title bar, resized from its edges and corners, no remembered position or size, a visible Close control and Escape to close, focus into the overlay on open and back to the opening button on close, role of a modal dialog with the accessible name "View Mermaid diagram", and a floating overlay under SNV-FR-56. Only the content differs.
- The overlay shows the diagram only: no source pane, no hints, no edit. It uses the same Mermaid render as the fence (bundled, offline, strict security level, active theme).
- The diagram fits the window on open. The author zooms with the mouse wheel and the keys `+` and `-`, pans by drag and with the arrow keys, and returns to fit with the key `0` or a visible Fit control. Zoom and pan are view state and reset at each opening.
- The viewer renders again when the source changes while it is open (undo, external reload, edit). It closes, and focus goes to the surface, when the fence is removed or is no longer a diagram fence, when the source stops being a valid diagram, or when its surface stops accepting input (as EDT-FR-SHYX and EDT-FR-WPGE do for the editor overlay).
- Opening, using, and closing the viewer is view state: no change to the buffer, no dirty mark, no undo step.
- `frontend-design` skill must be used for UI development.

# Change 2.

**Intent:** the author sees, while editing, how to write the elements of the chart type in use, such as `-->`.

**Requirements:**

- The diagram editor overlay (side-by-side) has a **hints area** across the bottom 20% of the whole overlay, under both panes. The panes and their divider use the top 80%. The area follows the overlay when the author resizes it, and it scrolls when its content is longer than the area.
- The area is a read-only reference. It shows a bundled static list of syntax snippets for the chart type, each with a short explanation. It has no insert, no click action, and no copy button. Its text can be selected. Its accessible name is "Mermaid syntax hints".
- The chart type comes from the first non-empty, non-comment line of the source (`flowchart`, `graph`, `sequenceDiagram`, and so on; `%%` lines and frontmatter are skipped). The hints update while the author types. When the type is unknown or the source is empty, the area shows general hints and a flowchart set.
- A hint set exists for every chart type that the bundled Mermaid version supports. The list of types comes from that version, and a test fails when a supported type has no hint set. The 10 most important types (for example flowchart, sequence, class, state, ER, gantt, pie, mindmap, timeline, journey) get rich sets: node and edge forms, labels, grouping, styling, comments, and a short complete example. The other types get at least their declaration line, their main element forms, and a short example.
- Hints are English text in the bundled application, with no network read. A snippet is code text in the Source code role.
- Viewing hints is view state and has no effect on the buffer.
- `frontend-design` skill must be used for UI development.

# Specifications to change.

- `specifications/ui/EDT-editor.md`: add the View button and the diagram viewer overlay; update EDT-FR-ADNU, EDT-FR-QTHL, EDT-FR-MOQW (third button), EDT-FR-NKGX and EDT-FR-ULPU (hints area takes the bottom 20%; panes use the top 80%), EDT-FR-HVGG (name of the hints area), and EDT-FR-TOVY (viewer is a floating overlay).
- `specifications/ui/SNV-shell-navigation.md`: add the diagram viewer overlay to the floating overlays of SNV-FR-56.
- `specifications/ui/DFV-diff-viewer.md`: DFV-FR-EGFA, the editable target has the View button; the original side has none.
- `specifications/ui/SET-project-settings.md` SET-FR-16 and `specifications/ui/NAW-new-artifact.md` NAW-FR-11: name the View button and the viewer overlay where they list the Edit button and the editor overlay.
- `specifications/ui/PCR-prompt-change-review.md`: state that the review modal candidate has no View button.