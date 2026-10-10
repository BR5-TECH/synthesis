# GitHub authentication

**Spec code:** `GHA`

## Intent

The surface where the author tells Synthesis who it is on GitHub. It has two halves that share one vocabulary: the **GitHub section** of the Global settings window, where personal access tokens are added, labelled, verified, and removed, and the **token picker modal**, where the author says which of several stored tokens the current project uses. It exists because every authenticated GitHub operation in the application — opening a pull request, reading its review comments, pushing over HTTPS — needs a credential, and an author who works across a personal account and one or more organisation accounts needs to keep several and point each project at the right one. The token itself is a secret the application holds on the author's behalf: it is typed once into the add dialog, handed straight to the backend, and never comes back — no surface in this specification ever renders more than its last four characters. Each token belongs to one GitHub host: `github.com` by default, or a GitHub Enterprise host that the author types as a domain when they add the token. Out of scope: credentials for any other provider; a change of the domain of a stored token; creating a token inside the application, since only GitHub can mint one and the Generate flow is a guided hand-off to the browser; and adding a token from the picker modal, which lists what is stored and routes to the Global settings section for anything else.

## User stories

- As an author with a personal and a work GitHub account, I want to store both tokens under names I recognise so that I can tell them apart six months from now.
- As an author, I want the application to walk me to the right GitHub page with the right scopes already ticked so that I don't have to work out which permissions it needs.
- As an author opening a pull request in a work repository, I want to be asked once which token applies and never asked again, so that the choice is a one-off rather than a recurring interruption.
- As an author, I want to know that a token I pasted is actually good before I discover it isn't in the middle of a push.

## Wireframes

The GitHub section of the Global settings window:

```
┌─ Global settings ──────────────────────────────────────────────────┐
│ Appearance          │  GitHub                                      │
│ Recent projects     │  ┌─────────────────────────────────────────┐ │
│ Installed plugins   │  │ work laptop                      valid  │ │
│ Installed adapters  │  │ @raver119 · repo, workflow · ••••a3f9   │ │
│ GitHub            ◂ │  │ added 12 Mar 2026   Verify Rename Remove│ │
│                     │  ├─────────────────────────────────────────┤ │
│                     │  │ ci bot                         invalid  │ │
│                     │  │ @synthesis-ci · repo · ••••7c21         │ │
│                     │  │ added 4 Jul 2026    Verify Rename Remove│ │
│                     │  └─────────────────────────────────────────┘ │
│                     │  [ Add token ]                               │
└────────────────────────────────────────────────────────────────────┘
```

The Add token dialog:

```
┌─ Add GitHub token ──────────────────────────┐
│  DOMAIN                                     │
│         [ github.com                      ] │
│                                             │
│  Don't have one yet? Open GitHub with the   │
│  scopes Synthesis needs already selected,   │
│  create the token, then paste it below.     │
│        [ Open GitHub token page ]           │
│                                             │
│  TOKEN                                      │
│         [ ghp_•••••••••••••••••••••••••••• ] │
│                                             │
│  LABEL (OPTIONAL)                           │
│         [ defaults to your GitHub account ] │
│                                             │
│  ⚠ That token was rejected by GitHub.       │
│  ⚠ That domain is not a valid host.         │
│                                             │
│                        [ Cancel ]  [ Add ]  │
└─────────────────────────────────────────────┘
```

The token picker modal:

```
┌─ Which GitHub token for acme-platform? ─────┐
│                                             │
│  ( • ) work laptop                          │
│        @raver119 · repo, workflow · ••••a3f9│
│                                             │
│  (   ) company                              │
│        @raver119-dev · repo · ••••1b04      │
│        company.ghe.com                      │
│                                             │
│  Synthesis remembers this choice for this   │
│  project. Change it in Project settings.    │
│                                             │
│                        [ Cancel ]  [ Use ]  │
└─────────────────────────────────────────────┘
```

Layout notes:

- A token row is two lines plus its actions: the label and verification state on the first, the account, granted scopes, and masked hint on the second. The state is a trailing marker on the label line, not a column. A token whose domain is not `github.com` shows the domain on a line of its own below the second.
- The Add token dialog presents the domain field, the hand-off to GitHub, the token field, and the label field in that fixed order; nothing in it appears or disappears in response to what the author has done so far.
- The dialog's actions sit in one right-aligned row separated from the body, as every modal in the application presents them. Controls inside the body size to their content rather than stretching to the dialog's width, so a button in a field group does not read as another input.
- Rejections attach inside the dialog directly above the action row, and the dialog does not resize its inputs to accommodate one.
- The picker modal renders one row per stored token, always as a single-selection group, with no action that adds, edits, or removes a token.

