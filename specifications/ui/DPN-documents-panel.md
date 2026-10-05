# Documents panel

**Spec code:** `DPN`

## Intent
The vertical-panel surface where a user manages the project's **reference documents**: PDF, Markdown, and text files that the user selects, wherever they live on the machine, so that the user and conversational AI agents can read them. The panel lists the supported documents and their containing folders as a tree, lets the user add files or a folder, shows every selected source with its status, and removes a source without touching the file or folder. A document opens in a read-only viewer tab (`DTV-document-text-viewer.md`, `PDV-pdf-viewer.md`). The panel keeps no selection of its own on disk: the collection of `../core/DCL-documents-collection.md` is the single owner of what is selected. Out of scope: editing, renaming, moving, or deleting a document or its folder; copying a document into the project; OCR; and any way to read a document that is not in the collection.

## Functional requirements
1. **DPN-FR-KYSK** Documents is a vertical-panel surface. Its activity-bar toggle sits in the leading cluster immediately after Project (per `SNV-shell-navigation.md` SNV-FR-44), carries the tooltip **Documents** (SNV-FR-49), and shows the panel on activation on the terms of SNV-FR-45. Documents is not the surface selected when a project opens (per `OVW-overview.md` OVW-FR-05).
2. **DPN-FR-VPRV** The panel renders, from top to bottom: a header with the panel's name and the **Add documents** button; the text filter directly beneath the header (per SNV-FR-58); the **document tree**; and the **Selected sources** section. The header, the filter, and the section heading stay visible while the tree scrolls.
3. **DPN-FR-NHDZ** The document tree shows every document of the collection and the folders that contain them, and shows each filesystem path **once**. A chain of folders that holds one folder and no document renders as one row labelled with the joined names. Folders sort before documents, and each group sorts by name, ignoring case.
4. **DPN-FR-MICG** A document row shows an icon for its format, its file name in the case it has on disk (per SNV-FR-57), and the text **Unavailable** when the document is unavailable. Its tooltip discloses the full path. A row never shows a path that the collection did not report.
5. **DPN-FR-UGVG** A folder row expands and collapses on click, on Enter or Space, and on the Right and Left arrow keys. It carries `aria-expanded`. The tree is one keyboard stop that moves between rows with the Up and Down arrow keys, and it exposes the tree roles to assistive technology.
6. **DPN-FR-TELU** The filter narrows the tree to the rows whose name contains the typed text, ignoring case. A folder row stays when its own name matches or when a row below it matches, and a matching folder shows everything below it. While the filter holds text, the folders that contain a match render expanded.
7. **DPN-FR-AREM** The filter text and the expanded or collapsed state of each folder last for the application session. They survive a switch to another panel, a change of the active tab, and a change of the active worktree. They are never written to disk. A folder that has not been toggled starts expanded.
8. **DPN-FR-ZMBQ** **Add documents** opens an in-app choice with two entries, **Files** and **Folder**, as a floating overlay that follows the rules of SNV-FR-56. The entries are reachable with the Up and Down arrow keys and Enter, and Escape closes the choice with no change.
9. **DPN-FR-FDVO** Choosing **Files** calls `"pick document sources (mode)"` with mode `files`, and choosing **Folder** calls it with mode `folder`. The backend opens the OS-native picker: the Files picker allows several files, and the Folder picker selects one folder. The panel passes no path to the backend and reads no path from the picker.
10. **DPN-FR-BADJ** While the picker call is pending, **Add documents** is disabled and exposes its busy state. A cancelled picker changes nothing and shows nothing. A result with `ignored_count` above zero shows one status line, announced politely, that says how many selected files were ignored because their type is not supported.
11. **DPN-FR-FZFL** The **Selected sources** section lists every source in stored order. A row shows a **File** or **Folder** label, the path as selected, and for an unavailable source the text **Unavailable** followed by the reason: *not found*, *cannot be read*, or *symbolic link*. A source row never depends on colour alone to show its state.
12. **DPN-FR-RTLG** Each source row carries a **Remove** button named for its path. Activating it calls `"remove document source (path)"` and renders the returned snapshot. The button removes the reference only, and the panel asks for no confirmation because no file or folder is deleted. Focus moves to the next source row, or to **Add documents** when none is left.
13. **DPN-FR-ZHEJ** A document that another source still includes stays in the tree after a removal. A document that the returned snapshot no longer holds is reported to the shell, which closes the viewer tabs of those documents (per `TAB-tabs.md` TAB-FR-KUIN). Removal is the only action of this panel that closes a viewer tab.
14. **DPN-FR-FAOR** While the first `"list documents"` call is pending, the panel shows the text **Loading documents…** in place of the tree. When that call fails, the panel shows **Documents could not be loaded.** with a **Retry** button, and logs the failure. The panel then follows `"documents changed"` and replaces its list from the payload without calling again.
15. **DPN-FR-UKXW** A collection with no source and no document is the first-class empty state (per SNV-FR-60): the line **No documents**, one sentence saying that documents are reference files that stay where they are on disk, and one **Add documents** action. The filter and the Selected sources section are not rendered beside it.
16. **DPN-FR-EPCH** A collection with sources but no document renders, in the tree region, the text **No supported documents in the selected sources.** and keeps the Selected sources section. A filter that matches nothing renders **No documents match** followed by the typed text in the tree region, with the filter still present and still holding its text (per SNV-FR-61).
17. **DPN-FR-CDFO** Activating a document row, by click or by Enter, opens the document's viewer tab: a `markdown` or `text` document in the Document tab kind and a `pdf` document in the PDF Viewer tab kind (per `TAB-tabs.md` TAB-FR-LKCT). An unavailable document opens its tab on the same terms, and the tab shows its unavailable state.
18. **DPN-FR-KEBH** Every action of the panel is reachable and operable with the keyboard alone: the filter, **Add documents** and its choice, the tree, each row, and each **Remove** button. Every control has an accessible name, shows a visible focus indicator, and uses the colours and type roles of the current theme in light and dark.
19. **DPN-FR-NQPS** The panel logs through `../core/LGC-logging.md` under the `frontend` domain: an `INFO` record when a source is added or removed with the source kind and counts, a `WARN` record when a pick ignores selected files, and an `ERROR` record for a failed call. No record carries a path, a file name, or the filter text.

