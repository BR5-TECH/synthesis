# Progress reporting

**Spec code:** `PRG`

## Intent
Backend facility through which any long-running operation in the application announces that it is running and how far along it is, so the UI has one channel to watch instead of a bespoke one per module. It exists because the status bar's progress region (`../ui/STB-status-bar.md`) must be able to display work originating in unrelated modules — agent runs, Git network operations, project scans, plugin and adapter installs, search, change-set computation — without knowing which of them exist or how any of them works. Reporting is asynchronous and never blocks the operation doing the work, so an operation that finishes quickly simply appears and disappears while a slow one stays visible for as long as it runs. Out of scope: this module starts, cancels, and retries nothing — it observes and republishes, so control over an operation stays with the module that owns it — and it keeps no history, so a terminated operation is gone rather than archived. An operation may carry a producer-defined **activation destination** that tells the status bar which surface owns the work and which target of that surface to select, so the author can move from a progress row to the activity it describes; the destination is data the producer supplies and the facility only carries it.

## Contract surface

### Tauri commands
- `"list in-flight operations"` → `list_in_flight_operations()` → `[Operation]` — the operations in flight at the moment of the call, most-recently-started first. Returns an empty list when none is in flight, and never returns an error.

### Events (Tauri event bus)
- `"operation progress"` — emitted when an operation is registered, when its progress or label advances, and when it reaches a terminal state. Payload: a single `Operation`.

### Payload shapes
```
Operation {
  id,                        // unique for the lifetime of the running application
  kind,                      // producing module, e.g. "run" | "git" | "scan" | "install" | "search" | "changes" | "index"
  label,                     // short human-readable description, e.g. "Indexing project…"
  state: "running" | "finished" | "failed" | "cancelled",
  completed?,                // units done; absent for an indeterminate operation
  total?,                    // total units; absent for an indeterminate operation
  sequence,                  // monotonically increasing registration ordinal
  activation?                // producer-defined destination; absent when the operation has none
}

Activation =
    { type: "graduation_run", runId }         // a graduation run, by its run id
  | { type: "discussion", discussionId }      // a discussion, by its discussion id
  | { type: "git_push", branch }              // a Git push, by the name of the branch it publishes
```

`activation` is optional and typed. Its `type` names the owning surface, and the remaining fields are the stable identity of the target on that surface. An operation without `activation` has no destination.

An operation is **in flight** while `state` is `"running"`, and **terminated** in each of the other three states.

