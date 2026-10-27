## Intent

Two changes. Change 1 removes the old floating chat from Markdown files that are not artifacts. Change 2 adds a ninth artifact type, Document (`DOC`), that puts files and folders into the project's Documents collection. Users: people who edit Markdown files, discuss them, and keep reference documents in a project.

### Change 1.

When working on a mardkown file that’s not artifact - I’m able to start old chat with flying textbox in the middle of my screen. The expectation is that we use new discussions UI everywhere.

- Trigger: the **Discuss** action in the corner action control, on an untyped (non-artifact) `.md` file in WYSIWYG mode.
- Expected: the discussion opens in the **comment rail** beside the page, the same as for artifacts and drafts (`ACT-action-control.md` ACT-FR-19, `CMT-comments.md`). The tab shows no floating chat box, no floating discussion panel, and no discussions control.
- ACT-FR-19 already requires this. Find why the untyped-file route differs and fix the code to match. Change a specification only if the code shows a gap in it.
- Out of scope: raw-Markdown mode keeps its current floating surface (ACT-FR-20). Generic project-level chats are a separate topic.

### Change 2

New artifact type to be added: Document, shown as `DOC`. Once a file or folder has this type, it is automatically added to the project's Documents collection. It is project context and it is visible in the Documents tab. A folder with the type includes every supported file below it, through type inheritance.

- **Type.** The stored slug is `doc`. The UI shows it as DOC. It is a ninth type for the UI and the library. The user assigns it, to a file or to a folder. Nothing infers it from a path.
- **Where selectable.** Everywhere the eight types are: New Artifact window, New Folder window, the Artifact Type submenu of the Project context menu, the tree type lens, and the tree tag.
- **Collection.** A DOC-typed file or folder is a source of the Documents collection. Only supported formats (`.pdf`, `.txt`, `.md`, `.markdown`) become documents. A DOC-typed file of another extension is ignored by the collection, and the tree still shows its DOC tag. The source comes from the type. It is not saved in the per-repository sources store, and clearing the type removes it.
- **Opening.** A DOC file inside the project path opens in an Editor tab: editable and discussable like any other Markdown artifact. This holds when it is opened from the Documents tab too. A document outside the project path opens in the read-only viewer, as today. A PDF always opens in the PDF viewer.
- **Removal.** Removing a DOC-typed item from the Documents tab removes it from the collection and clears its DOC type. The file or folder stays on disk.
- **Agents.** DOC is not reported to agents as an artifact type. The agent-facing list stays at eight types, and a DOC file reports no artifact type. Agents reach DOC files only through the Documents tools (Search for Documents, Get Document). Change proposal should work when user is disucssing the document with agent.
- **Specifications to change.** `core/ASC-artifact-scanning.md` (type assignment and the DOC-only handling), `core/DCL-documents-collection.md` (sources derived from DOC type), `ui/DPN-documents-panel.md` (listing and removal behaviour), `ui/TAB-tabs.md` and `ui/DTV-document-text-viewer.md` (in-project DOC files open in the Editor, not the viewer), `ui/LCM-library-context-menu.md`, `ui/LIB-library.md`, `ui/NFW-new-folder.md`, and `ui/NTA-new-typed-artifact.md` (the "exactly eight types" lists, the lens, and the tag).
- **Open for the implementer.** DOC assignments live in the project library file, but the manual sources are per repository. State in the specifications how a worktree change affects DOC-derived sources.

## Requirements

- Look and feel must be aligned across the application.
- DOC uses the same tag, colour, and menu treatment as the other types.
- A DOC change shows in the Documents tab and the tree without a manual refresh.
- Removing a source never deletes or edits the file on disk.