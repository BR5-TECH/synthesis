# Create a PR window

**Spec code:** `CPR`

## Intent
The one window where the author defines a pull request and submits it to GitHub without leaving the application. Three surfaces open it: a row of the work stream selector, the footer of the Changes panel, and the PRs section of the Git panel. Each surface knows a different thing — a stream knows its own branch and base, the Changes panel knows the branch the author works on — so each opens the window with a **source**: the head branch, the base branch, and the title to start from. Everything after that is the same, so the window is one component with one set of rules rather than one window for each opener. A pull request needs its head branch on GitHub, and an author who submits one has already decided to publish that branch. So when GitHub cannot yet see the head branch, or all of its commits, **Submit** pushes the branch first and then creates the pull request. The window does not stop the author to ask for a push they have already asked for. It commits nothing and pulls nothing: a commit needs a message only the author can write, and a pull changes the author's work. Those states stay blocks that the window names, together with what the author must do about them.

A pull request on a large project is the end of a feature: several drafts, several graduation runs, and several merged streams. The window therefore writes the first title and description from the run summaries of the runs whose work is on the head branch, and it can ask an agent for a new title and description from the diff, in the project's own pull request format. That work runs in the background as a PR summary run, so the author can close the window and come back to it, and the window keeps everything for each head branch for the application session. The pull request is linked to the tasks the drafts were published as and to the features the author picks, and the window knows when the pull request is stacked on another one. A branch that the author created on top of a GitHub stack belongs in that stack, so its source starts from the parent branch in the stack, and **Submit** adds the new pull request to the stack. A branch that already has an open pull request needs no second one, so every opener makes **Create a PR** unavailable for it and says why. The result is a pull request on GitHub and a link to it. Out of scope: reading, listing and reviewing pull requests, which `GIT-git.md` owns; authenticating to GitHub, which `GHA-github-authentication.md` owns; the GitHub operations themselves, which `../core/GTC-git.md` owns; the PR summary run itself, which `../core/GRD-graduation.md` owns.

## Functional requirements
1. **CPR-FR-FDVO** The window opens from three places: the **Create a PR** action of a work stream row (per `WSS-work-stream-selector.md` WSS-FR-KHGP), the **Create a PR** button of the Changes panel footer (per `CHG-changes.md` CHG-FR-UPFP), and the **Create PR for current branch** button of the Git panel's PRs section (per `GIT-git.md` GIT-FR-05). All three open this one window. No surface renders a window of its own for a pull request.
2. **CPR-FR-JOIG** The opener gives the window a **source**: the head branch, the base branch to start from, and the title to start from. When the window holds no saved state for the head branch (CPR-FR-SSQI), it starts from the source, with the description empty, the draft toggle off, and no feature selected, whichever surface opened it.
3. **CPR-FR-IWDK** The window is a floating overlay of the main window and is mutually exclusive with every other (per `SNV-shell-navigation.md` SNV-FR-56). Opening it closes any other overlay, the work stream dropdown included. It is modal, it takes focus when it opens, it keeps focus inside itself, and it returns focus to the control that opened it, or to the control's place when that control is gone, when it closes.
4. **CPR-FR-QVYZ** The window holds exactly these controls: a **Title** field, a **Description** field with **Write** and **Preview** tabs, **Generate**, a **Base branch** selector, a **Draft** toggle, a **Features** selector, a read-only **Head branch** line, a read-only **Tasks** list, **Cancel**, and **Submit**. The head branch is shown and cannot be edited.
5. **CPR-FR-FZHF** The base branch selector offers the project's branches by the names GitHub knows them, without the head branch, and holds the base the source gave. A base the project no longer holds stays shown and is marked missing, and **Submit** stays unavailable until the author chooses another.
6. **CPR-FR-HGXL** **Submit** is unavailable while the title holds nothing but white space, while the window reads the head branch's state (CPR-FR-SQGZ) or the Tasks list (CPR-FR-IDRP), while a request runs, and while any block of CPR-FR-VYPG stands. Each reason that applies is stated in words in the window and is the accessible description of **Submit**.
7. **CPR-FR-SQGZ** When it opens, and whenever the author changes the base branch, the window reads `"get pull request head state (head, base)"` and shows that it is loading until the answer arrives. A failed read shows its error in the window, offers **Try again**, and leaves **Submit** unavailable. The read is local, so a branch it does not find is stated as a branch the repository does not hold, and not in the words of a refusal from GitHub. The window offers **Check again**, which reads the state anew, so that an author who commits elsewhere can continue without closing the window.
8. **CPR-FR-VYPG** The window blocks **Submit** and says why in these cases, each in words with the figure it holds, and several may stand together:
    - The repository has no remote: the window says the repository has no remote to push to.
    - The working tree that holds the head branch has uncommitted paths: the window says how many, names the first few, and tells the author to commit first.
    - The head branch holds no commit that the base branch lacks: the window says there is nothing to propose.
    - The window commits nothing in any of these cases.
