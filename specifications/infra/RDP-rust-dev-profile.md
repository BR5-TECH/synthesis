# Rust dev profile

**Spec code:** `RDP`

## Intent
The debug information that a development build of the desktop application crate carries. Every local check, every CI lane, and every check that an agent runs in a graduation sandbox builds the `dev` or the `test` profile of `src-tauri/`, and the `test` profile takes its settings from `dev`. Full debug information for the application crate and its whole dependency tree makes the test binary of `synthesis_lib` approximately 2 GB. The linker writes this binary again after each change, so its size sets the time of each check. A developer reads a panic or a backtrace much more frequently than they step through the code in a debugger, and a backtrace needs only the line tables. Thus the profile keeps the line tables for the workspace crates and no debug information for the dependencies. Out of scope: the `release` profile, the other Rust crates of the repository (`tools/agentic-cli-mock/`, `server/`), and the linker that a build uses, which `AVI-agent-vendor-images.md` AVI-FR-NFFO sets for the agent images.

## Functional requirements
1. **RDP-FR-RFMT** The `[profile.dev]` table of `src-tauri/Cargo.toml` sets `debug = "line-tables-only"`. A panic message and a backtrace of a debug build or a test binary show the file and the line of each frame of a workspace crate.
2. **RDP-FR-RQEL** The `[profile.dev.package."*"]` table of `src-tauri/Cargo.toml` sets `debug = false`, so a dependency that is not a member of the workspace carries no debug information in a `dev` or `test` build.
   - *Why:* most of the debug information in the test binary comes from the dependencies. With RDP-FR-RFMT and this requirement, the test binary of `synthesis_lib` is approximately one third of its size with full debug information, and the `target` directory is approximately one half.
3. **RDP-FR-EKRY** The comment above the two tables in `src-tauri/Cargo.toml` tells a developer who needs full debug information for a debugger session how to get it for one build without a change to the file: `CARGO_PROFILE_DEV_DEBUG=full` for the workspace crates, and `--config 'profile.dev.package."*".debug="full"'` for the dependencies.

## Contract surface
This specification owns the `[profile.dev]` and `[profile.dev.package."*"]` tables of `src-tauri/Cargo.toml`. It registers no Tauri command, exposes no operation, and is reachable from no running code path.

## Non-functional requirements
- The profile changes no behavior of the application and no result of a test. It changes only the size of the build output and the time to link it.
