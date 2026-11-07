# Document export

**Spec code:** `DXP`

## Intent
The backend owner of the **export of a DOC artifact** to a `.pdf` or a `.docx` file. An author who writes a document in the IDE must give it to people who do not use the IDE, so the backend converts the saved Markdown file to a portable format with the same visual result the rich editor shows: diagrams as images, vertical breaks as space, aligned blocks as aligned, and an optional metadata table from the frontmatter. This module also owns the **export requests** that the agent tool `../tools/RDX-request-document-export-tool.md` records, and their outcome. Out of scope: the Export dialog and the request card (`../ui/EXD-export-dialog.md`); the save dialog itself (`FSA-filesystem-access.md` `browse_for_save_path`); and how the rich editor reads the markers (`../ui/EDT-editor.md`).

## Functional requirements
1. **DXP-FR-IEQA** `export_document(path, format, includeFrontmatter, destination, exportId)` converts the Markdown file at the project-relative `path` of the active worktree to `format`, `pdf` or `docx`, and writes the result to the absolute `destination`. It reads the file from disk through the IDE instance of `FSA-filesystem-access.md`. It never reads an editor buffer.
2. **DXP-FR-LNCQ** The source of an export is a **DOC artifact**: a file of the active worktree whose resolved artifact type is `doc` (per `ASC-artifact-scanning.md` ASC-FR-06) and whose name ends in `.md`, case ignored. A missing file is the typed error `not_found`, a file of another type `not_doc_artifact`, another extension `not_markdown`, and a path that leaves the worktree `outside_project`. A file that exists and does not read is `read_failed`.
3. **DXP-FR-XRAI** The export writes the destination through `write_bytes_at_user_choice` of `FSA-filesystem-access.md` (FSA-FR-28), in one atomic write that replaces an existing file. A destination that is not absolute is the typed error `invalid_destination`. A write that fails is the typed error `write_failed`, and leaves no partial file.
4. **DXP-FR-WLGE** The conversion runs on a worker thread and never on the thread that serves the window. The command returns when the file is written or the export fails. Two exports with different `exportId` values run independently.
5. **DXP-FR-LSYA** The export emits `"document export progress"` with `{ exportId, stage, done, total }`. The stages are `reading`, `diagrams`, `converting`, and `writing`, in that order, with at least one event for each. In the `diagrams` stage `done` counts the diagrams processed and `total` the diagrams of the document. In the other stages both are 0.
6. **DXP-FR-RKJC** A diagram fence (per `../ui/EDT-editor.md` EDT-FR-MOYS) is rendered to a PNG image with the `merman` crate, in the backend and with no webview. The image replaces the fence in the output, at its natural size scaled down to the text width where it is wider.
7. **DXP-FR-IOFM** A diagram that does not render, that renders to nothing, or whose renderer panics, stays in the output as a code block with its source. The export continues, and its result carries one warning `{ kind: "diagram_failed", index }` for that diagram, `index` counted from 1 in document order.
8. **DXP-FR-QCEB** The render of one diagram has a deadline of 20 seconds. A render that does not end before its deadline stops, and its diagram is a diagram that does not render (DXP-FR-IOFM).
    - *Why:* a diagram that never ends its render would otherwise hold the export, and the dialog that waits for it, for ever.
