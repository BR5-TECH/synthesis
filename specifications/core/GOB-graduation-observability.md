# Graduation observability

**Spec code:** `GOB`

## Intent
What a graduation run tells the author about itself. A run carries a versioned observability record describing the stage it stands at, the condition it stands there in, how it reached that stage, and what each pass was asked to do and judged. The record is a description of the run rather than a part of it: the state machine of `GRD-graduation.md` is unchanged by everything here, and nothing in this record gates, delays or decides a transition. It exists so a surface can say what a run is doing, and so an author looking back at a run that went round twice can read what the review asked for and what the next turn was told. A merge run (a graduation run that carries `merge` data) is described by the same record: it stands at the same four stages in the same conditions, and only the words a surface shows for its stages differ.

## Functional requirements
1. **GOB-FR-XMDU** Every run carries a `GraduationObservability` record holding `observability_version`, the current stage, the stage condition, the stage history, and the pass records. `observability_version` is `1`, and a reader that does not recognise a version renders none of it rather than guessing at its shape.
2. **GOB-FR-JAJU** The **four stages** are, in order, `queued`, `working`, `review`, and `done`. They are what the author is shown rather than phases the module branches on, and there is no fifth.
3. **GOB-FR-HXUZ** The current stage and the stage condition are **persisted rather than inferred**, and they are derived from the run's state exactly as the state-to-stage mapping of the contract surface sets out.
4. **GOB-FR-ZMCA** The stage condition is exactly one of six: `active`, `waiting`, `paused`, `blocked`, `stopped`, `complete`. Exactly one stage is current at a time.
5. **GOB-FR-YZAT** Nothing in this record gates, delays or decides a transition. A surface that could not read it would show a run less well; it would not change what the run does.
6. **GOB-FR-VVNI** `stage_history` is append-only, and an entry is appended **only when the stage changes**. A change of condition alone — a run coming to rest on an escalation, a block, an interruption or a terminal outcome — appends none.
7. **GOB-FR-HDZI** Each stage-history entry records the stage moved from, the stage moved to, the pass the move belongs to, the instant, and the reason. The reason is one of the stage-transition reasons of the contract surface.
8. **GOB-FR-SJVW** The move **into** a stage is set by the work starting rather than by the clock: `working` when a work turn is dispatched, `review` when a review turn is dispatched, and `done` when the run reaches a terminal state.
9. **GOB-FR-BTXN** Two reasons make a backward move. `review_revision` moves `review` to `working` when a review verdict starts another pass. `blocked_retry` moves the stage a blocked run stood at to `queued` when the author continues it. No other entry has a target stage that precedes its source.
10. **GOB-FR-XYCY** One **pass record** is appended for each pass the run makes. It holds the prompt the turn was asked to answer — the captured prompt on the first pass, the review's instruction on a later one — the review's verdict and rationale, the findings, and the instruction the next turn was told. Nothing is reduced to a code.
    - *Why:* The instruction every work turn carries is identical for every run and every project (per `../ai/GRL-graduation-loop.md` GRL-FR-DXLU), so a record that held it would say nothing about this pass.
