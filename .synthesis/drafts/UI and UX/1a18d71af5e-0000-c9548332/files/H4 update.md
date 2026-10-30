## Intent

Difference between H3 and H4 is too big. we should increase the size of H4, since it’s way too small right now. it’s actually smaller than bold style of a regular body.  

## Requirements

- **Scope.** The new heading scale applies to every Rich Markdown surface (per `OVW-overview.md` OVW-FR-13): the Editor WYSIWYG surface, the rich rendering of a Diff tab, and the rich view of a Document tab. Raw-text and source surfaces, and UI-role text (panels, comment bodies, note bodies), do not change. All surfaces share one scale, so a heading has the same size on each of them.
- **Order.** On every Rich Markdown surface, the text size is strictly H1 > H2 > H3 > H4 > body bold text. H4 is clearly larger than body text, not only by a rounding step. H5 and H6 are not larger than H4 and not smaller than body text.
- **Size rule.** The builder picks the exact values. It first reads the current H1, H2, and H3 sizes and keeps them unless the rules here cannot hold. Then it sets H4 so that: (a) H4 is at least 1.1 times the body size; (b) the size ratio between H3 and H4 is not larger than the ratio between H2 and H3, so the steps look even; (c) the choice is written in the final text of this prompt's change so a reviewer can check it.
- **Weight.** H4 keeps its current bold weight. Only its size changes. Body bold text keeps its current weight and size. H4 must still differ from body bold text by size at every Rich Markdown size setting.
- **Relative units.** Heading sizes are relative to the Rich Markdown role size (per `GLS-global-settings.md` GLS-FR-17), not fixed pixel values. The order above holds over the whole allowed range of that size setting (GLS-FR-19), and a change of the setting re-scales all headings at once with no relaunch (per `OVW-overview.md` OVW-FR-14).
- **Line height.** A larger H4 must not clip or overlap the next line at any allowed line height setting.
- **Table of contents.** Entry sizes in the table of contents panel keep the rule of `EDT-editor.md` EDT-FR-EKCY: an entry of levels 1 to 4 stays smaller than its heading on the surface. Check this rule against the new H4 size and adjust the panel entry sizes if the rule breaks.
- **Presentation only.** The change touches no byte of the document, no dirty state, and no undo history.

## Specifications to change

- `specifications/ui/EDT-editor.md`: EDT-FR-62 (or a new requirement next to it) states the heading scale order and the H4 rule for the WYSIWYG surface. Check EDT-FR-EKCY.
- `specifications/ui/DFV-diff-viewer.md`: DFV-FR-17 and DFV-FR-38 point to the same scale for the rich rendering.
- `specifications/ui/DTV-document-text-viewer.md`: DTV-FR-SSQI points to the same scale for the rich view.