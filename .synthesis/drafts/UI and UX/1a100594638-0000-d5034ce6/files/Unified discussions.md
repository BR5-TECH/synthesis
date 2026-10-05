## Intent

Replace the existing comment and discussion implementations with one **unified discussion** model and one shared discussion UI across the IDE.

A unified discussion is about an owner and may optionally target a fragment of that owner. A fragment is supported only for text artifacts and draft prompts. Flow, Diff, History, notes, and whole-artifact discussions are whole-target discussions with no fragment.

Remove the existing anchored-comment versus whole-target-discussion distinction from the user-facing model and migrate existing records without losing their ids, messages, attachments, lifecycle state, agent turns, or pending question sets. Remove Detached, Minimized, and Maximized conversation behavior. Keep each owner's existing layout, but render the same discussion surface and composer inside it. A note discussion opens or focuses a conversation tab because the Notes panel is not a discussion surface.

When a discussion about a fragment is open, the owning editor must show and highlight that fragment. Every route that reveals a discussion must focus the existing discussion surface instead of creating a second copy.
## User journey

- The user opens an Editor, Draft, Flow, Diff, History, or Notes surface and chooses **Discuss**, or selects text and chooses **Comment**.
- The shared opening composer appears in the owning layout. It uses the same Markdown input, agent mention picker, attachments, validation, error handling, and Ctrl+Enter/Cmd+Enter posting behavior on every surface.
- Posting creates or opens one unified discussion. A selected text range becomes its fragment target; an unselected action creates a whole-target discussion. Notes open or focus a conversation tab, and all other owners use their existing contextual discussion surface.
- The discussion renders with the same message list, composer, Quote action, attachments, agent pending and failure states, question-set block, Lock action, and Resolve action everywhere. Existing lifecycle behavior remains unchanged.
- While a fragment discussion is open, the editor keeps the fragment visible and highlighted. The highlight and the discussion focus each navigate to the other without changing the artifact or draft content.
- The user can continue the discussion from the owner surface, the Comments panel, or a route from a notification. Each route focuses the one existing discussion surface. It never creates a second card, panel, tab, or conversation instance.
- If the owner surface closes or is unavailable, the discussion remains persisted and can be reopened from the Comments panel or another owner route. A note discussion is reopened in its conversation tab.
- When a new reply arrives, a discussion that is at the message-list end follows the new reply; a discussion scrolled away from the end keeps its position and shows an unread indicator. Changing the owner surface must keep the pending agent placeholder, composer text, attachments, and discussion state.

## Requirements

### Unified model

- Use one `Discussion` record for every conversation. It has one stable id, one owner target, an optional `fragment_target`, ordered messages, attachments, agent-turn state, pending question-set state, lock state, and resolve state.
- An absent `fragment_target` means that the discussion is about the whole owner. A fragment target stores the owner identity, source range, and quoted source text needed to restore and highlight it.
- Allow fragments only for text artifacts and draft prompts. Do not allow fragments for Flow nodes, Diff regions, History revisions, notes, or whole-artifact discussions.
- Migrate existing anchored comment threads and whole-target discussion records into the unified record without losing stable ids, messages, quotes, attachments, lock state, resolve state, pending question sets, or agent-turn associations. Define how old records are read during migration and ensure migration is idempotent.

### Shared frontend behavior

