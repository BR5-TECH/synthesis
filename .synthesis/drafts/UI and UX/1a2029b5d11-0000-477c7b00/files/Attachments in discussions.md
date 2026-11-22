## Intent

The author can attach PDF and DOCX files to a comment in any discussion. The agent reads the text of the file, and the images embedded in it, as part of the comment. "Media files" means images only. Images are already accepted (`CMS-FR-47`) and stay as they are. Audio and video are out of scope.

## User journey

- The author opens the Attach affordance of the shared composer and picks a `.pdf` or `.docx` file. The pending strip shows a chip with the file name, type, and size. The chip has no view action. The author cannot see the extracted text.
- The author posts the comment. At that moment the backend extracts the text. If extraction succeeds, the comment is posted and the chip shows in the comment as for any non-image attachment.
- If extraction fails, the comment is not posted. A typed error shows inline at the foot of the card (`CMT-FR-51`). The composer text and the other pending attachments stay. The author removes the chip and posts again.
- The author addresses an agent. The agent receives the text of the file in the position of the comment. Images embedded in the file arrive in place, between the text blocks, as image content.
- An endpoint that does not accept image content gets the same text with safe metadata for each image in the same position. The surface says that pictures were not sent (`AGC-FR-37`, `CTA-FR-ARBB`).
- A later turn that reads the same comment reuses the extracted text from the session cache. It does not extract again.
- The author can set the extracted-text limit in Global Settings, AI API section.

## Requirements

- **Accepted files.** The accepted set of a comment attachment grows from images, PDF, and plain text to also include DOCX (`.docx`, media type `application/vnd.openxmlformats-officedocument.wordprocessingml.document`). The existing attachment size bound still applies to the file.
- **Deterministic extraction.** PDF and DOCX are converted to text deterministically: the same bytes always give the same text, and the same embedded images in the same order. No OCR, no network, no model call. Reuse the libraries the project already uses, and add no new crate for text. For PDF text use `pdf-extract`, as the Documents feature does (see the `Additional context` draft), and reuse its extraction code where it exists. For DOCX use the `docx-rs` crate, which the document export already uses (`core/DXP-document-export.md` DXP-FR-CPPT), in its reader mode. For embedded images, use the PDF parser that `pdf-extract` already depends on and the DOCX package media read through `docx-rs`. Add a new crate only where these libraries cannot read the images. Name that crate and the reason in the specification you change.
- **When.** The backend extracts at post time, to validate the file. It extracts again at turn time only when the session cache has no entry. The cache is in memory, per application session, keyed by the SHA-256 digest of the file bytes. It is not stored on disk or in the comment log. The comment log keeps only the original blob.
- **Text limit.** The extracted text is bounded. The default is 1 MB of UTF-8 text. The limit is configurable. A file above it is refused. The limit applies at post time only. A posted file is never refused later.
- **Typed errors at post time.** A refused file refuses the whole append, as in `CMS-FR-47`: no line, no blob, log unchanged. Use three typed errors: `no_text_extracted` (scanned file or no text layer), `document_unreadable` (corrupt or encrypted), and `extracted_text_too_large`. Each names the file.
- **Settings.** Add one **Extracted text limit** field to the AI API section of Global Settings, below the History size limit field. It holds a whole number of kilobytes, default 1024, and commits on Enter and on blur with no section save, with the inline error and restore behaviour of `GLS-FR-WQHT`. Add its load and save operations and storage with the matching specification of global settings storage.
- **Reaching the agent.** The text of the file goes into the section that holds the comment (`discussion_history` or `current_comment`), in the order and position of the comment's attachments, labelled with the file name and media type. It counts against the bound of that section and is cut like other history. The agent receives the whole extracted text, or the file is refused at post time. There is no chunked reading and no tool for it.
- **Embedded images.** Extract the images of a PDF or DOCX and place each one in its document position, between the text blocks around it. Use these fixed constants, not settings: skip an image with a side under 50 px, and keep at most 20 images per document in document order. A skipped tiny image leaves nothing in the output. An image above the cap is sent as safe metadata in its place (media type and position, no bytes). Image content follows the all-or-nothing rule of `AGC-FR-36` and the metadata fallback of `AGC-FR-37`.
- **Chip.** The chip of a PDF or DOCX shows name, type, and size only, in the pending strip and in the posted comment. It is keyboard reachable and named by the file name, as other chips (`CMT-FR-48`, `CMT-FR-49`). A posted attachment stays immutable (`CMT-FR-50`).
- **Specifications to change.** `core/CMS-comments-storage.md` (accepted media types, extraction at append, the three typed errors, read of the blob for extraction), `core/AGC-agent-conversations.md` (AGC-FR-09 and the image rules AGC-FR-35 to AGC-FR-37: document text and embedded images in place, session cache), `ui/CMT-comments.md` (CMT-FR-51 inline errors, chip), `ui/CVP-conversation-presentation.md` (file picker accepts `.docx`), `ui/GLS-global-settings.md` and `core/GSS-global-settings-storage.md` (the new field and its storage). Check `ai/CVL-conversation-loop.md` for any statement that non-image blobs reach the model as metadata alone.

## Expected deliverables

- Backend: extraction of PDF and DOCX text and embedded images, session cache, post-time validation with the three typed errors, and the new accepted media type.
- Agent input assembly: document text and images in comment position, with the image cap, the tiny-image filter, and the no-image fallback.
- Settings: the Extracted text limit field and its storage with a default limit of 1024KB.
- UI: DOCX in the Attach picker, chips for PDF and DOCX, and the inline error on refusal.
- Updated specifications from the list above, and tests: deterministic extraction (same bytes, same output), each typed error, the limit at its boundary, 20-image cap, tiny-image skip, in-place order, and the no-image fallback.