## UI contract boundary

- **Owned by the UI**: the GitHub section's placement inside the Global settings window and its list, row rendering, and empty state; the masking of every token to its last four characters in every rendered position; the Add token dialog, its inputs, and client-side enablement of its Add action; the confirmation shown before a removal; the token picker, its selection group, its anchoring and dismissal, the window it is presented in, and its mutual exclusion with the other overlays of the window it is presented in; the decision to open the picker in response to a typed selection-required error; inline rendering of typed errors in both the dialog and the section; and holding a pasted secret in transient component state for the duration of one submission and no longer. The UI never validates a token itself, never derives an account or a scope list, and never persists a secret anywhere.
- **Delegated to backend (abstract)**: "list github tokens", "add github token (label, secret, host)", "validate github token", "rename github token", "remove github token", "open github token creation page (host)", "get project github token binding", "set project github token binding". All are defined by `../core/GTS-github-token-storage.md`.

## Functional requirements

 1. **GHA-FR-01** GitHub is a section of the Global settings window (per `GLS-global-settings.md` GLS-FR-03) and lists the stored tokens returned by "list github tokens" on mount.
 2. **GHA-FR-02** A token row renders the label, the verification state, the account login, the granted scopes, the masked hint, and the date the token was added. It renders the domain of the token only when the domain is not `github.com`.
 3. **GHA-FR-03** No surface in this specification renders a token beyond its masked hint — the last four characters — because the backend never returns more (per `../core/GTS-github-token-storage.md` GTS-FR-02). This holds for rows, for the picker modal, and for the text of every error either surface renders.
 4. **GHA-FR-04** With no token stored, the section renders a first-class empty state that names what a token is for and offers the same Add token action; an empty registry is not an error.
 5. **GHA-FR-05** The Add token dialog is a single form — a pasted secret and an optional label — with no mode to choose between. Only GitHub can mint a token, so every route into this dialog ends in the same paste; a generate-versus-paste switch would gate nothing for the author who already holds a token and would hide the hand-off from the one who does not.
 6. **GHA-FR-06** The dialog always offers an action that invokes "open github token creation page (host)" with the domain as typed, above the token field and never as a precondition to it. The dialog stays open across it and keeps whatever the user has already typed, because the token arrives back by paste rather than by any channel the application controls.
 7. **GHA-FR-07** The dialog's Add action is enabled while the secret field is non-empty; this is the only client-side validation, and whether the secret is a usable GitHub token is decided by the backend. The label is optional and never gates the action — an unlabelled token is filed under the account the backend resolves for it (`../core/GTS-github-token-storage.md` GTS-FR-05), so the author is not asked for a name the application is about to learn.
 8. **GHA-FR-08** Submitting invokes "add github token (label, secret, host)" with the domain as typed. On success the dialog closes and the new token appears in the list with the account and scopes the backend resolved.
 9. **GHA-FR-09** The pasted secret lives in the dialog's transient component state only. It is cleared when the submission resolves either way, it is not written into any retained editing state, and closing the dialog discards it; nothing in the UI can reproduce it afterwards.
