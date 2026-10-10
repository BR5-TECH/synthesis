# Graduation runs

**Spec code:** `GRU`

## Intent
The surface an author watches a graduation run on. It is a section of the Runs bottom panel rather than a surface of its own: a history rail listing every run the project has made, and beside it the region for the selected run — what stream it runs in, what stage it stands at, what it changed, what it is waiting for, and what the author can do about it. It is where a run that stopped says why, where an escalation is answered, and where the work a run committed is read. Two kinds of run stand in it: a **draft run**, which turns a draft into committed work on a stream, and a **merge run**, which merges a stream into its base branch after Git could not settle the merge alone. A merge run is read, paused, continued, answered and discarded here like any other run; it is never a graduation of a draft. The section starts no run: a draft run begins from a draft's own **Graduate** action, and a merge run begins from the merge of `WSS-work-stream-selector.md`. Out of scope: creating streams and starting merges, which is `WSS-work-stream-selector.md`'s; the run log window, which is `GLW-graduation-log-window.md`'s.

## Functional requirements
1. **GRU-FR-MYFA** Graduation runs are a section of the Runs bottom-panel surface, chosen by that panel's own section control (per `RUN-runs.md`). The section renders the open project's run history: every run it has made, in the project's run order.
2. **GRU-FR-UKNC** Every run renders one **provenance** line, in every state. A stream run names the work stream it runs in and the branch that stream owns. A run whose stream has been deleted names the stream it ran in and says the stream is gone. A merge run names the stream and the base branch, on the terms of GRU-FR-NWEC. A direct run says it works directly, names the worktree and the branch it pinned, and names the stream where the worktree is a stream's.
3. **GRU-FR-IZKI** The selected run renders a **stage row** through the run-progress component (per `RPV-run-progress.md`), driven by the run's persisted observability record (per `../core/GOB-graduation-observability.md` GOB-FR-XMDU).
4. **GRU-FR-CKOB** A run whose observability record is at a version this build does not recognise renders without a stage row rather than with a guessed one. Everything else about the run still renders.
5. **GRU-FR-ZBMU** The stage row is built from **one ordered list of stage descriptors held as data**, in the order `../core/GOB-graduation-observability.md` GOB-FR-JAJU names. Every rule that reads a stage — eligibility, the sentence for one that holds nothing, and what opening one does — reads that list and branches on no stage id and on no stage count.
6. **GRU-FR-TQJW** A stage entry is **activatable** (per `RPV-run-progress.md` RPV-FR-JSQW) exactly where the log indexes the backend reports for the run hold at least one segment naming that stage (per `../core/GRS-graduation-run-log-storage.md` GRS-FR-WFWD). A segment naming a pass and a segment naming `pass = null` each make the stage activatable.
7. **GRU-FR-VKPD** A stage no segment names is rendered **disabled**, with the sentence saying that the stage holds no log yet (per `RPV-run-progress.md` RPV-FR-MAIP). Activating it opens nothing, and this section reads no log for it.
8. **GRU-FR-HXNC** Activating an eligible stage opens the **graduation log window** (per `GLW-graduation-log-window.md` GLW-FR-ALZI), bound to the selected run and to that stage's descriptor. Closing the window returns focus to the stage entry that opened it.
9. **GRU-FR-HKBD** A run in `queued` renders its position in **its own** queue — its stream's, or its worktree's for a direct run — and says when it is not eligible because its auto-start is off. It states why it waits: for its own queue, or for a project slot (GRU-FR-KMNF).
10. **GRU-FR-LBPR** A run in `working` or `reviewing` renders the stage as working, with the pass it is on. A run in `blocked` states what blocks it, how many times it blocked on the same thing, that nothing was committed, and the act that clears it.
11. **GRU-FR-BHJO** A run in `interrupted` states why it stopped in a headline that names its interruption reason (per `../core/GRD-graduation.md` GRD-FR-PUXO), renders the interruption's detail beneath it, and offers **Continue**. The headline for `retryable_failure` says that the cause was not recorded.
11. **GRU-FR-QLRQ** A run interrupted on `execution_timeout` offers **Change the time limit** beside its detail. It opens Project settings on the Graduation section (per `SET-project-settings.md` SET-FR-DHFS). The run stays interrupted until the author continues it.
12. **GRU-FR-FZCN** A run in `awaiting_author` renders the escalation and routes to the answering surface (per `GEA-graduation-escalation-answering.md`). A review that did not settle before the pass budget was spent renders its findings, the budget that was spent, and the same route. A run resting on a blocker that repeated states that blocker instead and offers **Continue**.
13. **GRU-FR-QYEE** A draft run in `completed` names the commits it made on the stream and offers to read them. A merge run in `completed` renders its result instead, on the terms of GRU-FR-JRMA. A run in `failed` states the typed failure and offers **Discard run** alone.
14. **GRU-FR-TFEZ** A run whose stream branch the remote did not take states it once in the run region, with the reason, and states that the run was not stopped by it (per `../core/GRD-graduation.md` GRD-FR-KDWA). The region offers no push of its own.
15. **GRU-FR-RRNN** The run region renders the run's **change set** as the paths that differ from its base commit, each with the operation that produced it. It renders no per-path judgement, no classification and no validation result.
    - *Why:* The application judges no path any more, so a column of verdicts would report a decision nothing takes.
