# Commit message window

**Spec code:** `CMW`

## Intent
The modal surface where the author writes the message for a commit and confirms it. It refuses to commit until a message has been written, because a commit is the unit an external agent picks work up from and an unlabelled one is a hand-off nobody can read. It is the single place a commit message is authored in the application, so a commit is worded the same way whichever action started it and wherever it lands. Two of the three routes into it settle their files first and open carrying that exact set — the ones the Changes panel has checked, or every uncommitted path a graduation's start preflight found in the stream — and show it, so the author can see what the message describes. Before the message, the Changes panel's route asks one question the author cannot ask themselves: whether the changed files their filters were hiding should go in too, because the lens that shows only artifacts hides exactly the code and tests those artifacts produced. The third route, a work stream's merge, carries no set: a merge decides what it writes while it runs, and can spend agent turns doing it, so this window takes the message, hands it back, and closes rather than standing over work of unbounded length. Out of scope: choosing which files are committed one by one, which is settled before the window opens (`CHG-changes.md` CHG-FR-27); performing a merge, showing one, or reporting what one settled, all of which belong to the surface that opened the window (`WSS-work-stream-selector.md`); pushing, which belongs to the panel that opened the window; and amending, sign-off, message templates, and recalling a previous message, none of which this window offers.

## User stories
- As a user, I want the files I am about to commit listed beside the message field so that I can tell the message and the change set agree before I confirm.
- As a user, I want a rejected commit to leave my message on screen so that I fix the cause and retry rather than retyping what I just wrote.
- As a user, I want the window to refuse an empty message so that I never end up with a commit I cannot identify a week later.
- As a user merging a work stream, I want the window to close the moment it has my message so that a conflict merge that a merge run carries on runs where I can watch it rather than behind a window I cannot shut.

## Wireframes
```
        ┌─ Commit 3 files ────────────────────────────┐
        │  MESSAGE                                    │
        │  ┌───────────────────────────────────────┐  │
        │  │                                       │  │
        │  │                                       │  │  ≥ 6 lines of text
        │  │                                       │  │
        │  └───────────────────────────────────────┘  │
        │                                             │
        │  FILES                                      │
        │  ┌───────────────────────────────────────┐  │
        │  │ .claude/skills/analyst/SKILL.md       │  │
        │  │ specifications/ui/CHG-changes.md      │  │
        │  │ src/components/Changes.tsx       new  │  │
        │  └───────────────────────────────────────┘  │
        │                                             │
        │  ⚠ Nothing to commit in the selected files. │
        │                                             │
        │                     [ Cancel ]  [ Commit ]  │
        └─────────────────────────────────────────────┘
```

The same window opened by a work stream's merge, which carries no file set:
```
        ┌─ Merge message · test-stream ───────────────┐
        │  MESSAGE                                    │
        │  ┌───────────────────────────────────────┐  │
        │  │                                       │  │
        │  │                                       │  │  ≥ 6 lines of text
        │  │                                       │  │
        │  └───────────────────────────────────────┘  │
        │                                             │
        │                      [ Cancel ]  [ Merge ]  │
        └─────────────────────────────────────────────┘
```
- Layout notes: the window is a centered modal overlay, not a tab or panel (CMW-FR-01), re-centered each time it opens (CMW-FR-02). On the two routes that carry files, the title states how many the commit holds and the **MESSAGE** editor sits above the **FILES** list, so the field the author must fill is the first thing under the cursor; the files list is read-only and scrolls internally when the set is long, so a large commit never grows the window past the screen, and an entry Git does not track yet is marked in place. On the merge route the heading names the stream, no files region is rendered at all, and the confirm is labelled for the merge it starts. Failures attach directly above the action row, which is one right-aligned pair as every modal in the application presents them, and the window does not resize its inputs to accommodate one.

