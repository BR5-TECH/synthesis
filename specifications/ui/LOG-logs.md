# Logs

**Spec code:** `LOG`

## Intent
Bottom-panel surface that shows the diagnostic records the application emitted during this session, so a user who hits something inexplicable can see what the application was actually doing and hand that account to whoever fixes it. It reads like the log view of an IDE: a dense list of one-line records the user narrows by level, by domain, and by text until only the interesting ones remain, with any single record expandable to the full structured object behind it. The records themselves and every act of narrowing them belong to `../core/LGC-logging.md`; this surface renders what that module returns and never filters a record itself. Out of scope: presenting a run as a stream — following it, expanding it, and scoping it to one run — which is `RUN-runs.md`'s, even though the same events are searchable here as records; it offers no control that clears the buffer, since a session's records live until the session, the project, or the worktree ends; and it neither redacts nor masks what a record holds, because keeping a secret out of a record is the obligation of whatever emitted it (LOG-FR-20).

## User stories
- As a user whose action silently did nothing, I want to open Logs and read what the backend reported, so I can tell a broken integration from a mistake of my own.
- As a user filing a defect, I want to narrow the panel to the errors of one domain and export exactly those, so the report carries evidence rather than my recollection.
- As a user watching a long operation, I want the newest record to stay in view without me scrolling, and to stop following the moment I scroll up to read something.

## Wireframes

```
┌───────────────────────────────────────────────────────────────────────────────┐
│ LOGS                                            [Follow ✓]  [Copy]  [Export] ✕│
├───────────────────────────────────────────────────────────────────────────────┤
│ Level: [ DEBUG ▾ ]   Domain: [✓]Frontend [ ]AI [✓]Backend [ ]Remote           │
│ Search: [ vendor                                    ] [.*] 42 of 1,208        │
├───────────────────────────────────────────────────────────────────────────────┤
│ ⚠ 3,140 older records were dropped                                            │
│ 12:04:29.101  DEBUG  Backend         scan enumerated 812 files                │
│ 12:04:31.882  WARN   AI·Remote       model request retried (attempt 2)        │
│ ▾12:04:31.902 ERROR  Backend         scan aborted: permission denied          │
│   {                                                                           │
│     "sequence": 4471,                                                         │
│     "ts": "2026-08-04T12:04:31.902Z",                                         │
│     "level": "ERROR",                                                         │
│     "domains": ["backend"],                                                   │
│     "message": "scan aborted: permission denied",                             │
│     "fields": { "path": "vendor/", "errno": 13 }                              │
│   }                                                                           │
│ 12:04:32.004  INFO   Frontend        logs panel activated                     │
└───────────────────────────────────────────────────────────────────────────────┘
```

Layout notes:
- The filter row and the search row are pinned above the scrollable list and stay in place while it scrolls; the list is the only scrolling region.
- Columns are fixed-width for time, level, and domains, so messages start at one left edge down the whole list and the eye can run it vertically.
- The match count sits at the trailing end of the search row, reading as `<matched> of <buffer total>`.
- The eviction notice occupies the head of the list rather than the chrome, so it scrolls away with the oldest record it describes.
- An expanded record's JSON renders inline beneath its row, indented to the message column, and pushes later rows down rather than overlaying them.
- No region scrolls horizontally: a message longer than the available width wraps at the message column's left edge rather than introducing a horizontal scrollbar.

## UI contract boundary
- **Owned by the UI**: the panel's layout and its two pinned control rows, the rendering of a record as a row and of an expanded record as JSON, per-row expansion state, row selection, the level-floor selector, the four domain checkboxes, the search box and its regex toggle together with the inline surfacing of a regex that does not compile, the follow/tail pin and its release-on-scroll behaviour, the eviction notice, the two empty states, the decision to redraw only while the panel is the visible bottom surface (LOG-FR-11), the cursor a page request carries, copying selected records to the clipboard, and the in-memory retention of every filter value for the life of the session (LOG-FR-08). The panel evaluates no filter and matches no text itself.
- **Delegated to backend (abstract)**: `"query logs (filter, cursor, limit)"`, `"append log records (records)"`, and `"export logs (filter, destination path)"` — all owned by `../core/LGC-logging.md`, which also owns the record shape, the session buffer, the eviction cap, and the clearing of the buffer when the project or the active worktree changes. The panel follows that module's `"log records appended"` event and re-queries from it rather than reading records out of it (LOG-FR-11). Choosing where an export is written is `"browse for save path"`, owned by `../core/FSA-filesystem-access.md` (FSA-FR-16). Copying selected records introduces no backend command, because it copies records the panel already holds.