11. **GOB-FR-ABRE** A pass record's status is `working` while its review has decided nothing, `passed` where the review answered `ready`, and `failed` where it answered `revise`, carrying the finding count.
12. **GOB-FR-DKCJ** A finding is serialized whole: its severity, its description, its affected paths and its correction. The application neither summarizes a finding nor drops one it did not understand.
13. **GOB-FR-EZDN** Every field of this record is persisted in the same durable write as the change that set it, and is read back after a relaunch like every other field of the run.
14. **GOB-FR-UASF** Nothing here carries a credential, a token, a session identity or a byte of a project file. A path is a project-relative path and never an absolute one.
15. **GOB-FR-KRWE** A merge run carries the same `GraduationObservability` record as every run, with the same four stages, the same six conditions, the same state-to-stage mapping, the same transition reasons and the same `observability_version`. The record holds no field that marks a run as a merge run: the surface reads the presence of the run's `merge` data to know it.
16. **GOB-FR-PLTB** The words a surface shows for the stages of a merge run are **data of this spec**, set out in the merge stage labels of the contract surface, and a surface selects them where the run carries `merge` data. No stage, condition, history entry or reason of a merge run has a name that differs from the ordinary one. A run without `merge` data is never shown the merge labels, and a merge run is never shown the ordinary labels or called a draft graduation.
17. **GOB-FR-DNHS** A merge run enters `working` when a `merge_work` turn is dispatched and `review` when a `merge_review` turn is dispatched, by the rule of GOB-FR-SJVW. The apply step that follows a `ready` merge review belongs to the `review` stage: the run enters `done` with the reason `finished` only when the apply succeeds. A merge run that rests `blocked` at the apply step stands at `review` with condition `blocked`, and its Continue moves it to `queued` with the reason `blocked_retry` by the rule of GOB-FR-BTXN.
18. **GOB-FR-VCXQ** A merge run has pass records on the terms of GOB-FR-XYCY, GOB-FR-ABRE and GOB-FR-DKCJ. The `task` of its first pass is the merge statement the run carries as its prompt, and the `task` of a later pass is the review's instruction. Every `revise` verdict of a merge review makes its pass record `failed` with its finding count, whatever the severities of the findings, because the advisory-minor rule of ordinary runs does not apply to a merge review. Pass numbers ascend through the whole life of the run: a Continue after the pass budget is spent starts the next number and does not restart at 1.
19. **GOB-FR-MZFA** A merge run has no source draft, so this record holds no draft name for it and no statistic of it. The result of an applied merge is not a field of this record: a surface reads it from the `merge.result` of the run, which holds how the merge was published (`uncommitted` or `commit`), the merge commit where one was made, and the project-relative paths the merge changed. Until the apply succeeds the run has no `merge.result`, and the surface shows none.

## Contract surface

### State to stage
```
queued                      → queued,  condition waiting
working                     → working, condition active
reviewing                   → review,  condition active
awaiting_author             → the stage the run stood at, condition waiting
blocked                     → the stage the run stood at, condition blocked
interrupted                 → the stage the run stood at, condition paused
                              (author pause) or stopped (every other reason)
completed                   → done,    condition complete
discarded | failed          → done,    condition stopped
```

### Stage transition reasons
```
enqueued            → into queued
work_started        → queued  → working
review_started      → working → review
review_revision     → review  → working   (a backward move, GOB-FR-BTXN)
blocked_retry       → any     → queued    (a backward move, GOB-FR-BTXN)
finished            → review  → done
ended               → any     → done      (discarded, failed)
```

### Merge stage labels
A surface shows these words for the stages of a run that carries `merge` data (GOB-FR-PLTB). The stage and the condition are those of the record.
```
stage    condition          label
queued   waiting            Queued
working  active             Reconciling
review   active             Reviewing
done     complete           Merged
done     stopped            Not merged
```
A stage of a merge run with another condition is shown with the label of its stage and the condition word every run is shown.

### Record shapes
```
GraduationObservability {
  observability_version,      // the integer 1
  current_stage,              // GraduationVisualStage
  stage_condition,            // GraduationStageCondition
  stage_history: [StageTransition],
  passes: [PassRecord]
}

GraduationVisualStage    = "queued" | "working" | "review" | "done"
GraduationStageCondition = "active" | "waiting" | "paused" | "blocked"
                         | "stopped" | "complete"

StageTransition {
  from, to,                   // GraduationVisualStage
  pass,                       // the pass number, 1 or more
  at,                         // RFC 3339 UTC
  reason                      // one of the reasons above
}

PassRecord {
  pass,                       // the pass number, 1 or more
  status,                     // "working" | "passed" | "failed"
  task,                       // GOB-FR-XYCY: the prompt this pass was asked to
                              //   answer, and not the instruction every turn carries
  verdict?,                   // "ready" | "revise"
  rationale?,                 // the review's own words
  findings: [ReviewFinding],  // per ../ai/GRL-graduation-loop.md
  next_instruction?,          // what the following turn was told, whole
  started_at, ended_at
}
```

## Non-functional requirements
- The record is read by surfaces alone. No dispatch, preflight, gate or publication in any module reads a field of it.
- A merge run adds no field to the record.
- A run whose observability record cannot be read is still a run: its state, its queue place and its commits are unaffected, and the surface renders the run without a stage row.
