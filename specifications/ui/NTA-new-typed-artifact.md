# New typed artifact

**Spec code:** `NTA`

## Intent
The modal surface for creating a **typed artifact file** in the open project: the user picks a location, types the file's complete name — extension and all — chooses one of the built-in artifact types, and confirms to write an empty file carrying that type. It exists because a file's type is otherwise a consequence of where it was put — a path convention, or the folder-scope assignment of the folder it landed in — and an author who already knows they are writing a Scenario should be able to say so while creating the file rather than tag it afterwards from the Project context menu. It is the single creation experience behind both the File menu's **New Artifact** item (`SNV-shell-navigation.md` SNV-FR-24) and the Project context menu's folder-scoped **New Artifact** entry (`LCM-library-context-menu.md` LCM-FR-08), so a typed artifact is born the same way regardless of where the user started, and the new file opens in its natural surface the moment it exists so creating it and writing in it are one motion. It is the typed sibling of the New File window (`NFI-new-file.md`) and is built from that window's own primitives, validation, folder list, focus, keyboard, overlay, and accessibility behaviour rather than from a second file-creation experience of its own. Out of scope: **drafts** — this window creates a project file and nothing else, never invokes a draft operation, never writes under `.synthesis/drafts/`, and never applies the project's draft template, a draft being created only from the Drafts panel (`DRP-drafts-panel.md` DRP-FR-06, DRP-FR-26); starting content and templates, so the file is always born empty; and creating the folders on the way to the file, which belongs to `NFW-new-folder.md`.

## User stories
- As an author, I want to say what kind of artifact I am creating at the moment I create it, so that the file is the type I meant regardless of where in the project I put it.
- As an author, I want to type the whole filename including its extension in one field, so that naming an artifact works the way it does in every other tool I use.
- As an author, I want the folder I right-clicked to be the starting location without being locked to it, so that a mis-click costs me a re-pick rather than a re-invocation.
- As an author, I want the artifact I just created to open in the surface it belongs in — a Flow in the Flow tab, everything else in the Editor — so that creating it and starting to work in it are a single motion.
- As an author, I want a creation that fails to leave nothing behind — no stray file and no type recorded for a file that was never written — so that I retry against the project I started from.
- As an author, I want to back out of creation at any time without leaving anything behind, so that opening the window is never a commitment.

## Wireframes
```
                 ┌──────────────────────────────────────────┐
                 │  New Artifact                           ✕ │
                 ├──────────────────────────────────────────┤
                 │  Location       [ specifications      ▾ ] │
                 │  Name           [ login.scenario.md      ]│
                 │  Artifact Type  [ Select a type       ▾ ] │
                 ├──────────────────────────────────────────┤
                 │                      [ Cancel ] [ Create ]│
                 └──────────────────────────────────────────┘

      Artifact Type ▾ (expanded):
                 │  ┌────────────────────┐                  │
                 │  │ Skill              │                  │
                 │  │ Agent              │                  │
                 │  │ Prompt             │                  │
                 │  │ Spec               │                  │
                 │  │ Flow               │                  │
                 │  │ Instructions       │                  │
                 │  │ Scenario           │                  │
                 │  │ Scratchpad         │                  │
                 │  └────────────────────┘                  │

      A creation that failed:
                 │  Artifact Type  [ Scenario            ▾ ] │
                 │  “login.scenario.md” is already in        │
                 │  specifications. Rename it or pick        │
                 │  another location.                        │
```
- Layout notes: the window is a centered modal overlay, not a tab or panel (NTA-FR-01), and is re-centered on the screen each time it opens (NTA-FR-02). It carries exactly three inputs, top to bottom: **Location** (required, pre-filled), **Name** (required, empty), and **Artifact Type** (required, unchosen). **Create** is the confirming action and **Cancel**/✕ the dismissing one; **Create** is disabled while the **Name** is empty or invalid or no type has been chosen (NTA-FR-09). The **Artifact Type** list holds exactly the eight built-in artifact types (`../core/ASC-artifact-scanning.md` ASC-FR-02) and offers no unset entry, unlike the New Folder window's own type control (`NFW-new-folder.md` NFW-FR-05), because this window's whole purpose is to settle the type. There is no content editor, no AI-prompt block, no draft control, and no template control: an artifact is named, placed, and typed in this window and authored in the surface it opens in. A creation error renders inline directly beneath the inputs, above the action row, with every input left as the user filled it in. The frame, spacing, overlay, dismissal, focus order, and the **Location** list are the New File window's (`NFI-new-file.md`), so the two read as one system.

