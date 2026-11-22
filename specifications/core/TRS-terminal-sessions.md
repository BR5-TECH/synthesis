# Terminal sessions

**Spec code:** `TRS`

## Intent
The backend module that runs the author's own interactive terminals. A terminal session is a process on a pseudo-terminal, bound to one worktree or one active work stream of the open project: a shell on the host, a shell in a new container of the project's agent environment, a shell in the micro VM that a graduation queue already runs, or a fresh interactive session of an agentic CLI. The author uses it to look at and change the code in a checkout while agents work there, or after they stop. A session outlives the tab that shows it and keeps its output history in memory. Where tmux is usable on the host, a Host shell or an Agent CLI on the host is **persistent**: it runs in a tmux server that belongs to the application, and it keeps running when the project closes or the application exits or restarts. When the project opens again, the author finds it with its recent output, because a long shell or agent session must not die with the IDE. Every other session ends when the author closes it, when its target is removed, or when the project closes. Out of scope: the panel and the tab that show sessions (`../ui/TRM-terminals.md`); any agent turn, which is `../tools/EAC-execute-agent-cli.md`'s headless path; Windows, where no terminal is offered; remote and mobile sessions, which reach no terminal; persistence of a Container shell, a Microsandbox shell, and an Agent CLI in a container or a dev container.

## Functional requirements

### Sessions and limits
1. **TRS-FR-YYER** A terminal session is one process that this module runs on a pseudo-terminal for one project. It is bound at creation to one target, a worktree or an active work stream of that project, to one kind, and, for an Agent CLI, to one vendor. None of these changes during the session's life.
2. **TRS-FR-AYCY** The module holds at most 16 sessions for each project, in every state, restored sessions included. `create_terminal` past that limit returns `terminal_limit_reached` and starts nothing.
3. **TRS-FR-IZWM** Each session keeps an **output history** of at most 4 MiB in memory: every byte the process wrote, in order, after the restored scrollback of TRS-FR-TCBS where the session is restored. When new output passes the cap, the module drops the oldest bytes first, up to the start of a line where the dropped range holds a line break.
4. **TRS-FR-BWVT** The module never writes a session's history, its input, its environment, or its metadata to disk, to a log record, or to any store. The scrollback and the metadata of a persistent session are in the memory of the tmux server alone.
5. **TRS-FR-TEHT** Only a persistent session survives the application process. Nothing about any other session is kept. After an application start, `list_terminals` returns only the sessions that TRS-FR-IEKU restored.
6. **TRS-FR-LRYH** Each session gets a **label** at creation: the kind name (`Host shell`, `Container shell`, `Microsandbox shell`) or the vendor's display name for an Agent CLI, then a space and the lowest positive number that no other session of the project with the same name holds.
7. **TRS-FR-VTYD** Every command of this module takes `project_id` and acts on the sessions of that project alone. `"terminals changed"` carries `project_id` and goes to the window of that project only (per `PWN-project-windows.md` PWN-FR-ISOL).

### Targets
8. **TRS-FR-ZCPH** `list_terminal_targets` returns one target for each worktree of `WTC-worktree-context.md` WTC-FR-04 that is not missing and is no stream's working copy, the active one first, and one target for each work stream of `WKS-work-streams.md` that is not missing. Each target names its branch, its run state, and the availability of each kind.
9. **TRS-FR-BGDL** A worktree target is identified by its path, and a work stream target by its stream id. The **queue scope** of a target is its stream id for a work stream and `worktree:<path>` for a worktree, the same key as the sandbox scope of `../tools/EAC-execute-agent-cli.md`.
10. **TRS-FR-QHKU** A Host shell is available on every target.
11. **TRS-FR-IRBL** A Container shell is available in a `vendor_images` project where the Docker backend is verified (per `GSS-global-settings-storage.md` GSS-FR-40) and the project configures an image for the vendor that `resolve_agentic_invocation` resolves for a `work` turn. In a `devcontainer` project, it is available where the dev container CLI is verified and the config is a file in the project root checkout.
12. **TRS-FR-KJYH** A Microsandbox shell is available only where the agent environment is `vendor_images`, the effective isolation for a launch of the target's queue scope is `microsandbox` (per `../tools/EAC-execute-agent-cli.md` EAC-FR-SNMS), and a VM of that scope runs now.
13. **TRS-FR-RCGL** An Agent CLI is offered for each CLI-kind vendor, `claude_code`, `codex` and `opencode`, whose integration is verified (per `AIC-agentic-integrations.md`). It runs in a dev container in a `devcontainer` project, in a Docker container where the Docker backend is verified and the project configures an image for that vendor, and on the host otherwise.
14. **TRS-FR-XXPE** Each kind that is not available carries one reason: `docker_unverified`, `vendor_image_unconfigured`, `devcontainer_not_ready`, `devcontainer_project`, `isolation_not_microsandbox`, `no_vm_running`, or `vendor_unverified`. `create_terminal` for a kind that is not available returns `kind_unavailable` with that reason and starts nothing.

