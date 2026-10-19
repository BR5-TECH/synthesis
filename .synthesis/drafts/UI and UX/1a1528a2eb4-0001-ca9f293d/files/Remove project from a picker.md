## Intent

Project picker window needs an improvement: every entry in its Recent projects list gets a context menu. The menu lets the user **Pin** or **Unpin** the project and **Hide** it from the recent list, without a trip to Global settings.

**Hide** is the existing "remove recent project" operation (`GSS-FR-08`) under a user-facing label. It removes the entry from every recent list (picker, project switcher, Global settings → Recent projects). It never deletes the project from disk. The entry returns only when the project is opened or created again.

## User journey

- User starts an application
- User sees a list of recent projects in the project picker window
- User can right-click on any entry in the list, including pinned entries and pinned entries flagged as missing. The menu also opens from the context-menu key and Shift+F10 on the focused entry.
- The menu has two entries: **Pin** (or **Unpin** on a pinned entry) and **Hide**.
- User clicks **Pin** or **Unpin**. The menu closes and the list re-orders at once: pinned entries first, then most-recent first.
- User clicks **Hide**. The menu closes and the entry leaves the list at once. There is no confirmation dialog and no undo. The project stays on disk.
- Hiding a pinned entry also unpins it, because the entry is removed. If the user opens the project later, it returns as an unpinned entry.
- If the list becomes empty, the picker shows its empty state (`PPK-FR-RQZV`).
- If the backend operation fails, the list stays unchanged and the picker shows an error message attached to the list.

## Requirements

- Look-and-feel of the context menu must match the application. Follow the existing context menus: `LCM-library-context-menu.md` LCM-FR-07 (leading icon on each entry) and the tab menu in `TAB-tabs.md`.
- The menu is a floating overlay. It is not clipped by the scrolling list or by the fixed window, which has no resize (`PPK-FR-09`). It opens at the pointer, and it stays fully inside the window (it flips or shifts near an edge). It adds no scrollbar to any picker region (`PPK-FR-10`, `PPK-FR-11`).
- Only one menu is open at once. The menu closes when the user chooses an entry, presses Escape, clicks outside it, or scrolls the list. Escape and a choice return focus to the entry.
- Keyboard: the menu takes focus when it opens, arrow keys move through the entries, Enter chooses one. Each entry has an accessible name. The menu has the role `menu`.
- Right-click on an entry does not open the project. A missing pinned entry has the same menu.
- The picker uses existing operations only: "pin recent project", "unpin recent project", "remove recent project" and "list recent projects". It adds no new storage and no hidden flag.
- After Pin, Unpin, or Hide, the picker reloads the list with "list recent projects" so that the ordering and pruning rules of `GSS-FR-05` to `GSS-FR-07` apply.

## Specifications to update

- `specifications/ui/PPK-project-picker.md`: change PPK-FR-13. The picker is no longer view-and-open only. It offers Pin, Unpin and Hide per entry through a context menu. Clear-all remains in Global settings only. Add requirements for the menu (opening routes, keyboard, layout, failure). Check PPK-FR-02 for the right-click rule.
- `specifications/ui/GLS-global-settings.md`: GLS-FR-07 stays valid. Add a cross-reference that the picker offers pin, unpin and remove too.
- `specifications/core/GSS-global-settings-storage.md`: no behaviour change. Optionally note in GSS-FR-08 and GSS-FR-09 that the picker also calls these commands.