## UI contract boundary
- **Owned by the UI**: the modal layout, its centering, its dismissal, and its mutual exclusion with the other overlays of the main window; the message editor and its minimum visible height; client-side validation of the message and the resulting enablement of the confirm action; the read-only rendering of the file set and its untracked markers on the routes that carry one, and the naming of the stream on the route that does not; the in-flight presentation while a commit runs; and inline rendering of a typed commit failure. The window does not decide which files it commits, nor which worktree they land in — both are fixed by the surface that opened it — and it neither classifies nor reads any file.
- **Delegated to backend (abstract)**:
 - `"commit paths (message, paths)"` — creates the commit from the message and the fixed file set in the active worktree, and returns the commit together with the paths it recorded. Owned by `../core/GTC-git.md` (GTC-FR-19). Invoked when the Changes panel opened the window, and when a graduation's **start preflight** opened it — that preflight commits the stream the graduation is about to run in, which is a worktree this operation already writes. It carries the operation's optional `expected_worktree` where the opening surface supplied one, so the commit is refused rather than redirected if that checkout is no longer the active one (per `../core/GTC-git.md` GTC-FR-31).

 This is the whole of what the window invokes. A window opened by a work stream's merge reaches no operation at all: it hands its message to the surface that opened it, which invokes `merge_work_stream` on its own terms (per `WSS-work-stream-selector.md`).

## Functional requirements
1. **CMW-FR-01** The commit message window is a modal overlay action surface — not a tab and not a panel. As a floating overlay it is mutually exclusive with the other overlays of the main window: opening it closes any other open overlay rather than coexisting with it (per `SNV-shell-navigation.md` SNV-FR-56).
2. **CMW-FR-02** The window is positioned at the center of the screen each time it opens, regardless of where any prior invocation left it.
3. **CMW-FR-03** The window opens from exactly three places, and every commit the application makes passes through one of them: the Changes panel's **Commit** and **Commit & Push** actions (per `CHG-changes.md` CHG-FR-39), a graduation's **start preflight**, and a work stream's merge. Which of the three opened it decides the file set, the operation the confirm reaches (CMW-FR-07, CMW-FR-KRVP), and whether the inclusion step of CMW-FR-13 exists. It decides nothing else, so the message is written, validated and refused the same way each time. No other surface opens it.
4. **CMW-FR-04** The window presents a multi-line message editor displaying at least six lines of text, and opens with keyboard focus in it.
5. **CMW-FR-05** The confirm action is enabled only while the message contains at least one non-whitespace character. This is the window's only client-side validation; whether the commit can be created is decided by the backend.
6. **CMW-FR-06** From the Changes panel the window carries that panel's commit set (CHG-FR-27); from a graduation's start preflight it carries every uncommitted path that preflight reported, together with the **worktree identity** it reported. It renders that set as a read-only list of project-relative paths, states its count, and marks each entry the set reports as untracked.
7. **CMW-FR-ZDMT** A window opened by a work stream's merge carries **no file set at all**. Its heading names the stream, and it lists no path and states no count — least of all a count of zero, which would describe a set it was never given. What a merge writes is settled while it runs (per `../core/GRB-graduation-rebase.md` GRB-FR-ASWC).
8. **CMW-FR-YQTX** Once the set is settled (CMW-FR-13) nothing in the window adds a path to it, removes one from it, or reorders it, and every path the commit carries is in the list the author read — including one added by CMW-FR-13.
9. **CMW-FR-07** From the Changes panel and from a graduation's start preflight, confirming invokes `"commit paths (message, paths)"` with the message and the whole file set; the preflight adds the **expected worktree** it was handed, which the window submits unread (per `../core/GTC-git.md` GTC-FR-31). The window stays mounted with its inputs and actions inert while that commit runs, and closes when it succeeds, reporting the commit and **the paths it recorded** to the surface that opened it (`CHG-changes.md` CHG-FR-41, `TAB-tabs.md` TAB-FR-22).
    - *Why:* The paths reported are the ones the commit recorded rather than the ones the window submitted, so a rename committed at both its locations closes a Diff tab open on either.
