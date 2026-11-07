# Export dialog

**Spec code:** `EXD`

## Intent
The modal dialog in which the author exports a DOC artifact to a `.pdf` or a `.docx` file, and the **export request card** that brings an agent's export request into it. An author who writes a document in the IDE must give it to people who do not use the IDE. The author reaches the dialog from the action control of the document's Markdown Editor tab, or from the card an agent posted in a conversation (`../tools/RDX-request-document-export-tool.md`). Out of scope: the conversion and the request records (`../core/DXP-document-export.md`), and the action control itself (`ACT-action-control.md`).

## Functional requirements
1. **EXD-FR-UQNO** The action control of a Markdown Editor tab whose file is a DOC artifact (per `../core/DXP-document-export.md` DXP-FR-LNCQ) carries the entry **Export to...** (per `ACT-action-control.md` ACT-FR-04), whichever surface opened the tab (per `TAB-tabs.md` TAB-FR-SZGD). No other tab carries it.
2. **EXD-FR-CDYS** Activating **Export to...** first writes the file's buffer through `"save artifact contents"` at once (per `EDT-editor.md` EDT-FR-70), so the file on disk equals the buffer. The dialog opens only after the write succeeds.
3. **EXD-FR-CBIW** A write that fails opens no dialog, and the author sees its error in an error notification toast (per `NTF-notifications.md`).
4. **EXD-FR-DPHQ** The dialog is modal. It shows the file name, a **Format** choice of `.pdf` and `.docx`, a checkbox **Include frontmatter as a metadata table**, a **Destination** row with the chosen path and a **Choose…** button, and the buttons **Cancel** and **Export**.
5. **EXD-FR-IVAE** The format is `.pdf` when the dialog opens, or the format of the export request that opened it. The frontmatter checkbox is off when the dialog opens.
6. **EXD-FR-LQAW** **Choose…** invokes `"browse for save path"` with the file's base name and the extension of the chosen format. A cancel keeps the earlier destination. A change of format after a destination is chosen replaces the destination's extension with the extension of the new format.
7. **EXD-FR-BKFL** **Export** is disabled while no destination is chosen, and states why on hover.
8. **EXD-FR-XNLZ** The dialog is one of the main window's mutually exclusive floating overlays (per `SNV-shell-navigation.md` SNV-FR-56).
9. **EXD-FR-ESQB** **Export** invokes `"export document (path, format, include frontmatter, destination, export id)"` with a new export id. While it runs the dialog stays open, every control of the dialog is disabled, and Escape does nothing.
10. **EXD-FR-QWZO** While an export runs, the dialog shows its progress from the `"document export progress"` events of its export id: the stage as text, and the count of diagrams processed in the `diagrams` stage.
11. **EXD-FR-YTQG** An export that succeeds with no warning closes the dialog and shows a success notification toast that names the file.
12. **EXD-FR-YALT** An export that succeeds with warnings keeps the dialog open, states that the file was written and which diagrams stayed as code, and shows one **Close** button in place of Cancel and Export.
13. **EXD-FR-RNJL** An export that fails keeps the dialog open, shows the error inline, and enables the controls again, so **Export** tries again and **Cancel** closes the dialog.
14. **EXD-FR-SVLO** Escape and **Cancel** close the dialog while no export runs.
15. **EXD-FR-OOTQ** Another overlay that opens closes the dialog, also while an export runs (per `SNV-shell-navigation.md` SNV-FR-56). The export continues in the backend. When it ends, a notification toast states its outcome, and a dialog opened from a card resolves its request with that outcome (EXD-FR-DHAN).
16. **EXD-FR-UQUD** A comment with an `export_request` attachment (per `../core/CMS-comments-storage.md` CMS-FR-OKDW) shows an **export request card** under its body. The card shows the file name, the requested format or "any format", and the state of the request read through `"get export request (id)"`.
17. **EXD-FR-WLUG** The card follows `"export request changed"` for its request. A `pending` card has an **Export…** button. An `exported` card shows the destination, a `cancelled` card says it was cancelled, and a `failed` card shows the error. A card that is not `pending` has no button.
18. **EXD-FR-UDHD** **Export…** on a card writes at once any pending write of the file's editing session (per `EDT-editor.md` EDT-FR-70), and then opens the dialog for the request's file with the request's format. A write that fails opens no dialog, as EXD-FR-CBIW states.
19. **EXD-FR-DHAN** A dialog opened from a card resolves the request through `"resolve export request (id, outcome, destination, error)"` when it closes: `exported` with the destination after a success, `cancelled` when the author closes it with no failed attempt, and `failed` with the last error when the author closes it after a failed attempt.
20. **EXD-FR-VFFP** A resolution that returns a `commentId` brings the agents back on the terms a decision on a proposal does (per `DCR-draft-change-review.md` DCR-FR-15). A resolution that fails, or that returns `commentFailed: true`, shows an error notification toast that says the conversation was not told.
21. **EXD-FR-MRTI** The dialog and the card log under the `frontend` domain: the open, the export start, its success with the warning count, its failure with the typed error, and each resolution with its outcome. No record carries the path, the destination, or document content.

## Wireframes

```
┌─ Export document ─────────────────────────────────────┐
│  guide.md                                             │
│                                                       │
│  Format       (•) .pdf    ( ) .docx                   │
│  [ ] Include frontmatter as a metadata table          │
│                                                       │
│  Destination  /Users/me/Desktop/guide.pdf  [Choose…]  │
│                                                       │
│  Rendering diagrams 2 of 5…                           │
│                                   [Cancel]  [Export]  │
└───────────────────────────────────────────────────────┘
```

```
┌───────────────────────────────────────────────┐
│ ⇩ Export request · guide.md · PDF             │
│   Waiting for you             [ Export… ]     │
└───────────────────────────────────────────────┘
```

- Layout notes: the dialog is centred over the window on a scrim. The progress line and the error line sit above the buttons. The card sits under the comment body, at the width of the comment.

## UI contract boundary
- **Owned by the UI**: the **Export to...** entry and its visibility; the dialog, its controls, their enablement, the default values, the extension rule, the progress text, and the error text; the card, its states, and its button; the choice of outcome when a dialog opened from a card closes; and the dispatch of agent turns after a resolution.
- **Delegated to backend (abstract)**:
  - `"save artifact contents"` — owned by `../core/PST-project-storage.md` — writes the buffer before the dialog opens (EXD-FR-CDYS).
  - `"browse for save path"` — owned by `../core/FSA-filesystem-access.md` — opens the OS-native save dialog (EXD-FR-LQAW).
  - `"export document (path, format, include frontmatter, destination, export id)"` and the event `"document export progress"` — owned by `../core/DXP-document-export.md`.
  - `"get export request (id)"`, `"resolve export request (id, outcome, destination, error)"`, and the event `"export request changed"` — owned by `../core/DXP-document-export.md`.
  - `"dispatch agent turn"` — owned by `../core/AGC-agent-conversations.md` — brings the agents back after a resolution (EXD-FR-VFFP).

## Non-functional requirements
- The dialog stays usable at the minimum window size the shell supports (≥1280×800).
- The window stays responsive while an export runs, because the conversion runs in the backend off the thread that serves the window.
