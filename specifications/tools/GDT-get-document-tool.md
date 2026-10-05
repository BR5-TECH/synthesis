# Get document tool

**Spec code:** `GDT`

## Intent
The tool an agent reaches for once it knows which reference document it wants and needs the text of that document. It takes the stable document id that `SDT-search-documents-tool.md` returned and gives back the text of the document: all of it by default, or a byte range or a line range. It reads only documents that are in the Documents collection of `../core/DCL-documents-collection.md`, and it reads them through the read-only grant for selected Documents paths. It never accepts a path, so it gives an agent no way to read any other file. For a PDF it returns the extracted text and never the PDF bytes. Out of scope: finding a document, which `SDT-search-documents-tool.md` does; reading a project file, which `RFT-read-file-tool.md` does; changing any document; and OCR of scanned PDFs.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`get_document` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19). Users and agents know it as **Get Document**.

### The description
The fixed text the model reads (per TLC-FR-05):

> Get Document: read the text of one reference document the user selected for this project. Use this when a search for documents named a document and you need what it says. Give the document id exactly as the search reported it. Returns the whole text by default. For a PDF the text is the text extracted from the PDF, because the PDF itself cannot be returned. To read part of a long document, pass a byte range with `byte_offset` and `byte_length`, or a line range with `line_offset` and `line_limit`. Do not pass both kinds of range in one call. This tool reads one document and returns it as it is written. It does not search, does not summarise, cannot read any file that is not a selected document, and cannot change anything.