## UI contract boundary
- **Owned by the UI**: the modal layout, its centering, and its dismissal; the three inputs (Location, Name, Artifact Type) and their enablement; the **Location** single-select over the project's existing folders and the project root, and the fact that the project root is its default (NTA-FR-04); the treatment of **Name** as the file's complete basename, extension included, with nothing appended or required (NTA-FR-05); the **Artifact Type** single-select over exactly the eight built-in types with no unset entry (NTA-FR-06); client-side **Name** validation and the **Create** enablement rule (NTA-FR-09); the two invocation points (File menu `SNV-shell-navigation.md` SNV-FR-24, Project context menu `LCM-library-context-menu.md` LCM-FR-08) and the location seeding each performs (NTA-FR-07, NTA-FR-08); the routing of the created file to its natural surface and the reveal-and-select request issued to the Project panel (NTA-FR-13); and the inline display of a creation error (NTA-FR-16). The window reads the project's folder set from the tree the Project panel already holds; it never scans the filesystem itself, and it decides nothing about the artifact type beyond carrying the user's choice to the backend.
- **Delegated to backend (abstract)**:
  - `"create typed file (location, name, artifact type)"` — creates an empty file and records the chosen type as that file's file-scope assignment, as one creation transaction; owned by `../core/PST-project-storage.md` (PST-FR-29). Returns the new file node so the UI can open and reveal it.
  - `"open artifact by id"` — opens the newly-created file in its natural surface (owned by `../core/PST-project-storage.md` PST-FR-08), routed to Editor or Flow per `LIB-library.md` LIB-FR-03.
  - The new node also reaches the Project panel through the `"project tree changed"` event (`../core/ASC-artifact-scanning.md` ASC-FR-10); the window does not push the node into the tree itself.
  - No draft operation is delegated here. `"create draft (name)"` and every other operation of `../core/DRS-draft-storage.md` are outside this window's contract (NTA-FR-14).

