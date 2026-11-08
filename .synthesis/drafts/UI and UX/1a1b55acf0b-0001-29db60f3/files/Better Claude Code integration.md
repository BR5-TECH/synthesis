## Intent

Agentic CLI integration needs a visibility improvement. IDE should be providing more visibility into what agent is doing during graduation run.

The graduation agent announces a short plan of milestones at the start of a turn, then announces when it starts and when it finishes each milestone. The application reads these announcements and shows the milestones of the current turn in the graduation run view. The author sees where the agent is in its work without reading the log. 

 

## User journey

The user is an author who watches a graduation run in the Runs panel.

1. The author graduates a draft. The run starts a work turn.
2. The agent announces its plan: 3 to 5 milestones. A list of these milestones appears under the run's stage bar and above the passes.
3. The agent starts a milestone: the list shows it as active. The agent finishes it: the list shows it as done.
4. The work turn ends and the review turn starts. The list now shows the milestones of the review turn only. It is empty until the review agent announces its plan.
5. A turn ends before every milestone is done (for example the run is paused, interrupted, or the agent forgot a marker). The milestones that are not done show as **unknown**.
6. The agent writes a bad marker. The author sees nothing wrong: the application ignores it.

## Requirements

### Scope

- Turns that announce milestones: `work`, `review`, `merge_work`, `merge_review`. Turns `semantic_rebase` and `pr_maintenance` announce nothing, and a correction turn of a review (GRL-FR-QKDC) announces nothing.
- The feature is vendor-neutral. The executor reads markers after it normalizes the vendor stream to its activity kinds, so it works for Claude Code and Codex alike.
- The feature changes no run state, no stage, no observability record (GOB), and no verdict. A milestone is a description for the author only.

### Marker format

- A marker is one line in an agent text message. The line starts with the prefix `SYNTHESIS_MILESTONE`, then one space, then one JSON object to the end of the line. The object has a closed field set. Three events exist:
  - plan: `{"event":"plan","milestones":["title","title"]}`. The prompt asks for 3 to 5 titles. The executor accepts 1 to 5. A title is non-blank, has one line, and has at most 60 characters.
  - start: `{"event":"start","index":1}`
  - done: `{"event":"done","index":1}`
- `index` counts from 1 in the order of the plan.
- Only the first valid plan of a turn counts. A later plan is ignored. A start or done marker before a plan, with an index outside the plan, or for a milestone that is already done is ignored.
- A malformed marker (not JSON, an unknown field, a wrong type, a bad title or index) is ignored. It makes no milestone record, never fails the turn, and its line stays in the ordinary `message` activity.

### Executor

- The executor scans each line of each agent text message. A line with a valid marker becomes one activity of the new safe kind `milestone`. It is delivered to the activity sink and to the durable sink, in the order observed, with the normal masking and `record_id`.
- The `summary` of a `milestone` activity is the validated marker written as one compact JSON line. It carries no verbatim payload. The title limits must let a full plan fit the summary bound of EAC-FR-32. If they do not fit, name the conflict and change the limit, do not truncate a plan.
- A line that becomes a `milestone` activity is not also stored as a `message` activity. Other lines of the same message stay in the `message` activity.

### Prompts

- `work.md`, `review.md`, `merge-work.md`, and `merge-review.md` each carry one added instruction block. It tells the agent to: write the plan marker before any other work; write a start marker when it begins a milestone; write a done marker when it finishes one; keep the plan to 3 to 5 high-level milestones; write each marker on its own line in a text message, never inside the final result document. It gives the exact format with one example.
- The block names no deliverable (GRL-FR-DAIB). It states that markers are for display only and that a missing marker is never a failure.

### Graduation run view

- The selected run shows a **milestone list** under its stage bar and above its passes. It shows the milestones of the **current turn only**. The current turn is the turn of the run's current stage and pass. For a run that has ended, it is the last turn the run dispatched. A new turn starts with an empty list.
- The list is not rendered when the current turn has no valid plan.
- Each milestone shows one state: **pending** (no marker yet), **active** (a start marker, no done marker), **done** (a done marker), or **unknown**. Once the turn has ended, every milestone that is not done shows as unknown. A turn has ended when the run is no longer working or reviewing that turn: it finished, paused, was interrupted, waits for the author, or ended. A done marker without a start marker still makes the milestone done.
- The list reads `milestone` records from the run's persisted activity stream, for the current phase and pass. It refreshes when new records are appended. It does not read the observability record and does not parse `message` summaries.
- Milestone titles are written by a model. They are rendered as escaped plain text (GRU-FR-MRPE).
- A `milestone` row in the log window and in the agent output section has its own kind label. The summary is shown as written.

### Out of scope

- Vendor plan tools (such as TodoWrite) are not used.
- No milestone history across passes, and no milestone highlight in the log.

### Specification impact

Update these together. Do not work around an incompatible specification.

- `specifications/tools/EAC-execute-agent-cli.md`: add `milestone` to `ActivityKind` and to the safe kinds of EAC-FR-DWGS; define the marker scan, the marker contract, and the summary form above; keep the rule that `message` lines without a valid marker are unchanged.
- `specifications/core/GRS-graduation-run-log-storage.md`: add `milestone` to the safe kinds of GRS-FR-WJIA; make the read of one phase and pass able to select by kind if it cannot now.
- `specifications/ai/GRL-graduation-loop.md`: the four prompt files carry the milestone block (GRL-FR-GADT, GRL-FR-DXLU, GRL-FR-OTRH); the correction turn and `refusals.md` carry none.
- `specifications/ui/GRU-graduation-runs.md`: add the milestone list, its states, and its reads; change the reload rule of GRU-FR-QKSY so that a selected non-terminal run refreshes its list on a `"graduation log records appended"` event.
- `specifications/ui/GLW-graduation-log-window.md`, `specifications/ui/RUN-runs.md` (RUN-FR-YQAE), and `specifications/core/AGV-agent-activity.md`: the new kind and its label.
- Review `specifications/ai/GLG-graduation-loop-logging.md`, `specifications/infra/CCP-claude-code-cli-protocol.md`, and `specifications/infra/CDX-codex-cli-protocol.md`; change them only if the new kind needs it.
- Add tests for: marker parsing (valid, malformed, duplicate plan, out-of-range index), masking of marker lines, delivery of the `milestone` kind to both sinks, and the list states including unknown.