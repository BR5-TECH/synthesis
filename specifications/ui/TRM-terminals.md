# Terminals

**Spec code:** `TRM`

## Intent
The Terminals panel and the terminal tab let the author open an interactive terminal into any checkout of the open project and use it beside the IDE. From a menu at the **New terminal** button, the author picks a target, the active worktree, another worktree, or an active work stream, and a kind: a shell on the host, a shell in a new container of the project's agent environment, a shell in the micro VM that a graduation queue runs, or a fresh interactive session of an agentic CLI. The panel lists every terminal of the project window, grouped by target, and says what each one is connected to; a tab shows one terminal live. The kinds of the active worktree are one click away, because that is the usual target. A terminal outlives its tab, so the author closes tabs freely and ends a terminal only on purpose. A persistent terminal also outlives the IDE, and a marker on its row and its tab tells the author so. Out of scope: the process and its life, which `../core/TRS-terminal-sessions.md` owns; Windows, where the surface is not offered; and remote and mobile sessions.

## Functional requirements

### Panel
1. **TRM-FR-LAZH** The Terminals panel is a vertical-panel surface that the Terminals toggle of the activity bar opens (per `SNV-shell-navigation.md` SNV-FR-PULG). It lists every session that `"list terminals"` returns, and it reads the list again on each `"terminals changed"`.
2. **TRM-FR-EAUY** The panel groups rows by target. A group header names the worktree directory or the stream name, its branch, and whether the target is the active worktree, another worktree, or a work stream. The group of the active worktree is first. The other groups follow in the order in which their earliest session was created.
3. **TRM-FR-ZQPC** Inside a group, the pinned rows come first in creation order, and then the unpinned rows in creation order.
4. **TRM-FR-BEYN** Each row shows the session's label, its kind (Host shell, Container shell, Microsandbox shell, or Agent CLI with the vendor name), where it runs (the host, or the container, dev container, or VM name), and its state: starting, running, exited with the exit code, or target gone.
5. **TRM-FR-DYKW** A pinned row shows a pinned marker, in space that every row reserves, so a pin change moves no label.
6. **TRM-FR-QFYS** A row of a persistent session (`persistent = true`, per `../core/TRS-terminal-sessions.md` TRS-FR-FKVW) shows a persistent marker, in space that every row reserves, with the tooltip "Survives IDE restart". The marker and the row's accessible name carry that text. Its icon is not the pinned icon, and it does not depend on colour alone.
7. **TRM-FR-QSRZ** A restored session needs no extra action. Its row, its tab, and its close act as for any other session in its state.
8. **TRM-FR-CTGK** A row whose target is busy (`busy_run_id` is not null) shows a busy mark with the tooltip "A graduation run is working in this target".
9. **TRM-FR-QSTC** With no session, the panel shows the empty state of `SNV-shell-navigation.md` SNV-FR-60: "No terminals", one sentence that says a terminal is a shell or an agent CLI session in a worktree or a work stream, and the **New terminal** action.
10. **TRM-FR-OXKC** The panel header holds a **New terminal** button. While the project holds 16 sessions (per `../core/TRS-terminal-sessions.md` TRS-FR-AYCY), the button and the empty-state action are disabled, and the tooltip says that the limit is reached.
11. **TRM-FR-PZJI** A click on a row, or Enter on the focused row, opens the session's tab, or focuses that tab where it is open (per `TAB-tabs.md` TAB-FR-VKPS). The row becomes the selected row.
12. **TRM-FR-UIWG** The rows are one keyboard list. The arrow keys move the focus between rows across groups, and the selected row carries `aria-selected`.

