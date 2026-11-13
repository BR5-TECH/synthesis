# OpenSpec acceptance tests

**Spec code:** `OAT`

## Intent
The acceptance suite that runs the production OpenSpec seam against a real OpenSpec CLI and a real agent image. Every other OpenSpec test uses a fake CLI whose answers the tests themselves define, so a change in the CLI's JSON, exit statuses, or status codes stays invisible to them. This suite reads what the real CLI answers on a project that `openspec init` made, through the same code the application runs, and fails when the application reads it differently from what its specifications say. It also builds the Claude Code agent image and runs the production image probe against it. The suite runs on a developer machine by hand, through the task file, and never in the pull-request pipeline.

## Functional requirements
1. **OAT-FR-KWDN** The suite is a set of ignored Rust tests in `src-tauri/` whose names start with `openspec_uat`. `cargo nextest run` and `cargo test` without `--ignored` run none of them.
2. **OAT-FR-ZRQC** The CLI tests drive the production seam of `../core/OSM-openspec-mode.md`: the real process runner, the real executable probe, and the environment of the process. They substitute no process, no probe, and no JSON answer, and they use the real wait seam of `../core/OFP-openspec-failure-policy.md` OFP-FR-MJZU.
3. **OAT-FR-HCBT** The CLI executable is the `openspec` that the production resolution of OSM-FR-EZKW finds. Where the variable `OPENSPEC_UAT_BIN` names a file, the suite puts its folder first in the search path, so the same suite runs against a second CLI version.
4. **OAT-FR-PLXY** Each test makes its own fixture project in a new temporary folder: `git init`, then `openspec init --tools none` with the CLI under test, then the capabilities and changes that the test needs as files, then one commit. Every temporary folder is removed when the test ends, also when it fails.
5. **OAT-FR-GNEU** The fixture holds at least two live capabilities, one capability whose id holds a `/`, one active change with ADDED, MODIFIED, REMOVED, and RENAMED deltas and a `tasks.md` with done and open tasks in two groups, and one change that the CLI does not accept.
6. **OAT-FR-MSXV** A test proves detection and the version check: the fixture uses OpenSpec (OSM-FR-LLRV), the CLI state is `available` with the path and the version of the CLI under test (OSM-FR-ZNVZ, OSM-FR-SKUP), and a version string that the CLI prints parses.
7. **OAT-FR-DJYT** A test proves each read of `../core/OSM-openspec-mode.md` against the real answer: `list_capabilities` (OSM-FR-JTOQ), `read_capability` and `capability_not_found` (OSM-FR-SIBT), `list_changes` with the capabilities of each change (OSM-FR-OPZF, OSM-FR-FCMY), and `read_change` with its artifacts, its deltas, and the live requirement of each delta (OSM-FR-LLIO, OSM-FR-NMHS, OSM-FR-PYHP).
8. **OAT-FR-VTRA** A test proves validation: `validate_change` answers valid for the valid change and not valid with its issues as diagnostics for the change that the CLI does not accept (OSM-FR-APVF, OSM-FR-XDRN, OFP-FR-RUOP). The project validation of `--all` and of `--specs` reads the issues of each item.
9. **OAT-FR-FOLQ** A test proves apply progress: `apply_instructions` gives the state, the total and done task counts, and the next task that equal what `tasks.md` holds (OSM-FR-QLRW, OSM-FR-WIQF). A change with missing artifacts gives `change_not_ready` with its diagnostics (OFP-FR-BWML).
10. **OAT-FR-CGIA** A test proves the spec index of `../core/OSI-openspec-spec-index.md`: the groups (OSI-FR-MAUQ), the capabilities and the requirements read from the CLI (OSI-FR-SJNC), a change chip on each capability that the change touches, and a validation issue of the CLI as a gap.
11. **OAT-FR-WYEN** A test proves the archive of `../core/OAR-openspec-archive.md` OAR-FR-IAKL: a change with all tasks done archives, the archive folder is valid (OAR-FR-VAGH, OAR-FR-XTLB), and the live spec holds the changed requirements. A test proves that an archive the CLI refuses fails with the `code` of the CLI's first `status` entry (OAR-FR-EUXZ).
12. **OAT-FR-OUBE** A test proves the failure classes of `../core/OFP-openspec-failure-policy.md` against the real CLI: an executable that is not found gives `cli_unavailable` with `not_found` and the class `environment`; an exit status 1 with a `status` list gives `cli_rejected`, the class `content`, and the CLI's entries as diagnostics (OFP-FR-NJSO).
13. **OAT-FR-RYAS** Each fixture is made below the temporary folder of the process, which is a symbolic link on macOS. A test proves that each read gives the same result where the location path is the link form and where it is the resolved form.
14. **OAT-FR-NVHI** The image test builds `docker/agent-images/claude-code/Dockerfile` with a local tag and runs the production image probe of `../core/OFP-openspec-failure-policy.md` OFP-FR-BZDP against it. The probe passes with the version that the Dockerfile pins (per `AVI-agent-vendor-images.md` AVI-FR-RZFR).
15. **OAT-FR-ISHM** The image test also proves the probe failures of OFP-FR-XBNJ against a real container: an image that holds no `openspec` gives `not_found`. It reaches no registry for that image other than the base image that its Dockerfile names.
16. **OAT-FR-TJAV** A test whose prerequisite is missing fails, and does not pass or skip. The failure names the missing prerequisite: no `openspec` that the resolution finds, a CLI version below 1, a `docker` executable that is missing, or a Docker daemon that does not answer.
17. **OAT-FR-UQCL** The CLI tests reach no network: each run of the CLI, the setup runs included, has telemetry turned off. Only the image build of the image test reaches a registry. The suite reads no credential. A test that runs on a CLI version below what the fixture needs reports the version in its failure.
18. **OAT-FR-PEFN** Each failed assertion names the test, the CLI version, the subcommand, and the compared thing. It does not print the files of the fixture.

## User stories
- As a developer, I want to run `task uat:openspec` so that I see whether the application reads the answers of the OpenSpec CLI that is installed on my machine.
- As a developer, I want to run the suite with `OPENSPEC_UAT_BIN` set so that I see whether a new CLI release breaks the application before users update.
- As a developer, I want to run `task uat:openspec-image` so that I see whether the agent image holds a usable OpenSpec CLI.

## Contract surface
This specification owns the ignored suite and its fixtures. It registers no Tauri command and changes no product behaviour. The tasks that run it belong to `TSK-taskfile.md` (TSK-FR-NJQE, TSK-FR-HEXG).

```text
src-tauri/src/openspec/tests/uat*.rs     the CLI tests and the fixture builder
src-tauri/src/tools/agent_exec/tests/    the image test, beside the Docker-in-Docker test
OPENSPEC_UAT_BIN                         optional: the path of another openspec executable
```

## Non-functional requirements
- One CLI test completes in less than 30 seconds on a developer machine.
- The image build takes the time of a normal `docker build`. A cached build takes less than one minute.
- Every file of the suite holds at most 1000 lines.
