# Graduation execution

**Spec code:** `GXD`

## Intent
The single seam through which a graduation run reaches an execution agent, and the durable checkpoint that lets a stopped run be continued. Every turn a graduation run makes is one call through this seam: the `work` and `review` turns of a draft run, and the `merge_work` and `merge_review` turns of a merge run. The `semantic_rebase` turn of a stream update is one call through the same seam. So there is one place where a turn is composed, bounded, logged, attributed and cancelled. It also owns the escalation path: what happens when an agent stops to ask the author something, and how the answer reaches the turn that asked. Out of scope: the container, which is `../tools/EAC-execute-agent-cli.md`'s; the loop that decides which turn comes next, which is `../ai/GRL-graduation-loop.md`'s.

## Functional requirements
1. **GXD-FR-DRFF** `dispatch_graduation_turn` is the single seam through which a graduation reaches an execution agent, in the substitution sense as well as the routing one. It is an internal Rust API, is registered as no Tauri command, and is reachable from `src/**` by no route.
2. **GXD-FR-KXXB** Every turn is one call to `../tools/EAC-execute-agent-cli.md`'s `execute_agent_cli`, with the turn's execution directory as its `execution_directory`: the stream's working copy for a work turn, the run's throwaway checkout for a review turn (per `../ai/GRL-graduation-loop.md` GRL-FR-YKRI), the run's merge worktree for a `merge_work` turn, and the run's throwaway review checkout for a `merge_review` turn (per `GRD-graduation.md` GRD-FR-KZPT).
3. **GXD-FR-ODGX** The graduation context travels in the task's structured `input` object and nowhere else — not in the instruction, not in an argument vector, not in an environment variable, and not in a mounted file.
4. **GXD-FR-XQJR** Every dispatch names its **turn kind** — `work`, `review`, `semantic_rebase`, `merge_work`, or `merge_review` — and the executor uses it to resolve the vendor, model and effort for that kind (per `AIC-agentic-integrations.md` AIC-FR-19). A run of a draft dispatches `work` and `review` turns, a merge run dispatches `merge_work` and `merge_review` turns, and `semantic_rebase` serves a stream update alone.
5. **GXD-FR-GBBC** This module supplies no vendor, model, reasoning effort, image, credential or container argument. It supplies the execution directory, the task, the turn kind, a cancellation token and the sinks.
6. **GXD-FR-LBYG** The response envelope is the agent's **report of its turn and never evidence about the filesystem**. No claimed file list and no summary is accepted as what changed; what changed is read from the working copy.
7. **GXD-FR-XEUX** **The seam ends no run.** Every process outcome and every pre-launch error maps to a run outcome, and not one of them is terminal. A run that could not start a container rests rather than fails.
8. **GXD-FR-GMDI** The run's persisted **checkpoint** carries the pass it stands at, its `PassBudgetWindow`, the phase it resumes at (`work`, `review` or, for a merge run, `apply`), the change set held against the base commit, what the ignore rules kept out of it, the work turn's reported failure, the next turn's instruction, the escalation state, and the last blocker code with its repeat count. It carries no secret.
9. **GXD-FR-QGYA** The checkpoint is written durably before the run is reported as stopped, so a run continued after a relaunch continues from what the store holds rather than from what a loop remembered.
10. **GXD-FR-PWYD** The checkpoint's pass is the authority the loop counts against its window, whose highest pass is the floor plus the run's pass budget less one and is recomputed at every dispatch. The run's pass budget is the project's pass budget for a draft run and the constant of GXD-FR-BQLN for a merge run. It advances at one moment alone: a review that answers `revise` and starts another pass. A resumed run and an answered escalation each continue the pass they were in.
    - *Why:* A pass counted from the record of the passes made instead advances on every turn, so one interruption spends a pass the author never used.
11. **GXD-FR-HGSU** An escalation pauses the run durably. The agent's reason and its ordered questions are persisted on the run, the run moves to `awaiting_author`, and the stream is released.
12. **GXD-FR-BJYT** `answer_graduation_escalation(run_id, answers)` accepts an ordered set covering **every** recorded question and nothing else. Each entry carries the question's position, the answer, and a summary. A set that does not cover them all is refused and records nothing. A set against a run in a terminal state is refused with `run_state_not_permitted`. A set against a merge run whose pinned tip moved is refused with `merge_branch_moved` and records nothing (per `GRD-graduation.md` GRD-FR-XHSE).
13. **GXD-FR-XPUR** An answered escalation goes into the **phase that asked**, and the pass does not advance. A work turn's answers resume the vendor's session where it can, and go into a fresh work turn where it cannot. A review turn's answers go into a fresh review turn that resumes no session, and are cleared once it returns a readable verdict.
14. **GXD-FR-IOZU** Every dispatch carries an activity sink bound to the run, so what the agent does reaches the run it is doing it for (per `AGV-agent-activity.md`), and the run's two durable log sinks (per `GRS-graduation-run-log-storage.md`).
15. **GXD-FR-CYIW** Every dispatch of a run that has a source draft is attributed: the operation's interval and every provider-reported usage record are appended against that draft when the turn ends (per `GRD-graduation.md` GRD-FR-NHRY). A merge run has no source draft, so its dispatches are attributed to no draft.
16. **GXD-FR-TJRV** A continued run that stopped in a phase — on a blocker, an author pause, an application shutdown or a project change — resumes at the phase the checkpoint records, not at the start of the pass. The pass does not advance. A run that stopped in the review asks for a review turn and composes no work turn. A merge run that stopped in the phase `apply` resumes at that phase, dispatches no turn and spends no pass (per `GRD-graduation.md` GRD-FR-JSBE).
    - *Why:* The work did not change while the run was stopped, so a work turn spends a container to decide again what nobody reported a problem with.
