## Intent

Sometimes AI API allows transparent change of the model. I.e. agent used for conversational AI requests Claude Opus 5.5 and in model’s response we’re getting a different model, i.e. Claude Opus 4.8. This stuff should be transparent and visible to user.

User should receive a warn toast at least, with an option to acknowledge or suppress such messages.

## User journey

- The author talks to an agent, or runs a graduation, on an AI API provider. The request names model A. The response names model B, and B is not the same id as A.
- The application raises one **Warn** notification: *Model changed. Requested A, served B.*
- IF the main window holds OS focus: a toast shows. It shows even when the author is looking at the target tab. It stays until the author acts.
  - The close control is **Acknowledge**. It removes only that toast and stores nothing.
  - The **Suppress** control opens a small menu: *1 hour*, *1 day*, *1 week*, *Forever*. The author picks one. The toast is removed and the pair stays silent until the choice expires.
  - A click on the toast body opens the target: the conversation tab for an agent turn, the run for a graduation.
- IF the main window has no OS focus: an OS notification posts, like every other raise (NTF-FR-02). It has no Suppress action. A click on it opens the same target.
- IF the pair is suppressed: no toast and no OS notification show. The log record is still written.

## Requirements

### Detection

- The check covers two callers only: conversational agent turns on an AI API provider (`AGC-agent-conversations.md`, `CVL-conversation-loop.md`) and the graduation loop when it calls an AI API provider (`GRL-graduation-loop.md`). Agentic CLI backends are out of scope.
- A **downgrade** is any difference between the requested model id and the model id the provider reports in the response. Compare the two ids as exact strings. Do not normalise them. Do not rank them as higher or lower. A response that reports no model id raises nothing.
- The conversation loop must expose the served model id from the response to its caller. If the framework adapter does not report it today, the work adds it.
- A turn or a run raises at most once for one requested→served pair, even when the turn has several provider rounds.

### Notification

- The raise uses the one raise API (NTF-FR-01) with level **Warn**. It is a new raise source, so it is added to the sources of NTF-FR-24 and to the toast sources of the `Toasts` draft.
- Channel: the regular rule applies. The main window holds OS focus → toast. Otherwise → OS notification. Never both. The Notifications switch and the OS permission govern the OS notification only (GLS-FR-25, NTF-FR-11).
- Address: for an agent turn, the tab where the conversation happens, by an existing address kind (`draft` or `file`, NTF-FR-03). For a graduation loop downgrade, the `run` address.
- A downgrade raise is **exempt from NTF-FR-08**. It shows its toast even when its address names the tab, panel, or run the author is looking at. This exemption is a property of the raise, not of the address, so no other raise changes.
- The raise key is derived from the requested→served pair. A second raise of the same pair replaces the toast that shows (NTF-FR-HZNF).
- The toast is a Warn toast: it never auto-dismisses.

### Toast actions

- Acknowledge is the toast's existing dismiss control. It removes only that toast, records nothing, and mutes nothing. The next downgrade of the same pair raises again.
- Suppress is one control after the body, in the tab order. It opens a **menu** with four items: *1 hour*, *1 day*, *1 week*, *Forever*. No item is pre-selected, and no duration is the default.
- NTF-FR-XNCG allows one link, and only on a toast with no address. This work extends the toast with an optional **action menu** that an addressed toast may carry. Activating the menu control or an item never triggers the click-to-open of the toast body. Keyboard: the control has `aria-haspopup="menu"`, Arrow keys move between items, Enter picks one, Escape closes the menu and returns focus to the control.
- Picking an item saves the suppression and removes the toast. If the save fails, the toast stays and an Error toast reports it (NTF-FR-TSRL).

### Suppression

- A rule covers one requested→served model pair. It does not cover other pairs.
- A rule stores the pair and an expiry time. *Forever* stores no expiry. An expired rule is ignored and removed when read.
- Scope: per project, per machine. Store it in the per-project slot of the user-global store, keyed by the repository identity (`GSS-global-settings-storage.md` GSS-FR-18), so all worktrees of one repository share it. Never write it into the repository.
- There is no list and no undo control in Settings. A rule ends only by expiry.
- A suppressed pair raises no toast, no OS notification, and no tab indication. The check runs before the raise.
- Suppression is only offered from the toast. An OS notification carries no action.

### Log

- Every downgrade writes one `WARN` record through `LGC-logging.md`, in the `ai` domain, whether or not the pair is suppressed and whether or not an OS notification can post. Fields: requested model id, served model id, provider, source (`agent` or `graduation`), and `suppressed` (true or false). No message text or prompt reaches the record.

### Accessibility

- The toast shows the Warn icon with a text label, announces as `role="alert"`, and never takes focus (NTF-FR-BVCG, NTF-FR-OPCD). The menu is fully keyboard-operable.

## Specifications this work must change

- `specifications/ui/NTF-notifications.md`: add the raise source (NTF-FR-24); add the NTF-FR-08 exemption for a raise that asks for it; extend NTF-FR-XNCG with the action menu on an addressed toast; add the toast requirements, wireframe, and key rule.
- `specifications/ui/GLS-global-settings.md`: GLS-FR-25 note that the switch does not stop the log record.
- `specifications/core/GSS-global-settings-storage.md`: add the suppression record to the per-project slot, with load, save, and expiry rules.
- `specifications/ai/CVL-conversation-loop.md` and `specifications/core/AGC-agent-conversations.md`: expose the served model id; detect and raise for an agent turn.
- `specifications/ai/GRL-graduation-loop.md` and `specifications/ui/GRU-graduation-runs.md`: detect and raise for a graduation, with the `run` address.
- `specifications/core/LGC-logging.md`: only if the `ai` domain needs a documented record shape.


- Add frontend tests for: the Warn toast, the exemption from NTF-FR-08, Acknowledge, the four menu durations, expiry, suppressed pair, OS notification when unfocused, replace-in-place by pair key, and keyboard operation of the menu.