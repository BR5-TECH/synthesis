# Logging

**Spec code:** `LGC`

## Intent
The application-wide diagnostic channel every other module emits through, so a developer or a user reporting a defect can see what the application was doing rather than guessing from its outward behaviour. It exists because the modules worth diagnosing — the agent adapters, the Git network operations, the project scan, the AI providers, and the frontend itself — have no shared place to say what happened, and a bespoke channel per module is a channel nobody watches. A record is structured rather than a formatted string: it carries a level, one or more domains, a message, and a flat bag of fields, so `../ui/LOG-logs.md` can filter and search a session's records on terms the emitter never had to anticipate. The whole buffer lives in memory for the duration of a session and is discarded rather than archived, because these records exist to explain the run in progress. Out of scope: this module writes no log file of its own and rotates nothing, so the only record that reaches disk is one the user explicitly exports; it redacts nothing, since keeping secrets out of a record is the emitter's obligation (LGC-FR-16); it observes no module and instruments nothing on its own, so a module logs what it chooses to and nothing else; it is not where an agent run is watched — `AGV-agent-activity.md` holds a run's own stream, per run and unbounded by this buffer's session cap, and what reaches this buffer is the same events as ordinary `DEBUG` records for a reader who is searching rather than watching; it is not where a **graduation run's own logs** live either — `GRS-graduation-run-log-storage.md` holds a run's two persisted streams, durable, complete, and retained for the lifetime of that run's record, and nothing this buffer holds is written into either of them and nothing either of them holds is read back into this buffer, so the Structured stream a reader opens from a run's progress bar is that run's persisted observability rather than this session-only diagnostic buffer; and it reports no operation to `PRG-progress-reporting.md`, because emitting a record is not work with a duration.

## Contract surface

### Internal Rust API
The surface every backend module emits through. Exposed by `synthesis-core` alongside `FSA-filesystem-access.md`'s primitives, and imported directly by core modules rather than reached over the wire.

- **`log(level, domains, message, fields)`** — appends one record to the session buffer and returns immediately. `domains` is a non-empty set drawn from the four of LGC-FR-03; `fields` is a flat map of JSON values and may be empty.
- **`log_debug(domains, message, fields)`** / **`log_info(...)`** / **`log_warn(...)`** / **`log_error(...)`** — thin wrappers over `log` that fix the level. They add no behaviour of their own and exist so an emit site reads as its level.

### Tauri commands
Names match `../ui/LOG-logs.md` byte-for-byte:

- `"append log records (records)"` → `append_log_records(records)` — appends a batch of records emitted by the frontend, in the order given. Each element is a `LogInput`. Returns nothing and never returns an error.
- `"query logs (filter, cursor, limit)"` → `query_logs(filter, cursor, limit)` → `LogPage` — the records matching `filter`, positioned by `cursor`.
- `"export logs (filter, destination path)"` → `export_logs(filter, destination_path)` → `count` — writes every record matching `filter` to `destination_path` as JSONL and returns how many were written.

### Events (Tauri event bus)
- `"log records appended"` — emitted when records have been appended to the buffer and when the buffer is cleared. Payload: `BufferState`. It carries no record content, so no consumer can evaluate a filter from it (LGC-FR-12).

### Payload shapes
```
LogRecord {
  sequence,            // monotonic ordinal, unique for the life of the running application
  ts,                  // ISO-8601 UTC, millisecond precision
  level,               // "DEBUG" | "INFO" | "WARN" | "ERROR"
  domains: [Domain],   // non-empty subset of the four of LGC-FR-03
  message,             // one-line human-readable summary
  fields               // flat map of JSON values; may be empty
}

LogInput {              // what a caller supplies; sequence is stamped by this module
  ts, level, domains, message, fields
}

Domain = "frontend" | "ai" | "backend" | "remote"

LogFilter {
  min_level,           // records below this level do not match (LGC-FR-09)
  domains: [Domain],   // a record matches when it carries at least one; empty matches every record
  query?,              // absent or empty matches every record
  query_is_regex       // false: case-insensitive substring; true: regular expression
}

Cursor =               // absent: the newest `limit` matching records
    { after: sequence }    // matching records newer than `sequence`, oldest-first
  | { before: sequence }   // the `limit` matching records nearest below `sequence`

LogPage {
  records: [LogRecord], // ascending by `sequence`, whatever the cursor
  generation,           // the buffer generation this page was read from (LGC-FR-14)
  matched_total,        // records in the buffer matching `filter`
  buffer_total,         // records in the buffer, filter disregarded
  dropped_total,        // records evicted since the buffer was last cleared
  highest_sequence      // the newest sequence in the buffer; absent when it is empty
}

BufferState {
  generation, buffer_total, dropped_total, highest_sequence
}
```

