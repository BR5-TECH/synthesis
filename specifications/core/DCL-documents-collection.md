# Documents collection

**Spec code:** `DCL`

## Intent
The backend owner of the **Documents collection**: the reference documents a user selects for a project — PDF, Markdown, and text files, wherever they live on the machine — so that the user and conversational agents can read them. The collection stores **references to selected files and folders** and nothing else. It never copies, edits, moves, or deletes a source. It discovers the supported documents behind the references, keeps them current as the files change, feeds the `documents` index of `BMI-bm25-indexing.md`, and serves their content to the viewers and to the two document tools. Out of scope: the panel and the viewers (`../ui/DPN-documents-panel.md`, `../ui/DTV-document-text-viewer.md`, `../ui/PDV-pdf-viewer.md`); the tools (`../tools/SDT-search-documents-tool.md`, `../tools/GDT-get-document-tool.md`); OCR of scanned PDFs; and writing to any document.

## Functional requirements
1. **DCL-FR-TYOU** A **source** is one selected reference, `{ kind, path }`, where `kind` is `file` or `folder` and `path` is the absolute path exactly as the user selected it. A **document** is a supported file that at least one source includes: a `file` source includes that file, and a `folder` source includes every supported file below that folder, at any depth.
2. **DCL-FR-FGGU** The supported file types are `.pdf`, `.txt`, `.md`, and `.markdown`. Extension matching ignores case. The `format` of a document is `pdf` for `.pdf`, `markdown` for `.md` and `.markdown`, and `text` for `.txt`. Every other file is ignored, whether the user selected it directly or a folder scan met it.
3. **DCL-FR-QKJO** A document's **id** is `doc-` followed by the first 32 lowercase hexadecimal characters of the SHA-256 of its normalised path. Normalisation makes the path absolute, collapses `.` and `..` segments without consulting the disk, turns `\` into `/`, and removes a trailing `/`. One path gives one id, whatever source reached the file.
4. **DCL-FR-YWCP** The collection lists each supported file **once**, however many sources include it. A file stays in the collection while any one source includes it, and it leaves only when no source includes it. A source that includes a file twice by overlap, or a file selected both directly and through a folder, produces one entry.
5. **DCL-FR-HNRM** The sources persist **per repository** in the per-project slot of `GSS-global-settings-storage.md` (GSS-FR-CDYK), outside the repository and outside Git. Every worktree of one repository reads the same sources. A change of the active worktree changes no source, and a path inside a worktree stays the exact path that was selected.
6. **DCL-FR-VEVZ** The store holds references only. It holds no document content, no extracted text, no index, and no copy of any source. Removing a source deletes that one reference, and it creates, modifies, moves, and deletes no file or folder of the user.
7. **DCL-FR-VXXI** `pick_document_sources(mode)` opens the OS-native picker for the `mode`: `files` allows several files, and `folder` selects one folder. It adds each selection as a source, ignores a selected file of an unsupported type, and returns whether the user cancelled, how many files it ignored, and the new snapshot. A cancel adds nothing and is not an error.
8. **DCL-FR-PHYP** Adding a source whose normalised path equals the path of a stored source adds nothing and is not an error. The stored order of sources is the order of first addition. A path that is not absolute is refused with the typed error `invalid_path`, and the command stores nothing.
9. **DCL-FR-ATNL** `remove_document_source(path)` removes the stored source whose normalised path equals the normalised `path`, and returns the new snapshot. A path that matches no source removes nothing and is not an error. Documents that another source still includes stay in the collection.
10. **DCL-FR-SQEP** `list_documents` returns the **snapshot**: the sources in stored order, each with its `status`, and the documents sorted by normalised path, each with its `id`, `path`, `name`, `format`, `status`, and `revision`. A document's `name` is its file name. Its `revision` is the lowercase hexadecimal SHA-256 of its content bytes, and it is absent while the document is `unavailable`.
11. **DCL-FR-EWPO** A source is `unavailable` when its path does not exist, cannot be read, or is a symbolic link; its `reason` is `missing`, `unreadable`, or `link`. An unavailable source stays in the store and in every snapshot, and contributes no document. An unavailable `file` source keeps its own entry as an `unavailable` document.
12. **DCL-FR-QTGN** A document is `unavailable` when its file cannot be read, or when a `markdown` or `text` file is not valid UTF-8. It stays in the collection while a source includes it. An unavailable document contributes no text to the `documents` index and returns no content from any read.
13. **DCL-FR-KMHY** Folder discovery walks each `folder` source recursively through the documents instance of `FSA-filesystem-access.md` (FSA-FR-DMKC). It follows no symbolic link and lists none, and it lists only regular files: a FIFO, a socket, or a device that carries a supported extension is skipped with a `WARN` record. It skips a subfolder it cannot read, records a `WARN` record for it, and continues. An entry that disappears during a listing is skipped with a `WARN` record, and the listing still succeeds. The order of discovery does not change the snapshot, which is sorted by path.
14. **DCL-FR-BAQY** All reads of a source or of a document go through the documents instance of `FSA-filesystem-access.md` (FSA-FR-DMKC), which reaches the selected paths read-only. The collection uses no other route to a selected path, and it hands no selected path to the frontend as something the frontend may read.
15. **DCL-FR-NCBQ** `read_document(id)` returns `{ id, name, format, text, revision }` for a `markdown` or `text` document, reading the current file content. It fails with `no_project_open` when no project is open, `unknown_document` when the id is not in the collection, `unavailable` when the document is unavailable, and `wrong_format` for a `pdf` document.
16. **DCL-FR-TEUK** `read_document_pdf(id)` returns `{ id, name, revision, bytes_base64 }` with the whole PDF content, read through the Documents instance. It fails with the typed errors of DCL-FR-NCBQ, and with `wrong_format` for a document that is not a `pdf`. No other operation returns PDF bytes, and no document tool returns them.
17. **DCL-FR-GLUS** The internal operation `document_text(id)` returns the full text of any available document. For `markdown` and `text` it is the file text. For `pdf` it is the text that `pdf_extract` extracts from the PDF bytes, with no OCR. A PDF with no embedded text, or one that cannot be parsed, has no text and returns the typed error `no_text`.
18. **DCL-FR-VRTS** PDF extraction runs on bytes read through the Documents instance. A failure or a panic inside the extractor is caught, recorded as a `WARN` record that names the document id, and reported as `no_text`. It does not fail the pass, the command, or the application. The extractor runs on a worker thread under a deadline and a maximum input size (DCL-FR-DLPX).
19. **DCL-FR-QVYZ** The extracted text of each PDF is cached for the application session, keyed by the document id and the PDF's `revision`. A later read or index pass reuses the cached text while the revision is unchanged. A changed revision discards the cached text and extracts again. The cache is never written to disk.
20. **DCL-FR-QGLH** The collection opens with the project. It loads the stored sources, and runs a **refresh** in the background. A refresh discovers the documents, reads the changed ones, updates the PDF cache, informs the `documents` index (BMI-FR-MWNQ), and emits `"documents changed"` when the snapshot differs from the previous one. No command blocks on a refresh.
21. **DCL-FR-LRUP** Adding or removing a source runs a refresh before the command returns its snapshot. The refresh brings the Documents instance in line with the stored sources as FSA-FR-WBKZ states, so that its reach equals the grantable stored sources exactly, and it keeps the same instance while the grantable sources are unchanged. A removed source stops being readable through the instance at that moment.
22. **DCL-FR-ZYQC** The collection watches every available source path: a folder recursively, and a file through its parent folder. A change that reaches a watched path runs one refresh after a short debounce, so a burst of changes costs one refresh. A document whose content bytes changed gets a new `revision`, and a new index text.
23. **DCL-FR-PTRP** `"documents changed"` carries the full snapshot. It is emitted after every refresh whose snapshot differs from the previous one, in the order the snapshots arose. A change of `revision`, `status`, a document set, or a source status counts as a difference.
24. **DCL-FR-XHSJ** A change of the active worktree leaves the sources, the snapshot, the PDF cache, and the `documents` index as they are, and runs one refresh. Closing the project stops the watchers, discards the snapshot, the Documents instance, and the index, and keeps the PDF cache for the application session. Stopping the watchers is bounded in time: a close does not wait for a watch that is still being set up, and a watch that finishes setting up after the close is discarded.
25. **DCL-FR-MSHW** With no project open every command of this module fails with the typed error `no_project_open`, and the internal operations return the same refusal. A stored-source read failure is reported as `store_unavailable` by every command and every internal operation, and the collection then shows no documents rather than an empty store. The stored-source read is tried again at the next refresh, and the collection recovers when it succeeds.
26. **DCL-FR-UPFP** Each source addition or removal and each refresh logs an `INFO` record through `LGC-logging.md` under the `backend` domain, with the source kind, the counts, and the duration. A PDF with no text, a PDF whose extraction ran past its limits, an unreadable folder, a skipped non-regular or vanished entry, a file over the size limit, and an unavailable source each log a `WARN` record. A failed refresh logs an `ERROR` record.
27. **DCL-FR-TAGV** A record names a document by its id and its format, and a source by its kind. No record carries document bytes, document text, a file name, a path, or a query.
28. **DCL-FR-RXMB** The collection reads only regular files, through `read_regular_bytes` of `FSA-filesystem-access.md` (FSA-FR-NRQE), and at most 64 MiB of one document. A document that is not a regular file, or whose file is larger than that limit, is `unavailable` and contributes no text. The collection records a `WARN` record for it. A FIFO or a device in a selected folder therefore never blocks a refresh, and a very large file is never read into memory whole.
29. **DCL-FR-DLPX** PDF extraction runs on a worker thread. It has a deadline of 30 seconds and a maximum input of 32 MiB. A PDF larger than the maximum is not extracted. A PDF whose extraction has not ended at the deadline is abandoned, and the caller does not wait for it. Both cases are reported as `no_text` and record a `WARN` record that names the document id and the reason, and neither holds the refresh or a tool call longer than the deadline. The outcome is cached for the revision (DCL-FR-QVYZ).
30. **DCL-FR-VWSG** An operation that changes the collection applies its result only while the project it began for is still the open project. A source addition or removal that a project switch overtakes stores its list under the key of the project it began for and changes the sources in memory of no other project. A refresh that a close or an open overtakes emits no `"documents changed"` event, and no event is emitted after the close returned.

