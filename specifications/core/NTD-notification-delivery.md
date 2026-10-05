# Notification delivery

**Spec code:** `NTD`

## Intent
The backend facility that puts a short message in front of the author through the operating system's own notification centre, and reports back when they act on one. It exists because the application is often not the window the author is looking at — a long agent run finishes while they are reading mail, or asks a question while they are in another project — and the only mechanism that reaches them there belongs to the OS rather than to any surface of this application. Delivery is deliberately thin: this module posts, replaces, withdraws, and reports activation, and it decides nothing — whether a message is worth sending, what it should say, and where the author should land are all `../ui/NTF-notifications.md`'s. Out of scope: rendering anything inside the application, since everything this module shows is drawn by the OS; interpreting the payload a notification carries, which travels through opaque; keeping any record of what was posted, so there is no history to query and nothing survives a relaunch; and registering a URL scheme, a protocol handler, or any other means by which a process outside this application could address it — a payload reaches this module only from within the running application and leaves it only into the same.

## Contract surface
The module owns the application's registration with the platform notification centre and the Tauri commands and event below. Names match `../ui/NTF-notifications.md` byte-for-byte.

### Tauri commands
- `"get notification permission"` → `get_notification_permission()` → `{ state: "granted" | "denied" | "not_requested" | "unsupported" }` — the platform's current disposition toward this application. Prompts for nothing and never returns an error.
- `"request notification permission"` → `request_notification_permission()` → the same shape, reflecting the outcome. This is the only operation here that can put a permission prompt in front of the author.
- `"post notification"` → `post_notification(notification)` → `{ id }` on success, or a typed error: `permission_denied`, `unsupported`, `delivery_failed`.
- `"withdraw notification"` → `withdraw_notification(id)` → nothing. Idempotent; an id that names nothing showing is not an error.
- `"withdraw all notifications"` → `withdraw_all_notifications()` → nothing. Idempotent.

### Events (Tauri event bus)
- `"notification activated"` — emitted when the author activates a notification this running application posted. Payload: `{ id, key, payload }`, carrying exactly what the post supplied.

### Payload shapes
```
Notification {
  key,        // caller-chosen identity of the thing being notified about;
              // a post whose key matches one still showing replaces it in place
  title,      // short line the OS renders as the notification's heading
  body,       // the sentence beneath it
  payload     // opaque string handed back on activation; never parsed here
}
```