16. **GRU-FR-BRGO** Activating a path opens that path's change in the diff viewer (per `DFV-diff-viewer.md`), read against the run's base commit.
17. **GRU-FR-WJHV** Where a run's work included paths the repository's ignore rules hide, the region says how many and names them, because authored work an ignore rule hides is committed by nothing.
18. **GRU-FR-TXLW** The change set and the hidden paths each render as a **folder tree** in a box of bounded height that scrolls inside itself. Each folder row states how many paths it holds. A chain of folders that each hold only one folder renders as one row that names the whole chain.
19. **GRU-FR-HEQB** Each folder row opens and closes its folder by pointer and by keyboard, and states in accessible semantics whether it is open. A list of 24 paths or fewer renders with every folder open. A longer list renders with its root folders open and every deeper folder closed.
20. **GRU-FR-NUCJ** The heading of each path list names the list and its path count, and it opens and closes the whole list by pointer and by keyboard. A list is open when its run is first selected.
21. **GRU-FR-YYXN** The **pass history** renders one row per pass, each carrying the pass's own one-sentence description and its status. Opening a row shows what the pass was asked to do, the review's verdict and rationale, its findings, and the instruction the next turn was told.
21. **GRU-FR-JAEY** While a run is `interrupted`, a pass whose status is still `working` renders as **Stopped** with the short cause its interruption reason names. The pass record stays unsettled, and **Continue** resumes that pass.
22. **GRU-FR-GXQE** The pass history has no height bound of its own. An open account renders its whole task, rationale, findings and next instruction, and the run region's scroll is the only vertical scroll that moves them.
    - *Why:* A box that scrolls inside a region that scrolls gives the author two scroll positions to manage for one text.
22. **GRU-FR-VDSK** The row of an open pass stays visible at the top edge of the run region while the author scrolls that pass's account under it, so the author can close the pass at every scroll position.
22. **GRU-FR-ZLWI** Opening and closing pass rows, in any order and any number of times, keeps every pass row rendered and visible in the pass history.
22. **GRU-FR-ZYDL** When the author opens or closes a pass and that pass's row then stands outside the visible part of the run region, the region scrolls that row into view. A row already in view does not move.
23. **GRU-FR-MCYF** A run region 880px wide or wider renders the change set and the hidden paths in a left **paths column** and the pass history in a right column, with a **column divider** between them. A narrower region stacks them in one column and renders no divider.
24. **GRU-FR-KWRB** The paths column's width is a fraction of the width of the two columns: 0.30 where the author has set none, held between 0.15 and 0.70. The pass history takes the rest. The fraction is user-global and persists across restarts (per `../core/GSS-global-settings-storage.md` GSS-FR-QDNV).
25. **GRU-FR-ZPTE** The author drags the column divider to set the paths column's width. On the divider, Left and Right move the width by one percentage point, Home sets the lower bound and End the upper bound. The divider is a separator that states its name and its width as a percentage. A drag end or a key press stores the width.
19. **GRU-FR-MRPE** Every text this section renders that an agent or a model produced is treated as untrusted: it is rendered as escaped plain text, with no Markdown and no embedded markup.
20. **GRU-FR-ZVTC** The rail and the run region render from what the backend reports rather than from what the surface attempted. Every `"graduation queue changed"` and `"graduation run changed"` event reloads the listing it names.
20. **GRU-FR-QKSY** A `"graduation log records appended"` event reloads the listing only where it names a listed, non-terminal run whose progress and log indexes this build reads, and whose log indexes hold no segment naming the run's current stage with the current pass or with `pass = null`. Any other append event reloads nothing.
    - *Why:* a turn appends many records, only the first of a stage makes it activatable, and some stages hold only `pass = null` records.
