## Intent

When publishing an issue to GitHub project today, the publication metadata (parent issues, issue Types, milestones) is read when the chooser opens (`load publication metadata`). The chooser waits for this read, which is a poor experience. The backend must pre-load the three lists in the background when the project opens, and refresh them every 30 minutes while a GitHub Project is selected in Project settings. The chooser opens with the cached lists, requests one more background refresh, and updates its controls when that refresh ends.

Users: authors who publish drafts to GitHub. Scope: the backend cache, its refresh schedule, its change event, and the chooser's use of it. Publication, the GitHub Project polling, and the Project settings controls do not change.

## User journey

- User finishes working on a draft
- User clicks on `Publish to GitHub`
- Publish overlay window opens.
- The chooser opens for the resolved remote (per NAW-FR-DWKA) and shows the three lists from the backend cache at once. Where the cache holds no lists for that remote (for example, a remote the author picked in the remote picker, or a failed startup refresh), the chooser behaves as today: the lists show **loading** and the chooser requests the read itself.
- The chooser requests one background refresh at once. **Publish** is disabled until that refresh ends. The lists stay usable while it runs.
- When the refresh ends, the backend compares the fresh lists with the cache.
- IF there is a difference THEN the backend replaces the cache with the fresh lists and emits a change event. The chooser adds new items and removes missing items in the open dropdown. It keeps the filter text and the scroll position. IF the selected parent is no longer in the list THEN the selection is cleared. IF the chosen Type or milestone is no longer in its list THEN the control resets to **No Type** or **No milestone**.
- **Publish** becomes enabled when the refresh ends with no difference, or with a difference that still holds the selected parent. With the root option selected, no parent is needed. With the sub-issue option selected and the selection cleared, **Publish** stays disabled until the author chooses a parent or selects the root option (per NAW-FR-RBTE).
- IF the refresh fails THEN the chooser keeps the cached lists, shows no toast, and enables **Publish**. The backend still validates the parent on publish (per GHP-FR-RMVQ).

## Requirements

- **Cache.** The backend holds the cache in memory only. It holds the parent list, the Type list, and the milestone list of GHP-FR-DZLB for the resolved publication remote, each with its own state. It is shared by all windows. It is never written to disk. A project switch or close discards it, and a result that belongs to a project no longer open is discarded (as in PST-FR-37).
- **Read by the chooser.** The chooser reads the cache through a command and requests a refresh through a command. The refresh runs in the background and blocks no command and no UI thread. The backend emits an event with the new lists when a refresh changes the cache. The chooser and every other window follow that event.
- **Run condition.** The updater runs only when all three hold: the publication remote resolves, the project token is eligible, and a GitHub Project is selected in Project settings.
- **Startup.** The first refresh starts when the project opens. If it fails, the main window raises one **Error** toast, including when the remote or the token is missing while a GitHub Project is selected. The failure is also logged. Where no GitHub Project is selected, the updater does not run and raises nothing.
- **Periodic.** The backend refreshes all three lists every 30 minutes, as a best effort. It does not overlap two refreshes. A failure or timeout is logged and never shown to the author.
- **Invalidation.** A change of the selected GitHub Project, the publication remote, the token, or the parent issue Types of the Publication group starts one refresh at once and reschedules the timer.
- **Failure keeps the cache.** A failed or timed out refresh leaves every cached list as it was. One list that fails does not replace or change the other lists (per GHP-FR-DZLB).
- **Chooser.** The chooser applies the rules of the user journey. Root publication stays available in every state (per NAW-FR-TCQB).

## Specifications to update

- `specifications/core/GHP-github-publication.md`: add the cache, the cache read and refresh commands, the change event, the run condition, the schedule, and the invalidation. `load_publication_metadata` (GHP-FR-MDLD) stays the read that the refresh uses.
- `specifications/ui/NAW-new-artifact.md`: change NAW-FR-HNVR (it loads once when it opens), NAW-FR-RBTE (the disabled rule for the refresh), NAW-FR-ASYN and NAW-FR-WMEP (cached lists are shown while the refresh runs).
- `specifications/ui/NTF-notifications.md`: NTF-FR-24 lists exactly nine raises. Add the startup refresh failure as an **Error** toast and update the count.
- `specifications/core/PSS-project-settings-storage.md` and `specifications/ui/SET-project-settings.md`: only where they state that the publication settings or the Project selection change no other behavior. They now start a refresh.