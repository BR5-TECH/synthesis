## Intent

Graduation needs deterministic backend end-to-end tests that exercise the real graduation state machine through `dispatch_graduation_turn`, using protocol-valid scripted agent responses and isolated fixture repositories. The tests must show how a run changes its state, working copy, commits, checkpoint, and stream ownership for successful implementations, revisions, escalations, invalid reviews, cancellations, timeouts, and failures.

## Scope

- Cover the backend path from graduation start and dispatch through the final persisted run state and stream changes.
- Do not cover Tauri commands, UI rendering, or frontend persistence boundaries.
- Use the existing `agentic-cli-mock` binary for execution and CLI/container protocol tests.
- Use a deterministic dispatch-harness layer at the `dispatch_graduation_turn` seam for orchestration cases that must control working-copy changes and returned outcomes. The harness must not bypass the production graduation loop or state machine.
- Run offline, without provider credentials, network access, or a container runtime.

## Test framework requirements

- Introduce a declarative, fluent scenario API that reads as a user journey: arrange, script turns, act on the graduation run, and assert the resulting state and repository.
- Each scenario must use the production protocol types and validation rules for task input, turn outcomes, agent response envelopes, review verdicts, escalations, and checkpoint data. Do not assert private implementation details.
- Each scenario must use an isolated temporary project, stream, working copy, and run store. Tests must clean up all temporary data.
- Scripted turns must record every dispatch request and make it possible to assert turn kind, purpose, pass, task fields, execution directory, correction data, escalation answers, and dispatch order.
- Assertions must cover both persisted state and observable repository effects: state, pass and turn counts, checkpoint, stream claim, changed paths, commits, commit messages, and the absence of unintended changes.
- Failures must identify the scenario, turn, and assertion that failed. Avoid sleeps and wall-clock assertions; use bounded deterministic controls for timeout and cancellation cases.
- The harness must control project settings and expose deterministic controls for execution timeout, provider-call deadline, and the configured retry/pass budget. Tests must prove that settings are read before every graduation dispatch, including `semantic_rebase`, and that resumed runs use the setting values active at each resumed dispatch.

## Required scenario coverage

At minimum, cover these journeys:

1. A work turn implements the task and a `ready` review completes the run and commits only the changed paths.
2. A work turn is followed by a `revise` review; the next work turn receives the findings, and a later `ready` review completes the run.
3. Two substantive failed reviews leave the run in `awaiting_author` with the findings and no third pass.
4. A `revise` review with at most two minor findings completes as advisory-ready without another work turn.
5. A work turn reports an agent failure; the run still reaches review with the failure context, and the review result determines the next state.
6. A work or review turn requests an escalation; the run persists the ordered questions, releases the stream, and an answer continues the same pass with the answers delivered to the next turn.
7. A review returns an invalid verdict; one correction review is attempted, and a second invalid verdict blocks the run with the specified error code.
8. Cancellation, timeout, and non-retryable turn failure leave the run interrupted, persist the checkpoint, commit abandoned work when required, and release the stream.
9. A stopped run resumes from its checkpoint without repeating a completed pass or losing scripted context.
10. The binary mock rejects invalid arguments or stdin and replays valid protocol responses, proving the executor integration uses the declared protocol rather than test-only shortcuts.
11. A configured retry/pass budget is exhausted; the run enters `awaiting_author` with a clear reason and current context, and Continue resets only the exhausted loop budget and resumes without repeating completed work. Cover this for the graduation loop and for the conversation loop's equivalent author-visible pause and reset behaviour under the shared retry-budget contract.

## Requirements

- Tests must use the production protocols and the `dispatch_graduation_turn` seam.
- Tests must have a declarative fluent structure that is easy to read and extend.
- The suite must be deterministic and runnable in isolation and as part of the backend test lane.
- Add documentation for the scenario DSL, the harness boundary, how to add a scenario, and the coverage matrix.

## Specification impact

This feature requires coordinated specification updates before or during implementation; it is not complete if the tests only work around the current contracts.

- Update `GRL-graduation-loop.md` to replace fixed timeout, provider-call deadline, and maximum-pass constants with validated project settings read before every graduation dispatch, including `semantic_rebase`. Define the author-visible retry-budget exhaustion, `awaiting_author` pause, and Continue reset flow without changing the loop's completed-work or checkpoint semantics.
- Update `CVL-conversation-loop.md` to use the shared execution-timeout, provider-call-deadline, and retry-budget settings and to define the equivalent author-visible pause and Continue reset flow. Preserve the loop-specific meaning of its internal retry counter while using the shared setting and storage contract.
- Update the project-settings contract to define one shared setting for execution timeout, one shared setting for provider-call deadline, and one shared retry/pass-budget setting; use the current constants as defaults and define positive validation bounds. State how malformed stored values are repaired or rejected.
- Review `GXD-graduation-execution.md` and `GRD-graduation.md` for explicit contracts for fixture-controlled working-copy changes, recorded dispatches, timeout/cancellation control, checkpoint persistence, stream release, and Continue after budget exhaustion.
- Review `ACM-agentic-cli-mock.md` for invalid-argument and invalid-stdin behavior, valid protocol replay, deterministic timeout/cancellation control, and offline execution.
- Do not weaken production behavior only to make a test pass. If an existing specification conflicts with these accepted settings or author-visible pause decisions, name the conflict and update that specification as part of the graduated work.