21. **GRU-FR-BLSS** A run entering `awaiting_author`, `blocked`, or a terminal state raises for the author's attention (per `NTF-notifications.md`), addressed to that run. The raise carries a level: Warn for `awaiting_author` and `blocked`; Info for `completed` and `discarded`; Error for `failed` and for `interrupted` on a failure reason. A run entering `interrupted` raises for every reason except `author_pause`, `application_shutdown` and `project_changed`. Progress alone raises nothing.
21. **GRU-FR-FJZD** The raise for an interrupted run names the run and the cause its headline states (GRU-FR-BHJO).
22. **GRU-FR-RZDI** Every draft run's region carries a route to its **source draft**, which opens that draft in a New Artifact tab (per `NAW-new-artifact.md`). It routes by id, so a renamed or moved draft still resolves.
23. **GRU-FR-XQVG** The row actions of a queued run are its arrangement: pause, resume, auto-start and reorder, each invoking the operation of `../core/GRD-graduation.md` and each refused inline against the run it is about.
24. **GRU-FR-DVWY** A completed, interrupted, awaiting-author, discarded or failed run that made commits offers **Revert**, which reverts those commits as new commits on the stream branch and rewrites no history. A queued run, a run that holds its stream and a run that made no commit offer none. Reverting an interrupted or awaiting-author run ends it as discarded. A merge run offers no **Revert**, whatever it published.
25. **GRU-FR-WDWB** A discarded draft run offers **Restart**, which starts a new run over the same captured prompt (per `GRT-graduation-restart.md`). A discarded merge run offers no **Restart**.
25. **GRU-FR-OZAR** The action row is a **foot bar** pinned to the bottom edge of the run region. It stays visible at every scroll position of the region, and the region's content scrolls under it.
25. **GRU-FR-KQPE** A refusal of a selected run's action renders beside the action row that issued it, in the visible part of the run area with no scroll, as an alert. It renders only while its run is selected, and the region shows one refusal alert at most.
25. **GRU-FR-ZMHB** While a selected run's action runs, Restart included, the row's controls that act on the run are disabled; **Open draft** only navigates and stays enabled. When the action settles, keyboard focus returns to the row only where it sits on the page body or inside that row. Focus the author moved elsewhere stays.
25. **GRU-FR-PVXD** Focus that returns to the action row goes to the control the author pressed, else to the control that takes its place (Resume after Pause, Pause after Resume), else to the row's first control.
26. **GRU-FR-GLSO** This section offers no affordance to graduate a draft. A graduation begins in the start dialog of `GSD-graduation-start-dialog.md`, opened from the routes GSD-FR-QMTF names, and from nowhere else.
27. **GRU-FR-NBRO** The whole section is operable by keyboard alone, and every state, severity, blocking reason and outcome is carried in words and in accessible semantics rather than by colour or motion.
28. **GRU-FR-PNVX** The pass the section states is the pass the checkpoint holds (per `../core/GXD-graduation-execution.md` GXD-FR-PWYD). It is not counted from the pass records, which hold no entry for a pass that no turn started.
29. **GRU-FR-LORX** The rail and the region of a direct run state the pinned worktree and branch wherever they state a stream for a stream run. A direct run on an ordinary worktree names no stream and shows no stream placeholder.
30. **GRU-FR-PFBY** A queued direct run with a target hold states the branch it pinned, the branch the worktree holds now, and that it starts when the pinned branch is checked out again. It offers **Continue**, which invokes `continue_graduation_run` and renders a refusal inline.
31. **GRU-FR-KMNF** A queued run that is eligible and whose own queue is free states that it waits for a **project slot**, with the limit and the slots held, where `get_graduation_capacity` lists it in `waiting_for_slot` (per `../core/GRD-graduation.md` GRD-FR-GRHC). A run that waits behind a run of its own stream or worktree states that instead.
32. **GRU-FR-QKDB** The section reads `get_graduation_capacity` with the queue and again on every `"graduation queue changed"`. A capacity the section cannot read leaves a queued run on the sentence for its own queue and renders no guess.
33. **GRU-FR-HDPQ** A merge run's title is its `merge.name`, which reads `Merge <stream>` with the stream's name. The rail row and the run region carry that title in the place a draft run carries its draft's name, and the rail's text filter matches it. No text of a merge run names a draft.
34. **GRU-FR-NWEC** A merge run's provenance line names the stream, the stream's branch and the **base branch** the merge goes into. The region also states the **publication choice** the run holds: the result is left uncommitted in the base worktree, or it is committed under the author's message, which the region renders as escaped text.
35. **GRU-FR-ZKYU** No label, heading, sentence or accessible name of a merge run calls the run a graduation of a draft. The run says it is a merge and names its stream and base branch.
36. **GRU-FR-YSTV** A merge run's stage row has the same four stages in the same order as every run, labelled **Queued**, **Reconciling**, **Reviewing** and **Merged** where a draft run's are labelled for a draft. The `done` stage of a merge run that ended without applying its merge, standing in the condition `stopped`, is labelled **Not merged** (per `../core/GOB-graduation-observability.md` GOB-FR-PLTB). A run working shows **Reconciling** with its pass, and a run reviewing shows **Reviewing** with its pass. Which stage is activatable, disabled and opened follows GRU-FR-TQJW, GRU-FR-VKPD and GRU-FR-HXNC without change. The labels are part of the stage descriptors of GRU-FR-ZBMU and no rule branches on a stage id to choose one.
37. **GRU-FR-CXLB** A merge run renders on the terms every run has, without a rule of its own: the pass history and its findings (GRU-FR-YYXN through GRU-FR-ZYDL), the change set against the run's base commit (GRU-FR-RRNN, GRU-FR-BRGO), the queue position and the reason a queued run waits (GRU-FR-HKBD, GRU-FR-KMNF), the working, blocked, interrupted and awaiting-author states (GRU-FR-LBPR, GRU-FR-BHJO, GRU-FR-FZCN), the notification raise (GRU-FR-BLSS), the foot bar, the refusal alert and the focus rules (GRU-FR-OZAR, GRU-FR-KQPE, GRU-FR-ZMHB, GRU-FR-PVXD), the escaped rendering of agent text (GRU-FR-MRPE), the keyboard and accessible semantics (GRU-FR-NBRO), and the reload on every `"graduation queue changed"` and `"graduation run changed"` event (GRU-FR-ZVTC). A merge run's pass count and pass budget are the ones the run's own checkpoint holds (GRU-FR-PNVX).
38. **GRU-FR-AJGM** A merge run's region names the paths the merge changes. It renders `merge.unresolvedPaths` as the paths Git could not settle, each with what the base branch did to the path and what the stream did to it (created, updated, deleted or unchanged), and it states how many paths in all the merge changes (`merge.changedPaths`). The paths render as a folder tree on the terms of GRU-FR-TXLW, GRU-FR-HEQB and GRU-FR-NUCJ. They are read text and open nothing.
39. **GRU-FR-JRMA** A merge run in `completed` renders its result from `merge.result`. It says whether the result was published as an uncommitted change in the base worktree or as a commit, names the commit in short form with the whole id in accessible semantics where one was made, and renders the merged paths as a folder tree on the terms of GRU-FR-TXLW. A merge run not in `completed` renders no result.
40. **GRU-FR-ELFZ** A merge run's region offers no **Open draft**, no restart, no revert, no source-draft route and no **graduated** badge, and renders no link to a draft. The run has no draft.
41. **GRU-FR-TOKG** A merge run offers the controls every run offers in its state: **Pause** while it works or reviews, **Continue** where it is interrupted, blocked, or at rest on a spent pass budget or a repeated blocker, **Discard run** where `discard_graduation_run` is accepted, the answering surface of `GEA-graduation-escalation-answering.md` while it awaits the author, the arrangement actions of GRU-FR-XQVG while it is queued, and archive and unarchive. Each invokes the operation named for it in the contract boundary and renders a refusal inline against the run.
42. **GRU-FR-DBUS** A merge run in `blocked` states in words which side blocks the apply. The code `merge_dirty_side` says a working copy of the stream or of the base branch holds uncommitted changes. The code `merge_guard_held` says another merge or update of the repository holds the repository. The code `merge_apply_failed` says the application could not write the result. Each says that nothing was written to either branch, that no new pass is spent, and that **Continue** retries the apply.
43. **GRU-FR-QMWX** A merge run in `failed` with the typed failure `merge_branch_moved` states that the stream branch or the base branch moved after the merge was handed off, that nothing was written to either branch, and that the author starts the merge again from the work stream selector. It offers **Discard run** alone, on the terms of GRU-FR-QYEE.
44. **GRU-FR-EMNV** A **Continue** or an answer to an escalation of a merge run that the backend refuses with `merge_branch_moved` renders inline against the run (GRU-FR-KQPE) with the same statement as GRU-FR-QMWX. The run keeps the state it had, every entered answer stays (GEA-FR-TEMG in `GEA-graduation-escalation-answering.md`), and **Discard run** stays offered.
45. **GRU-FR-PAHN** The section selects a run by its id on request, for a draft run and a merge run alike. A request names a run the listing holds: the section makes the rail's view show that run, selects it, and renders its region. A request for a run the listing does not hold makes the section reload the listing once; if the run is still absent, the selection does not change and the section says that the run is not found. This is the target the work stream selector opens a merge run through (per `WSS-work-stream-selector.md` WSS-FR-AWRS).
46. **GRU-FR-VQJB** The activation of a status bar row whose operation carries a `graduation_run` destination (per `STB-status-bar.md` STB-FR-RWPD) opens the Runs bottom panel with its graduation section shown and makes the request of GRU-FR-PAHN for the run id the destination carries. The run is selected and its region renders, whichever section or run was shown before. A run id the listing does not hold is handled on the terms of GRU-FR-PAHN.