## Functional requirements
1. **PRG-FR-01** `list_in_flight_operations()` exists as a Tauri command and `"operation progress"` as an event, both with the documented payload shape. In the walking-skeleton build the implementation may register synthetic operations; `../ui/STB-status-bar.md` must be fully exercisable against them.
2. **PRG-FR-02** An operation is in flight from the moment it is registered until it reaches a terminal state. `list_in_flight_operations()` returns exactly the operations in flight at the moment of the call, ordered by descending `sequence`, so its first element is the most recently started.
3. **PRG-FR-03** `id` is unique across every operation for the lifetime of the running application and is never reused, so a consumer keying by `id` never confuses one operation with a later one. `sequence` increases with every registration and is the ordering key of PRG-FR-02.
4. **PRG-FR-04** Every registered operation emits `"operation progress"` on registration and again on reaching its terminal state. A consumer that observes only events therefore converges on the same in-flight set the command would return, without polling.
5. **PRG-FR-05** An operation carrying both `completed` and `total` is determinate; one carrying neither is indeterminate. `completed` never exceeds `total`, and neither value decreases across the updates of a single operation. An operation may start indeterminate and become determinate once its total is known; it never reverts to indeterminate.
6. **PRG-FR-06** Reporting is asynchronous: registering, updating, and terminating an operation never block the work being reported, and no consumer's absence, slowness, or failure to subscribe delays it. An operation that completes before it ever updates emits its registration and its terminal state and nothing between.
7. **PRG-FR-07** Intermediate updates are coalesced so a high-frequency reporter cannot flood the event bus: updates within a coalescing window collapse to the latest value for that operation. Registration events and terminal events are never coalesced away — each is delivered.
8. **PRG-FR-08** A terminal event is emitted exactly once per operation. After it, the operation is absent from `list_in_flight_operations()` and no further `"operation progress"` event carries its `id`.
9. **PRG-FR-09** An operation that fails or is cancelled reaches a terminal state exactly as a successful one does and leaves the in-flight set the same way, so a failure never strands an operation as permanently running. This module carries no error detail and surfaces no failure to the user; reporting the failure belongs to the module that owns the operation and to the surface that requested it.
10. **PRG-FR-10** This module retains no record of terminated operations. Nothing it holds is persisted, no command returns operations that have terminated, and there is no query for what ran earlier in the session.
11. **PRG-FR-11** Attribution is opt-in per operation: a module reports through this facility for the work it chooses to make visible and is not obliged to attribute every command it exposes. The operations expected to attribute in v1 are agent runs (`ADP-adapters.md`), Git network operations (`GTC-git.md`), the project scan (`ASC-artifact-scanning.md`), plugin and adapter installs (`GSS-global-settings-storage.md`), search (`SCC-search.md`), change-set computation (`CHC-changes.md`), BM25 index passes (`BMI-bm25-indexing.md`), the check of a work stream merge (`GRB-graduation-rebase.md` GRB-FR-RNGX, `WKS-work-streams.md` WKS-FR-MWYD), work stream updates (`GRB-graduation-rebase.md` GRB-FR-XPLV, GRB-FR-CLRO), graduation runs that are executing (`GRD-graduation.md` GRD-FR-ZHNV), and discussion agent turns (`AGC-agent-conversations.md` AGC-FR-24). A module that begins attributing later, or one added after v1, participates without any change to this contract.
12. **PRG-FR-12** `kind` names the producing module so a consumer may group or decorate by it, but recognising a `kind` is never required to render an operation: an unrecognised `kind` is displayed generically from `label` and progress alone (per `../ui/STB-status-bar.md` STB-FR-09).
13. **PRG-FR-13** Closing a project (per `PST-project-storage.md` PST-FR-14) and changing the project's active worktree (per `WTC-worktree-context.md` WTC-FR-08) terminate every operation scoped to the outgoing content root as part of the teardown, so none is left in flight against a root the application is no longer reading.
14. **PRG-FR-14** This module mutates nothing observable outside its own event bus and command: no file, no repository, no setting, and no state belonging to the operation it reports on.
15. **PRG-FR-15** `list_in_flight_operations()` answers normally when no project is open, because an operation need not be project-scoped — a plugin install can be in flight before or between projects.
16. **PRG-FR-KXQW** An operation may carry one `activation` destination, supplied by its producer at registration. The supported `type` values are `graduation_run`, which carries the run id of a graduation run; `discussion`, which carries the discussion id of a discussion; and `git_push`, which carries the name of the branch a Git push publishes. Both `list_in_flight_operations()` and every `"operation progress"` event of the operation carry the same `activation` it was registered with, and it does not change while the operation is in flight.
17. **PRG-FR-TBZN** An operation without an `activation` has no destination, and a consumer opens nothing for it. This module does not derive a destination from `kind`, from `label`, or from any other field, and a consumer must not either. A producer that has no defined destination for an operation, such as a fetch of remote branches, registers it without one.
18. **PRG-FR-HDQS** A producer owns the `label` of its operations. The label of an operation reporting work about a named subject, such as a draft or an artifact, names that subject, so a consumer displays the label as it is and adds no subject name of its own.

## Non-functional requirements
- The coalescing window is an implementation choice (order of a few tens of milliseconds); the contract is only that bursts collapse and that registration and terminal events always arrive.
- Registering and updating an operation must be cheap enough to sit inside a tight loop of the reporting operation without measurably slowing it.
- The in-flight set is held in memory only and is bounded by the number of operations genuinely running; nothing accumulates across a session.
- Neither the command nor the event requires network access.
