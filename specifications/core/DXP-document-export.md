# Document export

**Spec code:** `DXP`

## Intent
The backend owner of the **export of a DOC artifact** to a `.pdf` or a `.docx` file. An author who writes a document in the IDE must give it to people who do not use the IDE, so the backend converts the saved Markdown file to a portable format with the same visual result the rich editor shows: diagrams as images, vertical breaks as space, aligned blocks as aligned, and an optional metadata table from the frontmatter. This module also owns the **export requests** that the agent tool `../tools/RDX-request-document-export-tool.md` records, and their outcome. Out of scope: the Export dialog and the request card (`../ui/EXD-export-dialog.md`); the save dialog itself (`FSA-filesystem-access.md` `browse_for_save_path`); and how the rich editor reads the markers (`../ui/EDT-editor.md`).

## Functional requirements
1. **DXP-FR-IEQA** `export_document(path, format, includeFrontmatter, destination, exportId)` converts the Markdown file at the project-relative `path` of the active worktree to `format`, `pdf` or `docx`, and writes the result to the absolute `destination`. It reads the file from disk through the IDE instance of `FSA-filesystem-access.md`. It never reads an editor buffer.
2. **DXP-FR-LNCQ** The source of an export is a **DOC artifact**: a file of the active worktree whose resolved artifact type is `doc` (per `ASC-artifact-scanning.md` ASC-FR-06) and whose name ends in `.md`, case ignored. A missing file is the typed error `not_found`, a file of another type `not_doc_artifact`, another extension `not_markdown`, and a path that leaves the worktree `outside_project`.
3. **DXP-FR-XRAI** The export writes the destination through `write_bytes_at_user_choice` of `FSA-filesystem-access.md` (FSA-FR-28), in one atomic write that replaces an existing file. A destination that is not absolute is the typed error `invalid_destination`. A write that fails is the typed error `write_failed`, and leaves no partial file.
4. **DXP-FR-WLGE** The conversion runs on a worker thread and never on the thread that serves the window. The command returns when the file is written or the export fails. Two exports with different `exportId` values run independently.
5. **DXP-FR-LSYA** The export emits `"document export progress"` with `{ exportId, stage, done, total }`. The stages are `reading`, `diagrams`, `converting`, and `writing`, in that order, with at least one event for each. In the `diagrams` stage `done` counts the diagrams processed and `total` the diagrams of the document. In the other stages both are 0.
6. **DXP-FR-RKJC** A diagram fence (per `../ui/EDT-editor.md` EDT-FR-MOYS) is rendered to a PNG image with the `merman` crate, in the backend and with no webview. The image replaces the fence in the output, at its natural size scaled down to the text width where it is wider.
7. **DXP-FR-IOFM** A diagram that does not render, that renders to nothing, or whose renderer panics, stays in the output as a code block with its source. The export continues, and its result carries one warning `{ kind: "diagram_failed", index }` for that diagram, `index` counted from 1 in document order.
8. **DXP-FR-YVIV** A vertical break at the top level (per `../ui/EDT-editor.md` EDT-FR-QALB, EDT-FR-ANTD) becomes empty vertical space of 12, 8, 5, or 3 body line heights for V1, V2, V3, and V4 (per `../ui/EDT-editor.md` EDT-FR-CSKH).
9. **DXP-FR-ERZY** An alignment marker at the top level (per `../ui/EDT-editor.md` EDT-FR-XPMC) centres or right-aligns the top-level block on the next line. A paragraph, heading, list, or quote aligns its text. An image or a diagram moves with its block (per `../ui/EDT-editor.md` EDT-FR-GVJM).
10. **DXP-FR-ANXU** Every other HTML comment is absent from the output, at the top level and inside a paragraph. A comment inside a code span or a code block stays code text.
11. **DXP-FR-RPPW** With `includeFrontmatter` true, a leading YAML frontmatter block (per `../ui/EDT-editor.md` EDT-FR-18) renders before the body as a two-column **metadata table**: one row per top-level key in file order, the key in bold and the value in normal weight. A list or mapping value shows as its YAML text.
12. **DXP-FR-WAQF** With `includeFrontmatter` false, the frontmatter block is absent from the output. A file with no frontmatter gives no table either way. A frontmatter block that is not a YAML mapping renders as one code block of its text when included.
13. **DXP-FR-YRVE** The output keeps headings, paragraphs, emphasis, strong, strikethrough, inline code, links, ordered, bullet, nested, and task lists, block quotes, tables, code blocks, thematic breaks, and hard breaks. Other raw HTML shows as its text.
14. **DXP-FR-JYHX** An image with a relative path is read from the project, relative to the file, through the IDE instance. An image outside the project, a remote image, or one that does not decode, shows its alt text. The export makes no network request.
15. **DXP-FR-CPPT** A `docx` export is written with the `docx-rs` crate and a `pdf` export with the `markdown2pdf` crate.
16. **DXP-FR-NGUG** An export logs an `INFO` record under the `backend` domain when it starts and when it ends, with the format, `includeFrontmatter`, the diagram count, the warning count, and the duration. Each failed diagram logs a `WARN` record with its index. A failed export logs an `ERROR` record with its typed error. No record carries a path, a file name, or document content.
17. **DXP-FR-LHEV** A conversion that fails is the typed error `conversion_failed`, and writes nothing. With no project open every operation of this module fails with `no_project_open`.
18. **DXP-FR-PKDB** The internal operation `resolve_export_document(identifier)` takes a project-relative path or a Documents collection id (per `DCL-documents-collection.md` DCL-FR-QKJO) and returns the project-relative path of a DOC artifact. An id resolves to the document's `project_path` (DCL-FR-HYAF). An unknown id is `not_found`, and an id with no `project_path` is `outside_project`. The checks of DXP-FR-LNCQ then apply.
19. **DXP-FR-YEAQ** An **export request** is a record `{ id, path, format, discussionId, target, state, destination, error, createdAt, resolvedAt }`. `format` is `pdf`, `docx`, or absent. `state` is `pending`, `exported`, `cancelled`, or `failed`. The record is one TOML file `export-requests/<id>.toml` under the store root of the conversation it belongs to.
20. **DXP-FR-XZVZ** The internal operation `record_export_request` writes a `pending` record and then appends one comment through `append_as` (per `CMS-comments-storage.md` CMS-FR-26) with the agent's participant, the agent's message as its body, and one `export_request` attachment (CMS-FR-OKDW). When the append fails, the record is removed, and neither remains.
21. **DXP-FR-NXGQ** `get_export_request(requestId)` returns the record. An unknown id is the typed error `request_not_found`.
22. **DXP-FR-LHAG** `resolve_export_request(requestId, outcome, destination, error)` sets a `pending` request to `outcome`: `exported` with its `destination`, `cancelled`, or `failed` with its `error`. A request that is not `pending` is the typed error `already_resolved`, and nothing changes.
23. **DXP-FR-VFFP** A resolution appends one comment with the author's participant into the request's discussion, as a decision on a proposal appends its comment (per `DCP-draft-change-proposals.md` DCP-FR-PDCP). Its body states the outcome and the file name, and the destination of an export or the error of a failure. It returns `{ request, commentId }`.
24. **DXP-FR-FVTK** A resolution whose discussion is gone or resolved still changes the state, and returns no `commentId`. Each change of a request emits `"export request changed"` with the record.
25. **DXP-FR-IMSX** Recording and resolving a request each log an `INFO` record under the `backend` domain with the request id, the format, and the outcome. No record carries the path, the destination, the error text, or the message.

