# Change action requests

**Spec code:** `CAR`

## Intent
The persistence home for the requests an agent makes in an OpenSpec change discussion to **apply** the change or to **rework** it. A run costs time and tokens and writes the stream, so an agent never starts one: it asks, and the author decides. A request is posted into the conversation as one comment of the agent with a card, and the author confirms or declines it on the card, or tells an agent to decide it, in a comment or by voice. A confirmation starts the run through `OCR-openspec-change-runs.md`, and the request then names its run. Out of scope: composing a request, which is `../tools/RCA-request-change-action-tool.md`'s; the card, which is `../ui/OCW-openspec-change-workspace.md`'s; deciding by tool, which is `../tools/DCE-decide-change-edits-tool.md`'s; and the runs themselves.

## Functional requirements
1. **CAR-FR-VQJC** A **change action request** is an agent's request that the author start an apply run or a rework run on one change of one stream. Its record lives in the repository machine store at `changes/<stream-id>/<change>/action-requests/<request-id>.toml` (per `RMS-repository-machine-storage.md`), outside every worktree, and it is never committed.
1. **CAR-FR-MKQC** When a stream merge archives the change, its requests move with the change's store folder to `changes/<stream-id>/archive/<archivedAs>/action-requests/` (per `OAR-openspec-archive.md` OAR-FR-MSOM), and no operation of this module reads them there. A pending request refuses that merge (OAR-FR-BEIK).
2. **CAR-FR-TJAD** A record holds its id, the stream id, the change name, the action `apply` or `rework`, the instruction of a rework, the rationale, the thread id, the id of its comment, the agent participant, its state, the run id once a run started, the code of its last refused confirmation, and its times.
3. **CAR-FR-KYJU** A request stands in exactly one state: `pending`, `started`, `declined`, or `failed`. Only a `pending` request takes a decision, and the other three states are final.
4. **CAR-FR-KZOZ** `record_change_action_request(stream_id, change, action, instruction?, rationale, agent, origin)` is the only path by which a request comes into existence. It writes the record, then appends one agent comment into the turn's conversation through `append_as` with the rationale as its body and one `change_action_request` attachment (per `CMS-comments-storage.md` CMS-FR-QVZP). A failed append removes the record.
5. **CAR-FR-MNTP** A recording is refused, and writes nothing: `action_request_pending` while the change holds a pending request; `discussion_resolved` where the thread is resolved; `unknown_stream` where the stream is not live; `change_not_found` where `openspec/changes/<change>/` is not a folder in its working copy; and `blank_instruction` for a rework whose instruction is empty once trimmed.
6. **CAR-FR-MMPV** `confirm_change_action_request(request_id)` starts the run of the request through `OCR-openspec-change-runs.md`: `start_change_apply` for `apply`, and `start_change_rework_in_thread` with the request's instruction, its thread, and its agent as the requester for `rework` (per OCR-FR-TWDK).
7. **CAR-FR-UPOV** A start that succeeds writes the request as `started` with the run id and `decided_at`, then appends the decision comment (CAR-FR-TQJZ). The outcome carries the request, the run, the id of the decision comment, and the origin kind of the thread.
8. **CAR-FR-ARMK** A start that is refused keeps the request `pending`, records the refusal code as its last refusal, appends no comment, and returns that typed refusal. The refusals `unknown_stream` and `change_not_found` write the request as `failed` with that code in its place, and append the decision comment.
   - *Why:* Every other refusal can clear, so the author can confirm the same request again; a gone stream or change cannot.