### Launch
15. **TRS-FR-QPBL** A Host shell runs the user's login shell as an interactive login shell, with the target's directory as its working directory. The login shell is `SHELL` where it names an executable, else `/bin/zsh` on macOS and `/bin/bash` on Linux where they exist, else `/bin/sh`.
16. **TRS-FR-KMWO** A host process gets the environment of the application process with `TERM=xterm-256color` and `COLORTERM=truecolor` set. The process of a persistent session gets the environment of the tmux server, with those two given by `new-session -e`. It gets no credential of `AIC-agentic-integrations.md` and no variable that this module adds beyond those two.
17. **TRS-FR-ASKO** An Agent CLI on the host runs the verified executable of the vendor's integration with no argument, in the target's directory. The CLI uses the user's own login on this machine.
18. **TRS-FR-HXRL** A Container shell, a Microsandbox shell, and an Agent CLI that runs in a container go through the interactive launch of `../tools/EAC-execute-agent-cli.md` EAC-FR-CKGX, with the target's directory as the execution directory. This module creates no container and runs no process in a VM by any other route.
19. **TRS-FR-QDGW** Every Agent CLI session starts a fresh CLI session. It passes no resume argument and no session id. A Claude Code session reads no session-state directory of a graduation run (per `../tools/EAC-execute-agent-cli.md` EAC-FR-31). A Codex session uses its login directory, which also holds its sessions (per `../tools/EAC-execute-agent-cli.md` EAC-FR-DQZP).
20. **TRS-FR-XXCY** A session that starts a container, a dev container, or a VM process is in the state `starting` until its process runs. The output that the start writes, such as each line of `devcontainer up`, goes into the history.
21. **TRS-FR-VGGS** A start that fails ends the session in the state `exited` with the end reason `start_failed` and one error line of at most 300 characters. The module writes that line into the history too.

### Input, output and flow control
22. **TRS-FR-HIIK** `attach_terminal` sends the whole history on the given channel first and then the live output, with no byte lost or sent twice between the two. A session has at most one attached channel. A new attach ends the earlier channel.
23. **TRS-FR-PACY** The module sends output as binary frames of at most 64 KiB. A frame goes out at most 16 ms after its first byte was read.
24. **TRS-FR-VYIB** While more than 1 MiB that the module sent on the attached channel is not acknowledged, the module stops reading the session's output. It reads again when the unacknowledged amount is 256 KiB or less. `acknowledge_terminal_output` reports the bytes the tab has consumed.
25. **TRS-FR-KENG** While no channel is attached, the module keeps reading the session's output into the history, so a process never stops on a hidden or closed tab.
26. **TRS-FR-AVNH** `write_terminal_input` writes its bytes to the session's pseudo-terminal unchanged. One call carries at most 64 KiB. A session that is not `running` returns `terminal_not_running` and receives nothing.
27. **TRS-FR-UNIL** `resize_terminal` sets the window size of the pseudo-terminal, and of the terminal in the container or VM for those kinds. It accepts 2 to 1000 columns and 1 to 500 rows, and returns `invalid_size` for any other value.
28. **TRS-FR-NULR** `detach_terminal` ends the attached channel and changes nothing about the process. A detach of a session that is not `running` or `starting` also drops its history.

