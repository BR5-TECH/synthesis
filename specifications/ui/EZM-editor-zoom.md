# Editor zoom

**Spec code:** `EZM`

## Intent
Editor zoom makes the text of the tabs that show a document larger or smaller, so the author can read and edit at a comfortable size without a change to the typographic roles of Settings. One level applies to every such tab, in every project window, and the level stays after a restart. The author changes it with a trackpad or touchscreen pinch, with the wheel and Ctrl or Cmd, with the status bar control (`STB-status-bar.md` STB-FR-VRUS), and with the Zoom submenu of the Window menu (`SNV-shell-navigation.md` SNV-FR-LVDP). Out of scope: the zoom does not change the discussion columns, the panels, the chrome, or the status bar; PDF, Flow, and Map tabs keep their own canvas zoom; and nothing zooms the whole window.

## Functional requirements
1. **EZM-FR-SOZM** There is one **editor zoom level**. It applies to every zoom region (EZM-FR-QGOU) in every project window. Its default is 100%.
2. **EZM-FR-QGOU** A **zoom region** is one of these areas: the document column of an Editor tab, with its frontmatter, rich page, and source page; the document column of a New Artifact tab, live or past; a Diff tab's body; a Document tab's text; a draft preview tab's reading; and an OpenSpec change tab's reading pane.
3. **EZM-FR-UJWE** These areas are not zoom regions: the discussion column of an Editor tab and of a New Artifact tab, every toolbar and band above a region, the page width ruler below the document column (`EPW-editor-page-width.md` EPW-FR-ADPJ), the panels, the top chrome, the tab strip, the status bar, and the Conversation, PDF, Flow, Map, Dashboard, and Search results tabs.
4. **EZM-FR-JGRP** A tab is **zoomable** when it holds a zoom region: an Editor tab, a New Artifact tab, a Diff tab, also one whose comparison deletes the file (`DFV-diff-viewer.md` DFV-FR-15), a Document tab, a draft preview tab, or an OpenSpec change tab. Every other tab is not zoomable.
5. **EZM-FR-LRVS** Inside a zoom region, the zoom level multiplies the size of the Rich Markdown and Source code text (`OVW-overview.md` OVW-FR-13) and the page width of the document page (`EPW-editor-page-width.md` EPW-FR-GNNZ). UI-role text, images, and diagrams keep their size. The line height of each role stays the same multiple of its size.
6. **EZM-FR-RWKC** A document page that the zoom makes wider than its tab is as wide as the tab, with the same inset that the page keeps at 100%. The page never makes the tab scroll horizontally.
7. **EZM-FR-COYK** The **zoom steps** are 50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, and 300%. The level is never below 50% or above 300%.
8. **EZM-FR-DMLT** **Zoom In** sets the level to the smallest step above it, and **Zoom Out** to the largest step below it. A level between two steps goes to the nearest step in that direction. **Actual Size** sets the level to 100%. Zoom In at 300% and Zoom Out at 50% do nothing.
9. **EZM-FR-GMSX** A trackpad pinch, and the wheel with Ctrl or Cmd held, over a zoom region change the level continuously, by the same factor as the gesture. The level stays between 50% and 300%. The level shows as a whole percent.
10. **EZM-FR-VQHI** A two-finger pinch on a touchscreen over a zoom region changes the level as a trackpad pinch does (EZM-FR-GMSX), on macOS, Windows, and Linux. One pinch never changes the level twice, also when the platform reports it both as a touch and as a gesture.
11. **EZM-FR-PZSN** A pinch that ends at a level between 97% and 103% sets the level to 100%.
12. **EZM-FR-BNFJ** A pinch or a Ctrl or Cmd wheel over a surface that has its own zoom changes only that surface (`EDT-editor.md` EDT-FR-MZIF). Such surfaces are the diagram area of the diagram viewer and the interactive canvas of an Architecture tab (`CAD-calm-architecture-diagrams.md` CAD-FR-MBML). The editor zoom level does not change.
13. **EZM-FR-COUH** A pinch or a Ctrl or Cmd wheel outside every zoom region does not change the level, and the application does not zoom the window.
14. **EZM-FR-OXBL** When the level changes, each zoom region keeps the same part of its document in view. For a pinch, the text under the pointer stays under the pointer. For every other change, the line at the top of the region stays at the top.
15. **EZM-FR-MXAB** A change of the level applies to every open zoom region at once. It does not remount a surface, and it does not change the buffer, the dirty state, the undo history, the selection, or the mode of a tab.
16. **EZM-FR-VPYL** The level is saved through "save app preferences" (`../core/GSS-global-settings-storage.md` GSS-FR-KKYO). A pinch saves once, when it ends. A step, a menu item, or the status bar control saves at once.
17. **EZM-FR-HYQD** At startup, each project window reads the level from "load app preferences". A stored level below 50% or above 300% reads as 100%. When another window saves a new level, every open window shows it with no reload.
18. **EZM-FR-CFWK** The webview's own zoom keys and gestures stay off. Nothing zooms the whole window or a surface outside the zoom regions.

## User stories
- As an author with a large monitor, I want to make the document text larger in one gesture, so that I can read a long specification from a distance.
- As an author who compares a diff and a draft, I want one zoom level for both tabs, so that the two documents read at the same size.
- As an author, I want the discussion column to keep its size when I zoom the document, so that the comment threads stay compact.

## Wireframes
```
┌──────────────────────────────────────────────────────────────────────────────────┐
│ Editor toolbar (not zoomed)                                                      │
├──────────────────────────────────────────────────────────┬───────────────────────┤
│                                                          │ Discussion column     │
│      ┌────────────────────────────────────────────┐      │ (not zoomed)          │
│      │ Zoom region: the document page             │      │                       │
│      │ Text and page width × zoom level           │      │                       │
│      └────────────────────────────────────────────┘      │                       │
├──────────────────────────────────────────────────────────┴───────────────────────┤
│ ⚙ ⚙                              LF ▾   Spaces: 2 ▾   125% ▾   +412 −87          │
└──────────────────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the zoom changes what is inside a region and does not move the borders of the region. A toolbar or band above a region keeps its height at every level.

## UI contract boundary
- **Owned by the UI**: the zoom regions and the zoomable tabs, the steps and the range, the effect of the level on text and page width, the gestures and their filter by surface, the reading position after a change, the reading of a stored level, and the synchronisation of the level between windows. The Zoom submenu belongs to `SNV-shell-navigation.md`, and the status bar control belongs to `STB-status-bar.md`.
- **Delegated to backend (abstract)**:
  - "load app preferences" and "save app preferences" — the record holds the editor zoom level (`../core/GSS-global-settings-storage.md` GSS-FR-KKYO).

## Non-functional requirements
- A pinch changes the size of the text in the next frame. A change of the level runs no layout of a surface outside the zoom regions.
- A level change from a gesture writes the preferences file at most once for each gesture.
- At every level, the source text and its highlight layer stay aligned, and a line-number gutter stays aligned with its lines.
