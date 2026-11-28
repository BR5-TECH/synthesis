# Editor page width

**Spec code:** `EPW`

## Intent
The page width is the largest width of the text on the document page (`EDT-editor.md` EDT-FR-63). One width does not suit every screen: on a wide laptop or monitor, a narrow page leaves most of the tab empty. The author sets the width with a small ruler at the bottom of the document column, as in a word processor, or in the Documents section of Settings. One width applies to every document page in every project window, and it stays after a restart. Out of scope: the ruler sets no margin, indent, or tab stop, and the width is not kept per file.

## Functional requirements
1. **EPW-FR-QGUZ** There is one **page width**: the largest width of the text on the document page at 100% editor zoom, in whole CSS pixels. Its default is 820. It is never below 560 or above 1800.
2. **EPW-FR-NSNH** The page width sets the largest width of every document page: the Editor tab's rich page and source page, the document page of a New Artifact tab, a Diff tab's rich page (`DFV-diff-viewer.md` DFV-FR-38), and every other surface that sets its content on the document page.
3. **EPW-FR-KCZU** The page width is a largest width only. A document column narrower than the page shows the page at the column width, with the same inset (`EDT-editor.md` EDT-FR-63). The page never makes the tab scroll horizontally.
4. **EPW-FR-GNNZ** The editor zoom level multiplies the page width (`EZM-editor-zoom.md` EZM-FR-LRVS). The stored page width is the width at 100%, so a change of the zoom level does not change the stored page width.
5. **EPW-FR-ADPJ** The Editor tab shows a **page width ruler** at the bottom of its document column, below the scroll area, in every editing mode and for a source file. A New Artifact tab shows it below its document page. The ruler is not a zoom region (`EZM-editor-zoom.md` EZM-FR-UJWE) and does not scroll with the page.
6. **EPW-FR-WOZO** The ruler is a thin strip across the document column with a scale of ticks. It shows the page as a light band and the field beside it as a recessed band. Two **handles** mark the two edges of the text on the page, and they move with the page when the zoom level or the column width changes.
7. **EPW-FR-AOVQ** A drag of either handle changes the page width, and the page shows each new width during the drag. Both edges move by the same distance. The new width is the width at the drag start plus two times the outward pointer movement divided by the zoom level, rounded to a whole pixel, from 560 to 1800.
7. **EPW-FR-IMWS** While the column caps the page (EPW-FR-KCZU), the handles stay at the capped edges of the text, and the drag label and the separator value show the stored page width. A drag outward on a capped page makes the stored width larger and never smaller.
8. **EPW-FR-TQFY** During a drag, the ruler shows the page width in pixels next to the handle that the author holds.
9. **EPW-FR-NEFQ** A double-click on a handle sets the page width to 820.
10. **EPW-FR-IUHL** Each handle is a focusable separator with the page width as its value, from 560 to 1800. On a focused handle, the arrow key that points outward adds 20 pixels and the other arrow key removes 20 pixels. With Shift, the step is 100 pixels. Home sets 560, and End sets 1800.
11. **EPW-FR-JNCM** A drag saves the page width once, when it ends, through "save app preferences" (`../core/GSS-global-settings-storage.md` GSS-FR-UYXH). A key or a double-click saves at once. A drag that ends at the width it started at saves nothing.
12. **EPW-FR-ZCBT** At startup, each window reads the page width from "load app preferences". When another window saves a new page width, every open window shows it with no reload.
13. **EPW-FR-XOMR** A change of the page width applies to every open document page at once. It does not remount a surface, and it does not change the buffer, the dirty state, the undo history, the selection, the scroll position of a page, or the mode of a tab.
14. **EPW-FR-LSZF** A save that fails keeps the width on screen, writes a warning to the log, and does not show an error to the author. The next start reads the stored width.

## User stories
- As an author with a 16-inch laptop, I want a wider page, so that a long table or code block does not wrap in the middle of a wide screen.
- As an author, I want to drag the page edge in the editor as in a word processor, so that I can see the width change while I set it.
- As an author, I want the width that I set to stay for every document and after a restart, so that I set it once.

## Wireframes
```
┌──────────────────────────────────────────────────────────┬───────────────────────┐
│ Editor toolbar                                           │ Discussion column     │
├──────────────────────────────────────────────────────────┤                       │
│      ┌────────────────────────────────────────────┐      │                       │
│      │ Document page                              │      │                       │
│      │   text ≤ page width × zoom level           │      │                       │
│      └────────────────────────────────────────────┘      │                       │
├──────────────────────────────────────────────────────────┤                       │
│ ░░░░░░▕▼╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷╵╷▼▏░░░░░░    │                       │
└──────────────────────────────────────────────────────────┴───────────────────────┘
```
- Layout notes: the ruler is about 16 pixels tall and spans the document column only. The ▼ marks are the handles, at the edges of the text. The ░ bands are the field beside the page. The action control (`ACT-action-control.md` ACT-FR-09) stands above the ruler and never covers it.

## UI contract boundary
- **Owned by the UI**: the ruler and its handles, the drag, the keyboard steps, the double-click reset, the width label during a drag, the range and default of the page width, the conversion between the handle distance and the stored width at each zoom level, the application of the width to every document page, and the synchronisation of the width between windows. The Settings field belongs to `GLS-global-settings.md` (GLS-FR-IXAQ).
- **Delegated to backend (abstract)**:
  - "load app preferences" and "save app preferences" — the record holds the page width (`../core/GSS-global-settings-storage.md` GSS-FR-UYXH).

## Non-functional requirements
- A drag changes the page in the next frame. A drag writes the preferences file at most once.
- The ruler adds no layout work to a surface outside the document column during a drag.