## Contract surface
The module owns the conversion, the export requests, and the Tauri commands and events below. The UI consumer is `../ui/EXD-export-dialog.md`. The tool `../tools/RDX-request-document-export-tool.md` uses the internal operations through the module's Rust API.

### Tauri commands
- `"export document (path, format, include frontmatter, destination, export id)"` → `export_document(path, format, includeFrontmatter, destination, exportId)` → `ExportResult { destination: string, warnings: ExportWarning[] }`, where `ExportWarning { kind: "diagram_failed", index: number }`. Side effect: writes one file at `destination`.
- `"get export request (id)"` → `get_export_request(requestId)` → `ExportRequest`.
- `"resolve export request (id, outcome, destination, error)"` → `resolve_export_request(requestId, outcome, destination?, error?)` → `{ request: ExportRequest, commentId?: string }`. Side effect: writes the record and appends one comment.

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

Typed errors: `no_project_open`, `not_found`, `not_doc_artifact`, `not_markdown`, `outside_project`, `invalid_destination`, `conversion_failed`, `write_failed`, `request_not_found`, `already_resolved`, `invalid_outcome`.

### Events
- `"document export progress"` → `{ exportId: string, stage: "reading" | "diagrams" | "converting" | "writing", done: number, total: number }` (DXP-FR-LSYA).
- `"export request changed"` → `ExportRequest` (DXP-FR-FVTK).

### Internal (no Tauri reach)
- `resolve_export_document(identifier)` → project-relative path, or one of the typed errors of DXP-FR-PKDB.
- `record_export_request(path, format, message, agent, origin)` → `ExportRequest`.

## Non-functional requirements
- The export reads one file and its local images, and writes one file. It touches no other file of the project.
- A diagram is rendered with a bounded input, so one very large diagram cannot hold the export for long.
- The PDF uses the fonts of `markdown2pdf`, so the visual result is close to the editor and not identical to it.
