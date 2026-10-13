# Intent

GitHub projects integration needs an improvement 

## Issue 1

In the **Publish to GitHub** chooser, the parent-issue list renders one radio row for each parent (NAW-FR-YLKD). With many parents, the list overflows the window. This is not acceptable.

**Requirements**

1. The chooser keeps the **Publish as a root issue** radio as the first option, selected when the chooser opens (NAW-FR-TCQB).
2. A second option, **Publish as a sub-issue of**, replaces the parent rows. It holds one filterable dropdown (combobox) of the parent issues that the read returns. Each option shows the issue number, the title, the Type, and the milestone or a statement that it has none. The chooser lists no other issue.
3. Choosing an issue in the combobox selects the sub-issue option. Selecting the root option sends no parent.
4. The dropdown list opens as an overlay with a bounded height and scrolls inside. It stays inside the window, flips or shrinks where there is no room, and never grows the chooser or the window.
5. A filter field in the dropdown matches the issue number and the title. The match is a case-insensitive substring. The number matches with or without a leading `#`. A filter with no match shows a **No matching issue** notice in the list.
6. The four list states of NAW-FR-WMEP stay. An empty or unavailable parent list renders its notice in place of the combobox. The root option stays enabled in every state (NAW-FR-TCQB).
7. **Publish** is disabled while the sub-issue option is selected and no parent is chosen. Otherwise NAW-FR-RBTE stays.
8. Keyboard and accessibility: the combobox follows the ARIA combobox pattern. Arrow keys move through the options, Enter chooses, Escape closes the dropdown only (the chooser stays open), and the filter text is announced with the count of matches. Focus returns to the combobox after a choice. The dialog rules of NAW-FR-PSMO stay.

## Issue 2

Selecting a GitHub Project is refused with a message that the `In Progress` status does not exist. The Project in GitHub has `In progress`. Today the configuration is valid only when the `Status` field holds options named exactly `Ready` and `In Progress` (GPP-FR-DEZO). Make both statuses configurable in the **GitHub Project settings** section of Project settings.

**Requirements**

1. After the author selects a Project, the section reads the options of that Project's single-select `Status` field through a new read-only operation (for example `"list github project statuses (project node id)"`). It shows two selectors beneath the Project selector: **Ready status** and **In Progress status**. Each lists those options. The selectors are absent while no Project is selected. A failed read renders the error inline with **Retry**.
2. When the Project changes, each selector auto-selects the option whose name matches its default name (`Ready`, `In Progress`) case-insensitively. Where no option matches, the selector stays unset, and the section shows a configuration error that asks the author to choose an option.
3. The two chosen option names are polling settings. They persist at once, with the Project and the interval, through `"set github polling settings"` (extended with the two values), in the project-public store. The two values must differ. A change of the Project clears both and applies requirement 2.
4. The chosen statuses replace the fixed names everywhere polling uses them: the eligibility rule (the `Status` value equals the chosen Ready option), the validation of the configuration, and the Status update of a claim (the chosen In Progress option). Reads match the stored name exactly.
5. A saved status option that no longer exists in the Project (renamed or deleted on GitHub) makes the configuration `invalid` with a typed error (`ready_option_missing` or `in_progress_option_missing`). Polling stops, and claims refuse (GPP-FR-ANDE). The author must choose again. The IDE replaces nothing silently, and never falls back to the default names.
6. When the configuration becomes `invalid`, the IDE raises one Error-level notification through `NTF-notifications.md`, with a stable key, a body that names the missing status and the Project, and an address on the GitHub Project settings section. A toast shows when the main window holds focus. The raise happens once for each change into the error state, not once for each poll. A valid configuration retracts it.
7. The section states the error in words (SET-FR-GKTA) and states that polling is disabled while the error stands. A Project that stored no statuses before this change reads as unset and auto-selects the defaults on the next read.

## Specifications that this work changes

- `specifications/ui/NAW-new-artifact.md`: NAW-FR-YLKD, NAW-FR-WMEP, NAW-FR-RBTE, NAW-FR-PSMO (parent list becomes a combobox).
- `specifications/ui/SET-project-settings.md`: SET-FR-TTTB, SET-FR-VLQJ, SET-FR-GKTA (status selectors, error wording).
- `specifications/core/GPP-github-polling.md`: GPP-FR-NLPG, GPP-FR-DEZO, GPP-FR-EHRC, GPP-FR-IURX, GPP-FR-BOQX, GPP-FR-CRWY, GPP-FR-SUFH (configurable statuses, new read operation).
- `specifications/core/PSS-project-settings-storage.md`: PSS-FR-OPQD, PSS-FR-CNZO (two stored status names).
- `specifications/ui/GIT-git.md` and `specifications/ui/NTF-notifications.md`: the configuration-error state and the new notification raise.