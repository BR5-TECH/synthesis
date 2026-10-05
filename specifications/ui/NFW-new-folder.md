# New Folder

**Spec code:** `NFW`

## Intent
The modal surface for creating a new folder in the open project: the user picks a parent folder, names the folder, optionally associates it with an artifact type, and confirms to create it on disk. The optional type association is what makes this more than a bare directory creation — the chosen type becomes the folder's folder-scope assignment, so every file the user later puts inside it resolves to that type by inheritance instead of being tagged one by one. It is the single creation experience behind both the File menu's **New Folder** item (`SNV-shell-navigation.md` SNV-FR-24) and the Project context menu's folder-scoped **New Folder** entry (`LCM-library-context-menu.md` LCM-FR-09), so a folder is born the same way regardless of where the user started. Out of scope: creating a chain of nested folders in one action, moving or copying existing files into the new folder, and re-typing a folder after the fact — that last belongs to the context menu's **Artifact Type** submenu (`LCM-library-context-menu.md` LCM-FR-04).

## User stories
- As a user, I want to create a folder from inside Synthesis rather than reaching for a terminal or a file manager, so that shaping project structure is part of the tool I am already working in.
- As a user, I want to declare up front that everything in this folder is a Spec — or a Skill, or a Scenario — so that the files I add later are typed correctly without me tagging each one.
- As a user, I want to create a folder directly inside the folder I right-clicked in the Project panel, so that I don't re-pick a parent I already expressed by where I clicked.
- As a user, I want to leave the artifact type unset when I am only organising files, so that declaring a type is never a toll on creating a folder.
- As a user, I want the folder I just created to be visible in the Project panel the moment it exists, so that creating one does not feel like nothing happened.

## Wireframes
```
                 ┌──────────────────────────────────────────┐
                 │  New Folder                            ✕ │
                 ├──────────────────────────────────────────┤
                 │  Parent Folder  [ <project root>      ▾ ] │
                 │  Name           [ scenarios             ] │
                 │  Artifact Type  [ (not set)           ▾ ] │
                 ├──────────────────────────────────────────┤
                 │                      [ Cancel ] [ Create ]│
                 └──────────────────────────────────────────┘

      Artifact Type ▾ (expanded):
                 │  ┌────────────────────┐                  │
                 │  │ (not set)          │  ← default        │
                 │  │ Skill              │                  │
                 │  │ Agent              │                  │
                 │  │ Prompt             │                  │
                 │  │ Spec               │                  │
                 │  │ Flow               │                  │
                 │  │ Instructions       │                  │
                 │  │ Scenario           │                  │
                 │  │ Scratchpad         │                  │
                 │  └────────────────────┘                  │
```
- Layout notes: the window is a centered modal overlay, not a tab or panel (NFW-FR-01), and is re-centered on the screen each time it opens (NFW-FR-02). Inputs read, top to bottom: **Parent Folder** (required, pre-filled), **Name** (required, empty), **Artifact Type** (optional, unset). **Create** is the confirming action and **Cancel**/✕ the dismissing one; **Create** is disabled while the **Name** is empty or invalid (NFW-FR-08). The **Artifact Type** control is a single-select whose first option is an explicit unset state, followed by exactly the eight built-in artifact types (`../core/ASC-artifact-scanning.md` ASC-FR-02). The window carries no content, template, or AI-prompt input — a folder has no body.

## UI contract boundary
- **Owned by the UI**: the modal layout, its centering, and its dismissal; the three inputs (Parent Folder, Name, Artifact Type) and their enablement; the **Parent Folder** single-select over the project's existing folders and the project root, and the fact that the project root is its default (NFW-FR-04); the **Artifact Type** single-select over an unset state plus the eight built-in artifact types, and the fact that unset is its default (NFW-FR-05); client-side **Name** validation; the two invocation points (File menu `SNV-shell-navigation.md` SNV-FR-24, Project context menu `LCM-library-context-menu.md` LCM-FR-09) and the parent seeding each performs (NFW-FR-06, NFW-FR-07); the reveal-and-select request issued to the Project panel after a successful creation (NFW-FR-11); and inline display of a creation error. The window reads the project's folder set from the tree the Project panel already holds; it never scans the filesystem itself and never decides artifact types.
- **Delegated to backend (abstract)**:
  - `"create folder (location, name, artifact type)"` — creates the folder and, when a type is supplied, records it as that folder's folder-scope assignment; owned by `../core/PST-project-storage.md` (PST-FR-24). Returns the new folder node so the UI can reveal it.
  - The new node also reaches the Project panel through the `"project tree changed"` event (`../core/ASC-artifact-scanning.md` ASC-FR-10); the window does not push the node into the tree itself.