9. **CPR-FR-PLWK** When the repository has a remote that lacks the head branch, or lacks some of its commits, the window shows a notice that **Submit** pushes the branch first. The notice says which case applies, and how many commits are missing. It is information, not a block, and does not make **Submit** unavailable.
10. **CPR-FR-ODTB** When the notice of CPR-FR-PLWK stands, **Submit** first invokes `"push branch (branch)"` with the head branch. After a push that succeeds, the window invokes `"create pull request"` and reads the head state anew. A push that fails creates no pull request and is a failure of the submission (CPR-FR-SQEP). Without the notice, **Submit** pushes nothing.
11. **CPR-FR-WJIA** A source whose head branch has no checkout holds no uncommitted paths, so it raises no commit block. The block of the working tree is read from the checkout that holds the head branch, whichever checkout that is, and not from the active worktree alone.
12. **CPR-FR-KMHY** **Submit** invokes `"create pull request (title, body, base, head, draft)"`, after the push of CPR-FR-ODTB when one is due, with the title as typed, the description as typed, the chosen base branch, the head branch of the source, and the state of the draft toggle. The activation is refused on the client before any command is invoked while another one is running, including a second activation in the same moment as the first.
13. **CPR-FR-XMRL** While the request runs, **Submit** is unavailable and says `Pushing the branch…`, `Creating the pull request…`, then `Linking the issues…` (CPR-FR-BLEN). The three steps are one request. **Cancel**, Escape and the backdrop do nothing until it settles. No field, toggle, selector, or **Generate** can be used while it runs.
14. **CPR-FR-SSQI** **Cancel**, Escape, and a click on the backdrop close the window and start nothing. The window keeps a **saved state** per head branch for the application session: title, description, base branch, draft toggle, features, and the PR summary run it follows, with its result or failure. A new opening for that head branch restores it instead of the source.
15. **CPR-FR-ESXZ** A submission that creates the pull request clears the saved state of its head branch, so the next opening starts from the source. A change of the project clears every saved state. A failed submission keeps it.
16. **CPR-FR-ITWJ** On success the window closes and the toast of `NTF-notifications.md` NTF-FR-PRCK shows. The toast names the pull request by its number and carries a link to it. Activating the link opens the pull request's page in the system browser. The window leaves no notice of its own (NTF-FR-TSRL).
17. **CPR-FR-RDJP** The toast of CPR-FR-ITWJ is not a floating overlay of SNV-FR-56 (per `NTF-notifications.md` NTF-FR-21): it opens no window, it closes none, and no overlay closes it. A change of the project removes it (NTF-FR-15).
18. **CPR-FR-SQEP** A failure leaves the window open, keeps the title, the description, the base branch, the toggle and the features as the author left them, and shows the failure inline above the action row. The window does not resize its inputs to show it.
19. **CPR-FR-VZUZ** A failure with `github_token_selection_required`, from the push or from the creation, makes the window give way to the token picker (per `GHA-github-authentication.md` GHA-FR-16), because the picker is an overlay of the same window and the two cannot stand together (SNV-FR-56). What the author typed is held while the picker is open. When the author confirms a token, the submission runs once with what was held. When the author cancels, the window returns with what was held and says inline that no GitHub token was selected.
20. **CPR-FR-FGGU** A failure with `github_token_missing`, from the push or from the creation, shows inline that a pull request needs a GitHub token and offers a route to the Global settings GitHub section (per `GHA-github-authentication.md` GHA-FR-19). Taking the route leaves the window open.
21. **CPR-FR-VZNE** Every other failure shows inline in words, by its typed cause: no remote configured, a push that the remote refused with its own reason, a remote that is not on GitHub, a head or base branch that GitHub does not hold, a token GitHub refused, GitHub unreachable, a pull request that GitHub refused with its own reason, and a pull request that already exists for the head branch. No failure text holds a token.
22. **CPR-FR-ILFN** The **Write** tab holds the description as a plain multi-line field of Markdown text, exactly as typed. The **Preview** tab renders that text as Markdown, with inert links, and edits nothing. Switching tabs keeps the text and the caret of the Write tab. **Write** is the active tab when the window opens.
23. **CPR-FR-UNAN** **Generate** invokes `start_pull_request_summary` with the head branch, the chosen base branch, and the mode `diff`. The activation is refused on the client while another is running. A refusal of the start shows inline in words and changes no field.
24. **CPR-FR-VOEB** While the PR summary run of the head branch is not terminal, **Generate** is unavailable and says why, and the window shows the run's state and stage in words, through the run-progress component (per `RPV-run-progress.md`). The window learns the run from `list_graduation_queue` and follows it through `"graduation run changed"`.
25. **CPR-FR-GYVD** When that run completes, the window reads it with `get_graduation_run` and replaces the title and the description with `pr_summary.result`. When the window is closed at that moment, the result goes into the saved state of the head branch and shows at the next opening.
26. **CPR-FR-URIL** A PR summary run that fails or is discarded changes no field. The window says inline that the generation did not complete, with the typed cause, and **Generate** is available again.
27. **CPR-FR-IOGF** **Generate** is unavailable, and says why in words, while `"get project agentic integration"` reports that the project resolves no agentic integration.
28. **CPR-FR-WCBQ** At the first opening for a head branch, when no saved state exists, the window reads `list_branch_completed_runs` for the head branch. When at least one run in it holds a run summary, the window **combines** the run summaries into the title and the description (CPR-FR-PJTG). When no run holds one, the window starts from the source and starts no run.
29. **CPR-FR-PJTG** A combine sets the title to the title of the first run summary in landing order. When the project resolves an agentic integration, the window invokes `start_pull_request_summary` with the mode `combine`, follows the run as for **Generate** (CPR-FR-VOEB), and fills only the description from its result.
30. **CPR-FR-ZZNM** When the project resolves no agentic integration, or the combine run fails, the description holds one section for each run summary in landing order: a `##` heading with the summary's title, a blank line, and its description. A combine runs once for a head branch and never again while its saved state exists.
31. **CPR-FR-IDRP** The **Tasks** list names the GitHub issue of each draft of `list_branch_completed_runs` for the head branch that is published, read with `"get draft publication (draft id)"`, each once. A draft that is not published is not listed. An empty list says that no task is linked.
32. **CPR-FR-GDDO** The **Features** selector offers the parent issues of the publication repository (per `../core/GHP-github-publication.md` GHP-FR-FTMC), read with `"get publication metadata cache (remote name)"` for the remote that `"get note publication availability"` names. It filters by number and title, and holds any number of selections.
33. **CPR-FR-PXQN** When the window opens, the Features selector invokes `"refresh publication metadata"` and reads the cache again on `"publication metadata changed"`. When publication is not available, the selector is unavailable and states the reason that `"get note publication availability"` gives.
34. **CPR-FR-BLEN** After the pull request is created, the window invokes `"link pull request issues (number, issues)"` with the tasks of CPR-FR-IDRP and then the selected features, each issue once. A link that fails, wholly or for some issues, does not fail the submission.
35. **CPR-FR-UXXF** The toast of CPR-FR-ITWJ also names each issue the pull request is linked to, and the way it is linked, which is a closing keyword in the description. It names each issue whose link failed. When `closes_on_merge` is false, it says that the issues stay open when the pull request merges.
36. **CPR-FR-IPKX** The window reads `"list pull requests (state)"` with `open` when it opens. When an open pull request has the chosen base as its head, the window states that this pull request is **stacked** on it, by number and title. Otherwise it states that the pull request goes into the base branch. A failed read states no kind.
37. **CPR-FR-JBPQ** For a stacked pull request, the window states before submit that its closing keywords close no issue when it merges, because GitHub closes an issue only when a pull request merges into the default branch.
37. **CPR-FR-MBTA** Each opener reads `"get branch stack (branch)"` for the head branch. When the answer is available and names a parent, the source has that parent as its base branch and as its **stack parent**, in place of the opener's own base. A parent with no pull request is still the base. Any other answer, or none, keeps the opener's base. An opener builds the source only after the stack read of the head branch has an answer: it waits for a read that still runs, and otherwise it starts a new read, so that a stack that changed since the last read gives the base. While it waits, the control is busy and starts nothing more.
37. **CPR-FR-QXTB** When the source names a stack parent and the chosen base branch is that parent, **Submit** invokes `"submit pull request to stack (title, body, base, head, draft)"` in place of the creation of CPR-FR-KMHY, with the same arguments. With any other base, **Submit** acts as CPR-FR-KMHY states. The blocks, the push and the links of this window apply to both.
37. **CPR-FR-HZRN** While CPR-FR-QXTB applies, the window states, next to the base branch, that **Submit** adds the pull request to the stack of the base branch. The statement is in the window's accessible semantics. When the author chooses another base, the statement goes away.
37. **CPR-FR-YDLV** A submission to a stack that creates the pull request is a success on the terms of CPR-FR-ITWJ, whatever its stack step answered. The toast says that the pull request was added to its stack, or that it was not added and why (per `NTF-notifications.md` NTF-FR-SXLR). A failed creation is a failure of CPR-FR-SQEP.
37. **CPR-FR-RWOE** **Create a PR** is unavailable in each opener while `"list pull requests (state)"` with `open` names a pull request whose head is the head branch and whose head repository is the repository itself. A pull request from a fork (`headIsFork` true) with the same branch name does not block. The reason is `Pull request #<number> is already open for this branch.`, and is the accessible description of the control. Closed and merged pull requests, a failed read, and a running read add no block.
37. **CPR-FR-TJYF** The openers read the open pull requests of CPR-FR-RWOE again after the window creates a pull request, so the control that opened it becomes unavailable for that head branch.
38. **CPR-FR-NQPS** The whole window is operable by keyboard alone. Each field carries a visible label. The head branch, the tabs, the stacked statement, the generation state, each block of CPR-FR-VYPG, the notice of CPR-FR-PLWK and each failure are in the window's accessible semantics, and a failure is announced when it appears. The first focus lands on the **Title** field.
39. **CPR-FR-DWGS** The window looks like the other modal windows of the application: the same frame, backdrop, type scale, field styling, and one right-aligned row for **Cancel** and **Submit**, set apart from the body (per `CMW-commit-message.md`). The window is 880 px wide, at most the main window's width and height less 32 px each. Its body scrolls, not the page.
40. **CPR-FR-ODBY** The Description field takes the height the other controls leave in the window, and at least twelve lines. The action row stays visible at every window height.
41. **CPR-FR-FFQD** The window logs each generation start, each combine, the end of each PR summary run it follows, and the link outcome, with the head branch, the run id, and the issue count, and no title or description text.

