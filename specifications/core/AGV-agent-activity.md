# Agent activity

**Spec code:** `AGV`

## Intent
What an agent CLI did during one run, kept per run and readable while the run is still under way. It exists because an agent turn is minutes or hours of work inside a container nobody can see into: the executor captures both of the CLI's streams and, until they are read as they arrive, says nothing about either until the container exits — so a turn that is working and a turn that is wedged look identical from outside, and a turn that failed takes its account of why with it when the container is removed. This module is where that account lives: it holds the stream `../tools/EAC-execute-agent-cli.md` produces (EAC-FR-32), keyed by the piece of work it belongs to, and it serves it to whichever surface is watching. Out of scope: producing the activity, normalizing it, and masking it, all of which are the executor's and none of which is repeated here; deciding which run a surface shows, which is `../ui/RUN-runs.md`'s; the graduation queue and its states, which are `GRD-graduation.md`'s; the **persisted activity stream of a graduation run**, which is `GRS-graduation-run-log-storage.md`'s and which this module neither writes, reads, bounds, nor stands in for; and the application's own diagnostic records, which are `LGC-logging.md`'s and are a different channel with a different purpose. **This module is a cache of a live run and not the record of one**: everything here is bounded, evicted, forgotten, and discarded with the process, and every one of those rules is this module's alone (AGV-FR-XLZI).

## Contract surface

### The operation

```
read_agent_activity(run_id, after?, limit?) -> ActivityPage
```

### The record

```
ActivityRecord {
  seq:               integer,   // assigned here, ascending within a run
  at:                string,    // RFC 3339 UTC, when the line arrived
  channel:           "stdout" | "stderr" | "executor",
  kind:              string,    // the executor's normalized kind (EAC-FR-33)
  summary:           string,    // one line, for a row
  payload:           string,    // the whole event, verbatim
  payloadTruncated:  boolean
}
```

### The page

```
ActivityPage {
  runId:     string,
  records:   [ActivityRecord],  // ascending by seq, whichever cursor produced them
  total:     integer,           // every record ever appended for this run
  dropped:   integer,           // records no longer held in memory
  latestSeq: integer            // zero for a run with no records
}
```

### The event

- `"agent activity appended"` — one or more records were appended for a run. Payload: `runId`, `total`, `dropped`, `latestSeq`. It carries no record content, so a consumer re-reads under whatever cursor it holds rather than rendering what the emitter happened to send.

### The sink

A run-scoped sink, constructed against one run identifier and handed to `../tools/EAC-execute-agent-cli.md` as `AgentExecutionRequest.activity`. The executor is never told a run identifier; the sink is what carries the attribution.

## Functional requirements