17. **GXD-FR-MTVR** Every dispatch composes its execution controls from the project's settings read at that moment (per `../ai/GRL-graduation-loop.md` GRL-FR-KWNP): the turn carries the configured execution timeout and the caller-controlled cancellation, and nothing of an earlier turn's controls is reused.
18. **GXD-FR-QLFA** The seam is **substitutable whole**. A build under test binds an implementation that records every field of each request it was given, changes the execution directory as a real turn would, and returns any `TurnOutcome` of the contract surface. No command, project setting or provider record selects between the bound implementations.
19. **GXD-FR-NRWD** A substituted seam reaches the same run outcomes as the production one. A returned `cancelled`, `timed_out` or non-retryable `failed` outcome rests the run `interrupted`, writes the checkpoint durably before the run is reported as stopped (GXD-FR-QGYA), and releases the stream, with no wall-clock wait of any kind.
20. **GXD-FR-UZHX** A turn that stops on its own rests the run with the reason that names its cause (per `GRD-graduation.md` GRD-FR-PUXO): `timed_out` gives `execution_timeout`, a non-zero exit gives `agent_exited`, a process terminated from outside gives `agent_terminated`, an answer the application cannot read gives `unreadable_answer`, and a pre-launch error gives `launch_failed`. A stop reason already recorded on the run takes precedence.
    - *Why:* A pause that races the time limit is the author's stop, and the author must not read it as a timeout.
21. **GXD-FR-WJOW** The detail of a turn that stopped on its own states the executor's facts: for `execution_timeout`, how long the turn ran and the configured limit; for `agent_exited`, the exit code. It says whether the turn's work was committed. A `launch_failed` detail is one short sentence for the error kind, and the full error goes to the application log alone.

20. **GXD-FR-MKTZ** A merge run's turns are `merge_work` and `merge_review`. A `merge_work` turn's execution directory is the run's merge worktree. A `merge_review` turn's execution directory is the run's throwaway review checkout, a fresh session every time. Neither turn carries a supplementary mount. Both turns use the author's own selections of vendor, model and effort that the executor resolves for `work` and for `review` respectively (per `../tools/EAC-execute-agent-cli.md` EAC-FR-IRRD); the author sets no model or effort for the merge turns apart from those two selections. The task of each carries the merge context of `../ai/GRL-graduation-loop.md` GRL-FR-TXEB.
21. **GXD-FR-BQLN** A merge run's pass budget is **2** passes, a constant of this module and not a project setting. The run's `PassBudgetWindow` starts with a floor of 1 and a limit of 2 at its first dispatch. A merge run's Continue after the budget is spent moves the floor to the next pass and sets the limit to the floor plus 1, so that the window again holds 2 passes and no completed pass is repeated. A change of the project's pass budget does not change a merge run's window.
22. **GXD-FR-HSQV** An escalation of a `merge_work` or `merge_review` turn pauses the run on the terms of GXD-FR-HGSU and is answered through `answer_graduation_escalation` on the terms of GXD-FR-BJYT and GXD-FR-XPUR: the answers go into the phase that asked, and the pass does not advance. The escalation records the origin `work` for a `merge_work` turn and `review` for a `merge_review` turn.

## Contract surface

### Internal (Rust API, not registered as Tauri commands)
```text
dispatch_graduation_turn(request) → TurnOutcome
```

```
GraduationTurnRequest {
  run_id,
  turn_kind,                 // "work" | "review" | "semantic_rebase"
                             //   | "merge_work" | "merge_review"
  execution_directory,       // the stream's working copy, the review checkout, or,
                             //   for merge_work, the run's merge worktree
  task,                      // AgentTaskRequest carrying GraduationTaskInput
  supplementary_mount?,      // the rebase bundle, for semantic_rebase alone; a merge turn has none
  cancellation,
  activity_sink
}

PassBudgetWindow {
  pass_floor,                // the pass this budget window starts from; 1 for a new run
  pass_limit                 // pass_floor + the run's pass budget - 1 (the project's pass
                             //   budget, or 2 for a merge run); 0 before the first dispatch. Continue after exhaustion moves the floor
                             //   to the next pass, so no completed pass is repeated.
}

TurnOutcome =
    { kind: "completed", response }        // the agent answered
  | { kind: "escalation", reason, questions }
  | { kind: "cancelled" }
  | { kind: "timed_out" }
  | { kind: "failed", code, message, retryable }
```

### Tauri commands
- `answer_graduation_escalation(run_id, answers)` → `GraduationRun`, registered by `GRD-graduation.md`.

## Non-functional requirements
- Every behaviour here is exercisable with the dispatch seam bound to a recording double (GXD-FR-QLFA), on a machine with no container runtime, no credential and no network.
- The seam holds no mutable state between turns: everything a turn needs it is given, and everything a turn produced goes to the run record.
