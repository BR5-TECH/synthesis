# Graduation run log storage

**Spec code:** `GRS`

## Intent
The private, durable record of everything a graduation run's agents and loops emitted, kept per run so that an author can read what a phase actually did. It exists because a graduation turn is minutes or hours of work inside a container nobody can see into, and the two accounts that already exist answer different questions: `AGV-agent-activity.md` holds a session-lifetime, memory-bounded stream that is evicted, forgotten, and lost on relaunch, and `GOB-graduation-observability.md` holds the run's own account of its passes without a line of what any of them wrote. This module holds **two append-only streams per run** — the raw executor output, byte for byte after the redaction the executor already applied, and the structured graduation observability records — each written into the run's own directory under the application's private storage. Every record names the run, the phase, and the pass it belongs to, so a surface selects a run, a phase, and a pass without reading the text. Persistence here is **mandatory**: a record that cannot be written stops the agent work rather than being dropped, because output that vanishes silently is worse than a run that stops and says why. Out of scope: how the streams are rendered, searched, and followed on screen, which is `../ui/GLW-graduation-log-window.md`'s; the run's state machine, its queues, and its interruption record, which are `GRD-graduation.md`'s and which this module reports into rather than decides; what the structured records mean, which is `GOB-graduation-observability.md`'s; the masking of a credential, which is `../tools/EAC-execute-agent-cli.md`'s and is done before a byte reaches this module; and the application's own session diagnostic buffer, which is `LGC-logging.md`'s and is neither read nor written here.

## Contract surface

The module owns two files per run, the indexes and the persistence status those files are read through, the sinks that write them, and the Tauri command and event below.

### Storage

```text
short_data_dir()/g/<run-id>/logs/source.jsonl       raw executor output, one JSON object per line
short_data_dir()/g/<run-id>/logs/structured.jsonl   structured observability records, one per line
```

Both files sit beside the run's `run.toml` in the run's own directory (per `GRD-graduation.md` GRD-FR-OSCG) and outside every checkout that run owns. Nothing here is written into a project's repository, into its `.synthesis/`, into a graduation or implementation worktree, or into Git history. All access is through `FSA-filesystem-access.md` rather than through a bare filesystem call.

### The streams

```text
GraduationLogStream = "source" | "structured"
```

Exactly two, named on every read, every append, every change event, and every search.

### The source record

One JSON object per emitted stdout, stderr, or executor chunk:

```
GraduationSourceChunk {
  schema_version,   // 1
  record_id,        // opaque, assigned once by the producer, unique within the run
  sequence,         // integer, assigned here, ascending within this run and stream
  at,               // RFC 3339 UTC, when the chunk was emitted
  run_id,
  phase_id,         // one of the four of GRS-FR-MEPZ
  pass,             // the pass number, or null for a run-level record
  origin,           // GraduationLogOrigin
  producer,         // the component that emitted it (GRS-FR-NUXT)
  agent,            // the integration the turn ran under; null where none applies
  container,        // the container the chunk was read from; null where none applies
  source,           // "stdout" | "stderr" | "executor"
  encoding,         // always "base64"
  data_base64,      // the complete redacted bytes of this chunk
  byte_length       // the decoded length in bytes
}
```

### The structured record

```
GraduationStructuredRecord {
  schema_version,   // 1
  record_id,        // opaque, assigned once by the producer, unique within the run
  sequence,         // integer, assigned here, ascending within this run and stream
  at,               // RFC 3339 UTC
  run_id,
  phase_id,         // one of the four of GRS-FR-MEPZ
  pass,             // the pass number, or null for a run-level record
  origin,           // GraduationLogOrigin
  producer,         // the component that emitted it (GRS-FR-NUXT)
  level,            // "debug" | "info" | "warn" | "error"
  event,            // a short stable name for what happened
  fields            // a flat map of JSON values; may be empty
}
```

### The attribution

```text
GraduationLogOrigin = "agent" | "executor" | "application"

phase_id  = "queued" | "working" | "review" | "done"
```

### The attribution of every producer

`phase_id` and `pass` are settled by **what is producing the record**, and this table is the whole of that mapping. Every value of `phase_id` is one of the four progress-bar ids and no other value exists (GRS-FR-KJVN):

```text
producer                        phase_id        pass
--------------------------------------------------------------
queue_wait                      queued          null
work_turn                       working         that pass
clarification_judgement         working         that pass
review_turn                     review          that pass
commit                          done            null
semantic_merge_turn             done            null
stage_transition                the phase being left     null
```