## Contract surface
The module owns the stored sources, the Documents instance, the refresh, the PDF text cache, and the Tauri commands and event below. The UI consumers are `../ui/DPN-documents-panel.md`, `../ui/DTV-document-text-viewer.md`, and `../ui/PDV-pdf-viewer.md`. The tools consume the internal operations through the module's Rust API and have no Tauri reach.

### Tauri commands
- `"list documents"` → `list_documents` → `DocumentsSnapshot`. Reads the in-memory snapshot and does not wait for a refresh.
- `"pick document sources (mode)"` → `pick_document_sources(mode)` → `PickDocumentSourcesResult`, where `mode` is `"files"` or `"folder"`.
- `"remove document source (path)"` → `remove_document_source(path)` → `DocumentsSnapshot`.
- `"read document (id)"` → `read_document(id)` → `DocumentText`.
- `"read document pdf (id)"` → `read_document_pdf(id)` → `DocumentPdf`.

Typed errors: `no_project_open`, `store_unavailable`, `invalid_path`, `unknown_document`, `unavailable`, `wrong_format`.

### Events
- `"documents changed"` → `DocumentsSnapshot` (DCL-FR-PTRP).

### Internal Rust API
No UI consumer and no Tauri command.

- `document_text(id)` → the full text of one available document, or `no_text` (DCL-FR-GLUS), or the typed refusals of DCL-FR-MSHW.
- `resolve_document(id)` → the `DocumentEntry` for an id in the collection, or `no_project_open`, `store_unavailable`, or `unknown_document`. It is the only way a tool turns an id into a document.
- `documents_for_index()` → the available documents with their text, for the `documents` index of `BMI-bm25-indexing.md`, or `no_project_open` or `store_unavailable`.
- `load_document_sources(project_key)` and `save_document_sources(project_key, sources)` of `GSS-global-settings-storage.md`.

### Payload shapes
```
DocumentSource {
  kind,             // "file" | "folder"
  path,             // absolute, as selected
  status,           // "available" | "unavailable"
  reason?           // "missing" | "unreadable" | "link", when unavailable
}

DocumentEntry {
  id,               // "doc-" + 32 hex (DCL-FR-QKJO)
  path,             // normalised absolute path
  name,             // file name
  format,           // "pdf" | "markdown" | "text"
  status,           // "available" | "unavailable"
  revision?         // SHA-256 hex of the content; absent when unavailable
}

DocumentsSnapshot { sources: DocumentSource[], documents: DocumentEntry[] }

PickDocumentSourcesResult { cancelled, ignored_count, snapshot }

DocumentText { id, name, format, text, revision }

DocumentPdf { id, name, revision, bytes_base64 }
```

## Non-functional requirements
- The store holds paths and nothing else, so a user who deletes it loses a list of references and loses no document.
- A refresh reads a file again only when its size or modification time changed, so an idle collection costs one directory walk per watched change.
- Extraction of a large PDF runs off the thread that serves the UI, so the application stays usable while a PDF is read.
- The module needs no network access.