10. **GHA-FR-10** A rejected submission renders the typed error inline in the dialog above the action row and leaves the dialog open with the label preserved and the secret field cleared, so a mistyped paste is retried without retyping the label. A rejection distinguishes a token GitHub refused from GitHub being unreachable and from the OS keychain being unavailable, because the three call for different responses from the user.
11. **GHA-FR-11** A row's Verify action invokes "validate github token" and updates that row's state, account, and scopes in place from the result.
12. **GHA-FR-12** A row's Rename action invokes "rename github token" with the new label; a label that collides with another stored token's is rejected by the backend and surfaces inline on the row.
13. **GHA-FR-13** A row's Remove action asks for confirmation before invoking "remove github token", and the confirmation states that projects currently using the token will ask again which token to use.
14. **GHA-FR-14** Every action in the GitHub section applies immediately rather than through a section save, so the section holds no dirty state and contributes nothing to the Global settings window's save-before-close sweep (per `GLS-global-settings.md` GLS-FR-10 and GLS-FR-13).
15. **GHA-FR-15** The token picker modal is a single-selection list of the stored tokens, rendered with the same fields as a section row (GHA-FR-02) including the domain of a token whose domain is not `github.com`, naming the project it is choosing for, and stating that the choice is remembered.
16. **GHA-FR-16** The picker opens in response to a GitHub operation that reported it needs a token selected — the state "get project github token binding" reports when the project has no binding and more than one token is stored (per `../core/GTS-github-token-storage.md` GTS-FR-10). Confirming the selection invokes "set project github token binding" and the operation that was waiting then proceeds. Creating a pull request is such an operation: the Create a PR window (per `CPR-create-pull-request.md` CPR-FR-VZUZ) asks for the picker when its submission reports the need, and it submits once more with the same title, description, base branch, and draft choice when a token is confirmed.
17. **GHA-FR-17** Cancelling the picker abandons the operation that opened it, records no binding, and surfaces the cancellation on the surface that requested the operation — for the Create a PR window, inline in the window, which returns with what the author had typed — and the next authenticated operation in that project opens the picker again. Where the requesting surface has by design already carried on past the operation — a graduation start preflight's optional push, which is never allowed to block the graduation it precedes — the cancellation is recorded in the session log instead of being rendered, and it opens no recovery flow of its own.
18. **GHA-FR-18** The picker never opens when the project already resolves a token — because it is bound, or because exactly one token is stored and is therefore used implicitly (per `../core/GTS-github-token-storage.md` GTS-FR-10) — so the author who keeps a single token is never asked.
19. **GHA-FR-19** When no token is stored at all, an authenticated GitHub operation does not open the picker. The surface that requested it renders the typed error inline with a route to the Global settings GitHub section, because there is nothing to pick between. The Create a PR window renders it inline in the window and offers the route there, leaving the window open with its input kept.
20. **GHA-FR-20** The picker is also opened explicitly from the Project settings Project section (per `SET-project-settings.md` SET-FR-12) with no operation waiting on it, and is presented **within the Project settings window** rather than over the main window, that window being modal to it (per `SWN-settings-windows.md` SWN-FR-02). In that mode it preselects the currently bound token, confirming rebinds the project, and cancelling leaves the existing binding untouched.
21. **GHA-FR-21** The picker is a floating overlay of the window that presents it — the main window where a GitHub operation opened it (GHA-FR-16), and the Project settings window where that section did (GHA-FR-20) — and is mutually exclusive with every other overlay of that window: opening it closes any open overlay, and it cannot be opened while one is showing an unresolved state. The Create a PR window is such an overlay, so it gives way to the picker and returns when the picker settles (per `CPR-create-pull-request.md` CPR-FR-VZUZ).
22. **GHA-FR-22** The picker offers no way to add, rename, or remove a token; it selects among what is stored and routes the author to the Global settings GitHub section for anything else.
23. **GHA-FR-PGPY** The Add token dialog has a domain field above the hand-off action. Its placeholder is `github.com`. An empty field means `github.com`, and the author is not asked to type that value. The dialog never changes the domain of a stored token, and a token for another host is added as a new token.
24. **GHA-FR-FUSZ** The Open GitHub token page action sends the domain as typed, and the page that opens is the token-creation page of that domain. With an empty field it opens the page of `github.com`. An `invalid_host` answer renders inline in the dialog, as GHA-FR-OGNL states.
25. **GHA-FR-OGNL** A domain that the backend refuses with `invalid_host`, on submit or on the hand-off action, renders inline in the dialog above the action row. The dialog stays open with the label and the domain kept and the secret field cleared on submit.
26. **GHA-FR-LBLM** The picker modal and the Project settings Project section render the domain of a token only when it is not `github.com`. A `github_host_mismatch` failure of an operation renders inline on the surface that requested the operation, in words that say the project token belongs to another host than the remote, and it opens no picker.

## Non-functional requirements

- The GitHub section renders its list and its empty state without network access; only adding and verifying a token contact GitHub.
- No secret is present in any component state after the submission that carried it resolves, in any rendered string, or in any error message the UI surfaces.
- The picker modal is reachable by keyboard alone: the selection group is focusable, and confirm and cancel are both operable without a pointer.
- A verification state older than the current session is rendered as the state at its recorded time rather than as a live fact, so a row never claims a token is valid on the strength of a check made days ago.