9. **CAR-FR-TQJZ** A decision appends one comment into the request's thread through the human write path, on the terms of `NCP-new-change-proposals.md` NCP-FR-EBSA. It states the decision and the action, names the title of the run where one started or the code where the request failed, and carries any feedback.
10. **CAR-FR-WXDA** `decline_change_action_request(request_id, feedback?)` writes the request as `declined` with `decided_at`, appends the decision comment, and starts nothing.
11. **CAR-FR-XNQC** A decision against a request that is not `pending` is refused with `already_decided`, and a decision that names no request of the open project is refused with `request_not_found`. Nothing is written.
12. **CAR-FR-OKOV** The recordings and the decisions of one change are serialized under one exclusion, so two confirmations of one request never start two runs.
13. **CAR-FR-RKRY** A request is decided only by the author: on its card, or by `../tools/DCE-decide-change-edits-tool.md` on the author's explicit instruction, which calls the functions behind the commands in-process with no feedback. No request expires or is decided on a timer, and no agent call alone starts a run.
14. **CAR-FR-ZSIQ** `"change action requests changed"` is emitted after a recording, a decision, and a refused confirmation are durable. It carries the stream id, the change name, the request id, and the cause.
15. **CAR-FR-ALHA** `list_change_action_requests(stream_id, change)` returns every request of the change, the most recently created first. It writes nothing.
16. **CAR-FR-IFGR** `read_change_action_requests(stream_id, change)` returns the same requests, each with the state of its run where one started, for a turn that assembles its input (per `AGC-agent-conversations.md` AGC-FR-KRMV). It reconciles nothing and writes nothing.
17. **CAR-FR-UCSJ** Every command errors with `no_project_open` when no project is open. A decision also returns the identity refusals of its comment (per `CMS-comments-storage.md` CMS-FR-12), and then writes nothing and starts nothing.
18. **CAR-FR-ENOZ** Every operation logs through `LGC-logging.md` under the `backend` domain: an `INFO` record on a recording and a decision, and a `WARN` record on every refusal. A record names the stream id, the change name, the request id, the action, the state, and the code. No record holds the instruction, the rationale, or feedback.
19. **CAR-FR-AQHX** Every filesystem operation goes through `FSA-filesystem-access.md`. Nothing in this module reaches the network, and this module dispatches no agent turn.

## Contract surface

### Tauri commands
- `"list change action requests"` → `list_change_action_requests(stream_id, change)` → `ChangeActionRequest[]` (CAR-FR-ALHA).
- `"confirm change action request"` → `confirm_change_action_request(request_id)` → `ChangeActionOutcome` (CAR-FR-MMPV).
- `"decline change action request"` → `decline_change_action_request(request_id, feedback?)` → `ChangeActionOutcome` (CAR-FR-WXDA).

Errors are typed: `no_project_open`, `request_not_found`, `already_decided`, `action_request_pending`, `discussion_resolved`, `unknown_stream`, `change_not_found`, `blank_instruction`, `write_failed`, every refusal of a start of `OCR-openspec-change-runs.md`, and the identity refusals of `CMS-comments-storage.md` CMS-FR-12.

### Events
- `"change action requests changed"` — carries `{ stream_id, change, request_id, cause }`, cause `"recorded" | "decided" | "refused"` (CAR-FR-ZSIQ).

### Payload shapes
```text
ChangeAction = "apply" | "rework"

ChangeActionRequestState = "pending" | "started" | "declined" | "failed"

ChangeActionRequest {
  id,
  stream_id, change,
  action: ChangeAction,
  instruction?,              // rework alone
  rationale,
  thread_id, comment_id,
  agent: Participant,
  state: ChangeActionRequestState,
  run_id?,                   // started alone
  last_refusal?,             // the code of the last refused confirmation, or of the failure, as a typed
                             //   string `code: detail`; it carries no diagnostics, which only the refusal
                             //   that a confirmation answers holds (OFP-openspec-failure-policy.md)
  created_at, decided_at?
}

ChangeActionOutcome {
  request: ChangeActionRequest,
  run?,                      // GraduationRun; a started request alone
  comment_id?,               // the decision comment
  origin_kind                // "change_discussion" | "change_comment"
}
```

### Internal (Rust API)
- `record_change_action_request(stream_id, change, action, instruction?, rationale, agent, origin)` — called by `../tools/RCA-request-change-action-tool.md` alone (CAR-FR-KZOZ).
- `read_change_action_requests(stream_id, change)` — called by `AGC-agent-conversations.md` alone (CAR-FR-IFGR).
- The functions behind the decision commands, called in-process by `../tools/DCE-decide-change-edits-tool.md` (CAR-FR-RKRY).

## Non-functional requirements
- A recording costs one record write and one comment line. A decision costs one record write, one comment line, and, for a confirmation, one start.
- The store is private to this machine, so a request never reaches the repository.
