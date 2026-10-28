# Create a PR window

**Spec code:** `CPR`

## Intent
The one window where the author defines a pull request and submits it to GitHub without leaving the application. Three surfaces open it: a row of the work stream selector, the footer of the Changes panel, and the PRs section of the Git panel. Each surface knows a different thing — a stream knows its own branch and base, the Changes panel knows the branch the author works on — so each opens the window with a **source**: the head branch, the base branch, and the title to start from. Everything after that is the same, so the window is one component with one set of rules rather than one window for each opener. A pull request needs its head branch on GitHub, and an author who submits one has already decided to publish that branch. So when GitHub cannot yet see the head branch, or all of its commits, **Submit** pushes the branch first and then creates the pull request. The window does not stop the author to ask for a push they have already asked for. It commits nothing and pulls nothing: a commit needs a message only the author can write, and a pull changes the author's work. Those states stay blocks that the window names, together with what the author must do about them. The result is a pull request on GitHub and a link to it. Out of scope: reading, listing and reviewing pull requests, which `GIT-git.md` owns; authenticating to GitHub, which `GHA-github-authentication.md` owns; the GitHub operations themselves, which `../core/GTC-git.md` owns.

## User stories
- As an author who has finished a graduation run in a stream, I want to open a pull request for the stream from the stream selector so that the work reaches review without a browser trip.
- As an author working on a branch, I want to open a pull request for it from the Changes panel, next to the commit controls, so that committing and proposing the work are one flow.
- As an author, I want **Submit** to push a branch that GitHub cannot see yet, so that I do not leave the window to publish the branch I am asking to merge.
- As an author, I want the window to tell me before I submit that it will push, so that I know what leaves my machine.
- As an author, I want the window to tell me to commit first, and not to commit for me, so that every commit carries a message I wrote.
- As an author, I want a failed submission to leave my title and description on screen so that I fix the cause and retry without retyping.
- As an author, I want a link to the new pull request when it exists so that I can go and read it.

## Functional requirements
1. **CPR-FR-FDVO** The window opens from three places: the **Create a PR** action of a work stream row (per `WSS-work-stream-selector.md` WSS-FR-KHGP), the **Create a PR** button of the Changes panel footer (per `CHG-changes.md` CHG-FR-UPFP), and the **Create PR for current branch** button of the Git panel's PRs section (per `GIT-git.md` GIT-FR-05). All three open this one window. No surface renders a window of its own for a pull request.
2. **CPR-FR-JOIG** The opener gives the window a **source**: the head branch, the base branch to start from, and the title to start from. The description starts empty and the draft toggle starts off, whichever surface opened the window.
3. **CPR-FR-IWDK** The window is a floating overlay of the main window and is mutually exclusive with every other (per `SNV-shell-navigation.md` SNV-FR-56). Opening it closes any other overlay, the work stream dropdown included. It is modal, it takes focus when it opens, it keeps focus inside itself, and it returns focus to the control that opened it, or to the control's place when that control is gone, when it closes.
4. **CPR-FR-QVYZ** The window holds exactly these controls: a **Title** field, a **Description** field that takes several lines, a **Base branch** selector, a **Draft** toggle, a read-only **Head branch** line, **Cancel**, and **Submit**. The head branch is shown and cannot be edited.
5. **CPR-FR-FZHF** The base branch selector offers the project's branches by the names GitHub knows them, without the head branch, and holds the base the source gave. A base the project no longer holds stays shown and is marked missing, and **Submit** stays unavailable until the author chooses another.
6. **CPR-FR-HGXL** **Submit** is unavailable while the title holds nothing but white space, while the window reads the head branch's state (CPR-FR-SQGZ), while a request runs, and while any block of CPR-FR-VYPG stands. Each reason that applies is stated in words in the window and is the accessible description of **Submit**.
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
13. **CPR-FR-XMRL** While the request runs, **Submit** is unavailable and says `Pushing the branch…` while the push runs, then `Creating the pull request…`. The push and the creation are one request. **Cancel**, Escape and the backdrop do nothing until the request settles. Title, description, base and the toggle cannot be edited while the request runs.
14. **CPR-FR-SSQI** **Cancel**, Escape, and a click on the backdrop close the window and start nothing. Closing the window discards what the author typed, and opening it again starts from the source.
15. **CPR-FR-ITWJ** On success the window closes and the toast of `NTF-notifications.md` NTF-FR-PRCK shows. The toast names the pull request by its number and carries a link to it. Activating the link opens the pull request's page in the system browser. The window leaves no notice of its own (NTF-FR-TSRL).
16. **CPR-FR-RDJP** The toast of CPR-FR-ITWJ is not a floating overlay of SNV-FR-56 (per `NTF-notifications.md` NTF-FR-21): it opens no window, it closes none, and no overlay closes it. A change of the project removes it (NTF-FR-15).
17. **CPR-FR-SQEP** A failure leaves the window open, keeps the title, the description, the base branch and the toggle as the author left them, and shows the failure inline above the action row. The window does not resize its inputs to show it.
18. **CPR-FR-VZUZ** A failure with `github_token_selection_required`, from the push or from the creation, makes the window give way to the token picker (per `GHA-github-authentication.md` GHA-FR-16), because the picker is an overlay of the same window and the two cannot stand together (SNV-FR-56). What the author typed is held while the picker is open. When the author confirms a token, the submission runs once with what was held. When the author cancels, the window returns with what was held and says inline that no GitHub token was selected.
19. **CPR-FR-FGGU** A failure with `github_token_missing`, from the push or from the creation, shows inline that a pull request needs a GitHub token and offers a route to the Global settings GitHub section (per `GHA-github-authentication.md` GHA-FR-19). Taking the route leaves the window open.
20. **CPR-FR-VZNE** Every other failure shows inline in words, by its typed cause: no remote configured, a push that the remote refused with its own reason, a remote that is not on GitHub, a head or base branch that GitHub does not hold, a token GitHub refused, GitHub unreachable, a pull request that GitHub refused with its own reason, and a pull request that already exists for the head branch. No failure text holds a token.
21. **CPR-FR-NQPS** The whole window is operable by keyboard alone. Each field carries a visible label. The head branch, each block of CPR-FR-VYPG, the notice of CPR-FR-PLWK and each failure are in the window's accessible semantics, and a failure is announced when it appears. The first focus lands on the **Title** field.
22. **CPR-FR-DWGS** The window looks like the other modal windows of the application: the same frame, backdrop, type scale, field styling, and one right-aligned row for **Cancel** and **Submit**, set apart from the body (per `CMW-commit-message.md`). The window has a fixed measure and scrolls its body, not the page, when the content is long.

