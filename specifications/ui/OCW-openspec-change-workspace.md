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
29. **OCW-FR-EJGH** While the change holds a pending edit proposal, a review bar above the reading pane states the count of undecided hunks across the change and offers **Reject all**. Reject all invokes `"decline change edit proposal"` after a confirmation that names the count, and then the tab reads the change and the proposals again.
30. **OCW-FR-RKCC** A tab whose location is a stream offers **History**. History shows the versions of the change from `"list change versions"` (per `../core/CVH-change-version-history.md`), newest first, each with the first line of its commit message, its author, its time, and the count of files it changed. A list that is truncated states it.
31. **OCW-FR-DRQD** Selecting a version shows the change as that version holds it, from `"read change version"`, in the reading pane with its own artifact list, read-only, with Rich and Source. A banner names the version and offers **Back to current**. No decision, no fragment discussion, and no edit is offered while a version is shown.
32. **OCW-FR-ABAR** A shown version offers **Compare with current**, which shows each file of the version beside the same file of the current change as a line diff, added, removed, and changed files each marked. A file that is not text in either is named with no diff.
33. **OCW-FR-QCSL** The History list reads the versions again when it opens, on **Refresh**, and on `"change edit proposals changed"` and `"new change proposals changed"` with the cause `committed` or `decided` for its stream and change.
34. **OCW-FR-ZZCS** A tab whose location is a stream offers **Information**, which opens a centred modal of the change's statistics from `"read change statistics"` (per `../core/CST-change-statistics.md`): Working time, AI interactions, Versions, Accepted proposals, Rejected proposals, and Conversation tokens. It renders unavailable and incomplete statistics, its footer, its dismissal, and its re-read on `"change statistics changed"` on the terms of `DFI-draft-information.md` DFI-FR-HSYB, DFI-FR-CVAX, DFI-FR-NTPG, and DFI-FR-EBIL.
35. **OCW-FR-MIEY** A working-activity observer reports settled foreground intervals of a change through `"record change working interval"`, on the terms `DFI-draft-information.md` DFI-FR-XKRM, DFI-FR-UKFR, and DFI-FR-JDWS give the editing observer of a draft. The surfaces of a change are its tab and its discussion column, and a conversation counts as related where its origin kind is `change_comment` or `change_discussion` for that change.
36. **OCW-FR-BGDN** A comment that carries a `new_change_proposal` attachment renders a card with the proposed name, the position and the total, the paths of its files, and its state. While the proposal is pending, the card offers **Accept**, **Reject**, and **Discuss**, which invoke `"accept new change proposal"`, `"reject new change proposal"`, and open a discussion with a `new_change_subject` attachment. **View files** shows the files from `"load new change proposal files"`, read-only.
37. **OCW-FR-ANKT** While the split holds an accepted proposal and none is pending, and the split is not confirmed, the tab offers **Confirm split**, which invokes `"confirm change split"` after a confirmation that names the accepted changes.
38. **OCW-FR-TZRT** A change whose split is confirmed shows a marker in its header that names the changes it was split into. Each name opens that change's tab on the same stream.
39. **OCW-FR-IJAI** An accepted proposal's card names the created change and opens its tab. An acceptance whose commit fails raises an Error toast with **Retry**, which invokes `"complete new change decision"`. A refusal `stream_busy` is stated on the card with the name of the run that holds the stream.
40. **OCW-FR-GCME** The tab stays usable at a window width of 800 px with a side panel open. Where the reading pane would be narrower than 360 px, the artifact list collapses to a menu above the reading pane, and the discussion column gives up width before the reading pane does, down to its own minimum of 280 px. While History is open, the document column keeps room for the History rail beside the reading pane before the discussion column gets more than its minimum. Where the reading pane beside the rail would be narrower than 360 px, the rail moves above the reading pane at the full width of the column, with a height of at most 40 % of the column, and its versions scroll. A file path and its Source text never wrap one letter to a line.
41. **OCW-FR-FMDZ** In Rich, an image whose Markdown destination is a relative path inside the change folder renders the image that `"read change image"` returns for it (per `../core/OSM-openspec-mode.md` OSM-FR-BLEY). A read that refuses, and any other destination, render the alt text and the destination as a link that the application does not fetch.

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
- **Owned by the UI**: the review bar and its Reject all, the History list, the version view and the compare, the Information modal and the working-activity observer, the new-change cards and Confirm split, the split marker, the narrow-window layout, and the Rich images (OCW-FR-EJGH through OCW-FR-FMDZ), the discussion column's placement, the Rich and Source switch, the hunk decorations and chips, the proposal marks of the artifact list, the toasts of a close (OCW-FR-PLDG through OCW-FR-MLXJ), the tab and its label, the header and its chips, the artifact list and its selection, the reading pane and the Markdown rendering of each file, the grouping and the rendering of the deltas, the loading state, the inline failures with their routes and **Retry**, the **Refresh** action, the **Open in editor** routes, and the reads on open and on reactivation (OCW-FR-DMGL through OCW-FR-HPHM). The identity of the tab and its closing belong to `TAB-tabs.md` (TAB-FR-KOTW, TAB-FR-DRXS).
- **Delegated to backend (abstract)**:
  - "read openspec change" (`read_openspec_change(location, name)`), owned by `../core/OSM-openspec-mode.md` (OSM-FR-MBTI).
  - "read change image" (`read_change_image(stream_id, change, path)`), owned by `../core/OSM-openspec-mode.md` (OSM-FR-BLEY).
  - "list change edit proposals", "load change edit proposal hunks", "accept change edit hunk", "reject change edit hunk", "hold change edit hunk", "edit change edit hunk", "decline change edit proposal", "complete change edit decision", and the event "change edit proposals changed", owned by `../core/CEP-change-edit-proposals.md`.
  - "list change versions" and "read change version", owned by `../core/CVH-change-version-history.md`.
  - "read change statistics", "record change working interval", and the event "change statistics changed", owned by `../core/CST-change-statistics.md`.
  - "list new change proposals", "load new change proposal files", "accept new change proposal", "reject new change proposal", "confirm change split", "complete new change decision", and the event "new change proposals changed", owned by `../core/NCP-new-change-proposals.md`.
  - The discussion operations of `CMT-comments.md` and `DDS-draft-discussion.md`, owned by `../core/CMS-comments-storage.md` and `../core/AGC-agent-conversations.md`.

## Non-functional requirements
- A read costs one backend call. The tab reads nothing while it is not the active tab.
- The artifact list is a list of buttons that the keyboard reaches with Tab, and the selected one is marked for assistive technology.
- A change of 50 deltas renders without a delay that the author can see on a machine that meets the application's floor.