### New terminal menu
13. **TRM-FR-JINY** **New terminal** opens the New terminal menu, a dropdown that is anchored to the activated button and is a floating overlay of `SNV-shell-navigation.md` SNV-FR-56. A second activation of the button closes the menu. The menu reads `"list terminal targets"` each time it opens, and shows "Reading targets…" until the read ends.
14. **TRM-FR-QJSW** A failed read of the targets shows "The targets could not be read." in the menu and emits a `WARN` record.
15. **TRM-FR-AASW** The menu starts with the section of the active worktree: a header with its name, its branch, and "active worktree", and then the kind entries of that target. A separator follows. Then come one submenu entry for each other worktree by name, and one for each work stream by name.
16. **TRM-FR-GYIR** A submenu entry names its target, its branch, and its role, worktree or work stream, and a busy target shows the busy mark of TRM-FR-CTGK. Its submenu holds the kind entries of that target. A menu with no other target shows no separator and no submenu entry.
17. **TRM-FR-CZTR** The kind entries of a target are Host shell, Container shell, Microsandbox shell, and then one Agent CLI entry for each vendor that the target lists. Each entry names where it runs.
18. **TRM-FR-HJUD** An entry that is not available is greyed out, carries `aria-disabled`, does nothing when chosen, and shows its reason as text, for example "No VM runs for this target" for `no_vm_running` (per `../core/TRS-terminal-sessions.md` TRS-FR-XXPE).
19. **TRM-FR-PVTT** When the targets are read, the focus goes to the first available kind entry of the active worktree, else to the first entry. The Up and Down arrow keys move the focus between the entries of the menu that holds it. A header and a separator take no focus.
20. **TRM-FR-VJXC** A pointer over a submenu entry, a click, Enter, or the Right arrow key opens its submenu beside the menu. The keys also put the focus on its first entry. One submenu is open at a time. The Left arrow key or Escape in a submenu closes it, and the focus goes back to its submenu entry.
21. **TRM-FR-JZPT** Escape in the menu closes it, and the focus goes back to the button that opened it. An activation outside the menu and its submenu closes both. A closed menu creates nothing.
22. **TRM-FR-FYYB** A click, or Enter, on an available kind entry calls `"create terminal (target, kind, vendor)"` with that entry's target. While the call runs, the menu ignores other choices. On success the menu closes, the new row appears, and its tab opens and takes focus.
23. **TRM-FR-YVPJ** A create that fails keeps the menu open, shows the error under the kind entries of the chosen target, and emits a `WARN` record.
24. **TRM-FR-IVGX** While a target is busy, its section or its submenu shows the line "A graduation run is working in this target." above its kind entries. The line disables no entry.

### Row context menu
25. **TRM-FR-EUMZ** A right-click on a row, or the context-menu key or Shift+F10 on the focused row, opens the row's context menu. It opens no tab. It is a floating overlay of `SNV-shell-navigation.md` SNV-FR-56, and it closes on a choice, on Escape (the focus goes back to the row), and on any activation outside it. The arrow keys move, and Enter chooses.
26. **TRM-FR-KFCS** The menu holds exactly four entries in this order: the pin action, a separator, **Close terminal**, **Close other terminals**, **Close all terminals**. The pin action is **Pin terminal** on an unpinned row and **Unpin terminal** on a pinned row, never both, and it is never disabled. Each entry has a leading icon (per `LCM-library-context-menu.md` LCM-FR-07) and an accessible name.
27. **TRM-FR-EKGK** The menu has the same look as the other context menus of the app: its entries show no button border or fill, and an entry shows a highlight over the full row on hover and on keyboard focus. A disabled entry shows dimmed and gets no highlight on hover.
28. **TRM-FR-AHXU** **Pin terminal** and **Unpin terminal** call `"set terminal pinned (id, pinned)"` and do nothing else: no tab opens or closes, and no session ends.
29. **TRM-FR-MANC** **Close terminal** ends the clicked row's session, pinned or not.
30. **TRM-FR-BHYF** **Close other terminals** ends every unpinned session of the project window except the clicked one, in all groups. **Close all terminals** ends every unpinned session of the project window, in all groups. Neither ends a pinned session.
31. **TRM-FR-EZMO** **Close other terminals** and **Close all terminals** are each disabled while the set of sessions they would end is empty.
32. **TRM-FR-JJEY** Where a close would end at least one session in `starting` or `running`, a confirmation dialog states how many running terminals end, and that their processes stop and they cannot be restored. **End terminals** does the close, and **Cancel** changes nothing. A close of sessions that are all exited or target gone asks nothing.
33. **TRM-FR-OCSN** A close calls `"close terminal (id)"` for each session it ends. The row and the tab of each one go away.

