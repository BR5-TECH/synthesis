# Request document export tool

**Spec code:** `RDX`

## Intent
The tool an agent uses when the author asks it to export a document, for example "export this document to pdf". The agent cannot choose where a file goes on the author's machine, so the tool does not export. It **records an export request** and posts it into the conversation as one comment of the agent with a card. The author opens the card when they want, chooses the destination in the Export dialog, and confirms (`../ui/EXD-export-dialog.md`). The outcome comes back into the conversation and brings the agent back. Out of scope: the conversion (`../core/DXP-document-export.md`) and the dialog and the card (`../ui/EXD-export-dialog.md`).

## Functional requirements
1. **RDX-FR-UBTA** The tool is a `rig` portable tool named `request_document_export` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares.
2. **RDX-FR-ORZH** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05, TLC-FR-06).
3. **RDX-FR-EDQY** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the origin of the turn's conversation and to the turn's agent participant. It takes no destination and no argument that names a conversation or a participant.
4. **RDX-FR-DFXB** The tool is attached to every turn (per `../ai/CVL-conversation-loop.md` CVL-FR-08). With no project open a call is the shared **no open project** refusal (per TLC-FR-13).
5. **RDX-FR-RRJQ** `document` is a project-relative path of a DOC artifact or its Documents collection id. The tool resolves it through `../core/DXP-document-export.md` DXP-FR-PKDB. It can name any DOC artifact of the project, not only the document of the conversation.
6. **RDX-FR-DVQL** A `document` that is blank once trimmed is the **blank document** refusal. The typed errors of DXP-FR-PKDB are the **document not found**, **not a DOC artifact**, **not a Markdown file**, and **outside the project** refusals.
7. **RDX-FR-POVJ** `format` is optional. A `format` that is present and is not `pdf` or `docx`, case ignored, is the **bad format** refusal.
8. **RDX-FR-AROQ** A `message` that is blank once trimmed is the **blank message** refusal. A `message` with no spoken part (per `../ai/CVL-conversation-loop.md` CVL-FR-AGNN) is the **no spoken part** refusal.
9. **RDX-FR-NOBI** A conversation whose folded state is resolved (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **conversation resolved** refusal: `PermissionDenied`, not retryable.
10. **RDX-FR-TMYF** A call that passes every check records one export request through `../core/DXP-document-export.md` DXP-FR-XZVZ, with the resolved path, the format in lowercase, and the message. The comment body is the `message` less its citation markers (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU).
11. **RDX-FR-IDWS** A failure to record the request or to append its comment is the **could not record** refusal: `Other`, retryable. Nothing is recorded.
12. **RDX-FR-PZGN** A successful call returns `{ requested: true }` and **ends the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-15). It returns at once. It holds no timer, waits for no outcome, and exports nothing.
13. **RDX-FR-DMLK** A reply that asks for two calls of this tool dispatches the first. Each other call of the tool in that reply is not dispatched and gets the **one at a time** refusal (per `../ai/CVL-conversation-loop.md` CVL-FR-15). One turn records at most one export request.
14. **RDX-FR-IEQA** Each call logs under the `ai` and `backend` domains (per TLC-FR-14): one `INFO` record when a request is recorded, with the tool, the discussion id, and the format; and one `WARN` record when it refuses, with the tool and the reason. No record carries the document path, the id the agent gave, or the message.
15. **RDX-FR-LNCQ** The tool reaches no network and writes no file but the request record and the comment. It commits nothing.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own.

### The tool
`request_document_export` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the origin of the conversation the turn belongs to and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Ask the author to export a document of the project to a PDF or a Word (.docx) file. Use it when the author asks you to export, convert, or download a document as PDF or DOCX. You can not choose where the file goes, so this tool does not export: it posts an export request into the conversation, and the author opens it, chooses the destination, and confirms. Give the document in `document`: its project-relative path, for example `docs/guide.md`, or its id from `search_documents` or `get_document`, for example `doc-…`. Only a Markdown document of the project that carries the document type can be exported. Give `format` `pdf` or `docx` when the author said which one; leave it out when they did not. Tell the author in `message`, in a sentence, what you ask them to export. Put one `<spoken>…</spoken>` tag in `message`: one short sentence that a voice can read aloud. A message with no tag is refused. Your turn ends when the request is posted, and the outcome comes back to you in the conversation.

### Arguments
```
RequestDocumentExportArgs {
  document: string,          // required; project-relative path or Documents id
  format:   string?,         // "pdf" | "docx"
  message:  string           // required; the comment the author reads
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `document` — *"The document to export: its project-relative path, for example `docs/guide.md`, or its Documents id, for example `doc-…`."*
- `format` — *"`pdf` or `docx`, when the author said which. Leave it out when they did not."*
- `message` — *"What you ask the author to export, in a sentence. It holds one `<spoken>…</spoken>` tag with one short sentence that a voice can read aloud."*

### Output
Data (per TLC-FR-08): `{ requested: true }`. The turn ends on the call that requested (per `../ai/CVL-conversation-loop.md` CVL-FR-15).

### The comment
The `message`, its spoken part hidden (per `../ui/CMT-comments.md` CMT-FR-BMCI). Beside it the comment carries exactly one `export_request` attachment (per `../core/CMS-comments-storage.md` CMS-FR-OKDW).

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank document** — kind `InvalidArgs`, retryable: *"Give the document to export in `document`: its project-relative path or its Documents id."*
- **document not found** — kind `NotFound`, retryable: *"No document of the project has this path or id. Find it with `search_documents` and give its id or its path."*
- **not a DOC artifact** — kind `InvalidArgs`, retryable: *"This file is not a document of the Documents collection, so it can not be exported. Only a Markdown file with the document type can be exported."*
- **not a Markdown file** — kind `InvalidArgs`, retryable: *"This document is not a Markdown file, so it can not be exported."*
- **outside the project** — kind `InvalidArgs`, retryable: *"This document is not in the project, so it can not be exported."*
- **bad format** — kind `InvalidArgs`, retryable: *"`format` is `pdf` or `docx`. Leave it out when the author did not say which."*
- **blank message** — kind `InvalidArgs`, retryable: *"Tell the author in `message` what you ask them to export."*
- **no spoken part** — kind `InvalidArgs`, retryable: *"The message has no spoken part. Put one short sentence in one `<spoken>…</spoken>` tag in `message`."*
- **conversation resolved** — kind `PermissionDenied`, not retryable: *"This conversation is resolved and takes no further comments, so nothing was requested. Answer with what you already have; asking again will not succeed."*
- **could not record** — kind `Other`, retryable: *"The export request could not be recorded."*
- **one at a time** — the result of a call that is not dispatched (RDX-FR-DMLK): *"Only one export request can be posted in a reply, so this one was not recorded. Ask for the next export after the author answered this one."*

### Side effects
Declared in full (per TLC-FR-17). A successful call produces **exactly one export request** and **exactly one comment**, through `../core/DXP-document-export.md` DXP-FR-XZVZ. It exports nothing, writes no file of the project, and changes no repository state.

## Non-functional requirements
- A call costs one record write and one comment append.
- Nothing in this tool requires network access.