### The indexes and the persistence status

Held on the run record, beside the run state, and holding no log payload:

```
GraduationStreamIndex {
  stream: GraduationLogStream,
  latest_sequence,            // 0 for a stream with no record
  record_count,
  durable_through_sequence,   // the newest sequence the file holds durably
  byte_length,                // the durable length of the file
  segments: [                 // append-only; one entry per phase and pass the
                              //   stream holds records for, in first-sequence order
    { phase_id, pass, first_sequence, last_sequence, record_count }
  ]
}

GraduationLogPersistence {
  status,            // "healthy" | "failed"
  failure,           // GraduationLogFailure; null while status is "healthy"
  pending_count,     // records accepted from a producer and not yet durable
  updated_at         // RFC 3339 UTC
}

GraduationLogIndexes {
  log_storage_version: 1,
  source: GraduationStreamIndex,
  structured: GraduationStreamIndex,
  persistence: GraduationLogPersistence,
  last_read_failure  // GraduationLogFailure of kind "read", retained from the most recent
                     //   read that met corruption; null where no read has met any. It is
                     //   a record of a damaged file and never a persistence status
                     //   (GRS-FR-EYNU)
}

GraduationLogFailure {
  kind,              // "write" | "read": which side of the stream failed
  code,              // write:  "log_storage_unavailable" | "log_stream_create_failed"
                     //         | "log_append_failed" | "log_flush_failed"
                     //   read: "log_stream_corrupt" | "log_stream_unreadable"
  stream,            // the stream the failure is about
  message,           // one sentence naming the act that clears it
  at,                // RFC 3339 UTC
  stopped_sequence,  // read failures: the newest sequence read whole before the failure,
                     //   and 0 where none was; null for every write failure (GRS-FR-EYNU)
  byte_offset,       // read failures: where in the file decoding stopped; null otherwise
  pending_record_ids // write failures: the records still to be written, in append order;
                     //   empty for every read failure
}
```

### The read

```
read_graduation_logs(run_id, phase_id, pass, stream, cursor, limit, query) -> GraduationLogPage

GraduationLogCursor {
  run_id,
  stream,
  direction,         // "after" | "before"
  sequence
}

GraduationLogPassScope =                 // what a read's pass scope names
    { kind: "pass", pass }                 // one pass's own records
  | { kind: "run_level" }                  // the records whose pass is null
  | { kind: "phase" }                      // every record the selected phase holds

GraduationLogPageEntry {
  record,            // the persisted record, verbatim and unchanged
  presentation: {    // computed for this read alone and written to neither file
    run_level             // true exactly where record.pass is null
  }
}

GraduationLogPage {
  run_id,
  stream,
  phase_id,
  scope,             // GraduationLogPassScope: the scope the page answers for
  entries,           // [GraduationLogPageEntry], ascending by record.sequence,
                     //   whichever cursor produced them
  next_cursor,       // the cursor for the next page in the requested direction; null at the end
  older_cursor,      // the "before" cursor for the page preceding this one; null where
                     //   this page already holds the scope's oldest record (GRS-FR-OWDT)
  matched_total,     // every record the request matches, whether or not this page holds it
  oldest_reached,    // bool: this page holds the scope's oldest matching record
  latest_sequence,   // the newest sequence this run and stream holds
  status,            // GraduationLogReadStatus
  search,            // GraduationLogSearchResult
  failure            // GraduationLogFailure; present for "unavailable" and for
                     //   "persistence_failed", and absent for the other two (GRS-FR-EYNU)
}

GraduationLogReadStatus =
    "available"          // the scope holds records and they were read
  | "empty"              // the scope holds no record at all
  | "unavailable"        // the stream could not be read
  | "persistence_failed" // a write of this stream failed and the run is interrupted

GraduationLogSearchResult =
    "not_requested"      // the read carried no query
  | "matched"            // the query matched at least one record in the scope
  | "search_no_match"    // the query matched none
```

### The sinks

Two run-scoped sinks, constructed against one run identifier and the scope its records belong to, and handed to the producer rather than the producer being told a run identifier:

- **the source sink** — handed to `../tools/EAC-execute-agent-cli.md` as `AgentExecutionRequest.durable_output` (per EAC-FR-VSNM). It accepts one chunk at a time and answers each with a durable acknowledgement or a typed failure.
- **the structured sink** — reached by `GOB-graduation-observability.md`'s producers and by `../ai/GRL-graduation-loop.md`'s drives. It accepts one record at a time on the same terms.

