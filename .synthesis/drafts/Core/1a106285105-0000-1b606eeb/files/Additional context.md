## Intent

Users need project-specific reference documents available to themselves and to conversational AI agents, including files outside the project checkout. Add a **Documents** vertical panel for managing references, read-only in-app viewers, and agent tools for finding and reading those documents. Persist only the selected filesystem references per repository, outside Git and shared across its worktrees; do not copy, edit, or delete the source documents.

## User journey

### Managing documents

- The user selects **Documents** from the vertical-panel activity bar. Its toggle sits immediately after **Project**. The panel shows the supported documents and their containing folders as a tree; folders expand and collapse.
- **Add documents** opens an in-app choice of **Files** or **Folder**, then the matching OS-native picker. The Files picker allows multiple selections; the Folder picker selects one folder. A selected folder is searched recursively, including its subfolders.
- The panel shows each supported filesystem path once, even when a file is covered by more than one selected source. Removing a source removes only that saved reference, never the filesystem object; a file remains in the collection while any selected source still includes it.
- The selected paths persist per repository across launches and worktree changes, in storage outside the repository. Keep each exact selected path: a path inside the project remains tied to the selected worktree path, and an external document has an absolute path. Keep a selected reference and show an unavailable state if its path is moved, deleted, or unreadable.
- A text filter narrows the displayed names. Its text and the folders' expanded or collapsed state last only for the current application session.
- Opening a Markdown or text document opens a separate, viewer-only tab, not the normal editable Editor tab. Markdown uses the rich view by default and can switch to source; `.txt` uses the source view. Neither viewer offers editing or Editor formatting controls. A PDF opens in its own in-app PDF Viewer tab.
- Reopening a document focuses its existing viewer tab; it does not open a duplicate. If the document leaves the Documents collection after its last selected source is removed, close its viewer tab automatically. Do not delete the file.

### Using documents

- During a conversation with an AI agent, the agent can call **Search for Documents** with a query and result limit. Results include a stable document ID that the agent can pass to **Get Document**.
- **Get Document** reads only a document in the Documents collection. It returns the whole document by default, or a byte range or a line range. The two range forms are mutually exclusive. For PDFs, both indexing and retrieval use the extracted text, not the PDF bytes.
- These tools can read selected document paths through the Documents-specific read-only access grant. They do not give agents' normal filesystem tools access to external paths.

## Requirements

- Support only `.pdf`, `.txt`, `.md`, and `.markdown` files, treating extensions case-insensitively. Ignore all other file types. The rules apply to direct file selections and recursive folder contents.
- Assign each supported document a stable deterministic ID based on its normalized filesystem path. Resolve the ID through the saved Documents collection to the exact selected path; never let a tool compose or submit an arbitrary path.
- Persist the selected file and folder references per repository across worktrees and launches, outside the repository and Git. Persist references only, not document contents. Removing a reference leaves the filesystem object unchanged.
- Deduplicate files reached through overlapping sources. Keep a file available while any selected source includes it; it leaves the collection only when no source includes it.
- Build a separate **documents** BM25 index alongside the existing indexes in `specifications/core/BMI-bm25-indexing.md`. Build it when a project opens, and update it when supported files are added, removed, or changed under selected sources. Keep unavailable or unreadable references visible with status, but do not return inaccessible content as a search result.
- Use the Rust `pdf-extract` crate (`pdf_extract`) to extract embedded text from PDF bytes read through the Documents-specific read-only filesystem grant; do not use OCR. Use Mozilla PDF.js (`pdfjs-dist`) in the in-app PDF Viewer to render pages from PDF bytes supplied by the backend. The viewer provides page navigation and zoom, plus text search within the open PDF, with matches highlighted and controls to move between matches. Users can select and copy PDF text through the rendered text layer. Bundle the PDF.js worker with the application; do not load viewer code from a remote CDN, and do not give the frontend or agents' normal filesystem tools direct access to selected paths.
- Cache each PDF's extracted text for the application session and reuse it until that PDF changes. On a PDF content change, extract again, replace its indexed text, and use the new text in open viewers. Apply the same content-change indexing and viewer-refresh behavior to Markdown and text files. Show an unavailable state in an open viewer if its document becomes unreadable or is removed from the collection.
- `Search for Documents` takes a query and limit, searches only the Documents index, and returns ranked results with each document's ID and enough identifying text for the agent to choose a result. `Get Document` accepts that ID and returns the document text, or a requested range. It must not expose original PDF bytes: PDF ranges apply to the cached extracted text's UTF-8 encoding or lines. If a byte-range edge falls inside a UTF-8 character, expand the range outward to include that character. Reject calls that supply both range forms. Follow the existing tool conventions for descriptions, argument validation, outputs, refusals, and logging.
- External paths are readable only through the Documents feature's read-only filesystem capability, and only while selected in the Documents collection. Adjust the filesystem access helper to enforce that grant without widening the filesystem access of agents' normal tools. Keep all reads within the selected files and selected folders; preserve the project's existing symbolic-link safety rules.
- Use the existing vertical-panel toggle, layout, filtering, empty-state, accessibility, and theme conventions. Keep filter text and folder expansion session-only. Make all panel actions and viewer controls keyboard accessible and give unavailable, loading, empty, and filter-no-match states clear text.
- Add or update tests for extension matching, recursive discovery, overlapping sources and removal, project/worktree persistence, selected-path access boundaries, file and PDF content changes, range behavior, viewer refresh and closure, duplicate-tab focus, PDF page navigation and zoom, in-document search and match navigation, and text selection and copying.

### Project specifications to change

Create specifications for the Documents collection and its backend lifecycle, the Documents panel, the read-only Markdown/text viewer, the PDF Viewer, and the `Search for Documents` and `Get Document` tools. Update these existing specifications to keep the new feature consistent with current contracts:

- `specifications/core/FSA-filesystem-access.md` — narrowly scoped read-only access to user-selected Documents paths.
- `specifications/core/BMI-bm25-indexing.md` — add the Documents index and its build/update lifecycle.
- `specifications/core/GSS-global-settings-storage.md` — persist per-repository document references outside Git and across worktrees.
- `specifications/ai/CVL-conversation-loop.md` — expose both tools to conversational agent turns.
- `specifications/ui/SNV-shell-navigation.md` — add the Documents vertical-panel toggle immediately after Project.
- `specifications/ui/OVW-overview.md` and `specifications/ui/TAB-tabs.md` — register the new panel and viewer tab kinds, their identities, and their lifecycle, including automatic viewer closure when a document leaves the collection.