9. **DXP-FR-YVIV** A vertical break at the top level (per `../ui/EDT-editor.md` EDT-FR-QALB, EDT-FR-ANTD) becomes empty vertical space of 12, 8, 5, or 3 body line heights for V1, V2, V3, and V4 (per `../ui/EDT-editor.md` EDT-FR-CSKH).
10. **DXP-FR-ERZY** An alignment marker at the top level (per `../ui/EDT-editor.md` EDT-FR-XPMC) centres or right-aligns the top-level block on the next line. A paragraph, heading, list, or quote aligns its text. An image or a diagram moves with its block (per `../ui/EDT-editor.md` EDT-FR-GVJM). In the `pdf` format the export breaks the text of an aligned paragraph, heading, list item (also a nested one), or quote into lines itself, and moves each line to the centre or to the right edge with no-break spaces. The padding of every line stays, also when the line starts with bold, italic, code, or link text. The text of a list item is measured after its marker: the width of the bullet, the number, or the task box, plus the gap of the list style.
11. **DXP-FR-ANXU** Every other HTML comment is absent from the output, at the top level and inside a paragraph. A comment inside a code span or a code block stays code text.
12. **DXP-FR-SRHN** A comment that continues on more lines, inside a paragraph, is absent from the output as a whole. A comment ends at the latest at the end of the paragraph, heading, or table cell that holds it, so the blocks after it stay in the output.
13. **DXP-FR-BXIO** An `<!--` with no `-->` after it opens no comment. The text from it onward stays in the output as text.
14. **DXP-FR-DJZM** A line that holds a comment and then other text exports that text as Markdown, as if the comment were absent. Text that is a paragraph in the editor is a paragraph in the output.
15. **DXP-FR-RPPW** With `includeFrontmatter` true, a leading YAML frontmatter block (per `../ui/EDT-editor.md` EDT-FR-18) renders before the body as a two-column **metadata table** with no header row: one row per top-level key in file order, the key in bold and the value in normal weight. A list or mapping value shows as its YAML text, with its line breaks. In the `pdf` format a value of more than one line is one table row per line, the key shows in the first row of the value, and the key cell of the next rows is empty. In the `docx` format the value stays in one cell, one paragraph per line.
16. **DXP-FR-WAQF** With `includeFrontmatter` false, the frontmatter block is absent from the output. A file with no frontmatter gives no table either way. A frontmatter block that is not a YAML mapping renders as one code block of its text when included.
17. **DXP-FR-YRVE** The output keeps headings, paragraphs, emphasis, strong, strikethrough, inline code, links, ordered, bullet, nested, and task lists, block quotes, tables, code blocks, thematic breaks, and hard breaks. Other raw HTML shows as its text.
18. **DXP-FR-IJOH** A table of the body has its cell borders in both formats. An aligned block changes the look of no other block.
19. **DXP-FR-JYHX** An image with a relative path is read from the project, relative to the file, through the IDE instance. An image outside the project, a remote image, or one that does not decode, shows its alt text. The export makes no network request.
20. **DXP-FR-CPPT** A `docx` export is written with the `docx-rs` crate and a `pdf` export with the `markdown2pdf` crate.
21. **DXP-FR-FNTP** The `pdf` export is set in font files that ship with the app in `src-tauri/fonts/`, never in a host font. Body text uses Liberation Sans in its regular, bold, italic, and bold-italic faces. Code uses Liberation Mono in its regular and bold faces. The widths for the lines of aligned blocks (DXP-FR-ERZY) come from the same files.
    - *Why:* Liberation Sans has the widths of Arial and Liberation Mono those of Courier New, so the `pdf` and the `docx` export break lines at the same places.