## User stories
- As an author who has finished a graduation run in a stream, I want to open a pull request for the stream from the stream selector so that the work reaches review without a browser trip.
- As an author working on a branch, I want to open a pull request for it from the Changes panel, next to the commit controls, so that committing and proposing the work are one flow.
- As an author, I want **Submit** to push a branch that GitHub cannot see yet, so that I do not leave the window to publish the branch I am asking to merge.
- As an author, I want the window to tell me before I submit that it will push, so that I know what leaves my machine.
- As an author, I want the window to tell me to commit first, and not to commit for me, so that every commit carries a message I wrote.
- As an author, I want a failed submission to leave my title and description on screen so that I fix the cause and retry without retyping.
- As an author, I want a link to the new pull request when it exists so that I can go and read it.
- As an author of a feature built over many runs, I want the first description to come from what each run did, so that I edit a draft and do not write from nothing.
- As an author, I want to ask an agent for a title and description from the diff, in my project's own pull request format, and close the window while it works.
- As an author, I want the pull request linked to its tasks and to the features I pick, so that my tracker shows the work.
- As an author of a stacked pull request, I want the window to say that it is stacked and that its keywords close nothing on merge.
- As an author of a branch on top of a stack, I want the window to start from the parent branch and to add the pull request to the stack, so that the stack on GitHub holds my work.
- As an author, I want **Create a PR** unavailable on a branch that already has an open pull request, with its number, so that I open that pull request and not a second one.