## Functional requirements
1. **NFW-FR-01** New Folder is a modal overlay action surface — not a tab and not a panel. As a floating overlay it is mutually exclusive with the other overlays of the main window: opening it closes any other open overlay rather than coexisting with it.
2. **NFW-FR-02** The window is positioned at the center of the screen each time it opens, regardless of where any prior invocation left it.
3. **NFW-FR-03** The window presents three creation inputs — **Parent Folder**, **Name**, and **Artifact Type** — where **Name** is required and must be supplied by the user, **Parent Folder** is required but always arrives pre-filled (NFW-FR-04), and **Artifact Type** is optional.
4. **NFW-FR-04** **Parent Folder** is a single-select over the project root and every folder that exists under it, listing folders the Project panel's active lens currently hides as well as those it shows (per `LIB-library.md` LIB-FR-09), so a folder may be created inside an empty or unclassified one. Its default value is the project root — the directory of the project's active worktree (per `../core/WTC-worktree-context.md` WTC-FR-03) — and the user may change it in every flow.
5. **NFW-FR-05** **Artifact Type** is a single-select whose options are an explicit unset state followed by exactly the eight built-in artifact types (`../core/ASC-artifact-scanning.md` ASC-FR-02). Its default is the unset state in every flow: the window never seeds a type from the selected parent folder, from that parent's own associated type, or from anything else, so a folder is typed only when the user says so.
6. **NFW-FR-06** In the File-menu flow, **Parent Folder** starts at the project root (NFW-FR-04) and **Artifact Type** starts unset (NFW-FR-05); both are the user's to change.
7. **NFW-FR-07** In the Project context-menu flow, **Parent Folder** starts at the folder the menu was opened on (`LCM-library-context-menu.md` LCM-FR-09) and remains editable, so the right-clicked folder is a starting point rather than a commitment. **Artifact Type** still starts unset, including when the right-clicked folder carries an associated type of its own.
8. **NFW-FR-08** **Name** is validated client-side as non-empty and free of path separators; **Create** is disabled while the **Name** is empty or invalid, so no creation call is made with a malformed name and one invocation creates exactly one folder rather than a chain of nested ones (mirroring the constraints of `../core/PST-project-storage.md` PST-FR-19 and PST-FR-21).
9. **NFW-FR-09** Confirming with **Create** invokes `"create folder (location, name, artifact type)"` with the current inputs and, on success, closes the window.
10. **NFW-FR-10** When an **Artifact Type** is chosen, the created folder carries it as a folder-scope assignment, so files placed inside it later resolve to that type by inheritance unless a per-file assignment or a path-convention inference overrides them (per `../core/ASC-artifact-scanning.md` ASC-FR-05 and the precedence of ASC-FR-06). When **Artifact Type** is left unset, no assignment is recorded and the folder's future contents fall to inference alone.
11. **NFW-FR-11** On a successful creation the new folder is revealed and selected in the Project panel, collapsed and empty, and the panel switches its artifact-type lens where the active lens would otherwise hide it (per `LIB-library.md` LIB-FR-18). No tab is opened, because a folder has no natural editing surface.
12. **NFW-FR-12** The user may dismiss the window — via its close control, Escape, or an outside click — at any time before confirming; dismissal cancels the creation, invokes no backend operation, and leaves nothing on disk.
13. **NFW-FR-13** A creation failure reported by `"create folder"` (for example a name collision with an existing entry in the parent) is surfaced inline; the window stays open with its inputs intact and no folder is created.

## Non-functional requirements
- The window opens with keyboard focus on the **Name** input, because it is the only value the user must always supply.
- The single-overlay rule of NFW-FR-01 is enforced by opening logic, not by listener coordination: the window assumes no other overlay is mounted while it is open.
- The **Parent Folder** select is populated from the tree already in memory, so opening the window issues no backend call and costs no filesystem walk.
- The window carries no expandable region and no optional block, so its height is fixed and it never reflows while the user fills it in.