### Tauri commands

- `"read graduation logs (run id, phase id, pass, stream, cursor, limit, query)"` → `read_graduation_logs(run_id, phase_id, pass, stream, cursor, limit, query)` → one `GraduationLogPage`. `pass` is the `GraduationLogPassScope` above. Read-only, and it writes nothing to either file.

### Events (Tauri event bus)

- `"graduation log records appended"` — one or more records became durable for one run and one stream. Payload: `{ run_id, stream, latest_sequence }`. It carries no record content, so a consumer re-reads under its own cursor rather than rendering what the emitter sent.

### Internal (Rust API, not registered as Tauri commands)

- `initialize_graduation_log_storage(run_id)` — creates the run's `logs/` directory and both files, and writes the run's `GraduationLogIndexes`.
- `append_graduation_source_chunk(record)` — appends one `GraduationSourceChunk` and returns when it is durable.
- `append_graduation_structured_record(record)` — appends one `GraduationStructuredRecord` and returns when it is durable.
- `recover_graduation_log_storage(run_id)` — repairs a partial tail and replays the pending set idempotently.

### Typed errors

`read_graduation_logs` returns `no_project_open`, `run_not_found`, `unknown_phase` naming the value it was given, `unknown_stream`, `unknown_scope` naming a pass scope kind it does not hold, and `cursor_not_for_this_scope` where a cursor names another run or another stream. It returns no error for a scope that holds no record and none for a query that matches none: each is a status of the page.

Both append operations return one `GraduationLogFailure`.

### Where each concern of this feature is specified

This module owns the storage, and the behaviour around it is settled across the specifications below. The map is here so that the whole of a graduation run's logging is reachable from one place rather than reassembled by a reader who has to guess which module took which part:

