# Graduation log window

**Spec code:** `GLW`

## Intent
The modal overlay an author opens from a graduation phase to read what that phase actually did. A run's stage row already says which phase it reached and how many times it went round; this window is where the output behind one of those phases is read, pass by pass. It holds one stream at a time — the **Source** stream of raw agent and executor output, or the **Structured** stream of graduation observability records — and it lets the author search that stream, scroll it, or follow it as it grows, the way `tail` follows a file. It is bound to **one run**: the run whose phase was clicked, and no other, so an author watching three graduations at once reads three separate windows opened from three separate progress bars rather than one window with a chooser in it. Out of scope: how the two streams are written, indexed, searched, and recovered, which is `../core/GRS-graduation-run-log-storage.md`'s; which phases a run has reached and which of them may be opened, which is `GRU-graduation-runs.md`'s and is read here; the run's own state and its controls, which are `GRU-graduation-runs.md`'s and which this window changes none of; and the bottom panel's agent-output section, which is `RUN-runs.md`'s and is a different surface reading a different record.

## User stories
- As an author whose validation phase went round three times, I want to open that phase and read what each pass printed, rather than scrolling one undivided stream.
- As an author watching a turn that is still working, I want the log to follow the end of the output like `tail` does, and to stop following the moment I scroll up to read something.
- As an author debugging a failed turn, I want to search the raw output for a phrase and be told how many lines match.
- As an author with two graduations running, I want each run's log to open from that run's own progress bar and to share nothing with the other's.
- As an author reading a stage that only queued or committed, I want its run-level output shown as run-level, rather than inside a pass that never printed it.
- As an author using a screen reader or a keyboard alone, I want to know which run and which phase I opened, to reach every control, and to be returned to the phase I came from when I close it.

## Wireframes

The window opened on the specification validation phase of one run, reading the
Source stream of the third pass and following it:

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│  artifact-window · Specification validation                              [ ✕ ]   │
│  ( Source ) ( Structured )    [ 🔍 escalate_to_user            ]  4 matches       │
├────────────────────────┬─────────────────────────────────────────────────────────┤
│ PASSES                 │ 12:04:31  stdout  agent · claude-code                   │
│ ▸ pass 4               │   Reading specifications/core/GRD-graduation.md         │
│ ▸ pass 3   ●           │ 12:04:33  stdout  agent · claude-code                   │
│   pass 2               │   The tool ⟨escalate_to_user⟩ is not attached to        │
│   pass 1               │   this phase.                                           │
│   Run-level            │ 12:04:35  stderr  executor · eac                        │
│                        │   warning: stream reassembly bound reached              │
│                        │ …                                                       │
│                        │                              [ ⤓ Following ]            │
└────────────────────────┴─────────────────────────────────────────────────────────┘
        ( )   = the stream toggle: exactly one of the two is selected
        ●     = the entry being read
        ⤓     = follow is on; the viewport is at the end and stays there
