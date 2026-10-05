# New File

**Spec code:** `NFI`

## Intent
The modal surface for creating a plain file in the open project: the user picks a location, types the file's complete name — extension and all — and confirms to write it to disk. It exists because a project is more than its artifacts: a `.ts` module, a `tsconfig.json`, a `.gitignore`, a bare `Makefile` are all part of the material a user shapes, and creating one should not mean leaving Synthesis for a terminal or a file manager. It is the single creation experience behind both the File menu's **New File** item (`SNV-shell-navigation.md` SNV-FR-24) and the Project context menu's folder-scoped **New File** entry (`LCM-library-context-menu.md` LCM-FR-10), so a file is born the same way regardless of where the user started, and the new file opens in its editing surface the moment it exists so creating it and writing in it are one motion. Out of scope: choosing an artifact type — a regular file is created untyped, and whatever type it later resolves to comes from the project's own path inference and folder-scope inheritance (`../core/ASC-artifact-scanning.md` ASC-FR-06) rather than from this window, a file whose type the author wants to settle at creation being created in the New Artifact window instead (`NTA-new-typed-artifact.md`); starting content and templates, so the file is always born empty; and creating the folders on the way to the file, which belongs to `NFW-new-folder.md`.

## User stories
- As a user, I want to create a plain file such as `vite.config.ts` or `.gitignore` from inside Synthesis rather than reaching for a terminal, so that shaping a project's non-artifact files is part of the tool I am already working in.
- As a user, I want to type the whole filename including its extension in one field, so that naming a file works the way it does in every other tool I use.
- As a user, I want the file I just created to open in an editor immediately, so that creating it and starting to write in it are a single motion.
- As a user, I want the folder I right-clicked to be the starting location without being locked to it, so that a mis-click costs me a re-pick rather than a re-invocation.
- As a user, I want to back out of creation at any time without leaving a stray file behind, so that opening the window is never a commitment.

## Wireframes
```
                 ┌──────────────────────────────────────────┐
                 │  New File                              ✕ │
                 ├──────────────────────────────────────────┤
                 │  Location   [ src/hooks               ▾ ] │
                 │  Name       [ useProjectFolders.ts       ] │
                 ├──────────────────────────────────────────┤
                 │                      [ Cancel ] [ Create ]│
                 └──────────────────────────────────────────┘

      Location ▾ (expanded):
                 │  ┌────────────────────┐                  │
                 │  │ <project root>     │  ← default        │
                 │  │ src                │                  │
                 │  │ src/components     │                  │
                 │  │ src/hooks          │                  │
                 │  │ specifications/ui  │                  │
                 │  │ …                  │                  │
                 │  └────────────────────┘                  │
```
- Layout notes: the window is a centered modal overlay, not a tab or panel (NFI-FR-01), and is re-centered on the screen each time it opens (NFI-FR-02). It carries exactly two inputs, top to bottom: **Location** (required, pre-filled) and **Name** (required, empty). **Create** is the confirming action and **Cancel**/✕ the dismissing one; **Create** is disabled while the **Name** is empty or invalid (NFI-FR-08). There is no artifact-type control, no content editor, and no AI-prompt block — a regular file is named and placed, not typed or authored, in this window.

## UI contract boundary
- **Owned by the UI**: the modal layout, its centering, and its dismissal; the two inputs (Location, Name) and their enablement; the **Location** single-select over the project's existing folders and the project root, and the fact that the project root is its default (NFI-FR-04); the treatment of **Name** as the file's complete basename, extension included, with nothing appended or required (NFI-FR-05); client-side **Name** validation; the two invocation points (File menu `SNV-shell-navigation.md` SNV-FR-24, Project context menu `LCM-library-context-menu.md` LCM-FR-10) and the location seeding each performs (NFI-FR-06, NFI-FR-07); the routing of the created file to its natural surface and the reveal-and-select request issued to the Project panel (NFI-FR-12); and inline display of a creation error. The window reads the project's folder set from the tree the Project panel already holds; it never scans the filesystem itself and never assigns or infers an artifact type.
- **Delegated to backend (abstract)**:
  - `"create file (location, name)"` — creates an empty file with no type assignment; owned by `../core/PST-project-storage.md` (PST-FR-25). Returns the new file node so the UI can open and reveal it.
  - `"open artifact by id"` — opens the newly-created file in its natural surface (owned by `../core/PST-project-storage.md` PST-FR-08), routed to Editor or Flow per `LIB-library.md` LIB-FR-03.
  - The new node also reaches the Project panel through the `"project tree changed"` event (`../core/ASC-artifact-scanning.md` ASC-FR-10); the window does not push the node into the tree itself.