1. **AGV-FR-01** The store is process-wide and holds one entry per run. It outlives a project switch and a worktree switch, because a run that is working during one is still working after it, and it is discarded when the process ends.
2. **AGV-FR-02** A record carries the sequence, the instant the line arrived, the channel it came from, the executor's normalized kind, a one-line summary, the whole event verbatim, and whether that event was cut. The summary and the verbatim event are both kept: the summary is what a row shows, and the verbatim event is what a reader needs when this build's reading of it is the thing that is wrong.
3. **AGV-FR-03** Activity reaches the store through a sink bound to one run identifier at construction. The executor receives a sink and never a run identifier, so no launch can attribute its activity to another run, and a caller that starts no run has nowhere to put activity.
4. **AGV-FR-04** This module masks, redacts, rewrites, and inspects nothing. Every record arrives already masked by `../tools/EAC-execute-agent-cli.md` (EAC-FR-29), which is the only place that holds the credentials to mask against, and storing a second opinion here would be a second place for the rule to be wrong — the same arrangement `LGC-logging.md` LGC-FR-16 makes for diagnostic records.
5. **AGV-FR-05** A run keeps at most 4,096 records or 8 MiB of record text in memory, whichever bound is reached first, evicting oldest-first and counting what it evicted as `dropped`. `total` keeps counting everything appended, so a reader can always tell a stream that started where it is showing from one that was trimmed. The newest record is never the one evicted: a bound that can discard the line a reader is waiting for is worse than one briefly exceeded.
6. **AGV-FR-06** Two bounds hold across runs, and they are separate because what makes a run expensive to remember is its records and what makes it dangerous to forget is its sequence. At most 16 runs hold **records** at once, the least recently appended-to losing theirs first; a project's queue holds several runs and exactly one of them works (`GRD-graduation.md` GRD-FR-BNTC), so this bounds the accumulation of finished runs nobody has looked at rather than anything live. At most 256 runs are **tracked** at all, the least recently appended-to being forgotten outright first. Neither bound removes a run's file.
7. **AGV-FR-07** Every record is also appended to that run's own file, one JSON document per line, under `app_data_dir()` and through `FSA-filesystem-access.md` rather than through a bare filesystem call. The file is what makes a run readable after a relaunch and what holds the records the memory bounds evicted. It stops growing at 128 MiB, and the record that reaches that bound is followed by one notice in the file saying so, once, so a reader of the file alone can tell a stream that stops short from a run that stopped.
8. **AGV-FR-08** A failure to write the file is never reported to the caller and never interrupts the run. The file is the durable copy of something already held in memory and already announced, and a full disk is not a reason to stop an agent. A run whose file cannot be created at all streams and reads exactly as one whose file can.
9. **AGV-FR-09** Sequences are assigned by this module, ascend within a run, and are never reused within it. Each run numbers from its own beginning, so a cursor is meaningful only inside the run it came from, which is why every read names one. A run whose records were reclaimed under AGV-FR-06 keeps the sequence it had reached, and a line arriving for it afterwards continues that numbering. Restarting would be the worst failure available here and a silent one: a consumer holding a cursor would be told the run's newest sequence is *below* what it already holds, and would go on being told so for the rest of the run — a panel that simply stops, with nothing anywhere saying why.
10. **AGV-FR-10** `read_agent_activity` with no cursor returns the newest `limit` records, so a surface opening on a run that has been working for an hour starts at the end rather than paging to it. With a cursor it returns the records after it, oldest first, which is the delta a consumer reads when it has been told the run grew. A page holds at most 1,000 records whatever was asked for, records are always ascending by sequence, and a cursor naming a sequence the store no longer holds is positional rather than an error.
11. **AGV-FR-11** `"agent activity appended"` is emitted when records are appended, carrying the run's state and no record content. A consumer therefore re-reads rather than rendering the event, so what a surface shows is what the store holds rather than what an emitter believed.
12. **AGV-FR-12** A run that is discarded is forgotten here: its records leave memory and a later read of it answers as an unknown run does. What is left is a marker rather than nothing, and a line arriving for a forgotten run is refused rather than recorded — discarding a run and the run stopping are not the same instant. A discard is permitted while a run is still working (`GRD-graduation.md` GRD-FR-GMTX), the cancellation it performs only sets a flag the loop notices on its next poll, and the container's streams go on being drained until the child actually dies, so every line written in that window arrives *after* the author threw the run away. Without the marker one of those lines would recreate the entry the discard had just removed, numbering from one again and putting the run back in the eviction order, and a run the author discarded would start answering reads again. The marker is reclaimed by AGV-FR-06's bounds like any other entry, and it is reclaimed first. The run's file is left where it is and gains nothing further, because it is the only account of what an agent did to a directory that may still hold its edits, and discarding a run is a decision about the run rather than about the record of it.
13. **AGV-FR-13** A read of a run this store knows nothing about answers with an empty page rather than an error, because a surface opens on a run before its first line arrives and a run with nothing to say is not a failure.

14. **AGV-FR-XLZI** **Every bound and every forgetting in this module governs this module's own stream and nothing else.** The record and byte bounds of AGV-FR-05, the per-run bounds of AGV-FR-06, the tolerance of a failed file write in AGV-FR-08, the file's own 128 MiB stop in AGV-FR-07, and the forgetting of a discarded run in AGV-FR-12 are properties of a session-lifetime cache of a live run. They are **explicitly excluded from a graduation run's two persisted log streams, the activity stream among them,**, which `GRS-graduation-run-log-storage.md` owns and which are complete, durable, mandatory, and retained for the lifetime of the run record (GRS-FR-JQOO, GRS-FR-FORV, GRS-FR-NFLO). Nothing here evicts, trims, forgets, or tolerates the loss of a record of either of those streams, because nothing here writes one.
15. **AGV-FR-EFNP** A **failed write is tolerable here because the record is elsewhere.** AGV-FR-08 stands unchanged for this module's own file, and it stands because that file is a convenience copy of something already in memory and already announced — a run's mandatory account is `GRS-graduation-run-log-storage.md`'s, where a failed write stops the run instead (GRS-FR-EIXS). A build in which this module's tolerance were read as the graduation streams' would be a build that lost an agent's output and said nothing.
16. **AGV-FR-VZBU** This module is **not the source a graduation log surface reads**. `../ui/GLW-graduation-log-window.md` reads the persisted activity stream and invokes nothing here, and `read_agent_activity` answers for this module's stream alone whoever calls it.
17. **AGV-FR-ZSRV** The activity this module holds is **unfiltered**: every kind the executor reports, the payload, and the excluded kinds of `GRS-graduation-run-log-storage.md` GRS-FR-VZUZ among them. A graduation turn reaches this module and the persisted activity stream through two separate sinks, and the safe-content boundary of the persisted stream changes nothing here.

## Non-functional requirements

- Appending never blocks the run that is appending. The executor calls the sink from the thread draining the container's pipe, so an append that waited would stop the container being read.
- Memory is bounded by AGV-FR-05 and AGV-FR-06 together, so no run and no number of runs can grow the process without limit however much an agent prints.
- Nothing here is a diagnostic channel. `LGC-logging.md`'s buffer is the application's account of itself, capped for a whole session and cleared when the project changes; one turn can emit more events than that buffer holds, and evicting a session's diagnostics to make room for one agent's narration would cost more than it gives. The two channels carry the same events for a while and answer different questions.
- This module starts nothing, cancels nothing, and decides nothing about a run. It is a cache of a run in flight, and the durable record of one is `GRS-graduation-run-log-storage.md`'s.
