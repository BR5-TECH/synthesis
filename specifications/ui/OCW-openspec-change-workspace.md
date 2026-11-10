# OpenSpec change workspace

**Spec code:** `OCW`

## Intent
The change workspace is the tab where an author reads one OpenSpec change as one document: its proposal, its deltas as requirement diffs against the live specs, its design, and its tasks. A propose run writes the change on a stream (per `../core/GRD-graduation.md` GRD-FR-LKGC), and the author must read what it proposes before anything is built from it. A change is several files, and a delta is meaningful only beside the requirement it replaces, so a plain file view does not show what the change does. The tab shows a change in its **location**: the working copy of a stream, from the run or the draft that made it, or the active worktree, from the Library. On a stream, the tab is also where the author discusses the change with agents, in the discussion column every draft discussion has, and decides the edits they propose in the text of each file. Out of scope: applying the change, archiving it, and discussing a change in a worktree that is not a stream.

## Functional requirements
1. **OCW-FR-DMGL** The tab opens for one change in one location. A stream location comes from a completed propose run (per `GRU-graduation-runs.md` GRU-FR-LTWN), a draft row (per `DRP-drafts-panel.md` DRP-FR-YGOU), or a draft tab (per `NAW-new-artifact.md` NAW-FR-XDDQ). The active worktree location comes from the Library (per `LIB-library.md` LIB-FR-OKIZ).
2. **OCW-FR-IAPK** An open that names a file of the change selects the artifact that holds that file. An open that names no file selects the first artifact of the schema.
3. **OCW-FR-LABL** The tab invokes `"read openspec change"` with its location and change name when it opens, each time it becomes the active tab again, and on its **Refresh** action. A loading state stands in the reading pane until the first answer arrives.
4. **OCW-FR-XUJN** The tab's label is the change name. A stream location shows the stream name beside it.
5. **OCW-FR-LSFO** The header states the change name, the schema, the location, and the task progress as `<completed> of <total> tasks`. It shows one state chip for each artifact, in the build order of the schema: done, ready, blocked, or skipped.
6. **OCW-FR-QIZW** The artifact list on the leading side names each artifact of the schema in that order, with its state. Selecting one shows it in the reading pane. An artifact with no file states that the change holds no file for it.
7. **OCW-FR-NOTT** An artifact other than `specs` renders each of its files as a read-only Markdown document on the terms of `DTV-document-text-viewer.md` DTV-FR-SSQI: no script, no raw HTML, no remote resource, and inert links.
8. **OCW-FR-OUXA** The `specs` artifact renders the deltas grouped by capability, in the order the read gives them. Each group is headed by the capability id and states whether the live specs hold that capability.
9. **OCW-FR-NEQF** An `ADDED` delta renders the requirement name, its text, and its scenarios, marked as added.
10. **OCW-FR-WIKH** A `MODIFIED` delta renders the live requirement and the changed requirement side by side, the live one leading, each with its text and its scenarios whole.
11. **OCW-FR-YHIX** A `MODIFIED` delta with no live requirement renders the changed requirement alone, with a statement that the live specs hold no requirement of that name.
12. **OCW-FR-FPFE** A `REMOVED` delta renders the live requirement struck through, or its name alone with a statement where the live specs hold no requirement of that name.
13. **OCW-FR-IGTS** A `RENAMED` delta renders the old name and the new name, and the live requirement under its old name where the live specs hold one.
14. **OCW-FR-WXFK** A file that the read marks unreadable renders a statement that the file is not text, in place of its content.
15. **OCW-FR-UOKM** The tab changes a file only by a decision on a hunk of a proposal (OCW-FR-COLR). Where the location is the active worktree, each file offers **Open in editor**, which opens that file in an Editor tab (per `EDT-editor.md`). Where the location is a stream, no file offers it.
16. **OCW-FR-RFGG** A first read that fails renders its failure in the reading pane. `change_not_found` states that the change is not in that location any more. `openspec_mode_inactive` and `cli_unavailable` state that OpenSpec cannot be read, with a route to the OpenSpec section of Project settings. Every other code states that the read failed, with **Retry**.
17. **OCW-FR-PFUQ** A read that fails after a read that succeeded keeps the content of that read and states the failure above it, with **Retry**.
18. **OCW-FR-HPHM** Each read emits through `../core/LGC-logging.md`'s frontend helpers under the `frontend` domain: an `INFO` record with the location kind and the artifact and delta counts, or a `WARN` record with the location kind and the error code. No record holds the text of a file.
19. **OCW-FR-PLDG** An open in the active worktree, where the active worktree is the working copy of a stream of the project (per `WTC-worktree-context.md` WTC-FR-RDVK), is an open of that stream's location.
20. **OCW-FR-TRLG** A tab whose location is a stream carries the discussion column of `DDS-draft-discussion.md` beside the reading pane. The column holds the whole-change discussion and the fragment discussions of the change (per `../core/CMS-comments-storage.md` CMS-FR-UEXD).
21. **OCW-FR-ZZHS** A tab whose location is a worktree that is not a stream's working copy carries no discussion column, and states that a change is discussed on its work stream.
22. **OCW-FR-ABUR** Each file of an artifact offers **Rich** and **Source**. Rich is the rendering of OCW-FR-NOTT and OCW-FR-OUXA, and Source is the file's text as stored. The choice belongs to the tab.
23. **OCW-FR-JBFZ** In Source, a selection in a file of a stream location opens a fragment discussion of that file, on the terms of `CMT-comments.md` CMT-FR-06, with the change-relative path of the file.
24. **OCW-FR-COLR** While the change holds a pending proposal (per `../core/CEP-change-edit-proposals.md`), each hunk is decorated in Source of its file with the chips of `PCR-prompt-change-review.md`: accept, reject, edit, and discuss. A file with a hunk opens in Source.
25. **OCW-FR-RRXG** The artifact list marks each artifact that holds an undecided hunk with the count of those hunks. A hunk on a new file shows that file under its artifact, marked as new, before it is accepted.
26. **OCW-FR-DTRX** A decision invokes the command of `../core/CEP-change-edit-proposals.md` for it, and the tab then reads the change again. A refusal `stream_busy` is stated at the hunk, with the name of the run that holds the stream.
27. **OCW-FR-TAZJ** The tab follows `"change edit proposals changed"` for its stream and change, and reads the proposals again on it.
28. **OCW-FR-MLXJ** A decision whose proposal closes with a commit raises an Info toast that names the change. A close whose commit fails raises an Error toast with **Retry**, which invokes `"complete change edit decision"`.