### Arguments
```
GetDocumentArgs {
  id:           string,     // required; a document id
  byte_offset:  integer?,   // optional; 0-based first byte, default 0
  byte_length:  integer?,   // optional; byte count, default the rest of the text
  line_offset:  integer?,   // optional; 0-based first line, default 0
  line_limit:   integer?    // optional; line count, default the rest of the text
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `id` — *"The id of the document to read, exactly as a search for documents reported it. For example: 'doc-0123456789abcdef0123456789abcdef'. A path cannot be used here."*
- `byte_offset` — *"The first byte to return, counting from 0 in the UTF-8 text of the document. Use it with `byte_length` to read part of a long document. Do not combine it with a line range."*
- `byte_length` — *"How many bytes to return, starting at `byte_offset`. Defaults to the rest of the text. A range that cuts through a character is widened to include the whole character."*
- `line_offset` — *"The first line to return, counting from 0. Use it with `line_limit` to read part of a long document. Do not combine it with a byte range."*
- `line_limit` — *"How many lines to return, starting at `line_offset`. Defaults to the rest of the text. A value below 1 is treated as 1."*

### Output
The document's text, or the requested range of it, as text (per TLC-FR-08). The text is returned unwrapped, so the model receives the characters of the document and nothing this tool composed.

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank id** — kind `InvalidArgs`, retryable: *"The id must name a document. Give the id exactly as a search for documents reported it."*
- **both range forms** — kind `InvalidArgs`, retryable: *"Use either a byte range or a line range, not both. Call again with byte_offset and byte_length, or with line_offset and line_limit."*
- **unknown document** — kind `NotFound`, retryable: *"No document with that id is in the user's selected documents. Search for documents again to get a current id."*
- **document unavailable** — kind `Other`, not retryable: *"That document is unavailable. Its file was moved, deleted, or cannot be read."*
- **no text** — kind `Other`, not retryable: *"That document has no text that can be extracted. It may be a scanned PDF made of images."*

## Functional requirements
1. **GDT-FR-LITM** The tool exists as a `rig` portable tool named `get_document` and conforms to `TLC-tool-conventions.md` in full: its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
2. **GDT-FR-GNCR** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the five documented arguments with the documented parameter descriptions (per TLC-FR-05).
3. **GDT-FR-FZHF** The tool resolves `id` through the Documents collection (`../core/DCL-documents-collection.md` `resolve_document`) and reads the text through `document_text`. It accepts no path, composes no path, and reads no file that the collection does not hold. An `id` that is not in the collection, including any string that looks like a path, is the retryable unknown-document refusal.
4. **GDT-FR-HCYY** Every read goes through the documents instance of `../core/FSA-filesystem-access.md` (FSA-FR-DMKC) and never through the instance of an agent session. The tool gives no agent tool a new reach: `read_file` still refuses every path outside the project (per `RFT-read-file-tool.md` RFT-FR-04).
5. **GDT-FR-NKKK** With no range argument the whole text is returned, whatever its size. There is no size ceiling, no truncation, and no elision. The text is the current content of the document: a change of the file is visible on the next call (per `../core/DCL-documents-collection.md` DCL-FR-ZYQC).
6. **GDT-FR-HREW** For a `pdf` document the text is the extracted text of the PDF, from the session cache of `../core/DCL-documents-collection.md` (DCL-FR-QVYZ). The tool never returns the PDF bytes, and both range forms apply to the extracted text. After a PDF changes, the tool returns the text of the new content.
7. **GDT-FR-QTAA** A byte range is a half-open range `[byte_offset, byte_offset + byte_length)` over the UTF-8 encoding of the text. `byte_offset` defaults to 0 and a negative value is clamped to 0. `byte_length` defaults to the rest of the text, a value below 1 is clamped to 1, and a range that reaches past the end returns the bytes that exist.
8. **GDT-FR-YXCH** When an edge of a byte range falls inside a UTF-8 character, the range widens outward: the start moves back to the first byte of that character, and the end moves forward to the last byte of the character that holds the final requested byte. The returned text is valid UTF-8 and holds every requested byte.
9. **GDT-FR-IUFV** A `byte_offset` at or beyond the end of the text is a success carrying empty text (per TLC-FR-12). An empty document answers the same way.
10. **GDT-FR-RQZS** A line range is `line_limit` lines beginning at the 0-based `line_offset`, on the terms of `RFT-read-file-tool.md` RFT-FR-07 through RFT-FR-09 and RFT-FR-11: `line_offset` defaults to 0 and a negative value is clamped to 0, `line_limit` defaults to the rest of the text and a value below 1 is clamped to 1, and a range keeps each line's own terminator.
11. **GDT-FR-MVHY** A call that supplies any of `byte_offset` and `byte_length` together with any of `line_offset` and `line_limit` is the retryable both-range-forms refusal. The tool refuses before it resolves the id or reads any text, and it returns no partial result.
12. **GDT-FR-IPJG** An `id` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal. With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13). An unavailable document and a PDF with no extractable text are the two non-retryable `Other` refusals of the contract surface.
13. **GDT-FR-CHWC** The returned text is the text of the document and nothing else. No id, path, range marker, count, or sentence this tool composed is added to it, so a range read differs from a whole read only in which characters it carries.
14. **GDT-FR-IFCA** The tool answers from the collection as it stands and never blocks on background work (per TLC-FR-16). It waits for no index pass and no refresh. A document that became unavailable since the last refresh produces the unavailable refusal when the read fails.
15. **GDT-FR-PXQL** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no file, no source reference, no cache entry that outlives the call except the PDF text cache of DCL-FR-QVYZ, and no other state. A file's modification time is unchanged by being read.
16. **GDT-FR-VZXF** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `id`, the range form used (`none`, `bytes`, or `lines`), and the bytes returned; and a `WARN` record naming the tool, the `id`, and the reason when it refuses.
17. **GDT-FR-LDHA** The `id` is recorded untrimmed and bounded to its first 512 characters followed by an ellipsis. No record carries a range value, a document name or path, or any part of the document text.

## Non-functional requirements
- A call costs one read of the document text, or one cache lookup for a PDF. A range is cut from the text that read returned, so paging buys context and not I/O.
- The absence of a size ceiling puts the choice about a large document where the model can act on it: the range arguments are the tool's answer to a document too long to want whole.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences. This tool reads a selected reference document by id, `RFT-read-file-tool.md`'s reads a project file by path, and `SDT-search-documents-tool.md`'s finds which selected documents bear on a topic.
- The tool needs no network access.
