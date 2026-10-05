## Changes

Update the in-flight progress feature so authors can see concurrent work in the status bar and open the owning surface for supported activities.

### Change 1.

Show active graduation runs and running discussion agent turns in the status bar through the existing PRG progress stream. Keep all existing PRG activities visible. Do not show queued or waiting work as running progress.

### Change 2.

Clicking the status-bar progress control opens an overlay with every in-flight activity, ordered most-recently-started first. Show up to five rows at once; when more than five activities are in flight, keep the rows in a vertically scrollable list.

### Change 3.

Make an overlay row actionable only when its progress activity defines a destination in the PRG API contract. Activating a row with a destination opens the specific owning surface and selects its target:

- Graduation run: open the Runs panel's graduation section and select that run.
- Discussion agent turn: call the existing `revealDiscussion` route with the discussion id.
- Git activity: open the Git panel and select the target defined for that Git operation.

An activity without a destination remains visible but is not actionable; activating it opens nothing. Do not infer a destination from the operation's `kind` or label.

### Change 4.

For progress from a discussion about a draft or artifact, require the producer to include the draft name or artifact path in the operation label. Keep the label producer-owned; the UI displays it without adding or guessing the subject name. For example: `@Helga is thinking about <draft name>…`.## User journey

- The author sees the most recently started in-flight activity in the status bar.
- The author opens the progress overlay and sees all in-flight activities, newest first, with up to five rows visible at once.
- If more than five activities are in flight, the author scrolls the overlay vertically to see the other rows.
- The author activates a row with a defined destination. The overlay closes, and the app opens the owning surface with the activity's target selected.
- A row without a defined destination is visible but not actionable, and opens no surface.## Requirements

- Extend the existing `Operation` contract in `specifications/core/PRG-progress-reporting.md` with an optional, typed activation destination. A producer supplies the destination and its stable target identity; the UI must not infer one from `kind` or `label`. Define the supported target types for graduation runs, discussions, and Git activities. An absent destination means no navigation.
- Register each actively executing graduation run as an in-flight PRG operation with its run id as the activation target. End the operation when the run stops executing or reaches a terminal state; do not report queued or author-waiting runs as active work.
- Keep discussion agent turns on PRG. Give each turn an activation target containing its discussion id. For draft and artifact discussions, include the draft name or artifact path in the producer-supplied label. Preserve the existing rule that a turn ends when it reaches a terminal state, including `awaiting_reply`.
- For a Git progress operation that supports navigation, supply its operation-specific Git target. The Git panel must be able to select or reveal that target. Do not add a destination for a Git operation that has no defined destination.
- Keep the status bar's existing operation ordering, progress rendering, and overlay dismissal behavior. The overlay must stay current as operations start, update, and end; do not leave completed operations in its list.
- Keep every in-flight operation in the overlay, even when it has no activation destination. Make destination-bearing rows keyboard-operable and expose their target in the accessible name. Render rows without a destination as non-actionable, not as disabled buttons that imply an available action.
- Update the affected specifications: `specifications/core/PRG-progress-reporting.md`, `specifications/ui/STB-status-bar.md`, `specifications/core/GRD-graduation.md`, `specifications/ui/GRU-graduation-runs.md`, `specifications/core/AGC-agent-conversations.md`, `specifications/core/GTC-git.md`, and `specifications/ui/GIT-git.md`. Use the existing `revealDiscussion` contract in `specifications/ui/CVP-conversation-presentation.md`; do not add a second discussion route.
- Add frontend coverage for concurrent ordering, the five-row viewport and overflow scrolling, live updates and removal, supported navigation and target selection, missing destinations, keyboard access, and accessible row names. Add producer or contract coverage for the activation targets and required discussion labels.