## Wireframes

```
┌ add-logout-3f2a · stream: auth-work ───────────────────────────────── [Refresh] ┐
│ add-logout-3f2a   spec-driven   stream auth-work   1 of 4 tasks                │
│ [proposal ✓] [specs ✓] [design ✓] [tasks ✓]                                    │
├──────────────────┬──────────────────────────────────────────────────────────────┤
│ ▸ proposal  done │ auth                                   capability exists     │
│   specs     done │ ─ MODIFIED  Session Expiration ───────────────────────────── │
│   design    done │ ┌ live ───────────────────┐ ┌ changed ──────────────────────┐ │
│   tasks     done │ │ Sessions SHALL expire   │ │ Sessions SHALL expire after   │ │
│                  │ │ after 60 minutes.       │ │ 30 minutes of no activity.    │ │
│                  │ │ Scenario: Idle          │ │ Scenario: Idle                │ │
│                  │ └─────────────────────────┘ └───────────────────────────────┘ │
│                  │ ─ ADDED  Logout ─────────────────────────────────────────── │
│                  │   Users SHALL log out from every page.                       │
│                  │ ─ REMOVED  ~~Remember me~~ ──────────────────────────────── │
└──────────────────┴──────────────────────────────────────────────────────────────┘
```

- Layout notes: the artifact list has a fixed width and scrolls on its own. The reading pane takes the rest of the width and scrolls on its own. The two panes of a `MODIFIED` delta have equal width and wrap long words, and stack one above the other when the reading pane is narrower than 640 px.

## UI contract boundary
- **Owned by the UI**: the discussion column's placement, the Rich and Source switch, the hunk decorations and chips, the proposal marks of the artifact list, the toasts of a close (OCW-FR-PLDG through OCW-FR-MLXJ), the tab and its label, the header and its chips, the artifact list and its selection, the reading pane and the Markdown rendering of each file, the grouping and the rendering of the deltas, the loading state, the inline failures with their routes and **Retry**, the **Refresh** action, the **Open in editor** routes, and the reads on open and on reactivation (OCW-FR-DMGL through OCW-FR-HPHM). The identity of the tab and its closing belong to `TAB-tabs.md` (TAB-FR-KOTW, TAB-FR-DRXS).
- **Delegated to backend (abstract)**:
  - "read openspec change" (`read_openspec_change(location, name)`), owned by `../core/OSM-openspec-mode.md` (OSM-FR-MBTI).
  - "list change edit proposals", "load change edit proposal hunks", "accept change edit hunk", "reject change edit hunk", "hold change edit hunk", "edit change edit hunk", "decline change edit proposal", "complete change edit decision", and the event "change edit proposals changed", owned by `../core/CEP-change-edit-proposals.md`.
  - The discussion operations of `CMT-comments.md` and `DDS-draft-discussion.md`, owned by `../core/CMS-comments-storage.md` and `../core/AGC-agent-conversations.md`.

## Non-functional requirements
- A read costs one backend call. The tab reads nothing while it is not the active tab.
- The artifact list is a list of buttons that the keyboard reaches with Tab, and the selected one is marked for assistive technology.
- A change of 50 deltas renders without a delay that the author can see on a machine that meets the application's floor.