- **The two private streams, their files, and the run directory they sit in** — GRS-FR-MABD, GRS-FR-KDOY, GRS-FR-LXGT, GRS-FR-CGSP, and `GRD-graduation.md` GRD-FR-OSCG.
- **The source record and its schema** — GRS-FR-MEPZ, GRS-FR-JAPO, GRS-FR-QDVH, GRS-FR-YXZX.
- **The structured record and its schema** — GRS-FR-NPIB, GRS-FR-TCKD, and `GOB-graduation-observability.md` GOB-FR-UASF.
- **Attribution: phase, pass, origin, and producer** — GRS-FR-KJVN, GRS-FR-ZQEM, GRS-FR-XUOA, GRS-FR-HBQT, GRS-FR-WNRC, GRS-FR-URSZ, GRS-FR-NUXT, GRS-FR-GBLC; `GOB-graduation-observability.md`,; `../ai/GLG-graduation-loop-logging.md` GLG-FR-FZHN, GLG-FR-FBKP; `GRD-graduation.md`,.
- **Phase visibility, and the phase a transition record carries** — GRS-FR-BSJQ, GRS-FR-EPPM, and `GOB-graduation-observability.md`,.
- **Iteration records and their phase-entry intervals** — `GOB-graduation-observability.md`,.
- **The run-level scope, and the records whose `pass` is null** — GRS-FR-JXRV, GRS-FR-NKZP, GRS-FR-TQAO, GRS-FR-BWQK, GRS-FR-HVUJ, GRS-FR-EMTV, and `../ui/GLW-graduation-log-window.md` GLW-FR-CQXJ, GLW-FR-DDXJ, GLW-FR-DKWB.
- **Reads, cursors, paging, statuses, decoding, and search** — GRS-FR-PQVK, GRS-FR-RZXA, GRS-FR-UEIL, GRS-FR-GSUY, GRS-FR-DYPS, GRS-FR-OWDT, GRS-FR-CTQI, GRS-FR-IQUA, GRS-FR-NSGX, GRS-FR-THZA, GRS-FR-QVTA, GRS-FR-OYIC.
- **Durability, backpressure, recovery, idempotency, and corruption** — GRS-FR-RGPN, GRS-FR-CYAP, GRS-FR-KQHY, GRS-FR-MXDJ, GRS-FR-MQEQ, GRS-FR-KYWE, GRS-FR-EYNU.
- **Mandatory persistence, the interruption it causes, and the same-pass Continue** — GRS-FR-EIXS, GRS-FR-DDSB, GRS-FR-OVCO, and `GRD-graduation.md` GRD-FR-IKVE, GRD-FR-IKVE.
- **Retention, and the exclusion of the bounds that govern other records** — GRS-FR-JQOO, GRS-FR-FORV, GRS-FR-NFLO; `GRD-graduation.md` GRD-FR-OSCG, GRD-FR-OSCG; `AGV-agent-activity.md` AGV-FR-XLZI, AGV-FR-EFNP, AGV-FR-VZBU; `../ui/RUN-runs.md` RUN-FR-DTJO, RUN-FR-GTEU, RUN-FR-MVTX.
- **Redaction before persistence** — GRS-FR-YXVY, GRS-FR-JWBW; `../tools/EAC-execute-agent-cli.md` EAC-FR-FKCN; `../ai/GLG-graduation-loop-logging.md` GLG-FR-YGCI; `GRD-graduation.md`.
- **The two run-scoped sinks and the durable acknowledgement they answer with** — `GRD-graduation.md` GXD-FR-IOZU; `../ai/GLG-graduation-loop-logging.md` GLG-FR-QKVI, GLG-FR-AVVZ, GLG-FR-XQTM; `GRD-graduation.md`,; `../tools/EAC-execute-agent-cli.md` EAC-FR-VSNM, EAC-FR-CXUE, EAC-FR-DUTR, EAC-FR-RLIW.
- **Which progress-bar phase may be opened, and what opening one does** — `../ui/GRU-graduation-runs.md`,; `../ui/RPV-run-progress.md` RPV-FR-JSQW, RPV-FR-LQUP, RPV-FR-MAIP, RPV-FR-NEVJ; `GRD-graduation.md` GRD-FR-LGDV.
- **The log window: its scoping, its stream toggle, its search, its follow mode, its states, its focus, and the isolation of one run's window from another's** — `../ui/GLW-graduation-log-window.md` GLW-FR-BLWH, GLW-FR-BVYN, GLW-FR-ELJO, GLW-FR-FCVA, GLW-FR-FPUX, GLW-FR-FXAL, GLW-FR-IMKM, GLW-FR-KTWX, GLW-FR-NMOD, GLW-FR-ONEV, GLW-FR-OOYK, GLW-FR-QDWA, GLW-FR-QMRV, GLW-FR-TMRQ, GLW-FR-WBTE, GLW-FR-XZQM, GLW-FR-YVKD, GLW-FR-ZPUH; `../ui/GRU-graduation-runs.md`,.
- **The separation from the application's own session diagnostics** — GRS-FR-TCKD; `LGC-logging.md` LGC-FR-SFVS, LGC-FR-TCZC; `GOB-graduation-observability.md`; `../ui/GLW-graduation-log-window.md` GLW-FR-TIKK.

## Functional requirements

