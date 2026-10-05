# Document text viewer

**Spec code:** `DTV`

## Intent
The read-only viewer tab for a Markdown or text document of the Documents collection. It opens from the Documents panel and shows the document as the user's reference material, not as an artifact to edit. A Markdown document shows in a rich view by default and can switch to its source. A `.txt` document shows its source. The viewer offers no editing and none of the Editor's formatting controls, and it never writes the file. It follows the collection: when the file changes the viewer shows the new text, and when the file becomes unreadable the viewer says so. Out of scope: editing, saving, commenting on, or discussing a document; PDF documents (`PDV-pdf-viewer.md`); the panel that lists documents (`DPN-documents-panel.md`); and search inside the document.

## Functional requirements
1. **DTV-FR-MICG** A `markdown` or `text` document opens in a **Document tab**, a viewer-only tab kind that is separate from the Editor tab. The Document tab is not the Editor, hosts no Editor session, and opens no Editor tab for the same file (per `TAB-tabs.md` TAB-FR-LKCT).
2. **DTV-FR-UGVG** A Document tab is identified by the document id. A request to open a document that already has a Document tab focuses that tab and opens no second one (per `TAB-tabs.md` TAB-FR-QXRF). The tab label is the file name, and its tooltip is the full path of the document.
3. **DTV-FR-VZNE** A `markdown` document opens in the **rich view**. A switch with the two choices **Rich** and **Source** lets the user change between the rich view and the source view. The switch is a radio group, is operable with the keyboard, and shows which choice is active.
4. **DTV-FR-CIWP** A `text` document opens in the source view and shows no switch. The source view shows the text of the file exactly as stored, in the Source code typographic role (per `OVW-overview.md` OVW-FR-13), with its line breaks and spaces kept, and it does not highlight, rewrap, or reformat the text.
5. **DTV-FR-SSQI** The rich view renders the Markdown read-only in the Rich Markdown typographic role (per OVW-FR-13), with headings, lists, tables, emphasis, code, quotes, and images shown as Markdown shows them. It never runs a script, never renders raw HTML as elements, and never loads a remote resource. A link renders as text and does not navigate.
6. **DTV-FR-THTI** The viewer offers no way to change the document. It has no editable surface, no formatting toolbar, no mode that accepts typing, no Save action, and no dirty indicator. While a Document tab is active, File → Save stays unavailable (per `SNV-shell-navigation.md` SNV-FR-28). The user can select text and copy it.
7. **DTV-FR-SQNK** The viewer reads the text with `"read document (id)"`. While that call is pending, it shows the text **Loading document…**. A failure other than unavailability shows **The document could not be read.** and logs the failure. The viewer holds the text only for display and writes nothing to disk.
8. **DTV-FR-SAIG** The viewer follows `"documents changed"`. When the revision of its document differs from the revision it shows, the viewer reads the new text and replaces the view. The choice between Rich and Source, and the scroll position as far as the new text allows, stay as they were.
9. **DTV-FR-BEQL** The viewer shows its **unavailable state** when its document's entry is `unavailable`, when the document is no longer in the collection, or when a read fails as unavailable or unknown. The state shows **This document is unavailable.** and one sentence that gives the possible causes. It replaces the content, so the viewer shows no stale content.
10. **DTV-FR-EKFD** An unavailable viewer that sees its document become available again, with a revision, reads the text and shows it. An unavailable viewer stays open until the user closes it or until the document leaves the collection through the removal of its last source (per `TAB-tabs.md` TAB-FR-KUIN).
11. **DTV-FR-HQTN** The text region is a focus stop that scrolls with the keyboard, and the switch is reachable with the Tab key. Every control has an accessible name. The view uses the colours of the current theme in light and dark, and it states loading and unavailable states in text.
12. **DTV-FR-XMRL** The choice between Rich and Source belongs to the tab and lasts while the tab is open. It is not written to disk. A new Document tab for the same document starts with the rich view again.
13. **DTV-FR-BPXG** The viewer logs through `../core/LGC-logging.md` under the `frontend` domain: an `INFO` record when a document finishes loading, with its format and byte length, and a `WARN` or `ERROR` record for every failed read. No record carries a path, a file name, or any part of the text.

## UI contract boundary
- **Owned by the UI**: the Document tab, its identity and label, the Rich and Source switch, the rendering of both views, the absence of editing, the loading, error, and unavailable states, and the reload that a changed revision causes.
- **Delegated to backend (abstract)**: `"read document (id)"` — owned by `../core/DCL-documents-collection.md`, which also owns the `"documents changed"` event the viewer follows and the status of each document. The viewer invokes nothing else and writes nothing.

## Non-functional requirements
- The viewer renders a document of a few hundred kilobytes without a visible stall.
- The viewer needs no network access, and the rich view loads no resource from the network.
- The viewer keeps the text of its own document only while its tab is open.