### Lifetime
29. **TRS-FR-DVBF** When the process of a session ends on its own, the session goes to `exited` with the exit code and the end reason `process_exited`. The session and its history stay until it is closed.
30. **TRS-FR-CXSV** `close_terminal` ends the session in any state. It removes the tmux session of a persistent session, sends `SIGHUP` to the process group, sends `SIGKILL` after 3 seconds where a process still runs, removes the container or dev container of the session, drops the history, and removes the session.
31. **TRS-FR-OTLH** A project close ends every session of that project, and an application exit ends every session, on the terms of TRS-FR-CXSV. For a persistent session, both end only the tmux client: the tmux session, its process, and the tmux server keep running. Neither asks for a confirmation from this module.
32. **TRS-FR-KOYL** `set_terminal_pinned` sets or clears the session's **pinned** flag and does nothing else. The flag is in memory and goes with the session. A persistent session also keeps it in its tmux session (TRS-FR-QMZM). A pinned session ends on a close, a project close, and an application exit as any other does.
33. **TRS-FR-EHXS** The module emits one `"terminals changed"` after each create, each change of state, each pin change, each change of a target's run state, and each close.
34. **TRS-FR-HWNA** A Microsandbox shell ends with the end reason `vm_stopped` when its VM stops or reboots (per `../tools/EAC-execute-agent-cli.md` EAC-FR-INWE). The module never boots, reboots, or keeps a VM for a session.

### Target gone
35. **TRS-FR-RKAW** A target is **gone** when `WTC-worktree-context.md` removes its worktree or reports it with `is_missing = true`, when `WKS-work-streams.md` deletes or removes its stream, or when its directory is not found at a check. The module checks the directory of each target with a `starting` or `running` session every 5 seconds.
36. **TRS-FR-NPDK** When a target is gone, each of its sessions in `starting` or `running` ends as TRS-FR-CXSV ends it, tmux session included, except that the session and its history stay. The session goes to the state `target_gone`. A session that had ended already also goes to `target_gone`.
37. **TRS-FR-EJWM** The module never creates, restores, or registers a worktree, a branch, a work stream, or a directory, and never restarts a session. `create_terminal` on a gone target returns `target_gone`.

### Busy target
38. **TRS-FR-LUUX** A target is **busy** while a run holds its queue (per `GRD-graduation.md` GRD-FR-ZVNO). Each session and each target carries the id of that run as `busy_run_id`, or null. A busy target refuses no terminal.

### Persistent sessions
39. **TRS-FR-OMYR** tmux is **usable** when `tmux -V` reports version 3.2 or later. The module finds `tmux` on the `PATH` of the user's login shell, else in `/opt/homebrew/bin`, `/usr/local/bin`, and `/usr/bin`. It checks at each create. No setting turns persistence on or off.
    - *Why:* an application that Finder starts on macOS has a short `PATH` that does not hold Homebrew.