### Terminal tab
34. **TRM-FR-XBVB** A terminal tab shows its session in a `ghostty-web` terminal, at a version that `package.json` pins exactly.
35. **TRM-FR-CGRR** Each terminal that a tab makes has its own `ghostty-web` WebAssembly instance, also the terminal that a theme change makes again. A new terminal starts blank and shows only the output of its own session, never cells of another terminal.
    - *Why:* `ghostty-web` frees the screen memory of a disposed terminal but does not clear it, so a terminal that shares one instance with an earlier terminal shows the old cells.
36. **TRM-FR-WRVB** The terminal uses the Source code typographic role (per `OVW-overview.md` OVW-FR-13) and colors from the app theme. A change of the role's font applies to an open tab at once. A change of the theme applies at once to a tab that shows. A hidden tab gets the new theme when it shows next, before it writes more output.
    - *Why:* the terminal library cannot recolor an open terminal, so a theme change makes it again, and a terminal made out of the page cannot measure its size.
37. **TRM-FR-ILRQ** The terminal fills the tab. When the tab's size changes, the terminal fits its columns and rows to the new size and calls `"resize terminal (id, cols, rows)"` once for each new size.
38. **TRM-FR-AULS** When the tab opens, it calls `"attach terminal (id, output)"`, writes the history and then the live output into the terminal, and calls `"acknowledge terminal output (id, bytes)"` for each frame it received. A frame that the terminal fails to write is also acknowledged. The first failed write of each terminal emits one `WARN` record.
39. **TRM-FR-ITAK** A tab that is not the active tab stays attached and keeps writing output, so the tab shows the current output when it becomes active again.
40. **TRM-FR-JXLA** Closing a tab calls `"detach terminal (id)"` and ends no session. The row stays, with its pinned state.
41. **TRM-FR-RFVP** While the terminal has the focus, every key without the platform's command modifier goes to the session, Ctrl with a letter included. On Linux, no application menu accelerator acts on a key that goes to the session.
42. **TRM-FR-DHEM** The terminal copies selected text with Cmd+C on macOS and Ctrl+Shift+C on Linux, and pastes with Cmd+V and Ctrl+Shift+V. A paste uses bracketed paste where the session turned it on.
43. **TRM-FR-DTCA** The terminal keeps a scrollback of 10,000 lines, which the user scrolls with the wheel, the scroll bar, and Shift+PageUp and Shift+PageDown.
44. **TRM-FR-DPWA** The tab's title is the session's label. An exited session and a session with a gone target show a state marker on the tab.
45. **TRM-FR-PHTL** The tab of a persistent session shows the persistent marker of TRM-FR-QFYS after its title, with the same tooltip and accessible name.
46. **TRM-FR-GMGF** While the session is `starting`, the tab shows "Starting" over the start output.
47. **TRM-FR-FFPL** When the session is `exited`, the tab shows a line under the output with the exit code, or the error line of a failed start, and sends no more input.
48. **TRM-FR-XVJT** When the target is gone, the tab stays open and shows a **Target gone** banner. The tab is a read-only view of the history until the user closes it.
49. **TRM-FR-HQDM** A tab opened for an exited session that has no history shows the exit line of TRM-FR-FFPL and the text "The output of this terminal is gone."
50. **TRM-FR-XVFQ** While the target is busy, the tab shows a persistent warning, "A graduation run is working in this target." Input still goes to the session.
51. **TRM-FR-RCCE** When a terminal tab becomes the active tab, the panel selects its row on the terms of `SNV-shell-navigation.md` SNV-FR-64 and SNV-FR-66.
52. **TRM-FR-DURY** The tab's **Pin tab** and **Unpin tab** set the session's pinned state, and the tab shows the pinned marker while the session is pinned (per `TAB-tabs.md` TAB-FR-KIIW).

### Platform and logging
53. **TRM-FR-DQYZ** On Windows, the Terminals toggle, the panel, the New terminal menu, and the terminal tab are not rendered.
54. **TRM-FR-LNOZ** The panel and the tab emit a `DEBUG` record for each create, close, attach, and detach, and a `WARN` record for each failed call. No record carries input, output, or a path.