## Functional requirements
1. **NFI-FR-01** New File is a modal overlay action surface — not a tab and not a panel. As a floating overlay it is mutually exclusive with the other overlays of the main window: opening it closes any other open overlay rather than coexisting with it.
2. **NFI-FR-02** The window is positioned at the center of the screen each time it opens, regardless of where any prior invocation left it.
3. **NFI-FR-03** The window presents exactly two creation inputs — **Location** and **Name** — where **Name** is required and must be supplied by the user and **Location** is required but always arrives pre-filled (NFI-FR-04). It offers no artifact-type control, no initial-content editor, and no AI-prompt block.
4. **NFI-FR-04** **Location** is a single-select over the project root and every folder that exists under it, listing folders the Project panel's active lens currently hides as well as those it shows (per `LIB-library.md` LIB-FR-09), so a file may be created inside an empty or unclassified one. Its default value is the project root — the directory of the project's active worktree (per `../core/WTC-worktree-context.md` WTC-FR-03) — and the user may change it in every flow.
5. **NFI-FR-05** **Name** is the file's complete basename, extension included: the window neither requires an extension nor appends, substitutes, or privileges one, so `helpers.ts`, `.gitignore`, `Makefile`, and `notes.tar.gz` are all expressible as typed. The extension is whatever trails the name the user entered, and no extension is rejected on the grounds of not being recognised.
6. **NFI-FR-06** In the File-menu flow, **Location** starts at the project root (NFI-FR-04) and **Name** starts empty; the location is the user's to change.
7. **NFI-FR-07** In the Project context-menu flow, **Location** starts at the folder the menu was opened on (`LCM-library-context-menu.md` LCM-FR-10) and remains editable, so the right-clicked folder is a starting point rather than a commitment.
8. **NFI-FR-08** **Name** is validated client-side as non-empty and free of path separators; **Create** is disabled while the **Name** is empty or invalid, so no creation call is made with a malformed name and one invocation creates exactly one file rather than a file plus the folders leading to it (mirroring the constraint of `../core/PST-project-storage.md` PST-FR-25).
9. **NFI-FR-09** Confirming with **Create** invokes `"create file (location, name)"` with the current inputs and, on success, closes the window.
10. **NFI-FR-10** The created file is empty — zero bytes on disk — because the window captures no content to write into it.
11. **NFI-FR-11** The window records no artifact-type assignment for the file it creates, at file scope or any other. The type the new file resolves to is therefore whatever the project's own classification yields for its path — a path-convention inference, or the nearest ancestor folder-scope assignment, or none at all (per `../core/ASC-artifact-scanning.md` ASC-FR-06) — so a file created inside a typed folder inherits that folder's type without this window asking about it, and a file created anywhere else stays unclassified until the user types it from the Project context menu (`LCM-library-context-menu.md` LCM-FR-04).
12. **NFI-FR-12** On a successful creation the new file opens in its natural surface — an Editor tab, as a plain text file when it resolves to no artifact type (per `ESH-editor-source-files.md` ESH-FR-ATDS) and as an artifact when it resolves to one, or a Flow tab when its resolved type is Flow (per `LIB-library.md` LIB-FR-03, `FLO-flow.md`) — via `"open artifact by id"`, and is revealed and selected in the Project panel (`LIB-library.md` LIB-FR-18).
13. **NFI-FR-13** The user may dismiss the window — via its close control, Escape, or an outside click — at any time before confirming; dismissal cancels the creation, invokes no backend operation, and leaves no file on disk.
14. **NFI-FR-14** A creation failure reported by `"create file"` (for example a name collision with an existing entry in the destination) is surfaced inline; the window stays open with its inputs intact and no file is created.

## Non-functional requirements
- The window opens with keyboard focus on the **Name** input, because it is the only value the user must always supply.
- The single-overlay rule of NFI-FR-01 is enforced by opening logic, not by listener coordination: the window assumes no other overlay is mounted while it is open.
- The **Location** select is populated from the tree already in memory, so opening the window issues no backend call and costs no filesystem walk.
- The window carries no expandable region and no optional block, so its height is fixed and it never reflows while the user fills it in.
- The window imposes no extension allowlist or denylist (NFI-FR-05), so the range of files a project can hold is never narrowed by this surface.