Typed errors: `"invalid query"` from `query_logs` and `export_logs` (LGC-FR-11); `"export failed"` from `export_logs` (LGC-FR-19).

## Functional requirements
1. **LGC-FR-01** `append_log_records`, `query_logs`, and `export_logs` exist as Tauri commands, `"log records appended"` exists as an event, and `log` and its four level wrappers exist as the internal Rust API, all with the documented shapes. In the walking-skeleton build the buffer may be seeded with synthetic records; `../ui/LOG-logs.md` must be fully exercisable against them.
2. **LGC-FR-02** The buffer is held in memory alone. Nothing it holds is written to disk, to any `.synthesis/` directory, or to `app_data_dir()`, and nothing is read back at startup, so a session begins with an empty buffer however the previous one ended. The one exception is an export the user explicitly requests (LGC-FR-18).
3. **LGC-FR-03** A record carries exactly one `level` from `DEBUG`, `INFO`, `WARN`, `ERROR`, and a non-empty set of `domains` drawn from `frontend`, `ai`, `backend`, `remote`. A record may carry several domains at once, so a line about a model call made on the user's behalf can be attributed to `ai` and `remote` together. The domain set is chosen at the emit site and describes what the record is about rather than which process emitted it. A record supplying an empty domain set is rejected and not appended.
4. **LGC-FR-04** `fields` is a flat map of JSON values — it holds no nested object and no array — so a record serialises to one JSON line and a consumer can render every field as a row without recursing.
5. **LGC-FR-05** `sequence` is assigned by this module, increases with every appended record, is unique for the lifetime of the running application, and is never reused. It does not reset when the buffer is cleared (LGC-FR-14), so a cursor held from before a clear can never match a record appended after one.
6. **LGC-FR-06** `ts` is supplied by the caller so a batched frontend record keeps the instant it was emitted rather than the instant it arrived. Records are appended in the order given, and `sequence` follows append order, which is therefore the buffer's order regardless of whether `ts` is monotonic across emitters.
7. **LGC-FR-07** The buffer holds at most 20,000 records. Appending to a full buffer evicts the oldest record first. `dropped_total` counts the records evicted since the buffer was last cleared, so a consumer can state that the record it is showing is not the oldest the session produced (per `../ui/LOG-logs.md` LOG-FR-14).
8. **LGC-FR-08** A record whose serialised size exceeds a fixed ceiling is appended with its `fields` replaced by a single field naming the omitted size, rather than being rejected or truncated mid-value. One oversized record therefore costs one slot rather than the buffer, and the record's `level`, `domains`, `message`, and `ts` survive intact.
9. **LGC-FR-09** `min_level` is a floor, not an equality: a record matches when its level is at or above it, ordered `DEBUG < INFO < WARN < ERROR`. `domains` matches when the record carries at least one of the listed domains, and an empty list matches every record.
10. **LGC-FR-10** `query` matches against the record's `message` and against every key and value of its `fields`. With `query_is_regex` false it is a case-insensitive substring match; with it true the query is compiled as a regular expression and matched. An absent or empty `query` matches every record. No mode assigns meaning to a character the other treats literally.
11. **LGC-FR-11** A `query_is_regex` query that does not compile returns a typed `"invalid query"` error from `query_logs` and from `export_logs`. No page is returned, no file is written, and the buffer is not read.
12. **LGC-FR-12** Every filter and every search is evaluated here. No command returns records the caller must narrow further, and `"log records appended"` carries no record content, so no consumer ever holds a record the current filter excludes.
13. **LGC-FR-13** This module holds no filter state. A `LogFilter` is supplied per call, is used for that call alone, and is neither retained between calls nor persisted anywhere, so what a consumer is currently filtering by is that consumer's business (per `../ui/LOG-logs.md` LOG-FR-08).
14. **LGC-FR-14** `generation` identifies the buffer's contents between clears: it starts at zero for a session and increases by one each time the buffer is cleared. A `LogPage` carries the generation it was read from, so a consumer can tell a page that describes the current buffer from one that describes a discarded predecessor.
15. **LGC-FR-15** The buffer is cleared in full when the project closes (per `PST-project-storage.md` PST-FR-14) and when the project's active worktree changes (per `WTC-worktree-context.md` WTC-FR-08). The clear happens before the operation that performs the close or the switch is invoked, so the records that operation emits — including those explaining a switch that fails — land in the fresh buffer rather than being discarded by the clear that follows. A clear increases `generation`, resets `buffer_total` and `dropped_total` to zero, and emits `"log records appended"` carrying the post-clear `BufferState`.
16. **LGC-FR-16** This module stores what it is given verbatim: it inspects no message, no key, and no value, and it redacts, masks, hashes, and rewrites nothing. Keeping a secret out of a record is therefore the obligation of the emitter, which every module holding credential material already carries (per `GTS-github-token-storage.md` GTS-FR-01, `AAP-ai-api-integrations.md` AAP-FR-07, `AIC-agentic-integrations.md` AIC-FR-20, and `AGC-agent-conversations.md` AGC-FR-26). A record's `fields` are the place a value belongs that a message should not name.
17. **LGC-FR-17** `log` and its wrappers never block the caller: an emit is not delayed by the absence of a subscriber, by a slow one, or by a consumer that never queries, and it performs no I/O. A module may therefore emit from inside a tight loop, and `"log records appended"` is coalesced so a burst of appends collapses to one event carrying the latest `BufferState` rather than one event per record.
18. **LGC-FR-18** `export_logs(filter, destination_path)` writes every record matching `filter` — the whole match set, bounded by no `limit` — to `destination_path` as JSONL, one record's JSON per line in ascending `sequence` order, through `FSA-filesystem-access.md`'s `write_text_at_user_choice` (FSA-FR-28), which retains the atomicity of FSA-FR-04. `destination_path` is the `UserChosenPath` the save dialog returned (FSA-FR-16), and it is the only kind of value that write accepts, which is what lets an export land wherever the user pointed it — typically outside the project and outside every directory any instance allowlists — without any other operation in the application gaining that reach. It returns the number of records written, which is zero when the filter matches none, and it creates no file other than that one.
19. **LGC-FR-19** An export that cannot be written — an unwritable destination, a full disk — returns a typed `"export failed"` error and, by the atomicity of FSA-FR-04, leaves no partial file at `destination_path`. A pre-existing file at that path is replaced, since the path is one the user chose in the save dialog (per `FSA-filesystem-access.md` FSA-FR-16).
20. **LGC-FR-20** `append_log_records`, `query_logs`, and `export_logs` all answer normally when no project is open, because a record need not be project-scoped: application startup, a plugin install, and a failed project open all emit before any content root exists.
21. **LGC-FR-21** This module mutates nothing observable beyond its own buffer, its event, and the file an export writes: no project file, no repository state, no setting, and no state belonging to a module that emits through it.
22. **LGC-FR-22** `cursor` positions a page within the match set and `limit` bounds its size, while the records of a page are always ordered ascending by `sequence` whichever cursor produced it. An absent cursor returns the newest `limit` matching records, so a consumer opening on a buffer already full starts at the end rather than paging to it. `{ after: s }` returns the matching records newer than `s`, oldest-first, which is the delta a consumer reads when the buffer has grown. `{ before: s }` returns the `limit` matching records nearest below `s`, which is the page a consumer reads when scrolling back. A cursor naming a `sequence` no longer in the buffer is positional rather than an error: the page holds whichever matching records fall on the requested side of it.

