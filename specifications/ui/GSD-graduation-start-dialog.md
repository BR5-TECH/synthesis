# Graduation start dialog

**Spec code:** `GSD`

## Intent
The one surface a graduation run is started from. A draft's **Graduate** action opens it, and so does a claim of a ready GitHub Task, and it asks what a run cannot start without: where the run works. The author chooses an existing work stream, a stream the dialog creates, or **Work directly** in the worktree that is active at confirmation. Where a stream run must wait, either because the stream is busy or because the project-wide limit of graduation runs is full, the dialog also asks what the run does with work standing uncommitted there when its turn comes. Every answer travels with the run, so a run that waits behind another one needs no further decision before it starts. Out of scope: what the run then does, which is `../core/GRD-graduation.md`'s and is watched in `GRU-graduation-runs.md`; the streams themselves, which are `WSS-work-stream-selector.md`'s; and the checks a start makes before a run exists, which are `../core/GSU-graduation-start.md`'s.

## Functional requirements
1. **GSD-FR-QMTF** The dialog opens from a draft's **Graduate** action (per `NAW-new-artifact.md` NAW-FR-18), from the **Graduate** entry of a GitHub-shadow row in the Drafts panel (per `DRP-drafts-panel.md` DRP-FR-NPZO), and from the Ready tasks section of the Git panel (per `GIT-git.md` GIT-FR-NQTZ, GIT-FR-OLNA), and from nowhere else. It is a floating overlay of the main window (per `SNV-shell-navigation.md` SNV-FR-56), and it is the only surface that invokes `start_graduation` and `start_direct_graduation`.
2. **GSD-FR-VKLD** The dialog asks for the **destination** always (GSD-FR-BZHW), and for the **standing-work choice** only where the run is a stream run that will wait, for either reason of GSD-FR-WQPD. It asks nothing about the prompt or about what type the files take.
3. **GSD-FR-ZPWN** The stream choice lists every live stream of the project, each row naming the stream and how many runs are queued on it. A stream the backend reports as missing is not listed (per `WSS-work-stream-selector.md` WSS-FR-OQYG).
4. **GSD-FR-HRJE** A stream a run holds is offered like every other stream. The dialog states that a run works in that stream now, and that this run waits behind it.
5. **GSD-FR-TBQX** The standing-work choice has three positions: **keep** the work uncommitted, **commit** it, and **commit and push** it. `commit` is the resting position. The answer travels with `start_graduation`, and the run applies it when its turn comes (per `../core/GRD-graduation.md` GRD-FR-HQPD).
6. **GSD-FR-WQPD** The choice is asked whenever a stream run will **wait**: the chosen stream is occupied (a run holds it, or runs wait in its queue), or the project-wide limit of graduation runs is full (per `../core/GRD-graduation.md` GRD-FR-GRHC). A run onto a free stream with a free slot starts at once, carries the resting position, and is asked nothing.
    - *Why:* What a stream holds is unpredictable only while a run waits, so a run that starts at once makes the question one the author can already answer by looking.
7. **GSD-FR-MZTB** Where the choice is asked and it commits, the dialog offers an optional **commit message** of one line, which travels with `start_graduation`. The field is prefilled on the terms of GSD-FR-HVDN. A message the author leaves empty takes the run's own name (per `../core/GRD-graduation.md` GRD-FR-RJFC).
8. **GSD-FR-HVDN** The prefill of the commit message of an **Existing stream** is the captured draft name (`input.draft_name`) of the newest non-terminal run assigned to the selected stream. The newest run is the last one in the project's run order (per `../core/GRD-graduation.md` GRD-FR-VLFO). A terminal run is never the source, and neither is the name of the draft this dialog starts. A stream with no non-terminal run has an empty prefill, and an empty field takes the run's own name.
    - *Why:* Work standing in an occupied stream most likely belongs to the newest run queued or working there, so that run's name describes the commit better than the name of the run that waits behind it.
    - The dialog reads `list_graduation_queue` when it opens and again on every `"graduation queue changed"`, and it adds no operation. A read that fails leaves the prefill empty and shows no refusal of its own.
    - The prefill is set when the stream is first selected and again each time the author selects another existing stream. A selection of another stream replaces the field value with the default of that stream, also where the author edited the previous value. A queue change, a change of the standing-work choice, and an edit of the field change nothing else about the value.
    - A stream the dialog created is not a selection of another stream: a retry after a refused start keeps the field as the author left it. A **New stream** destination that asks the choice (GSD-FR-LHQY) starts with an empty field.
