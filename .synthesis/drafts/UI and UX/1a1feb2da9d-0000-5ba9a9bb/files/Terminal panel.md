## Intent

User working in IDE should be able to open Terminal from the left panel button. The panel should be showing the list of terminals opened. Click on each of them should be opening a tab with terminal.

User should be able to open terminal to the project folder on host machine where the agent is running, as well as into the docker container as interactive session (if agentic cli docker configured) and or just run interactive session of the agentic cli instead of the headless.

User should be able to do this in the current working tree/branch or in any workstreams that are active. 

The intent is to allow user to have a way to interact with the code inside working trees and workstreams if needed. Left panel should be showing what terminal is connected to. 

## User journey

- The user selects **Terminals** in the vertical-panel activity bar. The panel lists every terminal of this project window, grouped by target (worktree or workstream). With no terminal, it shows the empty state of SNV-FR-60 and the **New terminal** action.
- The user selects **New terminal**, then a target (worktree or workstream) and a kind: **Host shell**, **Container shell**, **Microsandbox shell**, or **Agent CLI** (a vendor). A new row appears and its tab opens.
- The user clicks a row. Its tab opens, or is focused if open. The tab shows the live terminal.
- The user closes a tab. The process keeps running and the row stays. The user clicks the row to see it again.
- The user ends a terminal with an explicit **Close terminal** action on the row. The row and its in-memory scrollback go away.
- The user right-clicks a row. A context menu opens with **Pin terminal** or **Unpin terminal**, **Close terminal**, **Close other terminals**, and **Close all terminals**. A pinned row shows a marker, sorts first in its target group, and is skipped by the two mass closes.
- The user chooses a close that ends a running process. A confirmation names how many running terminals end. The user confirms or cancels.
- The target worktree or workstream is removed. The row and tab show **target gone**. The tab stays open as a read-only view of the scrollback until the user closes it.
- The target has a busy graduation run. The terminal still opens, and its tab shows a warning that an agent is working in the same target.

## Requirements

