## Intent

Users need a way to export the DOC artifact they work on to `.pdf` or `.docx` from inside the IDE. An export is available to the user by the action control, and to the conversational agent by a tool.

Add one entry, `Export to...`, to the action control (the flying menu in the bottom-right corner, `specifications/ui/ACT-action-control.md`). The entry shows only on the Markdown Editor tab of a DOC artifact: not on other Editor tabs, Document viewer tabs, drafts, Diff tabs, or History tabs. It comes directly after **Discuss**.

## User journey

- User works on a DOC artifact in its Markdown Editor tab.

- User opens the action control in the bottom-right corner of the tab.

- User selects `Export to...`. The IDE first saves the document, so the live buffer and the file on disk are the same. If the save fails, no dialog opens and the user sees the error.

- A modal dialog opens. It holds: the format (`.pdf` or `.docx`); a choice to include the YAML frontmatter as a metadata table (off by default); and a destination chosen with the OS-native save-file selector. The confirm button stays disabled until a destination is chosen. The dialog is one of the mutually exclusive overlays (SNV-FR-56).

- User confirms. The dialog stays open, disables its controls, and shows progress while the backend converts the saved file. On success the dialog closes. On failure it stays open, shows the error, and lets the user retry or cancel.

### Journey of the agent tool

- User asks the agent in a discussion, for example "export this document to pdf".
- The agent calls the export tool. The tool records an export request, posts it in the conversation as an agent comment with a card, and ends the agent turn. It returns at once and holds no timer or open request, as the change-proposal tools do.
- The user opens the card at any time. It opens the same Export dialog, with the format pre-selected if the agent gave one. The user chooses the destination and the frontmatter option, and confirms.
- The outcome (exported with the file path, cancelled, or failed) is posted in the conversation and brings the agent back, as the decision on a proposal does.

## Requirements

- The export converts the saved Markdown file on disk, never the live buffer. Export runs in the Rust backend, off the UI thread, with progress reported to the dialog.
- Mermaid diagrams are rendered to images during the export, with the Rust `merman` crate (not a webview). The image replaces the source code. If one diagram fails to render, its source stays as a code block and the dialog reports a warning at the end.
- The export is a tool of the conversational agent, which follows `TLC-tool-conventions.md` and is attached by `CVL-conversation-loop.md` (CVL-FR-08). It takes one document identifier, either the project-relative path of the DOC file or its Documents collection ID, and an optional format (`pdf` or `docx`) that the user asked for. It can export any DOC artifact, not only the document of the discussion. It takes no destination. It refuses a path or ID that is not a DOC artifact, is not a Markdown file, or is not in the project. DOC is not an artifact type that agents see, so agents get the ID from the Documents tools.
- Custom markup converts to the same visual result as the rich editor: vertical-break spacers become vertical space (all four levels), and alignment markers (center, right) are applied to their blocks. Other HTML comments (chips in the editor) are dropped from the output.
- Frontmatter: when the user chooses to include it, it renders as a metadata table that looks like the editor's frontmatter presentation (EDT-FR-18). When not chosen, it is omitted.
- `docx-rs`, `merman`, `markdown2pdf` crates are expected to be used as needed to deliver this feature.
- Specifications to change: `ui/ACT-action-control.md` (ACT-FR-04 and ACT-FR-05, the action set and its order); `ui/EDT-editor.md` (the action control of a DOC Editor tab, EDT-FR-64 and EDT-FR-65); `ui/SNV-shell-navigation.md` (SNV-FR-56, the overlay list); `ai/CVL-conversation-loop.md` (CVL-FR-08, the tool list). Add: a tool spec in `tools/` for the export tool, a UI spec for the Export dialog and the request card, and a core spec for the conversion and its Tauri command. Check the spec of the comment card renderer (`ui/CMT-comments.md`) for the new card.