9. **GSD-FR-NWSC** The dialog renders no path set and reads no working copy. Each position states what the run does when its turn comes, and the keep position states that the standing work becomes part of what the run commits.
   - *Why:* The work standing in a stream when the run starts is not the work standing there now, so a path list here names files the choice does not apply to.
10. **GSD-FR-LDGM** A project that holds no live stream offers no existing-stream destination and says so. **New stream** and **Work directly** stay offered, and the dialog opens on **New stream**.
11. **GSD-FR-CXVA** A stream listing the backend refuses reads as a listing that could not be read, with the refusal beside it. The dialog does not report that the project holds no stream, and it offers no stream destination until the listing is read.
12. **GSD-FR-JYRP** Every typed refusal `start_graduation` and `start_direct_graduation` return is rendered in the dialog (per `../core/GSU-graduation-start.md` GSU-FR-ELZO, GSU-FR-SZTZ). The dialog stays open with every answer as the author left it, and nothing about the draft changes (per `NAW-new-artifact.md` NAW-FR-21).
13. **GSD-FR-BFOU** One start is in flight at a time. Every control of the dialog is inert while a start is in flight, and the dialog closes when the backend reports the run and at no earlier moment.
14. **GSD-FR-WMTD** The dialog is operable by keyboard alone. Focus moves into it when it opens, **Escape** and the backdrop dismiss it while no start is in flight, and every choice, state and refusal is carried in words rather than by colour alone.
15. **GSD-FR-LXAF** For a GitHub-shadow draft the dialog is unchanged: it asks the same questions, offers the same choices, and starts nothing on its own. Cancelling it, or a refused start, changes nothing about the draft or its GitHub issue (per `../core/GPP-github-polling.md` GPP-FR-ZLBF).

16. **GSD-FR-BZHW** The destination is one choice of three: **Existing stream**, **New stream** and **Work directly**. It rests on **Existing stream** where the project holds a usable stream. Each choice shows only its own fields, and the confirm names the choice.
17. **GSD-FR-QGTC** **New stream** asks a name and the branch the stream is created from, on the terms of `WSS-work-stream-selector.md` WSS-FR-XZRO, inside this dialog. Confirming invokes `create_work_stream`, then `start_graduation` on the new stream. The new stream is free, so the standing-work choice is asked only where the project-wide limit is full (GSD-FR-LHQY). A name refusal renders against the name field.
18. **GSD-FR-TZGT** A stream the dialog created stays when the start that follows is refused. The dialog lists it, selects it as an existing stream, and a retry starts on it without creating a second one.
19. **GSD-FR-FQQP** **Work directly** names the active worktree by its directory name and absolute path, and its branch, read through `preflight_direct_graduation`. It states that the run writes in that worktree on that branch and commits there. It asks no standing-work choice and no commit message.
20. **GSD-FR-USOH** The confirm of **Work directly** is enabled only while the preflight reports a branch and no uncommitted path. A detached worktree disables the choice and says why in words. A dirty worktree names every uncommitted path and says to commit or discard them first. A preflight that cannot be read disables the choice with the refusal beside it.
21. **GSD-FR-YBLC** The preflight is read when **Work directly** is chosen and again when `"worktree context changed"` arrives while the dialog is open. A refusal of the start that names a changed worktree, a changed branch, a detached worktree or uncommitted paths re-reads it, so the dialog states what is true now.
22. **GSD-FR-LHQY** The dialog reads `get_graduation_capacity` when it opens and again on every `"graduation queue changed"`. Where the selected stream is free and the project-wide limit is full, the dialog states that the run waits for a project slot, and it asks the standing-work choice.
23. **GSD-FR-KDBU** Where the selected stream is occupied and the limit is full too, the dialog states both reasons. A limit of **Unlimited** never makes a run wait for a slot.
24. **GSD-FR-NOID** A capacity the dialog cannot read is stated as unread, with the refusal beside it. The dialog then treats the project limit as not full, and it asks the standing-work choice for an occupied stream alone.

## Wireframes