## Functional requirements
1. **NTD-FR-01** The five commands and the `"notification activated"` event exist with the documented payload shapes. In the walking-skeleton build the implementation may deliver nothing to a real notification centre and may synthesise an activation on request, provided every documented shape is returned so `../ui/NTF-notifications.md` is fully exercisable against it.
2. **NTD-FR-02** `get_notification_permission()` reports the platform's current disposition and nothing else: `granted` when this application may post, `denied` when the author has refused it in the operating system's own settings, `not_requested` when the platform has never been asked, and `unsupported` on a platform with no notification centre this module can reach. It prompts for nothing, persists nothing, and is answerable at any time, whether or not a project is open.
3. **NTD-FR-03** `request_notification_permission()` is the only operation here that can present a permission prompt. It asks the platform once and returns the resulting state; on a platform that grants without asking it returns `granted` without a prompt, and on one that has already refused it returns `denied` rather than asking again, because a refusal is reversed in the operating system's settings and not by this application.
4. **NTD-FR-04** `post_notification` delivers through the platform's own notification centre and renders nothing itself: this module opens no window, draws no surface, and plays no sound of its own. How the message is presented, how long it stays, where it stacks, and what the platform's own do-not-disturb and focus rules do to it are the operating system's, not this module's.
5. **NTD-FR-05** `title` and `body` are rendered as the OS renders them. This module applies no markup, no formatting, and no truncation of its own; where a platform bounds their length, the platform's bound is what applies.
6. **NTD-FR-06** A post whose `key` matches a notification this application posted and that is still showing **supersedes** it rather than being treated as a new thing: one slot, one `id` — the earlier notification's, so an id a caller is holding stays valid — and the later payload, so an activation routes to the current target. Whether the operating system is asked to show anything for it depends on what changed. A superseding post whose `title` and `body` both match what is already showing is **not handed to the operating system at all**, so a caller reporting repeatedly about one thing occupies one notification however many times it posts; the payload sits outside that comparison and is carried through regardless, an address that moved while the words did not being nothing new to read. A superseding post whose `title` or `body` differs **is** delivered, because different words are different news: on a platform that can retract it replaces what was showing, and on one that cannot it appears alongside the superseded notification, which stays until the author clears it — both carrying the same `id`, so neither routes anywhere the other does not. Superseding is scoped to what is currently showing: once a notification has been activated or withdrawn, the same `key` carrying the same words is a new notification and is delivered again, so a thing that happens twice is announced twice.
7. **NTD-FR-07** `id` is unique across every notification for the lifetime of the running application and is never reused, so a caller keying by `id` never confuses one notification with a later one.
8. **NTD-FR-08** `payload` is opaque. It is carried byte-for-byte from the post to the activation, is never parsed, inspected, truncated, or acted on here, and no shape is required of it. A payload this module cannot interpret is not an error, because it interprets none.
9. **NTD-FR-09** Activating a notification this running application posted **raises the application's mounted window** — the main window, or the Project picker when that is what is mounted — and gives it OS focus, and only then emits `"notification activated"` carrying that notification's `id`, `key`, and `payload`. Raising precedes the event so the consumer routes into a window already in front of the author. Raising changes the window's focus alone: its outer dimensions, its maximized state, and its OS full-screen presentation are all left exactly as they were, and no preference describing any of them is written (per `../ui/SNV-shell-navigation.md` SNV-FR-38).
10. **NTD-FR-10** Exactly one `"notification activated"` event is emitted per activation, and the notification is withdrawn as part of it, so an activated notification is gone from the notification centre and can never be activated a second time.
11. **NTD-FR-11** Dismissing a notification without activating it — the platform's own dismiss gesture, or its expiry from the notification centre — emits nothing and changes nothing in the application: no window is raised, no event is delivered, and the application is not brought to focus.
12. **NTD-FR-12** Every notification this module posted is withdrawn as the application quits, so the notification centre holds none of them once the process is gone.
13. **NTD-FR-13** An activation of a notification posted by an earlier run of the application routes nowhere: no `"notification activated"` event is emitted, because the `id` and `payload` it would carry name a process that no longer exists. The application is left at whatever it does on an ordinary start, which is the Project picker (per `../ui/OVW-overview.md` OVW-FR-01). NTD-FR-12 is what makes this rare rather than routine.
14. **NTD-FR-14** `post_notification` returns `permission_denied` and posts nothing while permission is anything other than `granted`, and it never prompts on its own. A prompt therefore never appears because some background work reached this module; it appears only where the author asked for one (NTD-FR-03).
15. **NTD-FR-15** `post_notification` returns `unsupported` on a platform with no reachable notification centre, and every other operation here answers normally on such a platform: permission reads `unsupported`, both withdrawals succeed doing nothing, and no other part of the application fails on account of the absence.
16. **NTD-FR-16** `delivery_failed` covers a post the platform accepted the shape of but did not deliver. It is distinct from `permission_denied` and from `unsupported`, because each calls for a different correction, and it leaves nothing showing and nothing recorded.
17. **NTD-FR-17** Posting is asynchronous and never blocks the caller: a caller's post returns without waiting on the notification centre, and no consumer's absence, slowness, or failure to subscribe to `"notification activated"` delays it.
18. **NTD-FR-18** This module retains no record of what it posted. Nothing it holds is persisted, no command returns notifications that have been activated, dismissed, or withdrawn, and there is no query for what was posted earlier in the session. What is showing lives in the operating system's notification centre and nowhere else.
19. **NTD-FR-19** This module mutates nothing observable beyond the notification centre and the window raise of NTD-FR-09: no file, no repository, no setting, and no state belonging to whatever the notification was about.
20. **NTD-FR-20** Nothing here registers a URL scheme, a protocol handler, a file association, or any other means by which a process outside this application could address it or launch it. A `payload` enters this module only through `post_notification` called from within the running application, and leaves it only through `"notification activated"` into that same application.
21. **NTD-FR-21** Every command here answers whether or not a project is open, because a notification is a fact about the application rather than about a project, and one may need withdrawing while the Project picker is showing.
22. **NTD-FR-22** Every operation here works without network access.

## Non-functional requirements
- Posting is cheap enough to sit inside an event handler without measurably delaying it; the notification centre is reached off the calling path.
- The set of notifications currently showing is held in memory only, bounded by what the platform is actually displaying, and nothing accumulates across a session.
- The window raise of NTD-FR-09 completes before the activation event reaches any consumer, so a consumer never navigates a window the author cannot yet see.
- A permission refusal degrades the application to silence rather than to failure: every surface keeps working, and nothing anywhere reports an error the author did not ask a question to receive.
- The platform's own presentation rules govern entirely — this module competes with no other application's notifications and overrides no do-not-disturb, focus mode, or alert-style setting.
