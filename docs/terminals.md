# Terminals

A terminal is an interactive shell, or an interactive agent CLI session, in one checkout of the open project. Use a terminal to look at the code in a worktree or a work stream, to run a command there, or to correct something while an agent works.

Terminals are available on macOS and Linux. On Windows, the Terminals panel is not shown.

## Open the Terminals panel

Select **Terminals** in the activity bar. It is the last button of the top group.

The panel lists every terminal of this project window. It groups the terminals by target. The group of the active worktree is first. Each row shows:

- The label, for example `Host shell 1` or `Claude Code 2`.
- The kind, and where the terminal runs: the host, a container, a dev container, or a micro VM.
- The state: starting, running, exited (with the exit code), or target gone.

A pinned terminal shows a pin mark and is first in its group. A warning mark shows that a graduation run works in the same target.

## Open a terminal

1. Select **New terminal** in the panel header.
2. Select the target: the active worktree, another worktree, or a work stream.
3. Select the kind. A kind that is not available is grey and shows the reason.
4. Select **Create**.

The new terminal opens in a tab.

### Kinds

| Kind | Where it runs |
| --- | --- |
| Host shell | Your login shell on this computer, in the folder of the target. |
| Container shell | A new container of the project's agent environment. In a project that uses vendor images, the container uses the image of the agent that the project uses for graduation work. In a project that uses a dev container, it is a new dev container. The container is removed when the terminal ends. |
| Microsandbox shell | A shell in the micro VM that a graduation run already uses for this target. The terminal never starts a VM. It is available only when the isolation is Microsandbox and a VM runs for the target. When the VM stops, the terminal ends. |
| Agent CLI | A new interactive session of Claude Code, Codex, or OpenCode. It never continues the session of a graduation run. It runs in the same kind of container as a Container shell when the project configures an image for that agent, or in a dev container. Otherwise it runs on this computer. |

In a container, an Agent CLI uses the same login as a graduation turn. On this computer, it uses your own login of that CLI.

## Use a terminal

- Click a row to open its tab, or to go to the tab if it is open.
- Copy with Cmd+C on macOS or Ctrl+Shift+C on Linux. Paste with Cmd+V or Ctrl+Shift+V.
- The tab keeps 10,000 lines of scrollback. Use Shift+PageUp and Shift+PageDown to scroll.

When you close a tab, the terminal continues to run. Click the row to see it again. A worktree switch closes the tabs, but the terminals of every worktree stay in the list and continue to run.

## Pin and close terminals

Right-click a row, or press Shift+F10 on a selected row, to open its menu:

- **Pin terminal** or **Unpin terminal**. The tab's **Pin tab** does the same thing.
- **Close terminal** ends this terminal.
- **Close other terminals** ends every unpinned terminal except this one, in every group.
- **Close all terminals** ends every unpinned terminal.

When a close ends a running terminal, Synthesis asks you to confirm and tells you how many running terminals end.

A terminal ends when you close it, when you close the project, or when you quit Synthesis. Terminals do not stay after a restart.

## When the target is removed

When the worktree or the work stream of a terminal is removed, the terminal ends. Its row shows **target gone**. Its tab stays open and shows the output until you close it. Synthesis never makes the worktree or the work stream again.

## Limits

- 16 terminals in each project window, in every state.
- 4 MiB of output for each terminal, kept in memory. Synthesis never writes terminal output to disk.
- 10,000 lines of scrollback in a tab.

## Security

A terminal is a shell with your own user rights. Synthesis does not mask credentials in a terminal. Agents cannot read or write your terminals, and remote and mobile sessions cannot reach them.
