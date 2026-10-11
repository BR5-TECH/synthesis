# Graduation log window

**Spec code:** `GLW`

## Intent
The modal overlay an author opens from a graduation phase to read what the agent did in that phase. A run's stage row already says which phase it reached and how many times it went round; this window shows the **Agent Activity** stream behind one of those phases, pass by pass. Each row has the same local time, kind, and one-line summary as a row of the Agent Output section. The window shows persisted rows while the phase is still running, and it shows the rows of earlier passes after the run has moved on. It lets the author search the rows, scroll them, or follow them as they grow, the way `tail` follows a file. It shows no task input, no internal reasoning, no raw CLI output, and no full tool payload. It is bound to **one run**: the run whose phase was clicked, and no other. Out of scope: how the stream is written, indexed, searched, and recovered, which is `../core/GRS-graduation-run-log-storage.md`'s; which phases a run has reached and which of them may be opened, which is `GRU-graduation-runs.md`'s and is read here; the run's own state and its controls, which are `GRU-graduation-runs.md`'s and which this window changes none of; and the bottom panel's agent-output section, which is `RUN-runs.md`'s and reads the session-lifetime stream of `../core/AGV-agent-activity.md`.

## User stories
- As an author whose validation phase went round three times, I want to open that phase and read what the agent did in each pass, rather than scrolling one undivided list.
- As an author watching a turn that is still working, I want new rows to appear as they are persisted, and I want the window to follow the end like `tail` does and to stop following the moment I scroll up.
- As an author whose run returned from review to work, I want to open the review phase and read what the review did in each of its earlier passes.
- As an author debugging a failed turn, I want to search the rows for a phrase and be told how many rows match.
- As an author with two graduations running, I want each run's window to open from that run's own progress bar and to share nothing with the other's.
- As an author reading a phase that only queued or committed, I want its run-level rows shown as run-level, rather than inside a pass that never produced them.
- As an author using a screen reader or a keyboard alone, I want to know which run and which phase I opened, to reach every control, and to be returned to the phase I came from when I close it.

## Wireframes

The window opened on the specification validation phase of one run, reading the
Agent Activity of the third pass and following it:

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  artifact-window · Specification validation                              [ ✕ ]   │
│  [ 🔍 escalate_to_user            ]  4 matches                                   │
├────────────────────────┬─────────────────────────────────────────────────────────┤
│ PASSES                 │ 12:04:31  message      Reading the validation manifest   │
│ ▸ pass 4               │ 12:04:33  tool call    Read specifications/core/GRD-gr…  │
│ ▸ pass 3   ●           │ 12:04:34  tool result  ok                                │
│   pass 2               │ 12:04:35  diagnostic   warning: stream reassembly bound  │
│   pass 1               │ …                                                        │
│   Run-level            │                              [ ⤓ Following ]             │
└────────────────────────┴─────────────────────────────────────────────────────────┘
        ●     = the entry being read
        ⤓     = follow is on; the viewport is at the end and stays there