## Wireframes

```
┌ Runs ─────────────────────────────────────────────────────────────────────┐
│ [ Graduation · 3 ] [ Agent output ]                                       │
├──────────────────┬────────────────────────────────────────────────────────┤
│ ▸ editor scroll  │ "Editor scroll fix"  ·  stream: editor-work            │
│   Working  1/2   │ ●────────●────────○────────○                           │
│ ▸ empty state    │ queued   working  review   done                        │
│   Queued  #1     │ Working · pass 1 of 2                                  │
│ ▸ tab close      │                                                        │
│   Completed      │ ▾ Changed · 181   ┃ Passes                   2 passes  │
│                  │ ┌───────────────┐ ┃ ▸ First pass  The review asked …   │
│                  │ │ ▾ specs     24│ ┃ ▾ Pass 2      Working              │
│                  │ │   ▸ core     4│ ┃   What this pass was asked to do   │
│                  │ │   ▸ ui      20│ ┃   …                                │
│                  │ │ ▸ src-tauri 58│ ┃                                    │
│                  │ └───────────────┘ ┃                                    │
│                  │ ▾ Hidden · 4      ┃  ┃ = column divider (drag)         │
│                  ├────────────────────────────────────────────────────────┤
│                  │ [ Open draft ]  [ Pause ]  [ Discard run ]             │
└──────────────────┴────────────────────────────────────────────────────────┘
```