1. **GRS-FR-MABD** Every graduation run owns **two append-only JSONL files** in its own run directory: `logs/source.jsonl` for raw executor output and `logs/structured.jsonl` for structured graduation observability records. One line is one JSON object, and a line is never rewritten, reordered, or removed.
2. **GRS-FR-KDOY** Both files are created when the run's private log storage is **initialized**, which happens once, in the durable write that creates the run record, so a run that has emitted nothing still has two readable streams.
3. **GRS-FR-LXGT** The files live under `FSA-filesystem-access.md::short_data_dir()` in the run's own directory and **never in the repository, in a project's `.synthesis/`, in a graduation or implementation worktree, or in Git history**, and every read and write of them goes through `FSA-filesystem-access.md` rather than through a bare filesystem call.
4. **GRS-FR-CGSP** `run.toml` keeps the run state, the **log indexes**, and the **latest persistence status**, and it holds **no log payload**: no chunk, no decoded text, no structured field value, and no excerpt of any of them.
5. **GRS-FR-MEPZ** Every record of either stream carries `schema_version`, `record_id`, `sequence`, `at`, `run_id`, `phase_id`, `pass`, `origin`, and `producer`. `phase_id` is exactly one of the four progress-bar ids of `GOB-graduation-observability.md`: `queued`, `working`, `review`, and `done`.
6. **GRS-FR-JAPO** A **source** record additionally carries `agent`, `container`, `source`, `encoding`, `data_base64`, and `byte_length`. `source` is `stdout`, `stderr`, or `executor`; `agent` and `container` are null where they do not apply.
7. **GRS-FR-QDVH** `encoding` is always `base64`, and decoding `data_base64` recovers the **complete redacted bytes of that chunk exactly**, including its boundaries and its position in the order. One emitted chunk is one record: chunks are neither joined, split, re-wrapped, nor normalized on their way to the file.
8. **GRS-FR-YXZX** The metadata of a source record is **retained with the chunk** rather than derived later, so every text line a surface draws from that chunk can be shown with the run, phase, pass, origin, producer, agent, container, and source it was written under.
9. **GRS-FR-NPIB** A **structured** record additionally carries `level`, `event`, and `fields`. `fields` is a flat map holding the structured values a surface renders, and the order of the stream is the append order of the stream.
10. **GRS-FR-TCKD** `structured.jsonl` holds the **graduation observability records of `GOB-graduation-observability.md`** and holds no record of `LGC-logging.md`'s session diagnostic buffer. The two channels stay separate: nothing this module writes reaches that buffer, and nothing that buffer holds reaches either file.
11. **GRS-FR-BSJQ** `phase_id` is persisted on **every** record and is **authoritative for phase visibility**. No reader infers a phase from the text of a record, from its position, or from the pass it belongs to.
12. **GRS-FR-EPPM** A record emitted by a **phase-transition action** uses the phase being **left** as its `phase_id`, and a record emitted after the transition uses the new current phase.
13. **GRS-FR-URSZ** `pass` is the number of the pass the record belongs to, or **null** for a run-level record.
14. **GRS-FR-NUXT** A record with `pass = null` carries an explicit `origin` and a non-empty `producer` naming its **non-agent source**, and its `origin` is `executor` or `application` rather than `agent`. `producer` is a stable identifier of the component that emitted the record rather than a sentence composed per call.
15. **GRS-FR-KJVN** **Every record carries a `phase_id`, whatever produced it**, and the attribution table of the contract surface is the authoritative mapping from producer to both. A producer the table does not name is a producer this module does not accept a record from, and no record is ever written with a `phase_id` outside the four progress-bar ids — there is no `null` phase, no `unknown` phase, and no fifth value for work that happens between phases.
16. **GRS-FR-ZQEM** A **semantic merge turn** carries `phase_id = done` and `pass = null`. It is the reconciliation turn of a stream update, work that belongs to a stream and to no run, so it stands in no pass of any run. Only a stream update produces it: no turn of a merge run is a semantic merge turn.
17. **GRS-FR-XUOA** The remaining producers are attributed as the table sets out and by no other rule: a **queue wait** carries `queued`; a **commit** carries `done` and `pass = null`, whether it is the commit step of a draft run or the apply step of a merge run (GRS-FR-WNRC); and a **stage transition** carries the phase being **left** (GRS-FR-EPPM).
18. **GRS-FR-GBLC** The application **never rewrites this attribution for display**: a record's persisted `pass`, `origin`, `producer`, and `phase_id` are what it was written with, for as long as the run record exists.
19. **GRS-FR-SXNY** `sequence` is assigned by this module, **ascends within one run and one stream**, and is never reused within it. The two streams number independently, so a cursor is meaningful only for the run and the stream it came from.
20. **GRS-FR-HZRP** `record_id` is assigned once by the producer, is unique within the run, and travels with the record through every retry, which is what makes a replay idempotent (GRS-FR-KQHY).
21. **GRS-FR-WFWD** Each stream carries a `GraduationStreamIndex` on the run record: its newest sequence, its record count, the sequence it is durable through, its durable length, and an append-only list of **segments**, one per phase and pass the stream holds records for. A read resolves its scope from the segments rather than by scanning the whole file.
22. **GRS-FR-MHJM** The index advances in the **same step** that makes the records it describes durable. While this process writes a run, this **live index** is the authority, so an index never names a record the file does not hold and a file never holds a record the index does not name.
22. **GRS-FR-FCRC** `run.toml` holds a **saved copy** of the live index. The copy is written when the run record is saved, and within a turn it can name fewer records than the files hold.
    - *Why:* a turn can run for hours, and only the loop's thread writes the run record, so the executor's output thread cannot save it.