```

The Structured stream of the same pass, and the same window scrolled away
from the end:

```
│  ( Source ) ( Structured )    [ 🔍                            ]                   │
├────────────────────────┬─────────────────────────────────────────────────────────┤
│ PASSES                 │ 12:04:31  info   validation.judgement_started           │
│   pass 4               │           pass=3  manifest_entries=7                    │
│ ▸ pass 3   ●           │ 12:04:44  warn   validation.gate_refused                │
│   pass 2               │           path=specifications/ui/RUN-runs.md            │
│   pass 1               │           code=reason_missing                           │
│   Run-level            │                        [ ⤓ Resume following ]           │
```

A phase whose selected entry and stream hold nothing, and a run whose log
persistence failed:

```
│                        │  This pass wrote nothing to the Source stream.          │
│                        │  The Structured stream is read with the toggle above.   │
```
```
│                        │  ⚠ The log could not be written and the run stopped.    │
│                        │    log_append_failed · source                           │
│                        │    The run is interrupted. Continue retries the writes. │
```

A phase whose only records are run-level, opened on the run-level entry:

```
│ PASSES                 │  This stage holds run-level output alone.               │
│   Run-level  ●         │  The passes it ran hold no record of their own.         │
```

- Layout notes: the window is a **centred modal overlay of the main window**, mutually exclusive with every other overlay (per `SNV-shell-navigation.md` SNV-FR-56). Its head carries the run's captured draft name and the selected phase's label as its title, and the close control at the trailing edge. Beneath the head stands one control row: the **stream toggle** at the leading edge and the **search field** beside it, with the match count at the trailing edge of that row. The body is two regions: a narrow **pass list** at the leading edge, arranged like the review's path rail (`GRU-graduation-runs.md`), and the wider **log viewport** filling the rest. The pass list holds the passes of the selected phase in pass order, and the **run-level entry** stands after them, labelled as run-level rather than numbered as a pass. The viewport is the only region that scrolls vertically; a long line wraps rather than scrolling the region sideways. The follow control sits at the trailing-bottom corner of the viewport, over the output, and states which of its two positions it is in. The empty, loading, unavailable, persistence-failure, and no-match states each render inside the viewport where the output would be, so what is missing is stated where the reader is looking. Nothing in the two regions moves the viewport while follow is off.

## UI contract boundary
- **Owned by the UI**: the overlay, its title, its close control, its focus containment and its focus return; the two-region layout and its bounds; the pass list, which passes and which run-level entry it holds, their order, and which is selected; the stream toggle, its two exclusive positions, its labels, and its selected state; the search field, the debounce before a query is sent, and the rendering of the match count; the log viewport, the rendering of one source chunk as text lines with the metadata of the chunk it came from, the rendering of one structured record as its event and its fields, and the escaping every one of them is set in; the follow mode, what releases it, and the control that resumes it; the cursor the surface holds per run, phase, pass-or-run-level scope, and stream, and the discarding of it whenever any of the four changes; the run-level entry, its labelling, and the scope it reads under; the loading, empty, unavailable, persistence-failure, and search-no-match states and the words each reads; and the keyboard and assistive-technology exposure of all of it.
- **Delegated to backend (abstract)**:
  - `"read graduation logs (run id, phase id, pass, stream, cursor, limit, query)"` — one page of one stream for one run, phase, and pass scope, owned by `../core/GRS-graduation-run-log-storage.md` (GRS-FR-RZXA). The pass scope names one pass, the run-level records, or the whole phase. It is the only operation this window invokes.
  - the event `"graduation log records appended"` — carrying `{ run_id, stream, latest_sequence }` and no record content, owned by the same module (GRS-FR-UCZL). This window re-reads under its own cursor and renders nothing from the payload.
  - the whole of this feature is mapped in `../core/GRS-graduation-run-log-storage.md`'s index of where each concern is specified, so the storage rules this window depends on — attribution, phase visibility, the presentation fusion, paging, statuses, and search — are reachable from one place rather than reassembled from the surfaces that read them.
  - `"get graduation run (run id)"` and the event `"graduation run changed"` — the run record this window reads its **stage history** (per `../core/GOB-graduation-observability.md` GOB-FR-HDZI), its **log indexes** and their per-phase and per-pass segments (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD), and its interruption state from, owned by `../core/GRD-graduation.md` (GRD-FR-LGDV). This window invokes nothing that starts, stops, or changes a run.

## Functional requirements
1. **GLW-FR-ALZI** This window is a **modal overlay of the main window** opened from one phase of one run's stage row, and it is mutually exclusive with every other overlay of the window (per `SNV-shell-navigation.md` SNV-FR-56).
2. **GLW-FR-BLWH** It is **bound to the graduation run whose phase the author clicked** and to that run alone. It holds **no run selector**, no run list, and no control that changes which run it is reading.
3. **GLW-FR-BTKB** To inspect another concurrent graduation run, the author opens **that run's own eligible phase from that run's own progress bar**, which opens this window bound to that run.
4. **GLW-FR-BVYN** Opening the window for another run **reuses nothing** of the previous run's window: not its phase, its pass, its stream, its cursor, its scroll position, its search text, its follow state, or a single log record. Each opening begins from what the run it names actually holds.
5. **GLW-FR-BWOS** The **clicked phase is the active log scope**. The window reads that phase and no other, and the phase is not changed from inside the window.
6. **GLW-FR-CJBE** Opening a phase **deletes, rewrites, and removes nothing**: the passes and the logs of every other phase are untouched and are read by opening another eligible phase of the same run.
7. **GLW-FR-CKLZ** The window carries a **pass list** at its leading edge, arranged like the modified-path rail of the specification acceptance window. It lists the **passes that entered the selected phase**, and after them the **run-level entry** of GLW-FR-RVKT where the phase holds one.
8. **GLW-FR-CQSE** Whether a pass entered a phase is read from the run's **persisted `stage_history`** (per `../core/GOB-graduation-observability.md` GOB-FR-VVNI and GOB-FR-HDZI). A pass is listed for the selected phase exactly where an entry of that history has the phase as its destination and carries that pass. The list is never inferred from the run's current phase, its current pass, the text of a record, or the presence of a log, so a pass the run has since left, or a phase the run has since moved on from, stays listed.
9. **GLW-FR-CQXJ** A record whose persisted `pass` is `null` is a **run-level record**. The window reads run-level records through the read's own **run-level scope** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-NKZP) and shows them under the run-level entry. It assigns no run-level record to a pass and computes no assignment rule of its own.
10. **GLW-FR-DDXJ** The **run-level entry is labelled as run-level** in words and in its accessible name, and it is never named as a pass or numbered as one. Every row it holds keeps a visible origin or producer marker, so a run-level record reads as the run-level record it is.
11. **GLW-FR-DKWB** `phase_id` stays **authoritative for phase visibility**. A run-level record is shown under the run-level entry of the selected phase only where its persisted `phase_id` equals that phase, and the window shows it under no other phase.
12. **GLW-FR-EUTW** **Every pass is tracked independently and is presented in its own entry** within the selected phase's pass history: one pass is one entry, named by its pass number. No two passes share an entry, and no entry holds a merged reading of two.
13. **GLW-FR-ELJO** Exactly one entry is selected at a time. The window opens on the **newest listed pass whose own records the selected stream's index holds** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD). Where no listed pass holds one, it opens on the **run-level entry** where that entry stands, and on the newest listed pass otherwise.
   - *Why:* A window that opens on a pass which printed nothing reads as a broken window, even where an earlier pass of the same phase holds the output the author came for.
14. **GLW-FR-YVKD** A segment whose `pass` is `null` names run-level records. It **counts for no pass** in the opening test of GLW-FR-ELJO and counts for the run-level entry alone, so a phase holding run-level records alone opens on that entry.
15. **GLW-FR-XZQM** That opening selection reads the **log index the run record already carries** and makes no read of its own. It settles the selection at opening alone: switching the stream toggle afterwards changes no pass (GLW-FR-GJLO), and the empty state renders where the newly selected stream holds nothing for that pass.
16. **GLW-FR-WBTE** Log presence settles **which entry is selected at opening and never which passes are listed**. The list stays exactly the passes that entered the phase (GLW-FR-CKLZ, GLW-FR-CQSE), so a pass that wrote nothing is still listed and still selectable.
17. **GLW-FR-FCVA** The **selected run, phase, and pass-or-run-level scope jointly determine which records are shown**, together with the selected stream, and the window derives none of the four from the text of a record.
18. **GLW-FR-FPUX** The window carries a **mutually exclusive stream toggle** with exactly two positions, **Source** and **Structured**, of which exactly one is selected. The selected position decides which persisted stream is rendered and which is searched.
19. **GLW-FR-TMRQ** The window opens on the **Source** position of the toggle, and it does so for every run and every phase. The opening position is remembered from no other window and is derived from what no scope holds, so the selected stream of GLW-FR-ELJO is settled before the first read.
20. **GLW-FR-FXAL** **Source** renders the decoded raw agent and executor output, **including the metadata attached to each persisted raw chunk**. Every text line derived from a chunk exposes **all ten of that chunk's displayed fields**: its `run_id`, `phase_id`, `pass`, `origin`, `producer`, `agent`, `container`, and `source`, together with its instant `at`, rendered in the local time zone from the instant the record carries, and its persisted `sequence`. A chunk decoding to several lines shows the same ten on each of them, because a line torn from its metadata is a line whose run, phase, and pass a reader has to guess at. A field the record holds as null — `agent` and `container` where they do not apply — is rendered as absent rather than as an invented value, and `pass` is rendered as the run-level marker of GLW-FR-DDXJ where the record holds null.
21. **GLW-FR-GIWK** **Structured** renders the structured observability records: each record's event, its level, its instant, and every field of its `fields`.
22. **GLW-FR-GJLO** Switching the toggle **changes neither the selected run, the selected phase, nor the selected pass**, and it **re-reads only the stream that became selected**.
23. **GLW-FR-GQQB** **Exactly one stream is shown at a time.** A source record never appears in the Structured view and a structured record never appears in the Source view.
24. **GLW-FR-HCOI** The **selected stream is part of every read, every change event this window acts on, and every search request** it makes.
25. **GLW-FR-HPWG** Source output is decoded as **UTF-8 with replacement characters for invalid byte sequences** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-QVTA), so a chunk that is not text still renders as lines rather than emptying the viewport.
26. **GLW-FR-IMKM** **Search applies only to the selected stream and to the selected run, phase, and pass.** Searching Source searches the decoded output and **every one of the ten fields GLW-FR-FXAL displays**, the instant and the sequence among them; searching Structured searches the event and every structured field the window renders. **What is displayed and what is searched are one set** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-OYIC): the window matches nothing a reader cannot see and leaves nothing a reader can see unmatchable.
27. **GLW-FR-INLR** Search is **case-insensitive substring** matching, this surface having no other mode (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-OYIC), and the window offers no regular-expression toggle.
28. **GLW-FR-KBZE** The **match count** the window states is the `matched_total` of the page it read, so a count is the whole scope's rather than the visible page's.
29. **GLW-FR-KDQG** **Search results, match counts, follow mode, and empty states never include a record of the unselected stream**, and a search that matched nothing in one stream never states that the other holds nothing.
30. **GLW-FR-KEXN** Clearing the search returns the viewport to the unfiltered stream at the position the reader was at, and it starts no new selection.
31. **GLW-FR-KKUM** The viewport **scrolls** the selected scope, and **follow mode** keeps it at the end as records arrive, the way `tail` does.
32. **GLW-FR-KTWX** **Follow stays enabled only while the viewport is at the end.** Scrolling away from the end **releases** follow and exposes a **labelled control to resume it**. That control is the only thing that resumes follow: activating it returns the viewport to the end and enables follow again, and scrolling back to the end does not.
33. **GLW-FR-KVXI** **New records do not move the viewport while follow is off.** They are read into the window and are reachable by scrolling, and the reader's position is where they left it.
34. **GLW-FR-KWHL** Follow and search apply to the **currently selected run, phase, pass-or-run-level scope, and stream** and to nothing else. Changing any of the four releases follow to its opening position for the new scope.
35. **GLW-FR-LUQI** The window reads its own records: it takes the **newest page** when a scope is selected, on being told that run and stream grew it asks for the records **after** the newest it holds, and it asks for the records **before** the oldest it holds when the reader scrolls back toward them (GLW-FR-QDWA). It renders nothing from an event payload and holds no record twice.
36. **GLW-FR-QDWA** **Older output is reachable.** A scope larger than one page opens on its newest page, and as the reader scrolls **toward the leading edge** the window asks for the page **before** the oldest record it holds, using the `older_cursor` that page carried (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-OWDT). It asks when the viewport approaches that edge rather than on every scroll event, it holds at most one older-page request in flight at a time, and it stops asking once a page reports `oldest_reached`, from which point the window states that the beginning of the scope is on screen.
37. **GLW-FR-RQEV** Pages are **merged in ascending sequence order and never duplicated**. An older page is prepended and a newer page appended, both by `sequence` rather than by arrival, and a record the window already holds is not held a second time however the pages overlap. A page that answers for a scope the reader has since left is discarded rather than merged (GLW-FR-NFLN).
38. **GLW-FR-TCQP** **Prepending an older page does not move what the reader is looking at.** The window keeps the record at the top of the viewport anchored: after the merge that record is in the same place on screen, and the rows that arrived are above it. Follow mode is untouched by an older-page load — it was already released by the scroll that asked for one (GLW-FR-KTWX) — and no older-page arrival ever scrolls the viewport.
39. **GLW-FR-UZAB** **Loading an older page is its own indication**, shown at the leading edge of the viewport while that request is in flight, and it is **none of the five states of GLW-FR-NMOD**: the records already read stay on screen and readable throughout, so a window fetching history never reads as a window that is loading, empty, unavailable, failed, or without matches. An older-page request that fails leaves what is held on screen, says that the older records could not be read, and offers to ask again.
40. **GLW-FR-NFLN** A **cursor belongs to one run, phase, pass-or-run-level scope, and stream**. The window discards every record and every cursor it holds whenever any of the four changes, and an answer arriving for a scope that is no longer selected is discarded rather than merged.
41. **GLW-FR-NMOD** The window renders **five states distinctly**, each inside the viewport: **loading** while a read is in flight and nothing is yet held, **empty** where the selected combination holds no record, **unavailable** where the stream could not be read, **persistence failure** where a write of that stream failed, and **no match** where a search matched nothing.
42. **GLW-FR-NYQF** None of the five is rendered as any of the others, and none of them says that the **run produced no output**: an empty scope states that this entry and this stream hold nothing, and it names the other stream as the one the toggle reads.
43. **GLW-FR-OATU** The **persistence-failure** state names the typed failure and states that the run is interrupted and that **Continue** retries the writes (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-THZA), and it offers no control that acts on the run.
44. **GLW-FR-VJNK** The **unavailable** state and the **persistence-failure** state are told apart in words and are never merged. Unavailable states that the stream could not be read whole and names the typed read failure the page carried — that the file is damaged or that it could not be read at all — together with the **sequence the read got as far as** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-EYNU), so a reader knows how much of the scope is in front of them and that the run itself is unaffected. Persistence failure states instead that a write failed and that the run stopped. Neither is rendered as the other and neither is rendered as an empty scope.
45. **GLW-FR-OGFF** Each stream shows **its own** loading, empty, unavailable, persistence-failure, and no-match state, and no state of one stream implies anything about the other.
46. **GLW-FR-OJXH** A read that fails leaves the window rendered and stating why rather than blank or thrown out of, and the run it is reading is unaffected either way.
47. **GLW-FR-OMZA** The window **names the run and the selected phase in its visible title and in its accessible name**, so a reader who opened it from one of three progress bars is told which one they are in.
48. **GLW-FR-ONEV** Opening the window **moves focus into the overlay**, and focus stays inside it while it is open.
49. **GLW-FR-OOYK** The window closes on **Escape** and on its **visible close control**, and closing **returns focus to the progress-bar phase that was clicked**.
50. **GLW-FR-OPQQ** **Closing the window stops no run, changes no run state, and deletes no log.** It is a reading surface: the only operation it invokes is the read of GLW-FR-LUQI.
51. **GLW-FR-PHBA** The **stream toggle carries accessible labels and a visible selected state**, and which of the two is selected is carried in words and in accessible semantics rather than by a colour or a mark alone.
52. **GLW-FR-QMRV** Every control is reachable and operable **from the keyboard alone**: the stream toggle, the search field, every entry of the pass list, the follow control, and the close control.
53. **GLW-FR-QNHH** Moving between the entries of the pass list with the keyboard selects the entry, and the viewport re-reads the newly selected scope.
54. **GLW-FR-RUNX** Every text the window renders is **untrusted**, whether a model, an executor, or the application wrote it: decoded source output, a structured event name, a structured field value, a producer, and an agent name alike.
55. **GLW-FR-SHAF** Each renders as **escaped plain text with its line structure preserved** and nothing else interpreted: no Markdown, no HTML, no image, no link, and nothing that reads as a command or an action. A path in the output is a path to read rather than a control that opens a file.
56. **GLW-FR-SLEK** The window **introduces no colour, icon, typography, or interaction pattern of its own**: it takes the tones, spacing, borders, and status treatments the graduation surfaces already use, and the **UI** typographic role (per `OVW-overview.md` OVW-FR-13), in the light and the dark theme alike.
57. **GLW-FR-SNDH** The whole of it is legible at the smallest supported window size, and the log viewport never scrolls horizontally: a long line wraps.
58. **GLW-FR-THAX** The window **holds no source of truth**. Every record it shows was read back through the one operation it invokes, and a window reopened after a relaunch renders what the run's streams actually hold rather than what this session watched.
59. **GLW-FR-ZPUH** Every read the window makes is **bounded to one page**: the newest page, the page after the newest record it holds, or the page before the oldest. It asks for no whole scope and for no unbounded limit, so a scope of a million records costs the page rather than the stream.
60. **GLW-FR-RVKT** The **run-level entry** stands in the pass list exactly where the run's log indexes hold a segment of the selected phase whose `pass` is `null` (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD). Both streams' indexes settle it, so the stream toggle adds and removes no entry.
61. **GLW-FR-PDGA** Selecting the run-level entry makes the read's scope the **run-level scope**, and selecting a pass makes it that **pass scope** (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-NKZP). The window reads one scope at a time and reads the phase scope for neither entry.
62. **GLW-FR-MZUP** The window **knows no stage id, no stage label, and no stage count**. The phase it reads is the descriptor the stage row passed it, its title is that descriptor's label, and no behaviour here is conditional on which phase it is or on how many the run has.
63. **GLW-FR-TIKK** The window renders **no log record of `../core/LGC-logging.md`'s session diagnostic buffer** and offers no route to it: the Structured view reads the persisted graduation observability stream (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-TCKD).
64. **GLW-FR-RQLV** The window shows the records a phase has **already made durable while that phase is still running**. Opening an eligible phase reads its newest page at once. It does not wait for a phase change, the end of a turn, or a change of run state.
65. **GLW-FR-GZWN** While the window is open, every `"graduation log records appended"` event for its run and its selected stream makes it read the records **after** the newest it holds, whether or not the selected phase is the run's current phase. An event whose run or stream differs is ignored. An event that arrives while the newest page is still being read is acted on once that page has arrived, so no record falls between the two reads.
66. **GLW-FR-JMXD** A phase the run has left stays openable for as long as the run's log indexes hold a segment naming it (per `GRU-graduation-runs.md` GRU-FR-TQJW). A review that sent the run back to the working phase leaves the review phase openable, and its pass list keeps one entry for every pass that entered it (GLW-FR-CQSE), each reading only its own records.
67. **GLW-FR-VRTC** A record is shown only where it belongs to the selected run, phase, pass-or-run-level scope, and stream. A page that names another run, phase, scope, or stream is discarded, and a record in an accepted page whose persisted `run_id`, `phase_id`, or `pass` differs from the selection is not shown. A page that is older than one already merged, judged by its `latest_sequence`, adds its records without replacing the status, the count, or the newest sequence the window holds.

## Non-functional requirements
- Rendering costs the page rather than the stream: a run that has written a million lines opens on its newest page as quickly as a run that has written ten.
- Following a working turn keeps up with a sustained agent output rate without stalling the window, and a window with follow off does no work per arriving record beyond holding it.
- The window does not require focus to stay current: a phase that grows while the author reads another entry is current when they select it.
- Every state is conveyed in words and in accessible semantics, never by colour, mark, or motion alone.
- The window is renderable against a backend that answers no read, in which case it states that the stream could not be read.
