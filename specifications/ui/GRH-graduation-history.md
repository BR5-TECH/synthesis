# Graduation history

**Spec code:** `GRH`

## Intent
The rail down the side of the graduation section that lists the project's runs and lets the author find one. It opens on the work that has not ended, narrows the listing by text and by view, files runs by the archive state their author set, and keeps the run the author was last looking at selected across a reopen. It is a way into a run and never a way to change one: nothing here starts, stops, reorders or ends a run. Out of scope: the run region beside it and the row actions on its rows, which are `GRU-graduation-runs.md`'s.

## Functional requirements
1. **GRH-FR-NCJK** The rail lists every run the project has made, in the project's own run order, newest insertion last. Nothing in the rail sorts by a timestamp.
2. **GRH-FR-FYTH** Each row names the run's source draft, the work stream it runs in, and its state in words. The row of a merge run has no source draft and names its title, `Merge <stream name>`, in the place of the draft. A row of a run in a queue also carries its position in that stream's queue.
3. **GRH-FR-WIMY** The rail offers a **view selector** with four positions, in this order: **In-flight**, **Completed**, **Archived**, and **All**. Exactly one position is active at a time, and **In-flight** is the resting position.
4. **GRH-FR-KZTP** **In-flight** admits every run that has not ended: the runs waiting in a queue and the runs under way together. **Completed** admits every run in the `completed` state and no other state. **All** admits every run the project has made.
   - *Why:* A run that failed and a run the author discarded are not completions, so **All** is the only position that lists them.
5. **GRH-FR-DYNU** **Archived** admits every **archived** run and no other run, whatever its state. An archived run stands in **Archived** and in **All**, and in a state position only where GRH-FR-YMIM admits it. Archiving is a filing act and changes nothing the run does.
6. **GRH-FR-YMIM** A run that **holds a work stream** is admitted by **In-flight**, whether the author filed it away or not. An archived run that holds one stands in **In-flight** and in **Archived** together.
   - *Why:* A stream held by a run nobody can find is a stream that has stopped for a reason the author cannot read.
7. **GRH-FR-BDMB** A text filter narrows the listing on the draft name, the stream name and the state word together. It narrows the view in force rather than replacing it.
8. **GRH-FR-QVEX** The rail holds its view position and its text filter in memory against the **open project**, for the life of the running application. One project's state is never read for another. A project the rail holds none for opens in **In-flight** with an empty text filter, and a relaunch keeps neither.
9. **GRH-FR-ODLT** Selecting a row selects that run for the region beside it and changes nothing about the run. The selection is remembered per project and is restored when the project is reopened.
10. **GRH-FR-XBWU** The rail **reveals** the selection it remembers for the open project, and every run another surface names. It relaxes only the filters that hide the run: the view moves to **Archived** for an archived run and to **All** for any other, and a text filter that excludes the run is cleared. A filter that admits it stays as it is.
11. **GRH-FR-WDSH** A selection naming a run the rail does not list falls back to the first row the text filter and the view in force admit together, and to no selection where they admit none. The fallback reveals nothing and changes neither filter.
    - *Why:* A rail that opened in **In-flight** must stay there when the run it fell back to is completed or archived, and show its empty result instead.
12. **GRH-FR-MCHQ** The rail's width is a fraction of the section's width, is adjustable by the author, and is remembered per project.
13. **GRH-FR-OFHS** The rail is operable by keyboard alone: the filter, the view selector and the rows are all reachable, and the selected row is announced when it changes.

## User stories
- As an author with twenty runs behind me, I want to see the runs that have not ended — the ones working and the ones waiting in a queue together — without reading past the rest.
- As an author who opens the graduation section, I want it to open on the work that has not ended, because that is the work that may need me.
- As an author looking for a run I filed away, I want one view that holds every archived run and a way to bring one back.

## UI contract boundary

**Owned by the UI**: the four view positions, which runs each admits, and which of them rests; the text filter and what it matches; the memory of both against the open project; the selection and its persistence; the rail's width; and the wording of every state label.

**Delegated to backend (abstract)**:
- `list_graduation_queue`
- `archive_graduation_run`
- `unarchive_graduation_run`

## Non-functional requirements
- The rail renders from the one queue read the section already makes and issues no read of its own.
- Filtering and view changes are computed in the frontend and invoke nothing.