## Wireframes

```
┌ Create a PR ─────────────────────────────────────────────────────────────┐
│ TITLE                                                                    │
│ [ Git flow: PR summaries and issue links                               ] │
│                                                                          │
│ DESCRIPTION   ( Write │ Preview )                          [ Generate ] │
│ ┌──────────────────────────────────────────────────────────────────────┐ │
│ │ ## Summary                                                           │ │
│ │ * Adds the PR summary run                                            │ │
│ │                                                                      │ │
│ │                                                                      │ │
│ └──────────────────────────────────────────────────────────────────────┘ │
│ ◐ Summarizing synthesis/git-flow…  Queued ─ Summarizing ─ Done           │
│                                                                          │
│ HEAD BRANCH   synthesis/git-flow                                         │
│ BASE BRANCH   [ feature/pr-tools                                     ▾ ] │
│ ⓘ Stacked on #41 "PR tools". Its closing keywords close no issue.        │
│ ⓘ Submit adds the pull request to the stack of feature/pr-tools.        │
│ [ ] Draft                                                                │
│                                                                          │
│ FEATURES      [ #12 Git flow ✕ ]  [ filter features…                 ▾ ] │
│ TASKS         #31 PR summary run · #32 Issue links                       │
│                                                                          │
│ ⓘ The branch is not on the remote yet. Submit pushes it first.           │
│                                                                          │
│                                                     [ Cancel ] [ Submit ]│
└──────────────────────────────────────────────────────────────────────────┘
```

