# [CLAUDE.md](http://CLAUDE.md)

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project rules

- Always use Simplified Technical English (ASD-STE100) when writing comments, specifications any communicating to user in any other way.
- Never add comments to markdown files.
- Files with source code and tests should not have more than 1000 lines: always refactor large files to smaller ones.
- Always use `analyst` skill when adjusting or creating specifications in the project.
- Always use `engineer` skill when implementing specifications or in any other way adjusting the code.
- Always use `frontend-design` skill when working on UI/UX components.

## Development process

- Always start with capturing the planned changes in specifications, either by creating new feature specifications or amending existing ones. `analyst` skill must be used for this.
- Once specifications are adjusted - implement the requested changes using the `engineer`  skill. If during implementation gap in spec was identified - use `analyst` skill again, to adjust the specifications as needed.
- When ambiguity is identified - escalate to user with questions.

## Commands

Package manager: **pnpm** (v10+). Install with `pnpm install`.

- `pnpm tauri dev` — run the full desktop app (spawns Vite on port 1420, then launches the Rust shell). Use this for normal development.
- `pnpm dev` — frontend-only Vite dev server. Rarely useful on its own since `invoke()` calls will fail outside the Tauri runtime.
- `pnpm build` — typecheck (`tsc`) + production frontend build into `dist/`. **Does not typecheck test files** — they are excluded from `tsconfig.json`.
- `pnpm exec tsc -p tsconfig.test.json --noEmit` — **required** typecheck for test files (`src/**/*.test.ts(x)` and `src/test/**`). Must pass with 0 errors before any change touching tests or test infrastructure is considered done. `pnpm build` will not catch type errors in tests; this command is the only thing that does.
- `pnpm test` — Vitest, jsdom environment. Setup file `src/test/setup.ts` registers `@testing-library/jest-dom/vitest` matchers (`toBeInTheDocument`, etc.). Test-side type augmentation reaches test files via `tsconfig.test.json`.
- `pnpm tauri build` — produce a bundled native app (runs `pnpm build` first via `beforeBuildCommand`).
- `cd src-tauri && cargo check` — quick Rust-side validation without launching the app.
- `cd src-tauri && cargo test` — run the Rust unit tests (colocated under `#[cfg(test)] mod tests`).
- `cd src-tauri && cargo build` — compile the Rust binary alone.

Definition-of-done for any frontend change: `pnpm test` green, `pnpm build` green, AND `pnpm exec tsc -p tsconfig.test.json --noEmit` clean. For any backend change: `cargo test` green and `cargo check` clean.

## Tests must not depend on timing

CI is always slower than a local machine. It runs each Vitest area (`src/test/testAreas.ts`) with 1 worker on 2 vCPUs, and one test can take 5 times longer there. A test that passes locally only because it is fast is a defect.

- Do not make a result depend on how much real time passes. Do not use fixed sleeps, real-time limits, or the order of two independent async events. Wait for a state with `findBy*` or `waitFor`, or use fake timers and advance them.
- Do not let a timer, a promise, or module-level state go from one test to the next. Reset each module store that the code under test touches in `beforeEach` (for example `resetLogBufferForTest`, `resetProjectIdentity`).
- An `invoke` mock must answer the commands that the app sends in the background (`append_log_records`). For a command that it does not expect, it must return a rejected promise. It must not throw synchronously.
- An unhandled error fails the CI lane, also when every test passes.
- When a change adds a timer, a debounce, a background call, or a log emit on an async path, run the affected area as CI does: `pnpm exec vitest run --project "<area>" --maxWorkers=1`.
- For Rust tests, run `TMPDIR=/tmp cargo test` when a result can depend on the length of a temporary path.

`.claude/skills/engineer/references/conventions.md` has the details.

## Architecture

Two-process Tauri 2 application:

- **Frontend** (`src/`) — React 19 + TypeScript + Vite 7, ESM modules, `react-jsx` runtime. Entry is `src/main.tsx` → `src/App.tsx`. Communicates with the backend only via `invoke()` from `@tauri-apps/api/core`.
- **Backend** (`src-tauri/`) — Rust library + thin `main.rs` binary. The library crate is named `synthesis_lib` (the `_lib` suffix is required to avoid a name clash with the binary on Windows — do not rename). Each `#[tauri::command]` function lives in its domain module (for example `src-tauri/src/graduation/commands.rs`). `src-tauri/src/lib.rs` registers all of them in `tauri::Builder` via `invoke_handler(tauri::generate_handler![...])`. Every new command must be added to that macro call or the frontend's `invoke()` will fail at runtime, and to the list in `src-tauri/src/command_names.rs`, which `lib_tests.rs` checks.

Cross-process contract: each frontend `invoke("name", { args })` maps to a Rust function annotated `#[tauri::command]` with matching parameter names (camelCase on the JS side becomes snake_case on the Rust side automatically).

## Filesystem access (backend)

All backend filesystem I/O goes through `crate::fs::FsAccess` (`src-tauri/src/fs/access.rs`). Never call `std::fs::*` or disk-touching `Path` predicates directly in `src-tauri/src/**`.

## Logging

Instrument new code with the logging helpers — `src/logging.ts` (`logDebug`/`logInfo`/`logWarn`/`logError`) on the frontend, `src-tauri/src/logging.rs` (`log_debug`/`log_info`/`log_warn`/`log_error`) on the backend. Log operation boundaries, external calls, and every error path, including handled ones. Nothing downstream redacts anything, so never log credentials, tokens, or user content.

## Capabilities & permissions

`src-tauri/capabilities/default.json` declares what the `main` window is allowed to call. New Tauri plugins or APIs almost always require adding a permission identifier here (e.g. `opener:default`); a missing permission shows up as a runtime "not allowed by ACL" error, not a compile error.

## Configuration coupling

`vite.config.ts` and `src-tauri/tauri.conf.json` are coupled on port **1420** (`strictPort: true` on the Vite side; `devUrl` on the Tauri side). Don't change one without the other. `vite.config.ts` also ignores `src-tauri/**` from its file watcher — keep it that way to avoid HMR loops triggered by Rust build artifacts.

## Toolchain

- Rust pinned via `src-tauri/rust-toolchain.toml` (channel `1.95`). The dep tree requires rustc ≥ 1.95 (`rig-core` 0.44 declares it; `time-macros` and `idna_adapter` need edition 2024); older toolchains fail to build it.
- Native libraries on macOS: every `-sys` dependency must compile its library into the executable (`vendored`/`static` feature in the `cfg(target_os = "macos")` table of `src-tauri/Cargo.toml`). The bundle is signed with the hardened runtime, so a Homebrew or other host dylib makes the app crash at launch. `task build:macos` fails on such a link (`tools/macos-linkage/check-linkage.sh`). See `specifications/infra/NLL-native-library-linkage.md`.
- Node/pnpm: pnpm 10+ uses `pnpm-workspace.yaml` as the install-script allowlist (`allowBuilds`). Adding a new dep with a postinstall script may require allowlisting it there.