```
        ┌─ ↥  Graduate "Run logs" ───────────────────────────── ✕ ─┐
        │                                                          │
        │  An agent does the work this prompt asks for, in a work  │
        │  stream, and commits it there when a review lets it       │
        │  through.                                                │
        │                                                          │
        │  WHERE THE RUN WORKS                                     │
        │  (•) Existing stream  ( ) New stream  ( ) Work directly  │
        │                                                          │
        │  WORK STREAM                                             │
        │  ┌────────────────────────────────────────────────────┐  │
        │  │ Test stream · 1 queued                          ▾  │  │
        │  └────────────────────────────────────────────────────┘  │
        │  A run works in "Test stream" now. This one waits        │
        │  behind it.                                              │
        │                                                          │
        │  WORK STANDING IN THE STREAM WHEN THIS RUN STARTS        │
        │  ( ) Leave it. The run works on top of it, and it        │
        │      becomes part of what the run commits.               │
        │  (•) Commit it first, under your name. The run starts    │
        │      from that commit.                                   │
        │  ( ) Commit it first, then push the stream branch.       │
        │                                                          │
        │  COMMIT MESSAGE (OPTIONAL)                               │
        │  ┌────────────────────────────────────────────────────┐  │
        │  │ Fix tab order                                      │  │
        │  └────────────────────────────────────────────────────┘  │
        │                                                          │
        │                              [ Cancel ]  [ Graduate ]    │
        └──────────────────────────────────────────────────────────┘
```

A stream that dispatches at once asks the stream and nothing else:

```
        ┌─ ↥  Graduate "Run logs" ───────────────────────────── ✕ ─┐
        │                                                          │
        │  An agent does the work this prompt asks for, in a work  │
        │  stream, and commits it there when a review lets it       │
        │  through.                                                │
        │                                                          │
        │  WHERE THE RUN WORKS                                     │
        │  (•) Existing stream  ( ) New stream  ( ) Work directly  │
        │                                                          │
        │  WORK STREAM                                             │
        │  ┌────────────────────────────────────────────────────┐  │
        │  │ Editor work                                     ▾  │  │
        │  └────────────────────────────────────────────────────┘  │
        │                                                          │
        │                              [ Cancel ]  [ Graduate ]    │
        └──────────────────────────────────────────────────────────┘
```

A stream that is free while the project limit is full asks the standing-work choice and names the project slot as the reason:

```
        ┌─ ↥  Graduate "Run logs" ───────────────────────────── ✕ ─┐
        │  WORK STREAM                                             │
        │  ┌────────────────────────────────────────────────────┐  │
        │  │ Editor work                                     ▾  │  │
        │  └────────────────────────────────────────────────────┘  │
        │  "Editor work" is free, but the project already works    │
        │  its limit of 2 graduation runs. This one waits for a    │
        │  project slot.                                           │
        │                                                          │
        │  WORK STANDING IN THE STREAM WHEN THIS RUN STARTS        │
        │  ( ) Leave it.  (•) Commit it first.  ( ) Commit, push.  │
        │                                                          │
        │                              [ Cancel ]  [ Graduate ]    │
        └──────────────────────────────────────────────────────────┘
```

Work directly names the worktree and asks nothing else:

```
        ┌─ ↥  Graduate "Run logs" ───────────────────────────── ✕ ─┐
        │                                                          │
        │  WHERE THE RUN WORKS                                     │
        │  ( ) Existing stream  ( ) New stream  (•) Work directly  │
        │                                                          │
        │  The run works in acme-platform-main, on branch          │
        │  feature/new-window, and commits there. Edits you make   │
        │  in that worktree while it works are part of the commit. │
        │  /Users/me/dev/acme-platform-main                        │
        │                                                          │
        │                              [ Cancel ]  [ Graduate ]    │
        └──────────────────────────────────────────────────────────┘
```

- Layout notes: the dialog is one modal window over a backdrop, with a head carrying the draft's name and a close control, a body, and an action row at the foot. The body scrolls on its own where the window is short, and the action row stays reachable at every window height. What is asked reads top to bottom in the order it is answered: the destination first, then the fields of that destination, then — for a waiting stream run — what the run does with standing work in it, then the message that commit takes. The busy statement stands with the stream control it is about, and the message field stands under the positions that commit. A refusal renders above the action row and moves no answer.

## UI contract boundary

**Owned by the UI**: which destination and which stream are proposed first, the default of the commit message and when it is replaced, the fields of **New stream**, the wording of the **Work directly** statement and its disabled reasons, whether the standing-work choice is asked, the wording of the three positions, of the busy-stream statement and of the full-limit statement, the enablement of the confirm, the inline rendering of every refusal, and the dismissal routes.

**Delegated to backend (abstract)**:
- `list_work_streams`
- `create_work_stream`
- `list_worktrees_and_branches`
- `preflight_direct_graduation`
- `start_graduation`
- `start_direct_graduation`
- `get_graduation_capacity`
- `list_graduation_queue`

## Non-functional requirements
- Opening the dialog costs one stream listing, one capacity read and one queue read. It reads no working copy until **Work directly** is chosen, and then one status read. It computes no change set.
- The dialog holds nothing that outlives it: a dismissal keeps neither the stream, nor the standing-work choice, nor the message, and the next opening rests where the first one did. A stream the dialog created is the one exception, being a stream of the project.
