## Intent

Agents running in the Docker Container should have a proper isolation to contain potential rogue agents from causing harm to the User’s host machine that’s running the IDE. 

## User journey

### Generic Journey

- User opens a settings window and navigates to the new Isolation section in the Global group. The section sits directly after Docker and before Remote connectivity.
- User can select between None and Microsandbox.
- Once user makes a choice, it’s persisted and respected on every graduation run: stream, direct and merge. Other agentic CLI invocations are not affected.
- Every change applies at once, with no dirty state, as in the Docker section.

### None option Journey

- Everything works as is today, Docker is invoked normally.

### Microsandbox Journey

- User can configure the number of CPU cores using a slider with min = 1 and max = number of logical cpus User has on his device. Default is min(4, device maximum).
- User can configure the amount of RAM available for an agent, with min = 1GB and max = total amount of RAM User has on his device. Default is min(8GB, device maximum).
- Outbound internet access is always allowed, because the agent needs it to work.
- Local network control: a switch "Block local network access" (loopback, private ranges, link-local). Default is on. Below the switch, User can edit a list of allowed local hosts or CIDRs that the agent can still reach. The list is validated inline. The section warns that a Custom Gateway on localhost or a local network address is blocked until User adds it to this list. The IDE does not add the gateway host automatically. 
- Once Graduation starts, the microsandbox is used to invoke the Docker image that was configured to agent CLI of Choice (e.g. Claude Code). This applies to stream, direct and merge runs.
- VM lifecycle: each stream has one shared microsandbox micro VM. The stream runs every agent invocation through that VM and reuses it. Different streams use different VMs. The IDE stops the VM when the stream ends. The VM uses the CPU and RAM values from the settings.
- Image sourcing: the IDE first asks microsandbox to pull the image from a registry. If that fails, the IDE exports the local image from the verified Docker backend (docker save) and boots the sandbox from the export. Local-only images work with no registry.
- Export cache: the IDE caches each exported image, keyed by the image ID (the content hash of the image). It reuses the cached export on later launches. A changed image has a new ID, so the IDE never reuses an outdated export. The IDE deletes stale exports.
- Docker-in-Docker: the setting is ignored under Microsandbox. The IDE mounts no host Docker socket, and the start does not check the socket. The Isolation section and the Project Docker section both say so.

### Unsupported machine Journey

- Microsandbox needs hardware virtualisation (KVM on Linux, Apple Silicon on macOS) and does not run on Windows.
- On an unsupported machine, the Microsandbox choice is disabled, the section states the reason, and None stays selected.

### Launch failure Journey

- If the sandbox cannot start (for example no virtualisation, image export failure, not enough memory), the run rests `interrupted` with the reason `launch_failed` and a detail that names the cause. Continue is offered.
- The IDE never falls back to None silently.
- The IDE also raises an Error toast that opens the run.

## Requirements

- Synthesis IDE once built must embed microsandbox using a corresponding feature.
- The microsandbox crate compiles only on Linux and macOS (target gate). The Windows build has no microsandbox code. On Windows the Isolation section shows the unsupported state.
- Look and feel of the new settings tab must match the IDE.
- Local Docker images must be supported as they are today, IDE must not require the Docker image to be published to any external registry.
- The CPU slider and the RAM control are bounded by the device maximums. The defaults never exceed those maximums.
- The isolation settings are stored in the user-global store, and the IDE loads and saves them through new backend operations.

## Specifications to change

- `specifications/ui/GLS-global-settings.md`: add the Isolation section, its order (directly after Docker), its controls, the Custom Gateway allow-list warning, and the unsupported-platform state.
- `specifications/core/GSS-global-settings-storage.md`: add the stored isolation record and its load and save operations.
- `specifications/ui/SET-project-settings.md`: the Docker section states that Docker-in-Docker is ignored under Microsandbox.
- `specifications/tools/EAC-execute-agent-cli.md`: launch through microsandbox, one shared VM per stream, image pull and export fallback, export cache keyed by image ID, no Docker-in-Docker additions under Microsandbox.
- `specifications/core/GSU-graduation-start.md`: GSU-FR-RQDS does not refuse a start on the Docker socket under Microsandbox.
- `specifications/core/GXD-graduation-execution.md`: GXD-FR-UZHX names a sandbox start failure as a `launch_failed` cause.
- `specifications/ui/NTF-notifications.md`: the Error toast for a sandbox launch failure.
- `specifications/ui/GRU-graduation-runs.md`: the interrupted run shows the sandbox failure detail if needed.