23. **LGC-FR-SFVS** This buffer and a **graduation run's two persisted log streams are separate channels with separate rules**, and neither is written from the other. `GRS-graduation-run-log-storage.md` owns `logs/activity.jsonl` and `logs/structured.jsonl` for one run (GRS-FR-MABD): those are durable, complete, mandatory, and retained for the lifetime of that run's record (GRS-FR-JQOO, GRS-FR-FORV), while this buffer is a **session-only** account of the application, capped for a whole session, cleared when the project changes, and discarded rather than archived (LGC-FR-12). No record of this buffer is appended to either stream, no record of either stream is read back into this buffer, and nothing here caps, evicts, or forgets a record of either.
24. **LGC-FR-TCZC** The **Agent Activity** view a reader opens from a graduation run's progress bar therefore reads **that run's persisted activity stream** (per `../ui/GLW-graduation-log-window.md` GLW-FR-FPUX and GLW-FR-TIKK) and **never this buffer**. `query_logs` and `export_logs` answer for this buffer alone whoever calls them, this module registers no operation that reads a run's log streams, and `../ui/LOG-logs.md` stays the one surface this buffer is read in.

## Non-functional requirements
- An emit is cheap enough to sit on a hot path: appending a record allocates and copies the record and nothing more, and the ring buffer reuses its slots rather than growing.
- The coalescing window for `"log records appended"` is an implementation choice of the order of a few tens of milliseconds; the contract is only that bursts collapse and that the state carried is the latest.
- A `query_logs` call over a full 20,000-record buffer completes fast enough to serve a keystroke-driven search box without the UI debouncing beyond what it would debounce anyway.
- The buffer's memory footprint is bounded by the record cap of LGC-FR-07 together with the per-record ceiling of LGC-FR-08, so a session cannot grow it without bound however much is emitted.
- Neither the commands, the event, nor the internal API requires network access.