The toast that a created pull request shows:

```
┌────────────────────────────────────────────┐
│ (i) Info   Pull request created          ✕ │
│ Pull request #42 is created.               │
│ Linked by closing keyword: #12, #31, #32.  │
│ Open pull request #42                      │
└────────────────────────────────────────────┘
```

- Layout notes: the window is a centered, wide modal. The title is one line, the description is the tall field and takes the free height, and **Generate** sits on the description's label row with the generation state directly below the field. The head branch and base branch stand as one labelled pair below them, with the stacked statement under the base, so the author reads where the pull request goes before they confirm. Features and tasks follow. Blocks, the push notice and failures attach above the action row. The push notice reads as information, not as a warning. A long branch name truncates from the middle and holds its whole value in accessible semantics. The toast shows in the toast stack (per `NTF-notifications.md` NTF-FR-LAWP).

## UI contract boundary

**Owned by the UI**: the window, its fields, its tabs and the Markdown preview, its validation of the title, its blocks, its push notice and their wording, when a submission pushes first, the saved state of each head branch, the combine of run summaries and its fallback sections, which issues it asks to link, the stacked statement, the choice between a submission to a stack and a plain creation, the stack statement, its holding of the author's input while the token picker is open, the text and the link of the pull request toast, and which surfaces open it with which source. The window decides nothing about a branch. It reads what the backend reports and shows it.

**Delegated to backend (abstract)**:
- `"get pull request head state (head, base)"` — owned by `../core/GTC-git.md`.
- `"push branch (branch)"` — owned by `../core/GTC-git.md`. It publishes the head branch to the project's primary remote.
- `"create pull request (title, body, base, head, draft)"` — owned by `../core/GTC-git.md`. It returns the number and the page address of the pull request it created.
- `"link pull request issues (number, issues)"` — owned by `../core/GTC-git.md`.
- `"submit pull request to stack (title, body, base, head, draft)"` — owned by `../core/GTC-git.md`. It returns the created pull request and the result of its stack step.
- `"get branch stack (branch)"` — owned by `../core/GTC-git.md`. Each opener reads it for the source (CPR-FR-MBTA).
- `"list pull requests (state)"` — owned by `../core/GTC-git.md`.
- `"list branches"` — owned by `../core/GTC-git.md`.
- `start_pull_request_summary`, `list_branch_completed_runs`, `list_graduation_queue`, `get_graduation_run`, and the event `"graduation run changed"` — owned by `../core/GRD-graduation.md`.
- `"get project agentic integration"` — owned by `../core/AIC-agentic-integrations.md`.
- `"get draft publication (draft id)"`, `"get note publication availability"`, `"get publication metadata cache (remote name)"`, `"refresh publication metadata"`, and the event `"publication metadata changed"` — owned by `../core/GHP-github-publication.md`.

The link of the toast is opened by the application's opener, and no backend operation of this specification does it.

## Non-functional requirements
- The window opens without waiting on the backend. It shows the loading state of CPR-FR-SQGZ while the read runs, and the author can type while it runs.
- No text the window or the toast renders holds a token secret, because none reaches it (per `../core/GTC-git.md` GTC-FR-11).
- A read response that arrives after the window closed, after the project changed, or after a newer read of the same kind is discarded and changes nothing the author sees. The result of a PR summary run is not such a response: it is kept in the saved state of its head branch.