22. **GRS-FR-IOHF** The first use of a run's index in a process **reconciles** the saved copy against the files. Where the two disagree, the files are the authority, so a later append never reuses a sequence the files already hold.
22. **GRS-FR-ZTCF** Every read that returns a run record, and every `read_graduation_logs`, answers with the **live index** where this process holds one for the run, and with the saved copy otherwise. A read never creates a live index.
23. **GRS-FR-RGPN** **Every append is durable before the producer receives success.** A producer that has been told a record was written may rely on that record surviving an immediate loss of the process.
24. **GRS-FR-CYAP** The writer uses **bounded backpressure or a durable pending-write queue**, and it **never drops, truncates, evicts, or silently acknowledges** a record. A producer that outruns the writer waits; it is never told a record was stored that was not.
25. **GRS-FR-JQOO** No size limit, memory eviction rule, rotation, or truncation applies to either stream. **Source output is complete for the lifetime of the run record**, and the bounds `AGV-agent-activity.md` AGV-FR-05 and AGV-FR-06 place on its own in-memory stream govern nothing here.
26. **GRS-FR-KQHY** Records accepted from a producer and not yet durable are the **pending set**, and each carries its `record_id`. Recovery **replays the pending set idempotently**: it reads the stream back from the sequence the index records as durable, collects the `record_id`s the file already holds, discards every pending record whose id is among them, and appends the rest in their original order.
27. **GRS-FR-MXDJ** A retry therefore **appends no record twice and loses no record whose acknowledgement was interrupted**, which is the one guarantee that makes a mandatory-persistence stop safe to retry.
28. **GRS-FR-MQEQ** A **partial final line** — the tail of a write the process did not finish — is repaired by truncating the file back to its last complete record before anything further is appended. That record was never acknowledged, so no producer believes it was stored, and the pending set is what restores it.
29. **GRS-FR-KYWE** A line **inside** the file that does not decode is **corruption rather than a partial tail**. The read that reaches it returns `unavailable` carrying the typed read failure of GRS-FR-EYNU — its `stopped_sequence`, its `byte_offset`, and the act that clears it — and this module **rewrites nothing and removes nothing**: a stream that cannot be read whole is reported as such rather than silently shortened, and the records before the damage are named by the sequence the failure carries rather than quietly returned as though they were the whole scope.
30. **GRS-FR-EIXS** **Creating or appending either required stream is mandatory.** Where a file cannot be created or appended, the backend **stops or cancels the current agent action**, persists the typed `GraduationLogFailure` in the run's interruption record, and moves the run to `interrupted` **before it performs another agent action or another state transition** (per `GRD-graduation.md` GRD-FR-IKVE).
31. **GRS-FR-DDSB** The interruption **identifies the persistence failure** — its code, its stream, and the act that clears it — and **retains the pending records for retry**, naming them by `record_id`, so nothing is lost while the run rests.
32. **GRS-FR-OVCO** **Continue retries the pending log writes first.** Where every pending write then succeeds, the run resumes the **same** pass: it opens no new pass and replays no already-persisted output. Where persistence still fails, the run stays `interrupted` and Continue stays available.
33. **GRS-FR-FORV** Both streams are **retained for the lifetime of the run record**. Archiving a run, discarding one, interrupting one, and continuing one each leave both files where they are and readable, and the reclamation of a run's branches and working copies reclaims neither file (per `GRD-graduation.md` GRD-FR-GMTX).
34. **GRS-FR-NFLO** The rule that a discarded run is **forgotten** by `AGV-agent-activity.md` AGV-FR-12 governs that module's own in-memory stream alone and reaches neither file here: a discarded run's logs are read exactly as a completed run's are.
35. **GRS-FR-YXVY** Executor output passes the **existing credential and session-identity redaction boundary** of `../tools/EAC-execute-agent-cli.md` EAC-FR-29 **before it reaches** `source.jsonl`. This module applies **no second redaction rule of its own** and inspects, masks, and rewrites nothing.
36. **GRS-FR-JWBW** It nevertheless **never writes an unredacted credential, session identity, login directory, or model transcript**: it accepts source chunks from the executor's masked delivery path and from no other, and it reads no vendor session-state directory (per `../tools/EAC-execute-agent-cli.md` EAC-FR-31). "Complete output" means complete output **after** that redaction.
37. **GRS-FR-PQVK** The module provides **four typed operations and no other route to either stream**: `read_graduation_logs(run_id, phase_id, pass, stream, cursor, limit, query)`, `append_graduation_source_chunk(record)`, `append_graduation_structured_record(record)`, and the event `"graduation log records appended"` carrying `{ run_id, stream, latest_sequence }` and no record content. **Every read, every append, every change event, and every search request names `run_id` and `stream`**, and a read additionally names the selected `phase_id` and the pass scope. Nothing writes either file by another path, and no operation anywhere reads one without naming both.
38. **GRS-FR-JXRV** **A record's scope is its persisted `pass` and nothing else.** A record whose `pass` is a number belongs to that pass. A record whose `pass` is `null` belongs to the **run-level scope**. This module assigns no run-level record to a pass, reads no phase-entry interval to place one, and returns no pass a record was not written with.
39. **GRS-FR-NKZP** **A read whose scope names one pass returns that pass's own records alone** — every record whose persisted `pass` is that number. **A read whose scope is `{ kind: "run_level" }` returns the run-level records alone** — every record whose persisted `pass` is `null`. Neither scope returns a record of the other.
40. **GRS-FR-TQAO** **Authoritative phase visibility narrows every scope alike.** A record is returned only where its persisted `phase_id` equals the phase the read names (GRS-FR-BSJQ). A run-level record of another phase is absent from the run-level scope of the phase being read, and a pass that never entered that phase holds no record there.
41. **GRS-FR-BWQK** **Each scope is one ordered result set.** Its records stand in ascending `sequence` order as the stream holds them, and `limit`, `next_cursor`, `older_cursor`, `oldest_reached`, `matched_total`, and the query all read that one set. A page holds no record twice, and a search of a scope searches every record that scope returns.
42. **GRS-FR-HVUJ** **The `presentation` object is computed for one read and is written to neither file.** It states whether the record is run-level and nothing more. The persisted `pass`, `origin`, `producer`, and `phase_id` are returned verbatim in `record`, so a record read through a pass scope and through a phase scope is byte-identical in both.
43. **GRS-FR-EMTV** A read whose scope is `{ kind: "phase" }` returns **every record of the named phase**, its pass-scoped records and its run-level records alike, in one ascending sequence order, each still carrying its own `presentation`. Nothing the phase holds is unreachable through it.
44. **GRS-FR-OWDT** **Paging reaches the whole scope in both directions.** Beside `next_cursor`, every page carries an `older_cursor` naming the `before` cursor for the page preceding it and an `oldest_reached` flag, so a caller that opened on the newest page can walk backwards to the scope's first record without guessing a sequence. `older_cursor` is null exactly where `oldest_reached` is true, a `before` page is returned in ascending sequence order like every other, and the two cursors of one page never overlap: the record set of a page and of the page before it are disjoint.
45. **GRS-FR-EYNU** **A read failure is reported on the page, is typed, and is never a persistence failure.** `unavailable` carries a `GraduationLogFailure` of kind `read` — `log_stream_corrupt` where a line inside the file did not decode, `log_stream_unreadable` where the file could not be read at all — carrying the `stopped_sequence` it read whole before stopping, the `byte_offset` it stopped at, and the act that clears it. `persistence_failed` carries a failure of kind `write` instead, with its pending record ids. The other two statuses carry no failure at all. A corrupt read **records that failure in the run's `last_read_failure`** and leaves `persistence.status` alone: a damaged file is not a failed write, so it interrupts no run, cancels no agent action, and moves no run state (GRS-FR-MRKO), and it stays readable in the run record until a later read succeeds.
46. **GRS-FR-RZXA** `read_graduation_logs` names the **run, the phase, the pass scope, and the stream** on every call, and a read answers for that combination alone. A read never crosses runs and never crosses streams.
47. **GRS-FR-UEIL** A **cursor is valid only for the run and the stream it names**. A cursor from another run or another stream is refused with `cursor_not_for_this_scope` rather than being answered from the wrong record set.
48. **GRS-FR-GSUY** An **absent cursor returns the newest page**, an **`after` cursor returns the records later than it in ascending sequence order**, and a **`before` cursor returns the page preceding it, also in ascending sequence order**, so a surface pages backwards without ever rendering a stream upside down.
49. **GRS-FR-DYPS** Every page carries its `entries`, `next_cursor`, `older_cursor`, `matched_total`, `oldest_reached`, `latest_sequence`, and the stream's own `status`, and each entry carries the persisted `record` beside the `presentation` computed for that read. `limit` bounds one page; `matched_total` counts everything the request matches whether or not the page holds it.
50. **GRS-FR-CTQI** A page's `status` is **exactly one of** `available`, `empty`, `unavailable`, and `persistence_failed`.
51. **GRS-FR-IQUA** `empty` means the **selected run, phase, pass, and stream hold no record**. It is never returned for a read that failed and never for a stream that could not be accessed.
52. **GRS-FR-NSGX** A search that matches nothing carries its own `search_no_match` result beside a status the scope's own records settle, so a query with no hit is never reported as the scope being empty and never as the **other** stream holding nothing.
53. **GRS-FR-THZA** `persistence_failed` carries the **typed persistence failure and the run's interruption state**, so a surface can say that output stopped rather than that a run produced none.
54. **GRS-FR-OYIC** **Source search matches the decoded output and every displayed source metadata field** — `run_id`, `phase_id`, `pass`, `origin`, `producer`, `agent`, `container`, `source`, the instant `at`, and the `sequence`, which are exactly the fields `../ui/GLW-graduation-log-window.md` GLW-FR-FXAL renders on every line (GRS-FR-YXZX). **Structured search matches the `event` and every structured field the surface renders.** Search is **case-insensitive substring** search, this surface having no other mode defined for it, the selected stream is part of every search request, and the set of searched fields and the set of displayed fields are one set, so nothing a reader can see is unsearchable and nothing invisible is matched.
55. **GRS-FR-QVTA** Decoding for display is **UTF-8 with replacement characters for invalid byte sequences**, and the **stored base64 stays unchanged**: what is decoded is what is searched and shown, and what is stored is what the executor produced.
56. **GRS-FR-UCZL** `"graduation log records appended"` is emitted **after** records become durable, carrying `{ run_id, stream, latest_sequence }` and **no record content**, so a consumer re-reads under its own cursor rather than rendering an event.
57. **GRS-FR-KCAK** The event names a **stream**: there is no form of it saying only that a run's logs changed, because a consumer told that much would have to re-read both streams to learn which grew.
58. **GRS-FR-TYQK** A read of a run this module holds no storage for answers with an `empty` page rather than an error where the run exists, and with `run_not_found` where it does not.
59. **GRS-FR-MRKO** This module **starts nothing, cancels nothing, and decides nothing about a run**. It reports a persistence failure to `GRD-graduation.md`, which is what stops the work and moves the run.
60. **GRS-FR-MBED** `log_storage_version` is `1`. A reader that meets a version it does not recognise renders no log for that run rather than guessing at the shape of the files, and that is neither a read failure nor a reason to refuse the run record.
61. **GRS-FR-JUFY** **A read rewrites nothing it returns.** Naming a record run-level (per `../ui/GLW-graduation-log-window.md` GLW-FR-CQXJ) rewrites no `pass`, `origin`, `producer`, or `phase_id` this module holds, and a re-read returns the record exactly as it was written.
62. **GRS-FR-HBQT** A turn of a **merge run** is attributed as a turn of any run, by the producer table and by no other rule. A `merge_work` turn is a `work_turn` producer: every record it produces, source and structured alike, carries `phase_id = working` and the number of the pass it belongs to. A `merge_review` turn is a `review_turn` producer: every record it produces carries `phase_id = review` and the number of its pass. The turn kind is not a producer, no producer named for a merge exists, and a clarification judgement taken over the questions of a merge turn is a `clarification_judgement` producer on the same terms. Pass numbers of a merge run ascend across a Continue and are never reused.
63. **GRS-FR-WNRC** The apply step of a merge run is a `commit` producer. It writes one structured record with the event `merge_applied` when the merge result reaches the base branch, and that record carries `phase_id = done` and `pass = null`, with `origin = application`. A refused or failed apply writes no `merge_applied` record, and the record holds the paths it names as project-relative paths and no file content.

## Non-functional requirements
- A read resolves its scope through the index segments, so opening a phase and a pass of a run that has written a million lines costs the page rather than the file.
- Appending never blocks the container's pipe from being drained beyond the bound backpressure sets: the writer's queue is what absorbs a burst, and a producer that must wait waits on the queue rather than on the disk for each line.
- Nothing here is a diagnostic channel and nothing here is a memory cache. `LGC-logging.md` holds the application's session diagnostics and `AGV-agent-activity.md` holds a run's in-memory stream; both are bounded and discarded, and this module is neither.
- Both files are plain JSONL, so a run's whole output is readable with ordinary tools when the application is not running.
- Every behaviour here is exercisable with no container runtime, no agentic CLI, and no provider credential, the sinks being fed by whatever the caller hands them.
