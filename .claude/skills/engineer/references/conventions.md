# Synthesis conventions (load-bearing)

These are the project-specific rules that are easy to miss and expensive to get wrong. Honor them while implementing, and hand this file to the review sub-agents so they check against it. The authoritative source is the repo's `CLAUDE.md`; this is the short list that bites in practice.

## Cross-process contract (Tauri)

- The frontend talks to the backend **only** via `invoke("op_name", { args })` from `@tauri-apps/api/core`. Each call maps to a Rust `#[tauri::command]` of the same name. Tauri auto-converts `camelCase` (JS) ↔ `snake_case` (Rust) for parameter and command names — treat that as matching, not a mismatch.
- **Operation names are a spec contract.** The names in a UI spec's "Delegated to backend (abstract)" section must match, byte-for-byte, the operation in the paired core spec's "Contract surface" (modulo the snake/camel auto-conversion). Derive names from the spec, not from intuition.
- **Every `#[tauri::command]` must be registered** in `tauri::Builder::invoke_handler(tauri::generate_handler![ ... ])` in `src-tauri/src/lib.rs`. A command that isn't registered does not exist for the frontend — it fails at runtime, not compile time.

## Capabilities / ACL

- New Tauri plugins or APIs (`opener`, `fs`, `dialog`, …) require a permission identifier in `src-tauri/capabilities/default.json`. A missing permission surfaces at runtime as a "not allowed by ACL" error — not a compile error, so it's easy to ship broken. Add only the permission you need; an over-broad permission is a security smell worth flagging.

## Don't break these

- **Crate name `synthesis_lib`.** The library crate's `_lib` suffix avoids a Windows binary-name clash. Renaming it is a regression.
- **Port 1420 coupling.** `vite.config.ts` (`strictPort: true`) and `src-tauri/tauri.conf.json` (`devUrl`) must agree on port 1420. Don't change one without the other; if a spec demands a port change, surface it to the user rather than silently editing both.
- **Vite watcher ignores `src-tauri/**`.** Keep that ignore — removing it causes HMR loops on Rust build artifacts.
- **Rust toolchain is pinned** via `src-tauri/rust-toolchain.toml` (edition 2024 / rustc ≥ 1.88). The dep tree requires it; use the pinned toolchain.

## Logging

The application has exactly one diagnostic channel: the session logging facility
(`specifications/core/LGC-logging.md`, surfaced by `specifications/ui/LOG-logs.md`). It is what the
Logs panel reads. A `println!`, an `eprintln!`, or a `console.log` reaches nobody once the app is
packaged, so a module that reports itself that way is silent in the only place anyone will look.

- **Backend** (`src-tauri/**`): `crate::logging::log_info(sink, &crate::logging::BUFFER, &[Domain::Backend], "message", fields)`
  — and the `log_debug` / `log_warn` / `log_error` siblings. `sink` is the `AppHandle` the command
  already holds. Emitting never blocks the caller, so it is safe inside a tight loop.
- **Frontend** (`src/**`): `logInfo(["frontend"], "message", { … })` and its siblings from
  `src/logging.ts`. Emission is batched and never awaited, so it is safe on a render path.

**Emit where a reader would want an explanation**, not at every function boundary: an operation
starting and finishing, a decision that changed what the user sees, an external call and its
outcome, and every error path — especially the handled ones, which are otherwise invisible precisely
because they were handled. The buffer is a bounded ring, so noise costs real evidence: every
uninteresting record pushes an interesting one out.

**Pick the level by what a reader should do about the record.** `ERROR` — something failed and the
user is affected. `WARN` — the application recovered, but the cause is worth knowing. `INFO` — a
milestone of normal operation. `DEBUG` — detail that only matters once you are already investigating.

**Domains describe what a record is _about_, not which process emitted it.** A model call the backend
makes on the user's behalf is `ai` and `remote` together. Every record carries at least one (a record
with none is rejected), and the panel's filters are built on them.

**Put variable data in `fields`, not in the message.** The message is what a reader scans down a
column, so keep it short and roughly constant; the path, the count, the id, the duration belong in
the structured map beside it. That is also what makes a record searchable on terms the emit site
never anticipated — the panel matches against field keys and values as well as the message. Build the
map with `log_fields! { "key" => value, … }` on the Rust side, and an object literal on the frontend.

### Never put sensitive data in a log

The facility stores exactly what it is handed: it inspects nothing, redacts nothing, and masks
nothing (`LGC-FR-16`). There is no scrubber downstream to catch a mistake, and a record travels
further than it looks — into the Logs panel, onto the clipboard, and into any file the user exports
and attaches to a bug report. **The emit site is the only place this can be enforced, which makes it
your responsibility on every call.**

- Never log a token, API key, password, secret, `Authorization` header value, cookie, or session id
  — not in the message, not in a field, not embedded in a URL, and not inside an error value you are
  passing through.
- Log the *reference* instead of the credential: a token's id or its stored description, a provider's
  name, the masked hint a record already exposes elsewhere. `GTS-FR-01`, `AAP-FR-07`, `AIC-FR-20`,
  and `AGC-FR-26` state this for the modules that hold credential material today; the same rule binds
  every new emit site.
- Watch the values that *carry* a secret without being one: a remote URL with credentials in it, a
  request body, a serialized settings record, an exception from an HTTP client that echoes the
  request it made. Log a field you chose over a blob you did not read.
- User content deserves the same judgement as credentials: an artifact's path is fine to log, its
  contents are not.

The two rules meet at almost every interesting emit site — a failed authenticated request:

```rust
// Leaks the credential through the URL, and buries what you would filter on in prose.
log_error(&app, &BUFFER, &[Domain::Remote],
          &format!("push to {remote_url_with_token} failed: {err}"), Fields::new());

// Says what happened, keeps the searchable parts structured, and names the
// token without revealing it.
log_error(&app, &BUFFER, &[Domain::Remote, Domain::Backend], "push rejected",
          log_fields! { "remote" => "origin", "status" => 403, "token_id" => token.id });
```

When you are unsure whether a value is sensitive, log its **shape** rather than its value — a length,
a boolean, a kind, a count. That is usually what helps debugging anyway, and it cannot leak.

## Build / test — definition of done

Run the checks that match what you touched, and get them green before declaring done. Never declare done on red, and never retry silently past a real failure — surface it.

**Frontend changes** (anything under `src/**`, `index.html`, Vite/TS config):
1. `pnpm test` — Vitest (jsdom). UI tests mock `invoke`; they must not need the Tauri runtime.
2. `pnpm build` — typechecks **production** sources via `tsc` and builds.
3. `pnpm exec tsc -p tsconfig.test.json --noEmit` — typechecks **test files** (`src/**/*.test.ts(x)`, `src/test/**`).

Step 3 is not optional: `pnpm build` excludes test files from typechecking, so type errors in tests (missing jest-dom matchers, wrong mock signatures, undeclared globals) slip past steps 1–2. This third check is the only thing that catches them, and the repo's definition-of-done lists all three.

**Backend changes** (anything under `src-tauri/**`), from `src-tauri/`:
1. `cargo test` — colocated `#[cfg(test)] mod tests`. For `#[tauri::command]` fns that need the runtime, factor logic into a pure helper and test that.
2. `cargo check` — don't introduce new warnings (pre-existing warnings aren't your concern).
