# Command execution

**Spec code:** `CEX`

## Intent
The rule for the thread on which each Tauri command of the application runs. The main thread of the application drives the windows, the WebView, and the native menus, so one command that does slow work on it freezes the whole interface: typing, scrolling, painting, and every other window. An external process that writes many files into an open repository, such as a submodule clone, makes the change set slow to read, and that read must never stop the interface. This spec sets that no command work runs on the main thread, names the few ordered commands that stay there, and sets the static check that keeps the rule. Out of scope: how fast each command is, which each module's own spec owns.

## Functional requirements
1. **CEX-FR-JBTB** Every Tauri command of the application runs off the main thread, except the commands of CEX-FR-IXRE.
   - *Why:* the main thread drives the windows and the WebView, so any slow work on it freezes every window of the application.
2. **CEX-FR-IXRE** These commands run on the main thread: `write_terminal_input`, `resize_terminal`, `acknowledge_terminal_output`, `set_save_menu_state`, `set_find_menu_state`, and `set_zoom_menu_state`.
   - *Why:* the frontend sends them without waiting for each answer, and only the main thread keeps the order in which they arrive.
3. **CEX-FR-VWRP** A command of CEX-FR-IXRE does constant-time work only. It reads no disk, no network, and no process output. It waits on no pseudo-terminal, container, VM, or other peer, and on no lock that slow work holds.
4. **CEX-FR-KFVT** A synchronous command body that runs on a worker of the async runtime tells the runtime that it blocks, so the runtime moves the other tasks of that worker to other workers before the body runs.
5. **CEX-FR-VKTJ** A synchronous command body runs whole on one thread, so the project context of `PWN-project-windows.md` PWN-FR-CTXT stays visible to every call in the body.
6. **CEX-FR-VYEQ** Called directly with no async runtime, or inside a single-threaded runtime, a synchronous command body runs on the calling thread and gives the same result as on a worker.
7. **CEX-FR-RWTW** A command that runs off the main thread and waits for the main thread, for example to read a window size, a window position, or the cursor position, holds no lock that a main-thread handler takes while it waits.
8. **CEX-FR-FUUP** Two calls of one command that run at the same time leave the state that the later of the two state changes made. An event that publishes that state never replaces a newer published state with an older one.
9. **CEX-FR-QZPN** The frontend sends the calls of one command whose name starts with `save_` one at a time, each after the call before it settled. `activate_project_window` and `set_project_window_attention` share one such order. A failed call does not stop the calls after it.
   - *Why:* callers send these calls without waiting, and calls that run on separate workers can finish in either order, so the earlier write could land last (CEX-FR-FUUP).
10. **CEX-FR-DBBS** The Rust test suite holds a static check of the source. It fails when a command that is not in CEX-FR-IXRE can run on the main thread, and it names each such command.

## Contract surface
This spec adds no command. It sets how every command that `src-tauri/src/command_names.rs` lists runs.

### Internal (Rust API, not registered as Tauri commands)
- The attribute `blocking_command`, which a synchronous command carries in place of `#[tauri::command]`. It registers the command to run off the main thread (CEX-FR-JBTB) and runs its body under `run_blocking` (CEX-FR-KFVT). The function stays synchronous, so tests call it directly.
- `run_blocking(f)`: runs `f` and tells a multi-threaded async runtime that it blocks (CEX-FR-KFVT). With no runtime, or in a single-threaded runtime, it runs `f` on the calling thread (CEX-FR-VYEQ).
- An `async fn` command already runs off the main thread and carries `#[tauri::command]` unchanged.

## Non-functional requirements
- The module rules that a command runs off the main thread, such as `ASC-artifact-scanning.md` ASC-FR-TPJO and `CVH-change-version-history.md` CVH-FR-GHMO, are cases of CEX-FR-JBTB.
- A command of CEX-FR-IXRE returns in well under one frame (16 ms) on any machine the application supports.