## Functional requirements
1. **NTA-FR-01** New Artifact is a modal overlay action surface — not a tab and not a panel. As a floating overlay it is mutually exclusive with the other overlays of the main window: opening it closes any other open overlay rather than coexisting with it. It is not the New Artifact **tab**, which is a draft's workspace and no overlay at all (`NAW-new-artifact.md` NAW-FR-01); the two share a name in the menus and nothing else.
2. **NTA-FR-02** The window is positioned at the center of the screen each time it opens, regardless of where any prior invocation left it.
3. **NTA-FR-03** The window presents exactly three creation inputs — **Location**, **Name**, and **Artifact Type** — where **Name** and **Artifact Type** are required and must be supplied by the user and **Location** is required but always arrives pre-filled (NTA-FR-04). It offers no Markdown editor, no initial-content editor, no AI-prompt block, no draft control, and no draft-template control.
4. **NTA-FR-04** **Location** is a single-select over the project root and every folder that exists under it, listing folders the Project panel's active lens currently hides as well as those it shows (per `LIB-library.md` LIB-FR-09), so an artifact may be created inside an empty or unclassified one. Its default value is the project root — the directory of the project's active worktree (per `../core/WTC-worktree-context.md` WTC-FR-03) — and the user may change it in every flow.
5. **NTA-FR-05** **Name** is the file's complete basename, extension included: the window neither requires an extension nor appends, substitutes, or privileges one, and it derives no extension from the chosen artifact type, so `overview.md`, `login.flow`, and `Makefile` are all expressible as typed. The extension is whatever trails the name the user entered, and no extension is rejected on the grounds of not being recognised or of not matching the type.
6. **NTA-FR-06** **Artifact Type** is a single-select whose options are exactly the eight built-in artifact types (`../core/ASC-artifact-scanning.md` ASC-FR-02) and nothing else. It offers **no unset entry and has no default**: the control opens with nothing chosen and holds no type until the user picks one, so this flow never creates an untyped file. A file with no type is created in the New File window instead (`NFI-new-file.md` NFI-FR-11).
7. **NTA-FR-07** In the File-menu flow, **Location** starts at the project root (NTA-FR-04), **Name** starts empty, and **Artifact Type** starts unchosen; the location is the user's to change.
8. **NTA-FR-08** In the Project context-menu flow, **Location** starts at the folder the menu was opened on (`LCM-library-context-menu.md` LCM-FR-08) and remains editable, so the right-clicked folder is a starting point rather than a commitment. **Artifact Type** still starts unchosen, including when the right-clicked folder carries a folder-scope assignment of its own: the window never seeds a type from the location.
9. **NTA-FR-09** **Name** is validated client-side as non-empty and free of path separators; **Create** is disabled while the **Name** is empty or invalid, or while no **Artifact Type** has been chosen. No creation call is therefore made with a malformed name or with no type, and one invocation creates exactly one file rather than a file plus the folders leading to it (mirroring the constraint of `../core/PST-project-storage.md` PST-FR-29).
10. **NTA-FR-10** Confirming with **Create** invokes `"create typed file (location, name, artifact type)"` once with the current inputs and, on success, closes the window. While a call is in flight **Create** is disabled, so one confirmation is one call and a second is not accepted until the first has returned.
11. **NTA-FR-11** The created file is empty — zero bytes on disk — because the window captures no content to write into it. A file created with the type Flow therefore opens on the empty graph its empty body deserializes to (per `FLO-flow.md` FLO-FR-04 and `../core/FGV-flow-graph-validation.md` FGV-FR-05) rather than in an error state.
12. **NTA-FR-12** The chosen type is recorded as a **file-scope** assignment for the new file (per `../core/ASC-artifact-scanning.md` ASC-FR-05, scope = file), which is the highest level of the classification precedence (ASC-FR-06). The file therefore resolves to exactly the type the user chose, with `type_source = "assigned"`, whatever its path would otherwise infer and whatever an ancestor folder would otherwise lend it.
13. **NTA-FR-13** On a successful creation the new file opens in its natural surface — a **Flow** tab when the chosen type is Flow (per `LIB-library.md` LIB-FR-03, `FLO-flow.md` FLO-FR-01), and an **Editor** tab for every other type, on the Markdown or source surface its own name decides (per `ESH-editor-source-files.md` ESH-FR-BLTT) — via `"open artifact by id"`, and is revealed and selected in the Project panel (`LIB-library.md` LIB-FR-18).
14. **NTA-FR-14** This window creates **no draft**. It never invokes `"create draft"` or any other operation of `../core/DRS-draft-storage.md`, it writes nothing under `.synthesis/drafts/`, it opens no New Artifact tab (`NAW-new-artifact.md` NAW-FR-01), and it never reads or copies the project's draft template (`../core/PSS-project-settings-storage.md` PSS-FR-21), which belongs to draft creation alone (`../core/DRS-draft-storage.md` DRS-FR-39). One confirmation writes exactly one project file and records exactly one type assignment, and nothing else is created anywhere.
15. **NTA-FR-15** The user may dismiss the window — via its close control, **Cancel**, Escape, or an outside click — at any time before confirming; dismissal cancels the creation, invokes no backend operation, leaves nothing on disk and no assignment recorded, and returns focus to the surface the window was invoked from.
16. **NTA-FR-16** A failure reported by `"create typed file"` — a name collision with an existing entry in the destination, a refused type assignment, or an I/O failure — is surfaced inline as an actionable message naming what went wrong and what to change. The window stays open with **Location**, **Name**, and **Artifact Type** exactly as the user left them, so the user corrects one value and retries rather than starting over. Nothing of the attempt survives it: the backend leaves neither a partially-created file nor a type assignment for a file that does not exist (per `../core/PST-project-storage.md` PST-FR-29), so a retry meets the project exactly as the first attempt found it.

## Non-functional requirements
- The window opens with keyboard focus on the **Name** input, as the New File window does, because it is the first value the user must supply.
- The whole window is operable from the keyboard: both selects open, list, and commit without a pointer, focus is trapped inside the frame while it is open, and dismissal returns focus to the invoking surface (NTA-FR-15). Each input carries a label naming it and its required state, and an inline creation error is associated with the window so it is announced when it appears.
- The single-overlay rule of NTA-FR-01 is enforced by opening logic, not by listener coordination: the window assumes no other overlay is mounted while it is open.
- The **Location** select is populated from the tree already in memory and the **Artifact Type** select from the fixed built-in set, so opening the window issues no backend call and costs no filesystem walk.
- The window carries no expandable region and no optional block, so its height is fixed and it never reflows while the user fills it in; an inline error is the one thing that adds to it.
- The window imposes no extension allowlist or denylist (NTA-FR-05), and it requires no relationship between the chosen type and the file's extension, so the range of artifacts a project can hold is never narrowed by this surface.