- Build one shared discussion surface and one shared opening/reply composer. Reuse them for anchored comments, artifact discussions, draft discussions, note discussions, Flow discussions, Diff discussions, History discussions, and every opening composer.
- Keep the existing owner layouts: the Editor comment rail, draft discussion column, action-control discussion panel, Flow panel, Diff panel, History panel, and conversation tab. Replace their conversation rendering with the shared surface; do not create a second visual language.
- Remove Detached, Minimized, and Maximized presentation modes, floating conversation overlays, minimized bookmarks, and mode-transition controls. Do not leave the old behavior in any specification or route.
- A discussion is rendered in one active owner surface at a time. Navigation resolves its stable id, focuses the existing surface when it is open, or opens the correct owner surface when it is closed. No route creates duplicate cards, panels, tabs, composers, or discussion state.
- Notes remain a list-only panel. **Discuss** opens or focuses a conversation tab with the shared discussion surface; no discussion card, placeholder, or composer is rendered inside a Notes row or Notes-panel subview.
- When a fragment discussion is opened, the owner editor shows the fragment and applies a distinct highlight. Focusing the discussion scrolls the fragment into view; activating the fragment focuses the discussion. If the fragment cannot be restored, show the existing orphaned/unavailable state without deleting the discussion.
- Preserve the existing behavior for Markdown rendering, Quote, attachments, Lock, Resolve, agent routing, agent pending and failed contributions, retry, question sets, typed backend errors, and identity errors. These behaviors must be identical in every owner surface.
- Preserve unsent composer text, pending attachments, scroll position, unread state, pending agent placeholders, failed agent contributions, and question-set answers when the discussion changes owner surface or its owner tab is reopened. A pending agent placeholder must remain visible after any owner-surface change.
- If the message list is at its end, append and reveal a new reply. If it is not at its end, preserve the user's scroll position and show an accessible unread divider or indicator. This rule applies to human replies, agent replies, and question-set updates.
- Support Ctrl+Enter on Windows and Linux and Cmd+Enter on macOS in every composer, including Notes discussion start and all discussion opening composers. Preserve IME and mention-picker key handling.
- Make every shared control keyboard operable and accessible. Unread, pending, failed, locked, resolved, fragment-highlighted, unavailable, and orphaned states must not depend on colour alone.

### Project-wide routing and indexing

- The Comments panel becomes the index of every unified discussion, including artifact, draft, note, Flow, Diff, and History targets. It must show whether the discussion is fragment-targeted or whole-target and route activation to the correct owner surface or conversation tab.
- A Comments-panel activation for an available artifact or draft focuses its owner surface and its fragment or whole-target discussion. A note activation opens or focuses its conversation tab. An unavailable owner opens the discussion in the fallback conversation tab and clearly identifies the unavailable owner.
- Existing project and worktree teardown, deletion, draft deletion, note deletion, and discussion persistence rules remain valid unless this feature explicitly replaces them. A deleted owner must remove or invalidate its discussions only where the current storage contract already requires that behavior.

## Specifications this work must change

Update these specifications together so they no longer contradict the unified model:

- `specifications/ui/CVP-conversation-presentation.md` — replace the Detached, Minimized, Maximized, placeholder, bookmark, geometry, and presentation-registry contract with the single contextual discussion-surface contract and the conversation-tab fallback.
- `specifications/core/CMS-comments-storage.md` — replace `anchored` versus `discussion` records with the unified record, optional fragment target, supported owner targets, migration, reads, writes, events, and typed errors.
- `specifications/ui/CMT-comments.md` — render fragment-targeted discussions with the shared surface and remove the separate comment-thread model and old presentation controls.
- `specifications/ui/CMP-comments-panel.md` — index every unified discussion, including draft and note discussions, and route each row to its owner surface or conversation tab.
- `specifications/ui/ACT-action-control.md` — use the shared opening composer and shared discussion surface for artifact, Flow, Diff, History, and draft entry points.
- `specifications/ui/DDS-draft-discussion.md` and `specifications/ui/NAW-new-artifact.md` — use the shared surface in the draft discussion column while preserving the draft column layout.
- `specifications/ui/NTS-notes.md` and `specifications/core/NTC-notes-storage.md` — make Notes open or focus a conversation tab and remove the Notes-panel discussion-subview behavior.
- `specifications/ui/FLO-flow.md`, `specifications/ui/DFV-diff-viewer.md`, and `specifications/ui/HVW-history-viewer.md` — define whole-target discussions with no fragment and shared rendering in their existing owner panels.
- `specifications/ui/TAB-tabs.md` — remove conversation-tab rules that depend on maximizing, and define the tab route used for note and unavailable-owner discussions.
- `specifications/ui/CTA-comment-agent-turns.md`, `specifications/ui/DQA-discussion-question-answering.md`, and `specifications/core/AGC-agent-conversations.md` — replace anchored-comment versus discussion branching with the unified target and preserve agent, question-set, routing, and context behavior.

Add or update frontend and backend tests for migration, one discussion identity, fragment restoration and highlighting, whole-target routing, all owner surfaces, Comments-panel indexing, Notes-to-conversation-tab routing, duplicate prevention, scroll-follow behavior, pending-agent preservation, keyboard submission, accessibility, deletion, unavailable owners, and project/worktree changes.