```

The same window scrolled away from the end:

```
│  [ 🔍                            ]                                                │
├────────────────────────┬─────────────────────────────────────────────────────────┤
│ PASSES                 │ 12:04:31  message      Reading the validation manifest   │
│   pass 4               │ 12:04:33  tool call    Read specifications/core/GRD-gr…  │
│ ▸ pass 3   ●           │                                                          │
│   pass 2               │                        [ ⤓ Resume following ]            │
```

An entry that holds nothing, and a run whose log persistence failed:

```
│                        │  Pass 3 holds no agent activity yet.                    │
```
```
│                        │  ⚠ The log could not be written and the run stopped.    │
│                        │    log_append_failed · activity                          │
│                        │    The run is interrupted. Continue retries the writes. │
```

A phase whose only records are run-level, opened on the run-level entry:

```
│ PASSES                 │ 12:09:02  message      Reconciling the stream update     │
│   Run-level  ●         │ 12:09:05  finished     completed in 3 s                  │
```

- Layout notes: the window is a **centred modal overlay of the main window**, mutually exclusive with every other overlay (per `SNV-shell-navigation.md` SNV-FR-56). Its head carries the run's captured draft name and the selected phase's label as its title, and the close control at the trailing edge. Beneath the head stands one control row: the **search field**, with the match count at the trailing edge of that row. The body is two regions: a narrow **pass list** at the leading edge, arranged like the review's path rail (`GRU-graduation-runs.md`), and the wider **log viewport** filling the rest. The pass list holds the passes of the selected phase in pass order, and the **run-level entry** stands after them, labelled as run-level rather than numbered as a pass. The viewport is the only region that scrolls vertically; a long summary wraps rather than scrolling the region sideways. The follow control sits at the trailing-bottom corner of the viewport, over the rows, and states which of its two positions it is in. The empty, loading, unavailable, persistence-failure, and no-match states each render inside the viewport where the rows would be, so what is missing is stated where the reader is looking. Nothing in the two regions moves the viewport while follow is off.

## UI contract boundary
- **Owned by the UI**: the overlay, its title, its close control, its focus containment and its focus return; the two-region layout and its bounds; the pass list, which passes and which run-level entry it holds, their order, and which is selected; the search field, the debounce before a query is sent, and the rendering of the match count; the log viewport, the rendering of one record as its time, kind, and summary, and the escaping each is set in; the follow mode, what releases it, and the control that resumes it; the cursor the surface holds per run, phase, and pass-or-run-level scope, and the discarding of it whenever any of the three changes; the run-level entry, its labelling, and the scope it reads under; the loading, empty, unavailable, persistence-failure, and search-no-match states and the words each reads; and the keyboard and assistive-technology exposure of all of it.
- **Delegated to backend (abstract)**:
  - `"read graduation logs (run id, phase id, pass, stream, cursor, limit, query)"` — one page of the activity stream for one run, phase, and pass scope, owned by `../core/GRS-graduation-run-log-storage.md` (GRS-FR-RZXA). The pass scope names one pass, the run-level records, or the whole phase. The window always names the stream `activity`. It is the only operation this window invokes.
  - the event `"graduation log records appended"` — carrying `{ run_id, stream, latest_sequence }` and no record content, owned by the same module (GRS-FR-UCZL). This window re-reads under its own cursor and renders nothing from the payload.
  - the whole of this feature is mapped in `../core/GRS-graduation-run-log-storage.md`'s index of where each concern is specified, so the storage rules this window depends on are reachable from one place rather than reassembled from the surfaces that read them.
  - `"get graduation run (run id)"` and the event `"graduation run changed"` — the run record this window reads its **stage history** (per `../core/GOB-graduation-observability.md` GOB-FR-HDZI), its **log indexes** and their per-phase and per-pass segments (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD), and its interruption state from, owned by `../core/GRD-graduation.md` (GRD-FR-LGDV). This window invokes nothing that starts, stops, or changes a run.

## Functional requirements
1. **GLW-FR-ALZI** This window is a **modal overlay of the main window** opened from one phase of one run's stage row, and it is mutually exclusive with every other overlay of the window (per `SNV-shell-navigation.md` SNV-FR-56).
2. **GLW-FR-BLWH** It is **bound to the graduation run whose phase the author clicked** and to that run alone. It holds **no run selector**, no run list, and no control that changes which run it is reading.
3. **GLW-FR-BTKB** To inspect another concurrent graduation run, the author opens **that run's own eligible phase from that run's own progress bar**, which opens this window bound to that run.
4. **GLW-FR-BVYN** Opening the window for another run **reuses nothing** of the previous run's window: not its phase, its pass, its cursor, its scroll position, its search text, its follow state, or a single record. Each opening begins from what the run it names actually holds.
5. **GLW-FR-BWOS** The **clicked phase is the active log scope**. The window reads that phase and no other, and the phase is not changed from inside the window.
6. **GLW-FR-CJBE** Opening a phase **deletes, rewrites, and removes nothing**: the passes and the logs of every other phase are untouched and are read by opening another eligible phase of the same run.
7. **GLW-FR-CKLZ** The window carries a **pass list** at its leading edge, arranged like the modified-path rail of the specification acceptance window. It lists the **passes that entered the selected phase**, and after them the **run-level entry** of GLW-FR-RVKT where the phase holds one.
8. **GLW-FR-CQSE** Whether a pass entered a phase is read from the run's **persisted `stage_history`** (per `../core/GOB-graduation-observability.md` GOB-FR-VVNI and GOB-FR-HDZI). A pass is listed for the selected phase exactly where an entry of that history has the phase as its destination and carries that pass. The list is never inferred from the run's current phase, its current pass, the text of a record, or the presence of a log, so a pass the run has since left, or a phase the run has since moved on from, stays listed.
9. **GLW-FR-CQXJ** A record whose persisted `pass` is `null` is a **run-level record**. The window reads run-level records through the read's own **run-level scope** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-NKZP) and shows them under the run-level entry. It assigns no run-level record to a pass and computes no assignment rule of its own.
10. **GLW-FR-DDXJ** The **run-level entry is labelled as run-level** in words and in its accessible name, and it is never named as a pass or numbered as one. The viewport that shows its rows carries the same label in its accessible name, so a run-level record reads as the run-level record it is.
11. **GLW-FR-DKWB** `phase_id` stays **authoritative for phase visibility**. A run-level record is shown under the run-level entry of the selected phase only where its persisted `phase_id` equals that phase, and the window shows it under no other phase.
12. **GLW-FR-EUTW** **Every pass is tracked independently and is presented in its own entry** within the selected phase's pass history: one pass is one entry, named by its pass number. No two passes share an entry, and no entry holds a merged reading of two.
13. **GLW-FR-ELJO** Exactly one entry is selected at a time. The window opens on the **newest listed pass whose own records the activity index holds** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD). Where no listed pass holds one, it opens on the **run-level entry** where that entry stands, and on the newest listed pass otherwise.
   - *Why:* A window that opens on a pass with no activity reads as a broken window, even where an earlier pass of the same phase holds the activity the author came for.
14. **GLW-FR-YVKD** A segment whose `pass` is `null` names run-level records. It **counts for no pass** in the opening test of GLW-FR-ELJO and counts for the run-level entry alone, so a phase holding run-level records alone opens on that entry.
15. **GLW-FR-XZQM** That opening selection reads the **log index the run record already carries** and makes no read of its own. It settles the selection at opening alone: later growth of the index moves no selection, and the empty state renders where the selected entry holds nothing.
16. **GLW-FR-WBTE** Log presence settles **which entry is selected at opening and never which passes are listed**. The list stays exactly the passes that entered the phase (GLW-FR-CKLZ, GLW-FR-CQSE), so a pass that wrote nothing is still listed and still selectable.
17. **GLW-FR-FCVA** The **selected run, phase, and pass-or-run-level scope** jointly determine which records are shown, and the window derives none of the three from the text of a record.
18. **GLW-FR-FPUX** The window renders **one persisted stream**, the **Agent Activity** stream of the run, and nothing else. It carries no stream toggle. It shows no raw CLI output and no graduation observability record.
19. **GLW-FR-KHGP** Every record renders as **one row** of three parts: the **local time** of the instant the record carries, its **kind**, and its **one-line summary**. The row has the presentation and the meaning of a row of the Agent Output section (per `RUN-runs.md` RUN-FR-03 and RUN-FR-11).
20. **GLW-FR-JOIG** A row **reveals nothing beyond those three parts**. It carries no expansion, no raw payload, no task input, no reasoning text, no invocation text, and no full tool argument or result. The row of the Agent Output section that expands to a verbatim event (per `RUN-runs.md` RUN-FR-14) has no counterpart here.
21. **GLW-FR-HGXL** The window reads **four fields of a record** and no other: `sequence`, `at`, `kind`, and `summary`. A record that holds any further field shows none of it.
22. **GLW-FR-HCOI** Every read, every change event this window acts on, and every search request it makes **names the activity stream**.
23. **GLW-FR-IMKM** **Search applies only to the selected run, phase, and pass or run-level scope.** It matches the **kind** and the **summary** of each record, which are the texts a row shows. **What is searched is a subset of what is displayed, and nothing that is not displayed is matched** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-OYIC).
24. **GLW-FR-INLR** Search is **case-insensitive substring** matching, this surface having no other mode (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-OYIC), and the window offers no regular-expression toggle.
25. **GLW-FR-KBZE** The **match count** the window states is the `matched_total` of the page it read, so a count is the whole scope's rather than the visible page's.
26. **GLW-FR-KEXN** Clearing the search returns the viewport to the unfiltered stream at the position the reader was at, and it starts no new selection.
27. **GLW-FR-KKUM** The viewport **scrolls** the selected scope, and **follow mode** keeps it at the end as records arrive, the way `tail` does.
28. **GLW-FR-KTWX** **Follow stays enabled only while the viewport is at the end.** Scrolling away from the end **releases** follow and exposes a **labelled control to resume it**. That control is the only thing that resumes follow: activating it returns the viewport to the end and enables follow again, and scrolling back to the end does not.
29. **GLW-FR-KVXI** **New records do not move the viewport while follow is off.** They are read into the window and are reachable by scrolling, and the reader's position is where they left it.
30. **GLW-FR-KWHL** Follow and search apply to the **currently selected run, phase, and pass-or-run-level scope** and to nothing else. Changing any of the three releases follow to its opening position for the new scope.
31. **GLW-FR-LUQI** The window reads its own records: it takes the **newest page** when a scope is selected, on being told that the run grew it asks for the records **after** the newest it holds, and it asks for the records **before** the oldest it holds when the reader scrolls back toward them (GLW-FR-QDWA). It renders nothing from an event payload and holds no record twice.
32. **GLW-FR-QDWA** **Older output is reachable.** A scope larger than one page opens on its newest page, and as the reader scrolls **toward the leading edge** the window asks for the page **before** the oldest record it holds, using the `older_cursor` that page carried (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-OWDT). It asks when the viewport approaches that edge rather than on every scroll event, it holds at most one older-page request in flight at a time, and it stops asking once a page reports `oldest_reached`, from which point the window states that the beginning of the scope is on screen.
33. **GLW-FR-RQEV** Pages are **merged in ascending sequence order and never duplicated**. An older page is prepended and a newer page appended, both by `sequence` rather than by arrival, and a record the window already holds is not held a second time however the pages overlap. A page that answers for a scope the reader has since left is discarded rather than merged (GLW-FR-NFLN).
34. **GLW-FR-TCQP** **Prepending an older page does not move what the reader is looking at.** The window keeps the record at the top of the viewport anchored: after the merge that record is in the same place on screen, and the rows that arrived are above it. Follow mode is untouched by an older-page load — it was already released by the scroll that asked for one (GLW-FR-KTWX) — and no older-page arrival ever scrolls the viewport.
35. **GLW-FR-UZAB** **Loading an older page is its own indication**, shown at the leading edge of the viewport while that request is in flight, and it is **none of the five states of GLW-FR-NMOD**: the records already read stay on screen and readable throughout, so a window fetching history never reads as a window that is loading, empty, unavailable, failed, or without matches. An older-page request that fails leaves what is held on screen, says that the older records could not be read, and offers to ask again.
36. **GLW-FR-NFLN** A **cursor belongs to one run, phase, and pass-or-run-level scope**. The window discards every record and every cursor it holds whenever any of the three changes, and an answer arriving for a scope that is no longer selected is discarded rather than merged.
37. **GLW-FR-NMOD** The window renders **five states distinctly**, each inside the viewport: **loading** while a read is in flight and nothing is yet held, **empty** where the selected entry holds no record, **unavailable** where the stream could not be read, **persistence failure** where a write of the stream failed, and **no match** where a search matched nothing.
38. **GLW-FR-NYQF** None of the five is rendered as any of the others, and none of them says that the **run produced no output**. An empty entry states that this entry holds no agent activity. A search that matched nothing renders the no-match state and never the empty state.
39. **GLW-FR-OATU** The **persistence-failure** state names the typed failure and states that the run is interrupted and that **Continue** retries the writes (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-THZA), and it offers no control that acts on the run.
40. **GLW-FR-VJNK** The **unavailable** state and the **persistence-failure** state are told apart in words and are never merged. Unavailable states that the activity stream could not be read whole and names the typed read failure the page carried, together with the **sequence the read got as far as** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-EYNU). Persistence failure states instead that a write failed and that the run stopped. Neither is rendered as an empty entry.
41. **GLW-FR-OJXH** A read that fails leaves the window rendered and stating why rather than blank or thrown out of, and the run it is reading is unaffected either way.
42. **GLW-FR-OMZA** The window **names the run and the selected phase in its visible title and in its accessible name**, so a reader who opened it from one of three progress bars is told which one they are in.
43. **GLW-FR-ONEV** Opening the window **moves focus into the overlay**, and focus stays inside it while it is open.
44. **GLW-FR-OOYK** The window closes on **Escape** and on its **visible close control**, and closing **returns focus to the progress-bar phase that was clicked**.
45. **GLW-FR-OPQQ** **Closing the window stops no run, changes no run state, and deletes no log.** It is a reading surface: the only operation it invokes is the read of GLW-FR-LUQI.
46. **GLW-FR-QMRV** Every control is reachable and operable **from the keyboard alone**: the search field, every entry of the pass list, the follow control, and the close control.
47. **GLW-FR-QNHH** Moving between the entries of the pass list with the keyboard selects the entry, and the viewport re-reads the newly selected scope.
48. **GLW-FR-RUNX** Every text the window renders is **untrusted**, whether a model, an executor, or the application wrote it: a kind and a summary alike.
49. **GLW-FR-SHAF** Each text renders as **escaped plain text** and nothing else is interpreted: no Markdown, no HTML, no image, no link, and nothing that reads as a command or an action. A path in a summary is a path to read rather than a control that opens a file.
50. **GLW-FR-SLEK** The window **introduces no colour, icon, typography, or interaction pattern of its own**: it takes the tones, spacing, borders, and status treatments the graduation surfaces already use, and the **UI** typographic role (per `OVW-overview.md` OVW-FR-13), in the light and the dark theme alike.
51. **GLW-FR-SNDH** The whole of it is legible at the smallest supported window size, and the log viewport never scrolls horizontally: a long line wraps.
52. **GLW-FR-THAX** The window **holds no source of truth**. Every record it shows was read back through the one operation it invokes, and a window reopened after a relaunch renders what the run's activity stream actually holds rather than what this session watched.
53. **GLW-FR-ZPUH** Every read the window makes is **bounded to one page**: the newest page, the page after the newest record it holds, or the page before the oldest. It asks for no whole scope and for no unbounded limit, so a scope of a million records costs the page rather than the stream.
54. **GLW-FR-RVKT** The **run-level entry** stands in the pass list exactly where the run's activity index holds a segment of the selected phase whose `pass` is `null` (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD).
55. **GLW-FR-PDGA** Selecting the run-level entry makes the read's scope the **run-level scope**, and selecting a pass makes it that **pass scope** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-NKZP). The window reads one scope at a time and reads the phase scope for neither entry.
56. **GLW-FR-MZUP** The window **knows no stage id, no stage label, and no stage count**. The phase it reads is the descriptor the stage row passed it, its title is that descriptor's label, and no behaviour here is conditional on which phase it is or on how many the run has.
57. **GLW-FR-TIKK** The window renders **no record of `../core/LGC-logging.md`'s session diagnostic buffer** and offers no route to it. It reads no graduation observability record either: observability reaches the run-progress surface through the run record (per `../core/GOB-graduation-observability.md`).
58. **GLW-FR-RQLV** The window shows the records a phase has **already made durable while that phase is still running**. Opening an eligible phase reads its newest page at once. It does not wait for a phase change, the end of a turn, or a change of run state.
59. **GLW-FR-GZWN** While the window is open, every `"graduation log records appended"` event for its run and the activity stream makes it read the records **after** the newest it holds, whether or not the selected phase is the run's current phase. An event whose run or stream differs is ignored. An event that arrives while the newest page is still being read is acted on once that page has arrived, so no record falls between the two reads.
60. **GLW-FR-JMXD** A phase the run has left stays openable for as long as the run's log indexes hold a segment naming it (per `GRU-graduation-runs.md` GRU-FR-TQJW). A review that sent the run back to the working phase leaves the review phase openable, and its pass list keeps one entry for every pass that entered it (GLW-FR-CQSE), each reading only its own records.
61. **GLW-FR-VRTC** A record is shown only where it belongs to the selected run, phase, pass-or-run-level scope, and stream. A page that names another run, phase, scope, or stream is discarded, and a record in an accepted page whose persisted `run_id`, `phase_id`, or `pass` differs from the selection is not shown. A page that is older than one already merged, judged by its `latest_sequence`, adds its records without replacing the status, the count, or the newest sequence the window holds.

## Non-functional requirements
- Rendering costs the page rather than the stream: a run that has persisted a million rows opens on its newest page as quickly as a run that has persisted ten.
- Following a working turn keeps up with a sustained activity rate without stalling the window, and a window with follow off does no work per arriving record beyond holding it.
- The window does not require focus to stay current: a phase that grows while the author reads another entry is current when they select it.
- Every state is conveyed in words and in accessible semantics, never by colour, mark, or motion alone.
- The window is renderable against a backend that answers no read, in which case it states that the stream could not be read.