22. **DXP-FR-OSRE** The `docx` export names Arial as the document default font for every script and Courier New for inline code and code blocks. The export embeds no font file. It reads no font setting of the user (per `../ui/GLS-global-settings.md` GLS-FR-18).
23. **DXP-FR-ODVI** Both formats set body text at 11 pt with a line height of 1.5 and 9 pt after each paragraph. Headings are bold. The size, the space before, and the space after are: H1 20.5, 0, 12 pt; H2 16, 18, 9 pt; H3 13, 15, 6 pt. H4 is 12 pt, and H5 and H6 are 11 pt.
24. **DXP-FR-RBAQ** Both formats put 3 pt between list items. A list, a code block, a table, and a quote have 9 pt after them. A thematic break has 18 pt above and below it. Code block text is 10 pt with a line height of 1.4. Inline code has the size of the text around it.
25. **DXP-FR-NGUG** An export logs an `INFO` record under the `backend` domain when it starts and when it ends, with the format, `includeFrontmatter`, the diagram count, the warning count, and the duration. Each failed diagram logs a `WARN` record with its index. A failed export logs an `ERROR` record with its typed error. A temporary file that the export can not remove logs a `WARN` record. No record carries a path, a file name, or document content.
26. **DXP-FR-LHEV** A conversion that fails is the typed error `conversion_failed`, and writes nothing. A `format` that is not `pdf` or `docx` is the typed error `invalid_format`. With no project open every operation of this module fails with `no_project_open`.
27. **DXP-FR-PKDB** The internal operation `resolve_export_document(identifier)` takes a project-relative path or a Documents collection id (per `DCL-documents-collection.md` DCL-FR-QKJO) and returns the project-relative path of a DOC artifact. An id resolves to the document's `project_path` (DCL-FR-HYAF). An unknown id is `not_found`, and an id with no `project_path` is `outside_project`. The checks of DXP-FR-LNCQ then apply.
28. **DXP-FR-YEAQ** An **export request** is a record `{ id, path, format, discussionId, target, state, destination, error, createdAt, resolvedAt }`. `format` is `pdf`, `docx`, or absent. `state` is `pending`, `exported`, `cancelled`, or `failed`. The record is one TOML file `export-requests/<id>.toml` under the store root of the conversation it belongs to.
29. **DXP-FR-XZVZ** The internal operation `record_export_request` writes a `pending` record and then appends one comment through `append_as` (per `CMS-comments-storage.md` CMS-FR-26) with the agent's participant, the agent's message as its body, and one `export_request` attachment (CMS-FR-OKDW). When the append fails, the record is removed, and neither remains.
30. **DXP-FR-NXGQ** `get_export_request(requestId)` returns the record. An unknown id is the typed error `request_not_found`.
31. **DXP-FR-LHAG** `resolve_export_request(requestId, outcome, destination, error)` sets a `pending` request to `outcome`: `exported` with its `destination`, `cancelled`, or `failed` with its `error`. A request that is not `pending` is the typed error `already_resolved`, and nothing changes.
32. **DXP-FR-VFFP** A resolution appends one comment with the author's participant into the request's discussion, as a decision on a proposal appends its comment (per `DCP-draft-change-proposals.md` DCP-FR-PDCP). Its body states the outcome and the file name, and the destination of an export or the error of a failure. It returns `{ request, commentId }`.
33. **DXP-FR-FVTK** A resolution whose discussion is gone or resolved still changes the state, and returns no `commentId`. Each change of a request emits `"export request changed"` with the record.
34. **DXP-FR-UOFT** A resolution whose comment append fails still changes the state. It returns no `commentId` and `commentFailed: true`, and logs a `WARN` record with the request id and the typed error of the append.
35. **DXP-FR-IMSX** Recording and resolving a request each log an `INFO` record under the `backend` domain with the request id, the format, and the outcome. No record carries the path, the destination, the error text, or the message.

## Contract surface
The module owns the conversion, the export requests, and the Tauri commands and events below. The UI consumer is `../ui/EXD-export-dialog.md`. The tool `../tools/RDX-request-document-export-tool.md` uses the internal operations through the module's Rust API.

### Tauri commands
- `"export document (path, format, include frontmatter, destination, export id)"` → `export_document(path, format, includeFrontmatter, destination, exportId)` → `ExportResult { destination: string, warnings: ExportWarning[] }`, where `ExportWarning { kind: "diagram_failed", index: number }`. Side effect: writes one file at `destination`.
- `"get export request (id)"` → `get_export_request(requestId)` → `ExportRequest`.
- `"resolve export request (id, outcome, destination, error)"` → `resolve_export_request(requestId, outcome, destination?, error?)` → `{ request: ExportRequest, commentId?: string, commentFailed: boolean }`. Side effect: writes the record and appends one comment.

```
ExportRequest {
  id: string,
  path: string,                 // project-relative path of the DOC artifact
  format?: "pdf" | "docx",      // the format the agent gave
  discussionId: string,
  target: DiscussionTarget,
  state: "pending" | "exported" | "cancelled" | "failed",
  destination?: string,
  error?: string,
  createdAt: string,
  resolvedAt?: string
}
```

Typed errors: `no_project_open`, `not_found`, `read_failed`, `not_doc_artifact`, `not_markdown`, `outside_project`, `invalid_format`, `invalid_destination`, `conversion_failed`, `write_failed`, `request_not_found`, `already_resolved`, `invalid_outcome`.

### Events
- `"document export progress"` → `{ exportId: string, stage: "reading" | "diagrams" | "converting" | "writing", done: number, total: number }` (DXP-FR-LSYA).
- `"export request changed"` → `ExportRequest` (DXP-FR-FVTK).

### Internal (no Tauri reach)
- `resolve_export_document(identifier)` → project-relative path, or one of the typed errors of DXP-FR-PKDB.
- `record_export_request(path, format, message, agent, origin)` → `ExportRequest`.

## Non-functional requirements
- The export reads one file and its local images, and writes one file. It touches no other file of the project.
- A diagram is rendered with a bounded input, so one very large diagram does not use much memory.
- The export uses the fixed fonts and sizes of DXP-FR-FNTP, DXP-FR-OSRE, and DXP-FR-ODVI, so the visual result is close to the default look of the rich editor and not identical to it. A Word installation that does not have Arial shows its own fallback font.