- `ghostty-web` library must be used for a terminal tab.
- Docker with or without the Microsandbox and Devcontainers must be equally supported as terminal targets.
- If the target worktree/workstream doesn’t exist anymore - terminal must NOT re-initialize the worktree/workstream, but still might be vizualizing the history of it.
- **Targets.** A terminal is bound to one worktree (the active one or any other) or one active workstream, fixed at creation. Host shell: starts in that checkout's folder on the host. Container shell: a new container per terminal, started with the project's agent environment (Vendor images through the configured Docker backend; Dev container through the dev container runtime). It is removed when the terminal ends. Microsandbox shell: attaches only to the micro VM that exists for that workstream's scope. It never boots a VM and never changes the VM's life. It is shown greyed out, with the reason, where no VM runs (no live workstream, isolation not Microsandbox, or a Dev container project). Agent CLI: an interactive session of an installed CLI vendor (Claude Code, Codex, OpenCode), always a fresh session. It never resumes or shares the session state of a graduation run. It runs in the same kind of environment as the Container shell where agent Docker is configured, and on the host otherwise.
- **Row content.** Each row shows a label, what it is connected to (worktree or workstream name, branch, target kind, container or VM), and a state: running, exited, or target gone.
- **Row context menu.** A right-click on a row, or the keyboard context-menu key or Shift+F10 on the focused row, opens it. It holds exactly four entries in this order: the pin action, a separator, **Close terminal**, **Close other terminals**, **Close all terminals**. The pin action is **Pin terminal** on an unpinned row and **Unpin terminal** on a pinned one, never both, and it is never disabled. Each entry has a leading icon (LCM-FR-07) and an accessible name. The menu is a floating overlay under the mutual-exclusion rule of SNV-FR-56. It closes on a choice, on Escape (focus returns to the row), and on any activation outside it. The arrow keys move, Enter chooses.
- **Close actions.** All close actions **end the terminal** (same effect as **Close terminal** in Lifetime): the process stops, the row, its tab and its scrollback go away. **Close terminal** acts on the clicked row only, pinned or not. **Close other terminals** and **Close all terminals** act on every unpinned terminal of the project window, all target groups included (**Close other** skips the clicked row), and never on a pinned one. Where a close would end at least one running terminal, a confirmation dialog states how many running terminals will end. It applies to **Close terminal** on a running row too. A close of exited or target-gone terminals asks nothing. Cancel changes nothing.
- **Pin.** Pinned rows sort first inside their target group, in creation order, then the unpinned rows in creation order. Pinning changes nothing else: it refuses no close and does not guard against project close or application quit. Pin state belongs to the terminal, is held in memory only, and is dropped when the terminal ends.
- **Lifetime.** A terminal process is not tied to its tab. Closing a tab, or the tab closing on a worktree switch (OVW-FR-12), ends nothing. Terminals end on **Close terminal**, project close, or application quit. They do not survive an application restart, and no list is persisted. Terminals of other worktrees stay listed after a worktree switch.
- **History.** Scrollback is kept in memory only. It is never written to disk. It is dropped when the user closes the terminal's tab after the process ended, or closes the terminal.
- **Target detection.** A target is gone when its worktree is missing (`is_missing`, WTC) or its workstream is removed (WKS removal notice). The terminal then ends, is never restarted, and nothing re-creates the worktree or workstream.
- **Busy target.** A terminal is never refused on a target with a busy run (`busy_run_id`). The tab shows a persistent warning. The user is meant to be able to intervene.
- **Security.** A terminal is a raw shell with the user's own rights. No credential masking applies to it. An agent never reads or writes a user terminal. Terminals are not exposed to remote or mobile sessions.
- **Platforms.** macOS and Linux only. On Windows the Terminals surface is not offered.
- **Backend.** A PTY layer in the Rust backend runs the processes. Output goes to the WebView over a binary Tauri channel, with batching and flow control. Input and resize are commands. An interactive launch is a new path beside the headless executor. It does not change the headless path, which allocates no PTY (EAC-FR-13). Allow the WASM that `ghostty-web` needs in the Tauri CSP. Pin its version and check its API (resize, fit, selection, scrollback) first.
- **Tab.** One tab per terminal, identified by the terminal id (single-tab rule of TAB-tabs.md). It uses the Source code font role and the app theme, resizes the PTY with the tab, and keeps its output while hidden. Copy and paste work, and a key that the shell needs is not taken by the global menu. When a terminal's tab becomes active, the panel selects its row (SNV-FR-64). The pin state is the terminal's, not the tab's: the tab's own **Pin tab** and **Unpin tab** (TAB-FR-33) change the terminal's pin state, the tab shows the pinned marker of TAB-FR-35, and the row shows its marker. Closing the tab keeps the pin state (this differs from TAB-FR-35, because the terminal outlives its tab). The tab's pin guards it from the tab mass closes of TAB-FR-37. The tab's **Close tab** and mass closes close tabs only and end no process.
- **Limits.** A maximum number of terminals per project window and a scrollback cap apply. The agent picks values and states them in the spec.
- **Scope.** Terminals belong to one project window.

## Expected deliverables

- Terminal panel and tab.
- New specifications: one UI spec for the panel and tab, one core spec for the PTY and terminal session lifecycle.
- Changes to existing specifications: `ui/SNV-shell-navigation.md` (activity-bar toggle), `ui/TAB-tabs.md` (new tab type, identity, single-tab rule, pin state kept by the terminal after the tab closes), `ui/OVW-overview.md` (worktree-switch tab closing leaves terminal processes running), `core/WTC-worktree-context.md` and `core/WKS-work-streams.md` (target gone), `tools/EAC-execute-agent-cli.md` (interactive launch path, Docker and Microsandbox attach), `core/DVC-devcontainer-environment.md` (shell in the dev container).
- Frontend and backend tests for the lifetime, gone-target, busy-target and Microsandbox-unavailable rules, and for the row context menu: pin and unpin, sort order, shared pin state with the tab, mass closes that skip pinned rows, and the confirmation for running terminals.
- Documentation update: user-facing docs for the Terminals panel, the targets and their limits.