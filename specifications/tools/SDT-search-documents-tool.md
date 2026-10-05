# Search documents tool

**Spec code:** `SDT`

## Intent
The tool an agent reaches for when it needs to know which of the user's reference documents bear on a topic. A reference document is a PDF, Markdown, or text file that the user selected for the project, and it may live outside the project checkout. This tool takes the agent's description of a topic, ranks the Documents collection against it, and returns each match with a stable document id and enough identifying text to choose between matches. The id is then passed to `GDT-get-document-tool.md`, which reads the document. Everything the tool knows comes from the `documents` index of `../core/BMI-bm25-indexing.md`, and every document it names is resolved through `../core/DCL-documents-collection.md`. Out of scope: reading a document, which `GDT-get-document-tool.md` does; searching specifications, drafts, notes, skills, or project files; adding or removing a document; and OCR of scanned PDFs.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached to every conversational agent turn by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach.

### The tool
`search_documents` — a `rig` portable tool, constructed by its own constructor and attached by a caller that names it (per TLC-FR-19). Users and agents know it as **Search for Documents**.

### The description
The fixed text the model reads (per TLC-FR-05):

> Search for Documents: find the reference documents the user selected for this project that are most relevant to a topic, ranked by how well each document's text matches what you describe. Use this to discover what the user's PDF, Markdown, and text references say about a subject, wherever those files live on the machine. Returns the best matches first, each with the document's id, its name, the folder it sits in, its format, a short passage that matched, and a score that orders this result set. Each document appears at most once. Pass the id to Get Document to read the document. Only the user's selected documents are searched — not specifications, not drafts, not notes, not project files. Nothing that is found is changed. Returns an empty list when nothing matches, which means no selected document bears on that topic.

### Arguments
```
SearchDocumentsArgs {
  query:  string,     // required
  limit:  integer?    // optional; default 5, clamped to 1..=20
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `query` — *"A plain description of the topic you want reference documents about, in your own words. For example: 'how the vendor API rate limits requests'."*
- `limit` — *"How many documents to return, best match first. Defaults to 5. Values below 1 or above 20 are clamped into that range."*

### Output
```
DocumentMatch {
  id,        // the document id (DCL-FR-QKJO)
  name,      // the document's file name
  folder,    // the name of the folder that holds the document
  format,    // "pdf" | "markdown" | "text"
  snippet,   // the best-matching passage, bounded
  score      // orders this result set
}

SearchDocumentsOutput { documents: DocumentMatch[] }
```

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **empty query** — kind `InvalidArgs`, retryable: *"The query must describe the topic you want reference documents about. Call again with a short plain-language description of it."*

## Functional requirements
1. **SDT-FR-LNNL** The tool exists as a `rig` portable tool named `search_documents` and conforms to `TLC-tool-conventions.md` in full: its name, description, parameter schema, output shape, refusal shape, logging, statelessness, and read-only posture are that spec's rules applied to this tool.
2. **SDT-FR-JMSA** Its `description()` returns the fixed text of the contract surface above, compiled into the binary, and its `parameters()` returns the JSON Schema for the two documented arguments with the documented parameter descriptions (per TLC-FR-05).
3. **SDT-FR-BKAW** The tool ranks by calling `../core/BMI-bm25-indexing.md`'s `search` (BMI-FR-10) against the `documents` index alone. It returns no document that call did not produce, holds no index of its own, and applies no ranking of its own.
4. **SDT-FR-BTOJ** `limit` counts documents. It defaults to 5 when absent and is clamped into 1..=20 (per TLC-FR-07). The tool never passes a limit of zero downward. The tool makes one call to the underlying search per invocation, and it never repeats or widens that call.
5. **SDT-FR-EMCC** A document appears at most once. A document holds several chunks in the index, so the tool keeps the highest-scoring chunk of each document and drops the others. Matches are ordered by descending `score`, and the score orders this one result set only.
6. **SDT-FR-ZWJW** Every match carries exactly `id`, `name`, `folder`, `format`, `snippet`, and `score`. The match carries no filesystem path, no source path, and no chunk ordinal, so a model has no path to compose and no way to read a document except through its id.
7. **SDT-FR-ONOF** `id`, `name`, `folder`, and `format` come from the document's current entry in the collection (per `../core/DCL-documents-collection.md` DCL-FR-SQEP), resolved by id at the moment of the call. A hit is returned only where the document is still in the collection and is available. A hit for a document that has left the collection or become unavailable is omitted without an error.
8. **SDT-FR-PJKE** `snippet` is the text of the best-matching chunk, cut at a character boundary to at most 600 characters, with an ellipsis when it was cut. For a PDF the snippet is extracted text. The tool never returns more than one snippet per document.
9. **SDT-FR-GGYO** A `query` that is empty, or blank once trimmed, is the retryable `InvalidArgs` refusal of the contract surface. With no project open the tool produces the shared no-open-project refusal (per TLC-FR-13).
10. **SDT-FR-YEPA** A query that matches no document in an open project is a success carrying an empty `documents` list (per TLC-FR-12). So is a project with no selected document.
11. **SDT-FR-AMNB** The tool never blocks on an index pass (per TLC-FR-16, and `../core/BMI-bm25-indexing.md` BMI-FR-10). A call made while the first build is still running ranks over what has been indexed so far. A change to the collection reaches the tool on the next index pass (per BMI-FR-MWNQ), and the tool holds nothing from a previous call.
12. **SDT-FR-NZEH** The tool is read-only (per TLC-FR-17): it creates, modifies, and deletes no document, no source reference, no index, and no other state. It reads no file and extracts no PDF text itself.
13. **SDT-FR-QSKI** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record naming the tool, the `query`, the `limit` the model asked for, the limit applied, and the number of documents returned; and a `WARN` record naming the tool, the `query`, and the reason when it refuses.
14. **SDT-FR-KRWA** The `query` is recorded untrimmed and bounded to its first 512 characters followed by an ellipsis. No record carries a returned id, name, folder, snippet, or any part of any document.

## Non-functional requirements
- A call is one in-memory BM25 lookup and one collection lookup per hit, so it is cheap enough to sit inside an agent's reasoning loop.
- The default `limit` of 5, the ceiling of 20, and the snippet bound of 600 characters bound the context a call can cost.
- The description is written so that a model reading every tool description in this group can tell them apart on their first sentences. This tool finds which of the user's selected reference documents bear on a topic, and `GDT-get-document-tool.md` reads one of them.
- The tool needs no network access.