## Wireframes

```
┌ Create a PR ───────────────────────────────────────┐
│ TITLE                                              │
│ [ editor-work                                    ] │
│                                                    │
│ DESCRIPTION                                        │
│ [                                                ] │
│ [                                                ] │
│ [                                                ] │
│                                                    │
│ HEAD BRANCH   synthesis/stream/editor-work         │
│ BASE BRANCH   [ main                           ▾ ] │
│ [ ] Draft                                          │
│                                                    │
│ ⓘ The branch is not on the remote yet. Submit      │
│   pushes it first.                                 │
│                                                    │
│                              [ Cancel ] [ Submit ] │
└────────────────────────────────────────────────────┘
```

The toast that a created pull request shows:

```
┌────────────────────────────────────────────┐
│ (i) Info   Pull request created          ✕ │
│ Pull request #42 is created.               │
│ Open pull request #42                      │
└────────────────────────────────────────────┘
```

- Layout notes: the window is a centered modal. The title is one line, the description is the tall field, and the head branch and base branch stand as one labelled pair below them, so the author reads where the pull request goes before they confirm. Blocks, the push notice and failures attach above the action row. The push notice reads as information, not as a warning. A long branch name truncates from the middle and holds its whole value in accessible semantics. The toast shows in the toast stack (per `NTF-notifications.md` NTF-FR-LAWP).

## UI contract boundary

**Owned by the UI**: the window, its fields, its validation of the title, its blocks, its push notice and their wording, when a submission pushes first, its holding of the author's input while the token picker is open, the text and the link of the pull request toast, and which surfaces open it with which source. The window decides nothing about a branch. It reads what the backend reports and shows it.

**Delegated to backend (abstract)**:
- `"get pull request head state (head, base)"` — owned by `../core/GTC-git.md`.
- `"push branch (branch)"` — owned by `../core/GTC-git.md`. It publishes the head branch to the project's primary remote.
- `"create pull request (title, body, base, head, draft)"` — owned by `../core/GTC-git.md`. It returns the number and the page address of the pull request it created.
- `"list branches"` — owned by `../core/GTC-git.md`.

The link of the toast is opened by the application's opener, and no backend operation of this specification does it.

## Non-functional requirements
- The window opens without waiting on the backend. It shows the loading state of CPR-FR-SQGZ while the read runs, and the author can type while it runs.
- No text the window or the toast renders holds a token secret, because none reaches it (per `../core/GTC-git.md` GTC-FR-11).
- A response that arrives after the window closed, after the project changed, or after a newer read of the same kind is discarded and changes nothing the author sees.
