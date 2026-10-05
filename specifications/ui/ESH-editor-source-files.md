# Editor source files

**Spec code:** `ESH`

## Intent
What the Editor does with a file that is not a Markdown document. Every file the Editor opens is one of two shapes and the file's **name alone** decides which: a name ending `.md` is a Markdown file and gets the rich document surface, and every other name is a **source file**, edited on one plain-text surface with no rich rendering and no mode to leave. A source file is read as the language it is written in — its tokens carry the colours the filename's extension resolves, or failing that the ones the text itself does — so a Rust file reads as Rust rather than as prose that happens to contain braces. That colouring is a layer over the text and never the text: it takes no keystroke, changes no byte, and gives way to plain text wherever no grammar fits or none arrives in time. This is also how the project's untyped **plain text files** get somewhere to open, so anything the Project panel or search can surface has a tab. Out of scope: the colouring of Markdown, which keeps the presentation it already has on both of its surfaces; any grammar of this project's own, since the schemes are the installed library's and are named by its own identifiers; and every other part of the editing session — the buffer, the undo history, the write schedule, the find panels — which a source file shares unchanged with a Markdown file and which `EDT-editor.md` owns.

## UI contract boundary
- **Owned by the UI**: which of the two shapes of file the Editor has opened and which surface that shape carries, together with the syntax highlighting of a source file — the extension map and the case-insensitive matching that reads it, the autodetection fallback, the set of grammars the installation registers, the token layer and its registration to the editable text, the theme-aware token colours, and the deferral and cancellation that keep a large file responsive. All of it is client-side over the text the load contract already supplies: it introduces no backend operation, reads nothing from disk, and changes no byte a save writes. The shape is read from the file's own name, so it needs nothing of the artifact type the load contract carries and disagrees with nothing that type says.
- **Delegated to backend (abstract)**: `"load artifact contents by id"` and `"save artifact contents"` — owned by `../core/PST-project-storage.md` — serve a plain text file exactly as they serve a Markdown artifact, the backend resolving it as `kind = "text"` (per `../core/PST-project-storage.md` PST-FR-23). This surface introduces neither operation and invokes both on the same terms `EDT-editor.md` does.
- **Delegated to backend (abstract)**: `"append log records (records)"` — owned by `../core/LGC-logging.md` — carries the one diagnostic this surface emits, the record naming a grammar the extension map calls for that the installation does not register. It mutates nothing, gates nothing, and is part of no editing or saving path.