10. **CMW-FR-KRVP** A window opened by a work stream's merge holds **no in-flight state**. Confirming hands the message to the surface that opened it and closes at once. That surface starts the merge, shows it while it runs, and reports what it settles (per `WSS-work-stream-selector.md` WSS-FR-HGWL). This window invokes no merge operation and waits for none.
    - *Why:* A merge that Git cannot settle is handed to a merge run that takes agent turns, so a window that waited on the merge would hold the whole application for as long as that took.
11. **CMW-FR-08** A commit the backend rejects renders inline above the action row and leaves the window open with the message intact and the file set unchanged, so the author corrects the cause and retries without retyping. The typed causes it distinguishes are the ones `../core/GTC-git.md` GTC-FR-20 returns, `worktree_identity_changed` among them. A merge's refusal never renders here: the window has already closed, and the refusal belongs to the surface that opened it (per `WSS-work-stream-selector.md` WSS-FR-NPXC).
12. **CMW-FR-09** The user may dismiss the window — via its close control, Escape, or an outside click — at any time before confirming; dismissal creates no commit, starts no merge, invokes no backend operation, and leaves the Changes panel's checkboxes exactly as they were. Dismissal is unavailable in one state alone: while a `"commit paths (message, paths)"` call is in flight (CMW-FR-07).
13. **CMW-FR-BZHM** The one state that refuses dismissal is **bounded**: it is a single Git commit in the active worktree, which reaches no agent, no container and no network. No route into this window can make it undismissable for a length of time the author cannot predict.
14. **CMW-FR-10** The message lives in the window's transient state only. It is not persisted, and a window opened after a commit succeeded or after a dismissal starts empty; the application recalls no previous message.
15. **CMW-FR-11** The window offers no amend, no sign-off toggle, no message template, no list of previous messages, and no per-file selection control: which files are committed is settled before the message step begins, by the panel and by the one question of CMW-FR-13, and cannot be edited path by path here.
16. **CMW-FR-12** The window is operable by keyboard alone: the message editor takes focus on open, the confirm action is reachable and confirmable without a pointer, and Escape dismisses it.
17. **CMW-FR-13** When the Changes panel reports changed files its filters were hiding (`CHG-changes.md` CHG-FR-48), the window opens on an inclusion step before the message step. The step states how many files are hidden and offers exactly three answers: **No**, which commits the set as it arrived; **All revisioned**, which adds the hidden files Git already tracks; and **All files**, which adds every hidden file, untracked ones included. Answering settles the file set for the rest of the window and moves to the message step, where that set is listed in full (CMW-FR-06); the step cannot be returned to, because a set that changed under a message already written would make the message describe something else. Dismissing from this step commits nothing, exactly as dismissing from the message step does (CMW-FR-09). When nothing was hidden the step does not exist and the window opens on its message.
18. **CMW-FR-WKDP** Neither graduation route ever has the inclusion step. A start preflight already carries every uncommitted path in the worktree, and a stream merge carries no file set to add to (CMW-FR-ZDMT). Neither has a filter over it, so nothing is hidden from either.

## Non-functional requirements
- The window knows which surface opened it and nothing else about it. It holds no reference to a panel, a run or a stream, it resolves no worktree of its own, and it decides nothing about where a commit lands — the operation it invokes carries that, and a worktree identity it was handed is carried through unread rather than interpreted, which is what keeps one message step correct for three very different commits.
- The single-overlay rule of CMW-FR-01 is enforced by opening logic, not by listener coordination: the window assumes no other overlay is mounted while it is open. The same logic never tears the window down while the bounded commit of CMW-FR-07 is running.
- The window renders without network access; only the confirmed commit reaches the filesystem, and nothing in it reaches a remote.
- The file set is rendered from what the window was handed at open. It does not re-read the working tree, so the list never changes under the author while the message is being written.
- A commit set of several hundred files renders without the window exceeding the screen: the list scrolls internally rather than growing the frame.