## Wireframes
The Terminals panel with three sessions in two groups:

```
┌ TERMINALS ─────────────── [+] ┐
│ ▾ main · active worktree      │
│   ▣ ↻ Host shell 1    running │
│       host                    │
│   ◇   Claude Code 1   running │
│       docker · syn-term-7f3a  │
│ ▾ auth-rework · stream  ⚠     │
│   ◇   Microsandbox 1  exited 0│
│       vm · syn-auth-rework    │
└───────────────────────────────┘
```

The New terminal menu under the panel header's **New terminal** button, with the submenu of a busy work stream open:

```
                                  [+]
┌─────────────────────────────────────┐
│ main · main · active worktree       │
│   Host shell                  host  │
│   Container shell           docker  │┌─────────────────────────────────┐
│   Microsandbox shell                ││ A graduation run is working in  │
│     No VM runs for this target      ││ this target.                    │
│   Claude Code               docker  ││   Host shell              host  │
│   Codex                       host  ││   Container shell       docker  │
│ ─────────────────────────────────── ││   Microsandbox shell        vm  │
│   feature-x · feat/x · worktree   ▸ ││   Claude Code           docker  │
│   auth-rework · auth · stream  ⚠  ▸ │└─────────────────────────────────┘
└─────────────────────────────────────┘
```

Layout notes:
- `▣` is the pinned marker, `↻` the persistent marker, and `⚠` the busy mark. Each row holds the space of the pinned and the persistent marks.
- The row's second line holds where it runs. The state sits on the first line, aligned to the trailing edge.
- A greyed kind entry keeps its reason visible without hover.
- The menu aligns its trailing edge with the button and stays inside the window. A submenu opens on the trailing side of its entry, or on the leading side where the window has no space.

## UI contract boundary
- **Owned by the UI**: the Terminals panel with its groups, sort order, rows, pinned, persistent and busy markers, empty state and keyboard list; the New terminal menu with its active-worktree section, its submenus, its keyboard, and its greyed entries; the row context menu, its entries, their enablement and the confirmation for running terminals; the terminal tab with `ghostty-web`, its fit, theme, font, keys, copy and paste, scrollback, banners and state lines; the tab's attach and detach; and the routing of the tab's pin actions to the session.
- **Delegated to backend (abstract)**:
  - `"list terminal targets"` — the targets and kinds of the New terminal menu. Owned by `../core/TRS-terminal-sessions.md`.
  - `"list terminals"` — the panel's rows. Owned by `../core/TRS-terminal-sessions.md`.
  - `"create terminal (target, kind, vendor)"` — the menu's kind entries. Owned by `../core/TRS-terminal-sessions.md`.
  - `"attach terminal (id, output)"` — a tab's output. Owned by `../core/TRS-terminal-sessions.md`.
  - `"detach terminal (id)"` — a tab's close. Owned by `../core/TRS-terminal-sessions.md`.
  - `"write terminal input (id, data)"` — keys and pastes. Owned by `../core/TRS-terminal-sessions.md`.
  - `"resize terminal (id, cols, rows)"` — a tab's fit. Owned by `../core/TRS-terminal-sessions.md`.
  - `"acknowledge terminal output (id, bytes)"` — flow control. Owned by `../core/TRS-terminal-sessions.md`.
  - `"set terminal pinned (id, pinned)"` — the pin actions of the row and of the tab. Owned by `../core/TRS-terminal-sessions.md`.
  - `"close terminal (id)"` — the close actions. Owned by `../core/TRS-terminal-sessions.md`.
  - `"terminals changed"` — the event that refreshes the panel and the tabs. Owned by `../core/TRS-terminal-sessions.md`.

## Non-functional requirements
- The Content Security Policy of the Tauri window allows the WebAssembly that `ghostty-web` loads (`wasm-unsafe-eval`), and nothing wider.
- A hidden terminal tab costs no layout work: it writes output into its terminal model and renders on the next frame that shows it.
- The terminal renders at the display's device pixel ratio.