```
┌ Runs ─────────────────────────────────────────────────────────────────────┐
│ [ Graduation · 2 ] [ Agent output ]                                       │
├──────────────────┬────────────────────────────────────────────────────────┤
│ ▸ Merge docs-pass│ "Merge docs-pass"  ·  stream: docs-pass → main         │
│   Reconciling 1/2│ Publication: left uncommitted in the base worktree     │
│ ▸ tab close      │ ●────────●────────○────────○                           │
│   Completed      │ Queued   Reconciling Reviewing Merged                  │
│                  │ Reconciling · pass 1 of 2                              │
│                  │ ▾ Unresolved · 3 of 12 changed                         │
│                  │ ┌───────────────────────────────────────────────────┐  │
│                  │ │ ▾ specifications 3                                │  │
│                  │ │   GRD-graduation.md  main: updated  stream: updated│ │
│                  │ └───────────────────────────────────────────────────┘  │
│                  ├────────────────────────────────────────────────────────┤
│                  │ [ Pause ]  [ Discard run ]                             │
└──────────────────┴────────────────────────────────────────────────────────┘
```

- Layout notes: the rail is a fixed fraction of the section's width and is resizable; the run region takes the rest and scrolls on its own. The stage row spans the region's width, and a stage entry that holds a log is a control on that row. Nothing scrolls horizontally at any width. The action row is a foot bar pinned to the bottom edge of the region (GRU-FR-OZAR). A path list longer than its box scrolls inside it rather than growing the region (GRU-FR-TXLW). The pass history grows with its content and scrolls with the region (GRU-FR-GXQE). A region 880px wide or wider sets its path lists and its pass history side by side, with a narrow paths column and a divider the author drags (GRU-FR-MCYF, GRU-FR-KWRB, GRU-FR-ZPTE).

## UI contract boundary

**Owned by the UI**: which run is selected, the rail's filter and ordering view, which pass rows are open, which path lists and which of their folders are open, the bounds and the keyboard steps of the paths column's width, the wording of every state, condition and blocking sentence, the stage labels of a merge run, the rendering of a merge run's paths and result, button enablement, and the escaped rendering of agent text.

**Delegated to backend (abstract)**:
- `load_app_preferences`
- `save_app_preferences`
- `list_graduation_queue`
- `get_graduation_run`
- `get_graduation_capacity`
- `continue_graduation_run`
- `pause_graduation_run`
- `set_graduation_auto_start`
- `reorder_graduation_run`
- `answer_graduation_escalation`
- `revert_graduation_run`
- `discard_graduation_run`
- `restart_graduation_run`
- `archive_graduation_run`
- `unarchive_graduation_run`

## Non-functional requirements
- The section renders from one queue read and reloads only the run an event names.
- A run region holding hundreds of changed paths renders without blocking the panel.
