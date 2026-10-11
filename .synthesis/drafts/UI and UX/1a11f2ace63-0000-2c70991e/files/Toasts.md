## Intent

The author who is working in the application window must learn at once, and without looking for it, that a graduation run finished, needs an answer, or failed, or that an agent proposed a change. Today the author gets only an OS notification (when the window is not in front) and a marked tab. This work adds an in-app **toast** for the focused case and makes it part of the one notification facility of `NTF-notifications.md`, not a second system.

## User journey

- User is working on a Draft
- IDE is in focus
- In background, there’s an ongoing graduation run 
- Eventually graduation run completes or requires user’s attention due to some issues or esacalations
- A surface raises through the notification facility. The main window holds OS focus, so the user sees a toast at the top-trailing corner of the window, below the top chrome. No OS notification is posted for this raise.
- The toast has one level: Info, Warn, or Error. Info goes away by itself after about 5 s. Warn and Error stay until the user dismisses them or clicks them.
- A click on the toast (not on its dismiss control) does what activating the OS notification does today (NTF-FR-16 to NTF-FR-19), then the toast goes away. The dismiss control removes only the toast.
- IF the raise comes from a graduation run: the click opens that run (the `run` address of NTF-FR-03).
- IF the raise comes from a change proposal: the click opens the draft tab (draft proposal) or the artifact tab (prompt artifact proposal), where the discussion lives, by the existing `draft` and `file` addresses. This work adds no new address target.
- IF the user is in another window of the application, or the application has no focus: the OS notification is posted, as today, and no toast shows.

## Requirements

### Channel and policy

- The raise API stays the one route (NTF-FR-01). Add a **required** field `level` (`Info` | `Warn` | `Error`) to the raise shape. Each raising surface sets it. The facility does not derive it.
- A raise that passes the post policy goes to exactly one channel: a **toast** when the main window holds OS focus; an **OS notification** otherwise (no window focused, or only a settings window focused). Never both.
- The suppression rule of NTF-FR-08 applies unchanged to toasts: no toast when the address names the active tab, the visible active panel surface, the open settings window, or the selected run in Runs. A suppressed raise stays silent (NTF-FR-10).
- The Notifications switch (GLS-FR-25) and the OS permission state govern OS notifications only. Toasts always show, like the tab indication. The tab indication (NTF-FR-26 to NTF-FR-37) stays as it is and is independent of toasts.
- The OS notification carries no level. `NTD-notification-delivery.md` does not change.

### Sources and levels

- Only the existing five raises of NTF-FR-24 raise toasts. Nothing new raises in this work. The API stays open for later surfaces.
- Default levels: rehearsal = Info; draft-prompt change proposed = Info; prompt-artifact change proposed = Info; GitHub poll with new ready tasks = Info; graduation run waiting on a question, a review, or a stopped publication = Warn; graduation run completed = Info; graduation run failed or interrupted on a failure reason = Error.

### Toast behaviour

- A toast shows a level icon with a text label (not colour alone), the title, and the body, with a dismiss control at its trailing edge. It shows no project subtitle.
- Info auto-dismisses after about 5 s; the timer pauses while the pointer is over the toast or keyboard focus is inside it. Warn and Error never auto-dismiss.
- Toasts stack vertically, newest on top, at most 3 visible. A raise with the key of a toast already showing replaces it in place and restarts its timer. Default for overflow: a raise beyond 3 waits in a queue and shows when a slot frees; a queued toast whose key is replaced, reached, or retracted is dropped.
- A toast is removed when the author reaches its target (NTF-FR-14), when the project or worktree changes (NTF-FR-15), and on retraction of its key (NTF-FR-38). Dismissing a toast clears no tab indication.
- Nothing is recorded: no inbox, no history, no unread count (NTF-FR-22). Toasts do not survive a relaunch, a project switch, or a worktree change.

### Unreachable address

- The **statement** (NTF-FR-19 to NTF-FR-21) is removed. An address that cannot be reached shows a **Warn** toast with the same sentence, key `unreachable-address`, and no address. A click on it only dismisses it. The statement's Escape and timed dismissal are removed.

### Layout and accessibility

- The stack is anchored at the top-trailing corner below the top chrome, clear of the tab strip controls and the action control (ACT-FR-01). It is opaque, bounded in width so text wraps, floats over the trailing panel, and resizes or reflows nothing. The stack region intercepts no pointer event outside the toasts themselves.
- A toast never takes focus when it appears and never interrupts typing. Its click target and its dismiss control are keyboard-operable and in the tab order.
- Info is announced politely (`role="status"`); Warn and Error are announced assertively (`role="alert"`). Entry and exit motion is absent where the platform reports reduced motion.
- The stack stays legible at 1280×800.
- The toast mechanism is part of the notification facility of `NTF-notifications.md`. It is not a separate API.

## Specifications this work must change

- `specifications/ui/NTF-notifications.md`: the raise shape (add `level`); intent, UI contract boundary, and wireframes (add the toast stack, remove the statement); NTF-FR-01, NTF-FR-08, NTF-FR-11, NTF-FR-14, NTF-FR-15, NTF-FR-19 to NTF-FR-23, NTF-FR-24 (level of each of the five raises), NTF-FR-25, NTF-FR-JLXL, and NTF-FR-38 (retraction also removes toasts); add the toast requirements.
- `specifications/ui/GLS-global-settings.md`: GLS-FR-25 (the switch governs OS notifications only; toasts always show) and GLS-FR-27 (the rehearsal shows a toast while the main window is focused).
- `specifications/ui/SNV-shell-navigation.md`: SNV-FR-56 (the statement is removed; the toast stack is not a mutually-exclusive floating overlay).
- The specifications of the raising surfaces, to state each one's level: `DCR-draft-change-review.md` (DCR-FR-18), `PCR-prompt-change-review.md` (PCR-FR-17, PCR-FR-26), `GRU-graduation-runs.md` (GRU-FR-BLSS).
- Add frontend tests for: channel choice by focus, suppression, level behaviour and timers, stacking, replace-in-place, overflow, click routing, dismissal, withdrawal on reach and retraction, the unreachable-address toast, and accessibility. 