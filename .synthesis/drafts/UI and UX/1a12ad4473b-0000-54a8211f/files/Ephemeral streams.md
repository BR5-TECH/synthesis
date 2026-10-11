## Intent

User should be able to create a short-living streams that will be auto-merged and auto-deleted after all the work in that stream is finished.

## User journey

- User finishes a draft
- User starts graduation
- In the graduation start dialog, user selects the destination of the run. The destination is one of four: **Existing stream**, **New stream**, **Temporary stream** and **Work directly**.
- IF: User selects **Temporary stream**, user types a name and selects the base branch, on the same terms as **New stream**. The draft name is the default name, shown as a placeholder. On confirm, the temp stream is created and the graduation starts on it.
- Streams button in IDE top chrome is automatically updated to reflect new stream with ongoing work. The dropdown row of a temp stream says in words that it is temporary.
- While the graduation runs user might decide to add more graduations to the queue of this stream.
- Once the queue of this stream is empty and every run of it is `completed` - user is prompted with a dialog to propose a merge to the base branch. No prompt appears while a run of the stream is failed, discarded, or waits on the author. No prompt appears at launch or project open for a stream whose last run finished while the app was closed; user merges that stream from the Streams dropdown.
- IF: User selects “Yes”, a merge confirmation opens, as for **Merge stream**: user leaves the result uncommitted or commits it with a message. The default message lists every graduation run of the stream, in queue order, one per line, each line preceded by `- `.
- After a merge that Git settles, or when the stream holds nothing to merge, the temp stream is deleted automatically. After a merge that Git cannot settle, a merge run is created, and the temp stream is deleted automatically only when that merge run is `completed` successfully. In every other outcome the stream and its work stay.
- IF: User selects “No” as an answer, the temp stream stays. User can merge it with **Merge stream** from the Streams dropdown in the top chrome at any time, and the same automatic deletion applies after that merge. The prompt shows again only after later work on the stream finishes and the queue is empty again.

## Requirements

- All regular stream queue rules shall apply.
- All regular merge/reconciliation logic shall apply.
- A temp stream records that it is temporary. The record survives relaunch, so a merge from the Streams dropdown after relaunch also deletes the stream.
- Automatic deletion uses the same deletion rules as **Delete stream**. A refusal (for example the stream is the active worktree, or its working copy holds uncommitted paths) deletes nothing. The refusal shows on the stream's row, and user deletes the stream by hand.
- A merge run that fails or is discarded does not delete the stream.
- Adding a graduation to a temp stream while it runs is allowed, and the prompt waits for the queue to be empty. A temp stream is not offered in the dialog as a destination of any other kind of run.
- The merge prompt is a floating overlay (per `SNV-shell-navigation.md` SNV-FR-56). It must not close another open overlay or discard what user typed there. It waits until no other overlay is open.
- Temporary stream, its fields and its errors follow the New stream rules in the start dialog, including GSD-FR-QGTC (create, then start) and GSD-FR-TZGT (a created stream stays when the start is refused).
- Specifications this work changes: `specifications/ui/GSD-graduation-start-dialog.md` (GSD-FR-BZHW, GSD-FR-QGTC, GSD-FR-TZGT), `specifications/ui/WSS-work-stream-selector.md` (temp marker on rows, merge confirmation default message), `specifications/core/WKS-work-streams.md` (temporary flag, automatic deletion after merge), `specifications/ui/CMW-commit-message.md` (merge route default message), and `specifications/ui/SNV-shell-navigation.md` (SNV-FR-56 overlay list).