40. **TRS-FR-FKVW** A Host shell and an Agent CLI with `runs_in = host` are **persistent** where tmux is usable at their create. Every other kind, and a host session created while tmux is not usable, runs on a pseudo-terminal of its own as TRS-FR-QPBL and TRS-FR-ASKO say, and is not persistent.
41. **TRS-FR-ZOZO** Every persistent session runs in one tmux server of the application, on the socket name `synthesis` (`tmux -L synthesis`). The module never reads, changes, or ends another tmux server or a session of one.
42. **TRS-FR-ANJK** The server shows a session as a plain terminal: no status bar, no prefix key and no key binding, so no key detaches the client; mouse off, so the terminal handles the mouse; no alternate screen in the client; and a history limit of 10,000 lines (per `../ui/TRM-terminals.md` TRM-FR-DTCA).
43. **TRS-FR-BHCB** The module creates a persistent session with `new-session -d`, with the target's directory as its start directory and the environment of TRS-FR-KMWO. Its name is `syn-`, the first 12 hex digits of the SHA-256 of the project key (`DVC-devcontainer-environment.md` DVC-FR-QGUT), `-`, and the session id. A `tmux attach-session` client is the process on the session's pseudo-terminal.
44. **TRS-FR-FIIJ** When a tmux call of a create fails, the module emits a `WARN` record, removes the tmux session where one exists, and starts the session as a session that is not persistent.
45. **TRS-FR-QMZM** The tmux session holds the metadata of the session as session options: `@synthesis_project` (the project key), `@synthesis_label`, `@synthesis_target`, `@synthesis_kind`, `@synthesis_vendor`, `@synthesis_runs_in`, `@synthesis_pinned`, and `@synthesis_created_at`. A pin change sets `@synthesis_pinned`.
46. **TRS-FR-CKIC** When the tmux client ends while its tmux session and process live, and the module did not end the client, the module attaches a new client. This is not an end of the session. After 3 clients that end in 10 seconds, the session goes to `exited` with no exit code, a `WARN` record, and its tmux session stays.
47. **TRS-FR-KBOS** The tmux server keeps the pane of a process that ended. When the process of a persistent session ends, the module reads its exit status, removes the tmux session, and the session goes to `exited` as TRS-FR-DVBF says. A tmux session that ends with no exit status gives `exited` with no exit code.
48. **TRS-FR-IEKU** When a project window opens a project, the module reads from the socket each tmux session whose `@synthesis_project` is that project's key and that it does not hold yet. It adds each one with its id and metadata, `persistent = true`, in the state `running`, and then emits one `"terminals changed"`.
49. **TRS-FR-HJZR** A restored tmux session whose process ended is `exited`, with its exit status and the end reason `process_exited`. Its history holds every line of the pane. The module then removes the tmux session.
50. **TRS-FR-QCOP** A restored session whose target is gone (per TRS-FR-RKAW) goes to `target_gone` on the terms of TRS-FR-NPDK, and its tmux session is removed.
51. **TRS-FR-TFAN** The module restores tmux sessions in the order of their creation time while the project holds fewer than 16 sessions. A tmux session past the limit stays alive and is not listed.
52. **TRS-FR-TCBS** A restored live session's history starts with the pane's lines above the visible screen (`capture-pane -p -e -J`), each line break as CR LF, cut to 4 MiB, then one line feed for each client row. The client's live output and screen redraw follow, so the visible screen is in the history once.

### Security and reach
53. **TRS-FR-CLDC** The module applies no credential masking to the input or the output of a session. No log record of this module carries input, output, a path, or an environment value. Records name the session id, the kind, the vendor, where it runs, and the state.
54. **TRS-FR-FDXW** No agent loop, tool, or remote session reaches a session. The commands of this module are Tauri commands of the project window alone, no loop or tool calls an API of this module, and the remote session relay forwards none of them (per `../server/SRB-server-relay-boundary.md`).
55. **TRS-FR-YZMR** On Windows, every command of this module returns `platform_unsupported` and starts nothing.
56. **TRS-FR-LXIE** The module emits an `INFO` record under the `backend` domain when a session starts, ends, or goes to `target_gone`, and when it is closed. It emits an `INFO` record for each restored session. A failed start, a failed write, a failed container removal, a failed restore, and a failed tmux call each emit a `WARN` record that names the cause.

## Contract surface

### Tauri commands
Names match `../ui/TRM-terminals.md` byte-for-byte:

- `"list terminal targets"` → `list_terminal_targets(project_id)` → `[TerminalTarget]`. No side effect.
- `"list terminals"` → `list_terminals(project_id)` → `[TerminalSummary]`. No side effect.
- `"create terminal (target, kind, vendor)"` → `create_terminal(project_id, target, kind, vendor?)` → `TerminalSummary`. Side effects: a process, and for some kinds a container, a dev container, or a tmux session; one `"terminals changed"`.
- `"attach terminal (id, output)"` → `attach_terminal(project_id, terminal_id, output: Channel<bytes>)` → `TerminalAttachment`. Side effect: an earlier channel of the session ends.
- `"detach terminal (id)"` → `detach_terminal(project_id, terminal_id)` → `()`. Side effect: the channel ends, and the history goes where TRS-FR-NULR says.
- `"write terminal input (id, data)"` → `write_terminal_input(project_id, terminal_id, data: bytes)` → `()`.
- `"resize terminal (id, cols, rows)"` → `resize_terminal(project_id, terminal_id, cols, rows)` → `()`.
- `"acknowledge terminal output (id, bytes)"` → `acknowledge_terminal_output(project_id, terminal_id, bytes)` → `()`.
- `"set terminal pinned (id, pinned)"` → `set_terminal_pinned(project_id, terminal_id, pinned)` → `TerminalSummary`. Side effect: one `"terminals changed"`.
- `"close terminal (id)"` → `close_terminal(project_id, terminal_id)` → `()`. Side effects: the process ends, the container or the tmux session goes, the history and the session are removed; one `"terminals changed"`.

### Events (Tauri event bus)
- `"terminals changed"` — payload `{ project_id }` (TRS-FR-EHXS).

### Payload shapes
```
TargetRef =
    { kind: "worktree", path }
  | { kind: "work_stream", stream_id }

TerminalTarget {
  target:              TargetRef,
  name,                // the worktree directory's basename, or the stream name
  branch?,
  is_active_worktree,
  busy_run_id,         // the run that holds the target's queue, or null
  kinds:               [KindAvailability]
}

KindAvailability {
  kind:      "host_shell" | "container_shell" | "microsandbox_shell" | "agent_cli",
  vendor?,   // agent_cli only: "claude_code" | "codex" | "opencode"
  runs_in:   "host" | "docker" | "devcontainer" | "microsandbox",
  available: bool,
  reason?    // TRS-FR-XXPE, when not available
}

TerminalSummary {
  id,                  // unique across application processes
  label,               // TRS-FR-LRYH
  target:              TargetRef,
  target_name,
  branch?,
  kind,
  vendor?,
  runs_in,
  environment_name?,   // the container name, the dev container id, or the VM name
  state:               "starting" | "running" | "exited" | "target_gone",
  exit_code?,
  end_reason?:         "process_exited" | "start_failed" | "vm_stopped" | "target_gone",
  end_detail?,         // the error line of a failed start
  pinned,
  busy_run_id,
  has_history,
  persistent,          // TRS-FR-FKVW
  created_at           // RFC 3339 UTC
}

TerminalAttachment {
  state,
  history_bytes        // the size of the history sent before the live output
}
```

Each message on the `output` channel is one binary frame of raw output bytes (TRS-FR-PACY).

Typed errors: `platform_unsupported`, `terminal_limit_reached`, `unknown_target`, `unknown_terminal`, `target_gone`, `kind_unavailable` (with its reason), `terminal_not_running`, `invalid_size`, `input_too_large`, `start_failed`.

### Internal (Rust API, not registered as Tauri commands)
- `end_terminals_for_target(project_id, target)` — called by `WTC-worktree-context.md` and `WKS-work-streams.md` when a target is gone (TRS-FR-RKAW).
- `end_terminals_for_project(project_id)` and `end_all_terminals()` — project close and application exit (TRS-FR-OTLH).
- `restore_terminals_for_project(project_id)` — called when a project window opens a project (TRS-FR-IEKU).

## Non-functional requirements
- The pseudo-terminal layer runs on macOS and Linux. The Windows build compiles the module with every command returning `platform_unsupported`.
- The worst-case memory of output history is 16 sessions of 4 MiB for each project window. The tmux server holds at most 10,000 lines of scrollback for each persistent session, outside the application process.
- Persistence needs tmux 3.2 or later on the host. The application installs no tmux and starts no tmux server other than its own.
- A keystroke reaches the process with no batching delay on the input path. Output reaches the tab at most 16 ms after the process writes it, while the channel is not held back by flow control.
- A session never blocks the async runtime: the module reads and writes each pseudo-terminal on a task of its own.