## Functional requirements
1. **ESH-FR-BLTT** Every file the Editor opens is one of two shapes, and the file's name alone decides which.
2. **ESH-FR-FKZB** A file whose final filename extension is `.md`, matched without regard to letter case, is a **Markdown file**: it carries the two editing modes, the WYSIWYG frontmatter region, and the round-trip fidelity of `EDT-editor.md` (EDT-FR-17, EDT-FR-18, EDT-FR-66–EDT-FR-69).
3. **ESH-FR-SSDV** Every other file the Editor opens is a **source file**: it is edited on the raw-text source surface alone, in one mode, with no rich rendering of its content anywhere in the tab.
4. **ESH-FR-LKNM** A source file's action cluster carries no mode toggle, there being one surface for a toggle to move between (per `EDT-editor.md` EDT-FR-16).
5. **ESH-FR-KOWE** The shape is read from the name and from nothing else, so an artifact whose file is not named `.md` is a source file and an untyped file that is so named is a Markdown file.
6. **ESH-FR-CXAI** The shape decides the surface and nothing beyond it: a source file is set on the same page over the same field, carries the same Find and Find & Replace panels in the same band, the same indentation convention and Tab behaviour, the same single undo history, the same dirty state and retained edit state, the same external-change resolution, and the same schedule on which an artifact writes itself (per `EDT-editor.md` EDT-FR-63, EDT-FR-37, EDT-FR-38, EDT-FR-22, EDT-FR-04, EDT-FR-28, EDT-FR-70, per `EFR-editor-find-replace.md` EFR-FR-ABHF, and per `EXC-editor-external-change.md` EXC-FR-VTUH).
7. **ESH-FR-ATDS** The Editor opens a **plain text file** — a file the project scan surfaces that carries no artifact type — reached from the Project panel's **All files** lens (per `LIB-library.md` LIB-FR-12), a Files result in search (per `SCH-search.md` SCH-FR-09), or the creation of one in the New File window, which opens it here the moment it exists (per `NFI-new-file.md` NFI-FR-12).
8. **ESH-FR-TLJZ** A plain text file loads, edits, and saves through the same `"load artifact contents by id"` and `"save artifact contents"` an artifact uses, the backend resolving it as `kind = "text"` (per `../core/PST-project-storage.md` PST-FR-23).
9. **ESH-FR-VASA** A plain text file carries the same dirty state, retained edit state, and undo history as an artifact, and writes itself on exactly the terms an artifact does (per `EDT-editor.md` EDT-FR-28, EDT-FR-70).
10. **ESH-FR-DCQX** What surface a plain text file opens on is decided by its name and not by its want of a type, so an untyped file named `.md` is a Markdown file and any other is a source file.
11. **ESH-FR-ROWK** Nothing else about the tab's chrome distinguishes a plain text file: the action cluster carries what the file's shape gives it and nothing further about the type it did or did not resolve as.
12. **ESH-FR-BABL** A source file's text is syntax-highlighted wherever a language resolves for it: the tokens that language recognises carry colours of the theme's own and every other character renders as ordinary text.
13. **ESH-FR-KQVP** Highlighting reaches the source surface of a source file and nothing else.
14. **ESH-FR-ZCKP** A Markdown file receives no highlighting in either of its modes, its raw-text mode included, so the raw Markdown an author reads is the raw Markdown they have always read.
15. **ESH-FR-RKHL** The WYSIWYG surface's rendering of Markdown, of inline and fenced code, and of the frontmatter region is untouched by any of this (per `EDT-editor.md` EDT-FR-18, EDT-FR-21).
16. **ESH-FR-XXYW** A source file for which no language resolves renders plain, and is edited exactly as one that highlights.
17. **ESH-FR-CFEQ** A source file's language is resolved from the file alone, in a fixed order whose first answer wins.
18. **ESH-FR-PBFJ** The first step is the file's final filename extension, matched case-insensitively against the map below. The map is at minimum:
    - C — `.c`, `.h`
    - C++ — `.cc`, `.cpp`, `.cxx`, `.hh`, `.hpp`, `.hxx`
    - C# — `.cs`
    - Go — `.go`
    - Rust — `.rs`
    - TypeScript — `.ts`, `.tsx`, `.mts`, `.cts`
    - JavaScript — `.js`, `.jsx`, `.mjs`, `.cjs`
    - Java — `.java`
    - YAML — `.yaml`, `.yml`
    - TOML — `.toml`
    - JSON — `.json`
    - SQL — `.sql`
    - Scala — `.scala`, `.sc`
    - Kotlin — `.kt`, `.kts`
    - Dart — `.dart`
    - Swift — `.swift`
    - Objective-C — `.m`, `.mm`
