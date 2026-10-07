# Intent

Users working with IDE are supposed to be able to create a PR from the IDE.

# User journey

## Stream Overlay

- User starts a graduation run in a stream.
- Graduation run finishes
- User clicks on streams button in the top chrome
- User sees `Create a PR` button between `Merge stream` and `Update stream` buttons
- User clicks `Create a PR` button
- Overlay modal window pops up where use can define a PR
- User adjusts title and description as needed
- User can click `Cancel` button to cancel the process
- User clicks `Submit` button to submit the PR.

### Requirements

- `Create a PR` button must be disabled (grayed out) when the stream was merged to its source branch, when the stream is busy, or when the stream has no commits ahead of its base. The row states the reason in words.
- Head branch is the stream branch. Base branch defaults to the stream base branch.
- Title is prefilled with the stream name. Description is empty.

## Left Changes Panel

- User opens a Changes panel.
- At the bottom of the panel user sees a button `Create a PR` at the same row with a “Commit” dropdown button, but aligned to the left side of the row.
- User clicks `Create a PR` button
- Overlay modal window pops up where use can define a PR
- User adjusts title and description as needed
- User can click `Cancel` button to cancel the process
- User clicks `Submit` button to submit the PR.

### Requirements

- If the branch user is working on has NO commits yet, `Create a PR` button must be disabled (grayed out).
- Head branch is the current branch. Base branch defaults to the repository default branch.
- Title is prefilled with the draft name (if the branch belongs to a stream, the stream name). Description is empty.

## Requirements

- Existing GitHub integration should be used for this implementation (existing "create pull request" operation).
- The Create a PR window holds: title, description, base branch (editable), and a draft toggle. Head branch is shown read-only.
- If the head branch has unpushed commits or no remote branch, `Submit` is blocked and the window tells the user to push first. If the working tree has uncommitted changes, the window tells the user to commit first. Nothing is pushed automatically.
- While the request runs, `Submit` is disabled so it cannot be sent twice.
- On success the window closes and a status notice shows a link to the new PR.
- Failures reuse the existing flow: `github_token_selection_required` opens the token picker; `github_token_missing` shows an inline error with a route to Global settings GitHub section; any other GitHub error shows inline. The window stays open on failure and keeps the user's input.
- The window is a floating overlay: opening it closes any other overlay (including the stream dropdown), per SNV-FR-56. Escape and `Cancel` close it.

## Specifications to change

- `specifications/ui/WSS-work-stream-selector.md` (new button and its disabled states)
- `specifications/ui/CHG-changes.md` (new button in footer)
- `specifications/ui/SNV-shell-navigation.md` SNV-FR-56 (add the window to the overlay list)
- `specifications/ui/GIT-git.md` and `specifications/ui/GHA-github-authentication.md` (PR creation errors and token picker routing)
- Overlay window must be reused for all use cases.
- `frontend-design` skill must be used for implementation.
- Look and feel of the new Create a PR window must match the styling we have in IDE.