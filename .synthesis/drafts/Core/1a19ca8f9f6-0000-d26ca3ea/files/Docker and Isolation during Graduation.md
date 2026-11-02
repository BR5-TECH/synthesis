## Intent

Isolation via microsandbox VM must be aligned with the use of Docker. User who needs Docker-in-Docker should have options to achieve that, and the options must be aligned with isolation.

## User journey

- User opens Project settings, Docker section, with Microsandbox on (Global settings, Isolation). User selects the **Docker-in-Docker** checkbox.
- Under the checkbox, one **selector** shows two choices: **Docker daemon in the VM** (default) and **No isolation: use the host Docker socket**. The section states that the first choice keeps the microVM isolation but needs a `dockerd` in the project's own image, and that the second choice gives up isolation and passes the host Docker socket into the graduation container.
- WHEN the user selects **Docker daemon in the VM**: the graduation turns run in the microVM. The executor starts `dockerd` in the guest at VM boot. Tests in the VM (for example Testcontainers) start containers on that daemon, not on the host.
- WHEN the user selects **No isolation**: the graduation turns of this project run as a Docker launch with the host socket mounted, as with isolation None. The socket path and Host gateway controls apply to this choice.
- IF the image has no `dockerd`, or `dockerd` does not become ready: the run rests `interrupted` with `launch_failed`, with a detail that names the cause. Continue is offered. The run view and the toast offer no control to select **No isolation**. The detail tells the user to add `dockerd` to the project's image, or to select **No isolation** in Project settings, and then to use Continue.
- WHEN the user adds `dockerd` to the project's image and uses Continue: the new launch boots a new VM from the updated image and finds `dockerd` in it. WHEN the user selects **No isolation** in Project settings and uses Continue: the new launch is a Docker launch with the host socket.
- WHEN the checkbox is off, Microsandbox applies as before and no Docker access exists in the VM.

## Requirements

- **Choice.** The project stores one new value in its Docker-in-Docker settings: `under_microsandbox`, one of `vm_daemon` and `host_socket`. Default `vm_daemon`. It is project-local, per worktree, never committed, and it is saved at once with the other values as one record, as the existing three values are. The selector shows only while Microsandbox is on and the checkbox is selected. It has no effect while isolation is None.
- **Effective isolation.** For a graduation launch, the effective isolation is None where isolation is Microsandbox, Docker-in-Docker is enabled, and `under_microsandbox` is `host_socket`. Otherwise it is the global mode. Where the effective isolation is None, the launch is a Docker launch with the additions of EAC-FR-DKRS, and the socket check of EAC-FR-HWQN and of the start preflight run. The host socket is mounted only where Docker-in-Docker is enabled. The masking rule of EAC-FR-41 stays: a masked turn gets no socket.
- **Daemon in the VM.** Where isolation is Microsandbox, Docker-in-Docker is enabled, and `under_microsandbox` is `vm_daemon`: the VM boots from the project's own image, which must hold `dockerd`. No shipped vendor image is assumed to hold it. The executor starts `dockerd` as root in the guest at VM boot, and waits for it to be ready for a fixed, bounded time. The turn process still runs as the host UID and GID (EAC-FR-KTWO) and must be able to use the guest Docker socket without root; the executor sets this up and sets `DOCKER_HOST` for the process. No host Docker socket, no host root, and no supplementary mount is added (EAC-FR-BJLI stays true).
- **Failure.** A missing `dockerd`, a `dockerd` that exits, and a `dockerd` that is not ready in time fail at VM launch as `SandboxLaunchFailed` with a new cause (for example `docker_daemon_unavailable`) in EAC-FR-NCSA. The run rests `interrupted` with `launch_failed` (GXD-FR-QUCP), with Continue, and the NTF-FR-BDNY toast applies. The executor never falls back to Docker or to None. The start preflight does not inspect the image, boot a VM, or contact a daemon (GSU non-functional requirements).
- **Continue after a daemon failure.** The executor stops a VM whose `dockerd` failed and does not keep it for reuse. Continue launches a new turn that reads the project's current settings and resolves the image again (EAC-FR-QULL), so an image rebuilt after the failure is the image that boots; a changed image ID is never served from an old export (EAC-FR-IGBY). Where the image comes from a registry, the builder verifies that the sandbox runtime does not serve an old cached copy of a changed image, and reports where it does.
- **Where No isolation is selected.** Only Project settings change `under_microsandbox`. No surface of a graduation run, and no notification, has a control that changes it, and no command of a run changes it. The run view only shows the detail text and Continue (GRU-graduation-runs.md, NTF-FR-BDNY).
- **VM reuse.** The daemon option is part of what a VM booted with. A launch reuses a VM only where the option equals the one it booted with. Otherwise the executor reboots the VM (EAC-FR-UEFI).
- **Resources.** The `dockerd` image store lives on the sandbox disk. The Isolation section states that images pulled by tests count against the Disk slider.
- **UI text.** The text of SET-FR-PMHB and GLS-FR-WAGX ("the setting is ignored") is replaced. SET-FR-JMHE: the socket path field and the Host gateway toggle show while the checkbox is selected and the effective isolation is None, and not under `vm_daemon`. The selector text states the isolation trade-off of the two choices.
- **Platform.** Existing limits stay: Windows cannot enable Docker-in-Docker, macOS is best effort for the host socket. Verify that the guest kernel of the sandbox runtime supports `dockerd` (overlay file system, cgroups, netfilter) before the builder relies on it. Report where it does not.
- **Tests.** Cover: the effective isolation for each combination of mode, checkbox, and choice; a `host_socket` launch mounts the socket only with the checkbox on; a `vm_daemon` boot starts `dockerd` and fails with the new cause when it is absent; VM reboot on a change of the option; the preflight reads metadata of the socket path only where the effective isolation is None.

## Specifications to change

- `specifications/ui/SET-project-settings.md`: SET-FR-PMHB and SET-FR-JMHE; add the selector and its text.
- `specifications/ui/GLS-global-settings.md`: GLS-FR-WAGX; the Disk text of GLS-FR-WMKD.
- `specifications/core/PSS-project-settings-storage.md`: PSS-FR-DIND, PSS-FR-QWTN, PSS-FR-VRHC add `under_microsandbox` (default `vm_daemon`; an unknown value is refused).
- `specifications/tools/EAC-execute-agent-cli.md`: EAC-FR-SNMS (effective isolation), EAC-FR-DKRS, EAC-FR-HWQN, EAC-FR-DARS (replace "adds nothing"), EAC-FR-UEFI (reuse), EAC-FR-NCSA (new cause), EAC-FR-BJLI; add the in-VM `dockerd` requirement.
- `specifications/core/GSU-graduation-start.md`: GSU-FR-RQDS, the Intent, and the non-functional requirement about the preflight use "effective isolation is not Microsandbox".
- `specifications/core/GXD-graduation-execution.md`: GXD-FR-QUCP names the daemon cause.
- `specifications/ui/NTF-notifications.md`: NTF-FR-BDNY covers the daemon cause and offers no control to change the choice.
- `specifications/ui/GRU-graduation-runs.md`: the action row of an interrupted run with `launch_failed` stays Continue alone; it adds no control for the choice.
- `specifications/ui/GSD-graduation-start-dialog.md`: GSD-FR-BXKR applies where the effective isolation is None.
- Draft `Isolation` states "no Docker-in-Docker additions under Microsandbox"; this draft replaces that rule. 