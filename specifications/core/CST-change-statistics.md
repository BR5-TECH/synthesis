# Change statistics

**Spec code:** `CST`

## Intent
The durable record of what an OpenSpec change cost to refine: the foreground time the author spent on it, the agent turns its conversations took, the tokens those turns reported, the edit proposals the author decided, and the versions it settled. A change is refined over many conversations, as a draft is, and an author who wants to know what that took has nothing to read once the session is over. So the account is kept on the terms of `DSS-draft-statistics-storage.md` — an append-only event log, duplicate-safe by event identity, folded on read, with captured activity alone and a statistic with no captured data reported **unavailable** rather than zero — in a log of the change's own. The change's own log stands beside its conversations in the repository machine store and survives the change and its stream, as they do (per `CMS-comments-storage.md` CMS-FR-ZILU). The draft a change graduated from keeps its own statistics, and this log adds nothing to them. Out of scope: rendering the totals, which is `../ui/OCW-openspec-change-workspace.md`'s; producing the activity, which belongs to `AGC-agent-conversations.md` and `CEP-change-edit-proposals.md`; the versions themselves, which `CVH-change-version-history.md` reads from Git; and the cost of a graduation run, which the run's draft records (per `GRD-graduation.md`).

## Functional requirements
1. **CST-FR-KGGT** A change's statistics log is `changes/<stream-id>/<change>/statistics.jsonl` in the repository machine store (per `RMS-repository-machine-storage.md`). It is created by the first event recorded for that change, holds one JSON object per line, and is never committed.
2. **CST-FR-WSQI** An event has the envelope of `DSS-draft-statistics-storage.md` with `stream_id` and `change` in place of `draft_id`. The vocabulary is `telemetry_activated`, `working_interval`, `conversation_turn`, `token_usage` of scope `conversation`, and `proposal_decision`, each with the fields DSS gives it, `working_interval` having the fields of `editing_interval`.
3. **CST-FR-NCLF** The log's **activation boundary**, the deduplication by `event_id`, the boundary clipping of intervals, the asynchronous recording, the per-statistic availability by source family, and the tolerance of an unreadable line are those of `DSS-draft-statistics-storage.md` DSS-FR-GAWO, DSS-FR-KYTB, DSS-FR-TUMX, DSS-FR-NCLP, DSS-FR-SVJU, DSS-FR-AWKD, and DSS-FR-DZQF.
4. **CST-FR-QJGO** `record_change_statistics_event(stream_id, change, event)` is the only path by which an event is recorded. It is internal, accepts the event, returns at once, and appends asynchronously.
5. **CST-FR-VJNQ** A conversation is **linked to a change** for these statistics when, and only when, its origin kind is `change_comment` or `change_discussion` and its target names that stream id and change name (per `AGC-agent-conversations.md` AGC-FR-05).
6. **CST-FR-WPHC** `read_change_statistics(stream_id, change)` folds the change's log and returns the `ChangeStatistics` of the contract surface. It reads the log and the version count alone, and creates nothing.
7. **CST-FR-WQKZ** **Working time** is the sum of the boundary-clipped durations of the distinct valid `working_interval` events, on the terms DSS-FR-RIDW gives editing time.
8. **CST-FR-HAMX** **AI interactions** is the count of distinct `conversation_turn` events whose `reached_model` is true, on the terms of DSS-FR-FXAE.
9. **CST-FR-PUAN** **Versions** is the number `CVH-change-version-history.md` counts for the change at the moment of the read (`count_change_versions`). It is `available` where that count answers and `unavailable` where it refuses. It is not an event of the log.
10. **CST-FR-OXGK** **Accepted proposals** and **rejected proposals** are the counts of distinct `proposal_decision` events by `decision`, on the terms of DSS-FR-OPMB. An edit proposal that closes with at least one accepted hunk is `accepted`, and one that closes with none is `rejected`.
11. **CST-FR-DCCV** **Conversation tokens** is the input and output total of the `token_usage` events of the log, on the terms of DSS-FR-UBLC.
12. **CST-FR-TSOU** `record_change_working_interval(stream_id, change, started_at, ended_at)` records one settled `working_interval`. It refuses `invalid_interval` where `ended_at` is not after `started_at`, and `invalid_change_name` where the name fails `OSM-openspec-mode.md` OSM-FR-IHUO.
13. **CST-FR-HDJD** No event of this log carries file text, a hunk, a rationale, feedback, a comment body, or conversation content, on the terms of DSS-FR-XRDM.
14. **CST-FR-PSUF** `"change statistics changed"` is emitted when one or more events for a change have been appended. It carries the stream id and the change name, and no totals.
15. **CST-FR-OZBY** Nothing in this module deletes a log. A log stays when the change is archived or removed and when its stream is removed.
16. **CST-FR-BLIF** `read_change_statistics` refuses `no_project_open` where no project is open and `invalid_change_name` where the name fails OSM-FR-IHUO. A stream that is no longer live still reads its log, with **versions** `unavailable`.
17. **CST-FR-AHLL** Every write goes through `FSA-filesystem-access.md`, and every operation logs under the `backend` domain on the terms of DSS-FR-ZMHT, naming the stream id and the change name. Nothing in this module reaches the network.

## Contract surface

### Tauri commands
- `"read change statistics"` → `read_change_statistics(stream_id, change)` → `ChangeStatistics` (CST-FR-WPHC).
- `"record change working interval"` → `record_change_working_interval(stream_id, change, started_at, ended_at)` (CST-FR-TSOU).

Errors are typed: `no_project_open`, `store_unavailable`, `invalid_change_name`, `invalid_interval`, and `read_failed`.

### Events
- `"change statistics changed"` — carries `{ stream_id, change }` (CST-FR-PSUF).

### Payload shapes
```
ChangeStatistics {
  stream_id, change,
  boundary_at,                // RFC 3339 UTC; null where the change has no boundary
  working_time_ms:     Measure,
  ai_interactions:     Measure,
  versions:            Measure,
  accepted_proposals:  Measure,
  rejected_proposals:  Measure,
  conversation_tokens: TokenPair,
  unreadable_lines
}
```
`Measure` and `TokenPair` are the shapes of `DSS-draft-statistics-storage.md`.

### Internal (Rust API)
- `record_change_statistics_event(stream_id, change, event)` — called by `AGC-agent-conversations.md` (AGC-FR-YFLJ), `CEP-change-edit-proposals.md` (CEP-FR-QVLO), and the working-interval command (CST-FR-QJGO).

## Non-functional requirements
- A read of a log of 10,000 lines answers in under 200 ms on a machine that meets the application's floor.
- Recording never blocks a turn, a decision, or the interface.
