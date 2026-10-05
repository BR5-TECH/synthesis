## Intent

Fix Claude Code OAuth token validation and remove GitHub authentication as a requirement for project discussions when the user has no GitHub tokens stored. Keep GitHub authentication for all other GitHub operations. Discussion comments posted without a stored token use a fixed local human participant. Its saved participant snapshot is non-GitHub; display it as **Me** while no project GitHub identity resolves, and otherwise display the currently resolved project identity without changing the snapshot. Do not create a separate identity record or change existing GitHub-authored comments.

## User journey

- The user enters or re-verifies a Claude Code OAuth token. The field accepts the existing token characters and `_`.
- The user opens, reads, and posts in a discussion when no GitHub tokens are stored. The composer is enabled, and their comments appear as **Me** without a token picker or a route to GitHub settings.
- If one GitHub token is stored, existing implicit-token behavior and GitHub username authorship stay unchanged. If several tokens need a project binding, the existing token picker remains required; **Me** is not a fallback for that state.
- Existing comment participant snapshots remain unchanged. For comments authored with the fixed local participant, show the currently resolved project GitHub identity using the same author label as an ordinary GitHub-authored comment; show **Me** while no project identity resolves. Apply this display-only rule to past and new comments on every discussion surface. Existing GitHub-authored comments keep their saved identity.

## Requirements

### Claude Code OAuth token validation

- Update the backend validation pattern to `^sk-ant-oat01-[A-Za-z0-9_-]+$`. This adds `_` to the existing character set and keeps the existing prefix and all other validation behavior.
- Apply the same pattern in the Claude Code token field's local validation. Keep the UI and backend rules aligned; do not send a request or call the CLI for local shape validation.
- Keep malformed-token errors and existing stored-token behavior unchanged. Do not change validation for other credentials.

### Discussion identity without a GitHub token

- Reading discussions remains available without a GitHub token.
- When the project has **no GitHub tokens stored**, allow all human discussion actions that currently require comment-author identity, including opening discussions, posting comments, and submitting discussion-question answers. Use one fixed non-GitHub human participant for each human-authored comment, with the display name **Me** and no GitHub login or email. Use this participant for other human-authored discussion events that require an actor, so lock, resolve, and fragment updates do not become unavailable without a token.
- Store the participant in each comment as its author snapshot. Do not create a separate identity record, machine-local identity, or user setting. The participant is not a GitHub account and must not be sent to GitHub.
- Resolve **Me** only when the token registry is empty. Preserve existing token resolution when a token is bound or resolves implicitly. When several stored tokens require a project binding, keep the existing picker flow and do not enable posting as **Me** until the user selects a token. Preserve existing errors for stored tokens that cannot resolve or authenticate.
- Keep every existing saved GitHub participant unchanged. Do not migrate, rewrite, or relabel historical comments. A later token change affects only new discussion writes.
- Render **Me** as a human participant in every discussion surface, including comment cards, the shared conversation tab, the draft discussion column, the Comments panel, and Notes discussions. Preserve existing rendering for GitHub human and agent participants.
- When **Me** resolves, enable the shared opening and reply composers and discussion actions that need an author identity. Do not show the token picker, a missing-token error, or a Global settings route. Keep existing picker and typed-error behavior for other identity-resolution states.
- Make no other GitHub operation token-optional. Push, pull requests, publication, polling, and other authenticated GitHub operations keep their current token requirements and error flows.

### Tests

- Test that the backend and UI accept the existing Claude token charset plus `_`, and reject characters outside the pattern.
- Test no-token discussion reads, opening, comments, question-set answers, and other human discussion writes; verify the stored participant is the fixed local human and the UI displays **Me** without blocking or offering token setup.
- Test that one stored token keeps existing GitHub username attribution, multiple stored tokens still require the picker, and existing saved GitHub participants remain unchanged.
- Test that other authenticated GitHub operations still require a token and use their existing recovery flows.

## Specifications this work must change

Update the existing requirements together so the UI and backend contracts agree:

- `specifications/core/AIC-agentic-integrations.md` and `specifications/ui/AII-ai-integrations.md` — add `_` to Claude Code OAuth token validation.
- `specifications/core/GTS-github-token-storage.md` — define discussion identity resolution for an empty token registry while preserving token resolution and binding rules for GitHub operations.
- `specifications/core/CMS-comments-storage.md` — define the fixed local human participant, its comment and event stamping, and how identity resolution permits discussion writes without a token; preserve existing participant snapshots.
- `specifications/ui/CMT-comments.md`, `specifications/ui/ACT-action-control.md`, and `specifications/ui/CVP-conversation-presentation.md` — enable the shared discussion composers and actions for **Me**, and render the local participant consistently without changing existing picker behavior.
- `specifications/ui/CMP-comments-panel.md` and `specifications/ui/DQA-discussion-question-answering.md` — render **Me** and allow discussion-question answers under the same no-token rule.
- `specifications/ui/NTS-notes.md` — keep note discussions available under the shared no-token identity behavior.

Check the other discussion surfaces that use these shared contracts and update any conflicting identity gating. Do not change unrelated discussion behavior or token-management UI.

Add or update focused backend and frontend tests for these requirements.