## Functional requirements
1. **LOG-FR-01** Logs is a bottom-panel surface selected by its own toggle in the activity bar's bottom-panel cluster, which holds Runs, Logs, History, and Git in that order (per `SNV-shell-navigation.md` SNV-FR-44). Its toggle is enabled whenever a project is open, whatever the active main-viewport tab is, because a diagnostic record is about the application rather than about the open artifact.
2. **LOG-FR-02** A record renders as one row carrying, in fixed columns, its timestamp rendered in the local time zone at millisecond precision, its level, its domains, and its message. A record carrying several domains shows all of them in that one column.
3. **LOG-FR-03** The four levels are distinguished by more than the shade of the row: each carries its own printed label alongside whatever colour treatment it is given, so `WARN` is told from `ERROR` without relying on colour perception and without hovering the row.
4. **LOG-FR-04** Activating a row expands it in place to show the full record as formatted JSON — every field included, nothing elided — and activating it again collapses it. Any number of rows may be expanded at once, and expansion is per-row state held in memory by the panel: it survives scrolling and is discarded when the panel's contents are replaced (LOG-FR-13).
5. **LOG-FR-05** The panel pins two control rows above the list, in this order from the top: the level-floor selector and the four domain checkboxes on the first, and the search box with its regex toggle on the second, immediately above the list they narrow. Both rows stay visible while the list scrolls.
6. **LOG-FR-06** Every act of narrowing — the level floor, the domain selection, and the search — is performed by `"query logs (filter, cursor, limit)"`. The panel holds no unmatched record and applies no predicate of its own, so what it renders is exactly what that operation returned.
7. **LOG-FR-07** The level selector picks a floor rather than a single level: choosing `WARN` shows `WARN` and `ERROR` records. The four domain checkboxes are independent, a record matches while any of its domains is checked, and checking none is the same as checking all. The panel's defaults are a `DEBUG` floor, all four domains checked, and an empty search, so a panel opened for the first time in a session hides nothing.
8. **LOG-FR-08** Every filter value — the floor, the domain selection, the search text, and the regex toggle — is held in memory by the panel for the life of the session. It survives hiding the bottom panel, switching the bottom panel to another surface and back, and switching the active project or worktree, and it is written nowhere: a relaunched application starts at the defaults of LOG-FR-07 rather than at what was last used.
9. **LOG-FR-09** The search box matches as a case-insensitive substring by default, and as a regular expression while its regex toggle is on. A pattern the backend rejects as uncompilable is reported beside the search box, and the list keeps showing the last result that did match rather than emptying, so a half-typed pattern does not blank the panel.
10. **LOG-FR-10** Follow mode is on when the panel is first shown in a session: the list stays pinned to the newest record as records arrive. Scrolling up releases the pin and reveals a control that resumes it; scrolling back to the foot of the list resumes it as well. While the pin is released, arriving records are added below without moving what the user is reading.
11. **LOG-FR-11** The panel subscribes to `"log records appended"` for as long as it is mounted, but acts on it only while Logs is the bottom panel's active surface and the bottom panel is visible. While it is not — hidden, or showing Runs, History, or Git — the panel issues no query and performs no redraw, so a burst of records costs nothing while nobody is looking at them.
12. **LOG-FR-12** When Logs becomes the visible bottom surface, the panel queries afresh under its current filter with no cursor, taking the newest page rather than replaying the events it ignored while inactive. What it shows on becoming visible is therefore the state of the buffer at that moment, not a reconstruction of it.
13. **LOG-FR-13** When a `"log records appended"` payload or a returned page carries a `generation` other than the one the panel is holding, the panel discards everything it holds — its rows, its selection, and its expansion state — and re-queries from scratch, because the buffer it was rendering has been cleared (per `../core/LGC-logging.md` LGC-FR-15).
14. **LOG-FR-14** While `dropped_total` is greater than zero, the head of the list carries a notice stating how many older records were dropped, so a user reading from the top of the list knows it is not the start of the session. The notice is absent while nothing has been evicted.
15. **LOG-FR-15** The panel pages through the match set rather than holding it whole: it takes the newest page on becoming visible, requests the records after the newest it holds when the event reports growth, and requests the page before the oldest it holds when the user scrolls to the top of what is loaded. The list of a session that has emitted more records than one page holds is therefore scrollable back to the oldest record still in the buffer.
16. **LOG-FR-16** One or more rows can be selected, and a copy control together with the platform copy accelerator copies the selected records to the clipboard as JSON, one complete record per line, in the order they appear in the list. Copying takes the full records rather than the rendered row text, so what is pasted into a defect report carries every field the row's columns did not show.
17. **LOG-FR-17** An export control opens the native save dialog through `"browse for save path"` and, on a path being chosen, invokes `"export logs (filter, destination path)"` with the panel's current filter. What is exported is the filter's entire match set rather than the pages currently loaded or the rows currently on screen. The outcome is reported in the panel — the number of records written, or the typed failure — and cancelling the dialog invokes nothing.
18. **LOG-FR-18** The panel tells an empty buffer from a filtered-to-nothing list, on the same terms `SNV-shell-navigation.md` SNV-FR-61 draws for vertical-panel surfaces. A buffer holding no record at all renders a block stating that this session has emitted none, with both control rows still present because they are what the user will reach for once records arrive. A buffer holding records that the current filter admits none of renders its message within the list's own region, with every control still holding what the user typed or checked, so the thing to change is beside the message that prompted it.
19. **LOG-FR-19** The frontend emits its own records through one module, which batches them and hands them to `"append log records (records)"`. Emitting never blocks a render and never awaits the round trip, and a batch the backend does not accept is dropped rather than retried indefinitely or surfaced to the user, because a failure to record a diagnostic is not itself worth interrupting the user over.
20. **LOG-FR-20** A record emitted from the frontend carries no secret: no token, key, credential, or authorization header value reaches a message or a field, on the same terms every module holding credential material already observes (per `../core/GTS-github-token-storage.md` GTS-FR-01 and `../core/AAP-ai-api-integrations.md` AAP-FR-07). Neither this panel nor `../core/LGC-logging.md` inspects or masks what it is given (per `../core/LGC-logging.md` LGC-FR-16), so the emit site is the only place this is enforced.
21. **LOG-FR-21** This panel renders records and never a run's stream as a stream. An agent's activity does reach the buffer, at `DEBUG`, because a run nobody can search is a run nobody can diagnose (per `../tools/EAC-execute-agent-cli.md` EAC-FR-32) — but it arrives as records like any other, narrowed by the same level, domain, and search controls, and it is `RUN-runs.md` that presents a run as a stream with its own following, expansion, and run scoping. The level floor is what separates the two readings: a reader chasing something else raises it to `INFO` and a chatty run leaves the panel without becoming less observable.
22. **LOG-FR-22** A filename or project-relative path appearing in a record's message or fields renders in exactly the case it carries on disk, in the row and in the expanded JSON alike (per `SNV-shell-navigation.md` SNV-FR-57).

## Non-functional requirements
- The list virtualises its rows, so a page of the maximum buffer size scrolls without the frame cost growing with the number of records held.
- Typing in the search box is debounced so a keystroke does not issue a query per character, and a query superseded by a later keystroke never renders after the one that replaced it.
- Records arriving while the panel is visible append without moving the scroll position when follow mode is released, and without a visible reflow of the rows already rendered.
- The panel requires no network access, and it renders its empty state without one.
- Frontend emission is bounded: a batch is flushed on a short timer or when it reaches a fixed size, so a render loop that emits per frame costs one round trip per flush rather than one per record.