19. **ESH-FR-QBZJ** Where the extension is not in the map, where the extension is unknown, and where the file has no extension at all, the text itself is put to the library's autodetection — an ordinary text file such as a `.txt` among them.
20. **ESH-FR-YTPJ** Where autodetection yields no usable language, the file renders plain, as an empty file does, there being nothing in it to detect.
21. **ESH-FR-TVUT** Autodetection never overrides an extension the map names, so the map's answer is deterministic and a given filename always highlights the same way whatever it happens to contain.
22. **ESH-FR-JHPR** Neither resolution step is ever run over a Markdown file, which is not highlighted at all.
23. **ESH-FR-SNJU** Each scheme is named by the installed library's own language identifier or one of its aliases, so several extensions mapping to one scheme highlight identically and a scheme is never invented under a name of this project's own.
24. **ESH-FR-LMNI** Highlighting is performed by the project's single `highlight.js` installation and by the grammars registered in it: the library's **common** language set together with **Dart** and **Scala**, which that set omits and the map calls for.
25. **ESH-FR-QFCD** No second highlighting library and no hand-written grammar stands beside it; where the library needs an adapter to yield tokens for an editable surface, that adapter arranges what the library produced and parses nothing itself.
26. **ESH-FR-XNRV** A scheme the map names that the installation does not register leaves the files that map to it plain, and produces one `WARN` record through `"append log records (records)"` under the `frontend` domain, naming the extension and the missing scheme, at most once per scheme in a session.
27. **ESH-FR-HHCR** That record is a diagnostic and not an interruption: nothing is blocked, no modal is raised, no indication is placed over the document, and the file opens, edits, and saves exactly as it otherwise would.
28. **ESH-FR-YTNN** The highlighted surface is the plain editable text with a token layer behind it. The editable control holds the text and nothing else; the layer reproduces that text character for character with its tokens marked, registered to the control so that the two never drift and following its scroll in both axes.
29. **ESH-FR-VNPW** The layer takes no part in editing: it receives no pointer, keyboard, selection, clipboard, drag, or composition event, and the editable control remains the sole target of every one of them, IME composition included.
30. **ESH-FR-RNFF** Nothing replaces the editable text with rendered markup or token elements — the text the author types into is never the tokens — and a surface that cannot carry the layer carries the plain editable text alone rather than a partial rendering of it.
31. **ESH-FR-BPLJ** Highlighting is presentation-only in the strictest sense: it changes no character of the buffer, no byte a save writes, no line ending, no dirty state, no position in the undo history, no caret, no selection, no scroll position, and nothing about the schedule an artifact writes itself on (per `EDT-editor.md` EDT-FR-40, EDT-FR-04, EDT-FR-23, EDT-FR-70).
32. **ESH-FR-GXUX** The find panels' match marking shares this same layer and composes with the token colours: a match reads as a match over any token colour, and the current match stays distinct from the rest (per `EFR-editor-find-replace.md` EFR-FR-DOQR).
33. **ESH-FR-UXQJ** The match set itself is computed over the buffer rather than over anything the layer renders (per `EFR-editor-find-replace.md` EFR-FR-DDUX, EFR-FR-DKQT).
34. **ESH-FR-VITW** A source file sets its text in the **Source code** role whether it highlights or renders plain, so a language resolving or failing to resolve moves no character on screen (per `EDT-editor.md` EDT-FR-62, per `OVW-overview.md` OVW-FR-13).
35. **ESH-FR-YNIV** Token colours are the application's own theme-aware semantic styles rather than colours a grammar or a stylesheet shipped with the library carries, and every token role is legible against the page in the light theme and in the dark one, and against the marking of a find match.
36. **ESH-FR-CAKG** A theme change while the file is open — the user pinning light or dark, and the OS switching under the "system" preference (per `OVW-overview.md` OVW-FR-09, OVW-FR-10) — recolours the layer in place, disturbing neither the text, the caret, the selection, nor the scroll position.
37. **ESH-FR-MJRH** A source file is editable from the moment it opens and is typed into immediately, rendering plain for however long resolving and tokenising it takes, so a large document shows its text rather than an empty or frozen surface.
38. **ESH-FR-VUVO** Work whose result no longer describes what is on screen is discarded rather than applied: a result computed for text the buffer has moved on from, for a file the tab no longer shows, or under a resolution since superseded never reaches the layer, so a late result cannot overwrite the current file's colouring.
39. **ESH-FR-GCEN** Tokenising that fails leaves the file plain and editable and is reported as the diagnostic record rather than as an error over the document; in every such case the plain text is the fallback and the surface stays usable throughout.
40. **ESH-FR-WWQG** What assistive technology reads, and what a copy, a cut, or a drag carries out of the surface, is the file's own plain text.
41. **ESH-FR-LXQR** The token layer is hidden from the accessibility tree and contributes no element, no generated label, and no character to a selection or to the clipboard, so a screen-reader user reading a highlighted file hears exactly what they hear reading a plain one and a paste elsewhere carries the source and not its colouring.

## Non-functional requirements
- Resolution, tokenising, and recolouring run client-side over text already in memory: none of it reads the filesystem or the network.
- A file of any size opens and accepts input immediately; highlighting is best-effort work layered on afterwards and never a precondition for editing.
- The token layer adds no element to the accessibility tree and no character to the clipboard, so assistive technology and copy-paste behave identically over a highlighted and a plain file.
- Token colours meet the same contrast floor in both themes, including where a find match is marked over them.