## Wireframes
```
Documents panel
┌────────────────────────────────────┐
│ DOCUMENTS            [+ Add documents]│
│ ┌────────────────────────────────┐ │
│ │ filter…                        │ │
│ └────────────────────────────────┘ │
│ ▾ Users/me/reference               │
│    ▸ specs                         │
│      ▤ api-guide.pdf               │
│      ▤ notes.md                    │
│ ▾ docs                             │
│      ▤ onboarding.markdown         │
│      ▤ old.txt        Unavailable  │
│ ──────────────────────────────────│
│ SELECTED SOURCES                   │
│  Folder  /Users/me/reference    [×]│
│  File    /work/docs/old.txt     [×]│
│          Unavailable — not found   │
└────────────────────────────────────┘

Add documents choice
        ┌─────────────┐
        │ Files…      │
        │ Folder…     │
        └─────────────┘
```
- Layout notes: the header, the filter, and the Selected sources heading are pinned and the tree scrolls between them. A long path in a source row truncates in the middle and discloses the whole path in its tooltip. The glyphs shown are illustrative; the contract is that a format, a folder, and an unavailable state each carry an icon or text of their own.

## UI contract boundary
- **Owned by the UI**: the tree and its compaction, ordering, and keyboard model; the filter and its matching; the session-only filter text and expansion state; the Add documents choice and its overlay behaviour; the Selected sources rows and their Remove buttons; the loading, error, empty, and no-match states; the ignored-files status line; and the report of removed document ids to the shell.
- **Delegated to backend (abstract)**: `"list documents"`, `"pick document sources (mode)"`, and `"remove document source (path)"` — owned by `../core/DCL-documents-collection.md`, which also owns the stored sources, the discovery of documents, the status of each source and document, and the `"documents changed"` event the panel follows. Opening a viewer tab uses the routes of `TAB-tabs.md` and introduces no operation of its own.

## Non-functional requirements
- The panel renders a collection of a few thousand documents without a visible stall, because the tree renders only the rows of expanded folders.
- Nothing in the panel requires network access.
- Nothing about the panel's own state is recorded outside the window.
