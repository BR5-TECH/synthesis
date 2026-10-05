# Agentic CLI mock

**Spec code:** `ACM`

## Intent
`agentic-cli-mock` is a standalone executable that stands in for an agentic CLI — Claude Code or Codex — for the container runtime that launches one, or for the two of them together, so a test can launch it instead of the real thing. It exists because the part of driving an agent that breaks quietly is the executable contract itself: which binary was chosen, how its argument vector was built, what reached its stdin, how its stdout and stderr were read back, what its exit status meant, and how long the caller was willing to wait. Against a real CLI those facts are non-deterministic, cost tokens, and need a network; against a real container runtime they additionally need a daemon and a pinned image; against this mock they are asserted byte-for-byte from a JSON scenario file, offline, in milliseconds. Standing in for `docker` is what lets a test assert the whole generated invocation at once. A container's argument vector carries the vendor's own vector as its tail, and an entrypoint forwards the caller's stdin straight to the program it launches — so there is no second process to substitute, and one mock run stands in for the entire stack, validates both halves of the vector, and replays the container's streams and exit status in the caller's place. It is test infrastructure that lives in this repository and is built when a test needs it. Out of scope: it is not a production agent backend, not the version-probe target `../core/AIC-agentic-integrations.md` AIC-FR-04 runs, and not a drop-in replacement for any of the three tools — it simulates nothing about what an agent *does* or what a container *is*, only about how either is *invoked*; and how an application integration test locates this executable and injects it in place of the real binary or the real runtime is the calling test's concern rather than a behaviour of the mock.

## Contract surface
The mock's contract has five parts, and callers depend on all five: where the executable comes from, the command line it accepts, the scenario document that drives it, the diagnostic record it emits when something is wrong, and the exit codes that classify every outcome. Nothing else about the process — its working directory, its environment, its internal module layout — is contractual.

### The crate and the executable
- **Crate** — `tools/agentic-cli-mock/`, package name `agentic-cli-mock`, with its own committed `Cargo.lock`. The repository root holds no Cargo manifest, so this crate is its own workspace root and is not a member of the `synthesis` package under `src-tauri/`.
- **Dependencies** — a JSON reader and a base64 codec, and nothing else. The crate does not depend on `synthesis_lib`, on Tauri, or on any crate the application links a system library through.
- **Build** — `cargo build --manifest-path tools/agentic-cli-mock/Cargo.toml`, optionally with `--release`.
- **Executable** — `tools/agentic-cli-mock/target/debug/agentic-cli-mock` (`target/release/…` for a release build), plus the platform's executable suffix.
- **Documentation** — `tools/agentic-cli-mock/README.md`.
- **Example scenarios** — `tools/agentic-cli-mock/scenarios/`.

### The command line
```text
agentic-cli-mock --tool <claude|codex|docker|docker+claude|docker+codex> \
                 --scenario <path-to-json> -- <tool arguments...>
```

`--tool` and `--scenario` are both required and both take a value. `--` terminates the mock's own options; every token after it is a *received tool argument* and is never interpreted by the mock.

### The scenario document
```text
Scenario {
  version:  1,                     // required, the integer 1
  expected: Expected,              // required
  response: Response,              // required
}

Expected {
  args?:          string[],        // exact, ordered, complete received vector
  required_args?: Requirement[],   // positional-independent requirements
  stdin?:         Payload | null,  // absent | null | exact bytes
}

Requirement = { value: string }
            | { name: string, value: string }     // `name` is diagnostic only

Payload = { encoding: "utf8",   value: string }
        | { encoding: "base64", value: string }
        | string                                  // equivalent to encoding "utf8"

Response {
  stdout:    Payload,              // required
  stderr:    Payload,              // required
  exit_code: integer,              // required, 0..=255, never 64 or 65
  delay_ms?: integer,              // optional, 0..=10000, default 0
}
```

### The failure record
A configuration error or an expectation mismatch writes exactly one JSON object followed by one newline to stderr, with these fields in this order:

```json
{"kind":"expectation_mismatch","code":65,"tool":"claude","check":"args","index":3,"name":"prompt","expected_length":12,"actual_length":7,"mismatch_type":"value"}
```

- `kind` — `"configuration_error"` or `"expectation_mismatch"`. Always present.
- `code` — `64` or `65`, equal to the process exit code. Always present.
- `tool` — the resolved `--tool` value. Present whenever it named a registered adapter.
- `check` — `"scenario"`, `"adapter"`, `"args"`, `"required_args"`, or `"stdin"`. Always present.
- `index` — a zero-based position. Present for an argument mismatch and for an unsatisfied requirement.
- `name` — the `name` a requirement supplied. Present only for an unsatisfied named requirement.
- `expected_length`, `actual_length` — lengths of the compared unit: element counts when two vectors are compared, byte counts when two arguments or two stdin streams are.
- `mismatch_type` — a stable category from the fixed set in ACM-FR-25.

### Exit codes
- `64` — mock configuration error.
- `65` — expectation mismatch.
- Any other value in `0..=255` — the simulated tool's configured exit code, passed through unchanged.

### The adapter registry
An adapter is one registry entry: a `--tool` value and the set of tokens that value requires somewhere in the received argument vector. Five are registered — `claude`, `codex`, `docker`, `docker+claude`, and `docker+codex`. Scenario loading, generic expectation matching, response emission, exit-code classification, and the failure record are shared and know nothing about any particular adapter, so a runtime and a whole stack are registry entries exactly like a single CLI rather than modes of the program.

The composite entries exist because a container runtime with an entrypoint does not add a process to stand in for — it passes everything after the image straight to the program it launches and forwards stdin to it. One invocation of this executable therefore replaces the runtime *and* the vendor CLI together, and the adapter that validates it requires both halves: `docker+claude` requires `run` and `-p`, `docker+codex` requires `run` and `exec`. A caller replacing only one layer, or checking one layer in isolation, uses the single-layer entry instead.

## Functional requirements
1. **ACM-FR-01** The mock is a Rust crate at `tools/agentic-cli-mock/` with its own committed `Cargo.lock`, depending on a JSON reader and a base64 codec and on no crate the application links a system library through. It builds and its suite runs with `src-tauri/` entirely absent from the checkout.
2. **ACM-FR-02** `cargo build --manifest-path tools/agentic-cli-mock/Cargo.toml` produces an executable named `agentic-cli-mock` under that crate's `target/` directory, and that path is what a caller launches. The mock is never installed onto `PATH` by its own build.
3. **ACM-FR-03** `--tool` and `--scenario` are both required and both take a value. An invocation missing either, repeating either, or supplying an option the mock does not define is a configuration error.
4. **ACM-FR-04** `--` terminates the mock's own options. Every token after it forms the received tool argument vector in the order given, preserving spelling, quoting boundaries as the operating system delivered them, empty-string arguments, repeated arguments, and tokens that look like mock options such as `--tool` or `--`. An invocation with no `--` supplies an empty received vector rather than being an error.
5. **ACM-FR-05** `--tool` is resolved against the adapter registry. A value that names no registered adapter is a configuration error, and the failure record for it carries no `tool` field because no tool was selected.
6. **ACM-FR-06** The mock opens exactly one file — the scenario at the path given to `--scenario` — and opens it read-only. It creates, writes, or deletes nothing anywhere, opens no socket, resolves no hostname, and spawns no child process.
7. **ACM-FR-07** The mock reads stdin to EOF on every run that gets past parsing its own options, whether or not the scenario configures a stdin expectation, and does so before opening the scenario file. A caller may therefore always write its payload and close the stream without risking a broken pipe; a caller that supplies no input must still close the stream or attach an already-closed one, because the mock waits for EOF before doing anything else.
8. **ACM-FR-08** `version` is required and must be the integer `1`. Any other value, including the string `"1"` or the number `1.0`, is invalid scenario data.
9. **ACM-FR-09** A payload is a JSON string, an object `{ "encoding": "utf8", "value": … }`, or an object `{ "encoding": "base64", "value": … }`; the bare string form is exactly equivalent to the `utf8` object form. The mock decodes the payload to bytes once and thereafter compares and emits those bytes verbatim, applying no Unicode normalization, no line-ending translation, and no newline addition or removal. An `encoding` outside the two named values, or a `base64` value that does not decode, is invalid scenario data.
10. **ACM-FR-10** `expected.args`, when present, is the complete received vector: matching requires the same number of elements in the same order with byte-identical values. An empty list is valid and requires an empty received vector.
11. **ACM-FR-11** `expected.required_args`, when present, is a list of requirements each of which must be satisfied by at least one element of the received vector, in any position. `name` is caller-supplied diagnostic metadata that appears in the failure record and is never compared against anything. An empty list is valid and requires nothing.
12. **ACM-FR-12** `expected.stdin` is tri-state and the three states are distinct: absent means stdin is not validated, `null` means the caller must have provided zero bytes, and a payload means the received bytes must equal the decoded payload exactly. A closed stream and a stream carrying zero bytes are the same state and both satisfy `null`.
13. **ACM-FR-13** `response.exit_code` is required and must be an integer in `0..=255` other than `64` and `65`. A negative value, a value above `255`, a non-integer, and either reserved code are invalid scenario data, so a scenario can never make a simulated tool failure indistinguishable from a mock failure.
14. **ACM-FR-14** `response.delay_ms` is optional, defaults to `0`, and must be an integer in `0..=10000`. A negative value, a non-integer, and any value above the bound are invalid scenario data.
15. **ACM-FR-15** A field the schema does not define, at any level of the scenario document, is invalid scenario data. The mock never ignores an unrecognised key.
16. **ACM-FR-16** A JSON object that repeats a key is invalid scenario data. The mock never resolves a duplicate by taking the first or last occurrence.
17. **ACM-FR-17** Checks run in a fixed order — adapter, then `expected.args`, then `expected.required_args`, then `expected.stdin` — and the first failing check is the one reported. Within `expected.args` the reported index is the lowest position at which the vectors differ; within `expected.required_args` the reported requirement is the first unsatisfied one in the order the scenario lists them.
18. **ACM-FR-18** Each adapter requires one or more exact tokens, each of which must appear at some position in the received vector: `claude` requires `-p`, `codex` requires `exec`, `docker` requires `run`, `docker+claude` requires both `run` and `-p`, and `docker+codex` requires both `run` and `exec`. No adapter constrains the position of a token or anything else about the vector, and an adapter requiring no token at all does not exist, because it would validate nothing. The tokens are the ones the real tools accept, so a vector a production call site generated reaches an adapter unmodified and an exact-vector expectation compares against what production actually emits.
19. **ACM-FR-19** Adapter validation and the scenario's own expectations both apply to every run, and the failure of either is an expectation mismatch. A scenario cannot waive its adapter's requirement, and satisfying that requirement does not exempt the run from `expected.args`, `expected.required_args`, or `expected.stdin`.
20. **ACM-FR-20** When the adapter and every configured expectation pass, the mock writes the configured stdout bytes to stdout and the configured stderr bytes to stderr, in full and unmodified, and exits with the configured exit code. An empty payload produces no bytes on that stream rather than an empty line.
21. **ACM-FR-21** A non-zero `delay_ms` is waited out after every check has passed and before any configured byte is written. The process stays alive and produces no output for the duration, so a caller that terminates it mid-delay observes a killed process with empty streams rather than a partial or early response.
22. **ACM-FR-22** stdout and stderr are never merged into one stream and neither is truncated, whatever the payload size. Both are flushed before the process exits, and the process exits on its own once the response is written, leaving nothing running.
23. **ACM-FR-23** A configuration error or an expectation mismatch writes exactly one JSON object followed by exactly one newline to stderr and nothing else. The object carries `kind`, `code`, and `check` always, `tool` whenever an adapter was resolved, and each remaining field only where it applies, always in the order given in the contract surface — so the same input and the same failure produce byte-identical stderr on every run.
24. **ACM-FR-24** The failure record never contains an expected or an actual value, any decoded payload or fragment of one, or the scenario path. A mismatch is described by location, length, and category alone, so a scenario holding a token, a prompt, or a secret in an argument can fail without disclosing it and without disclosing where on disk it lives.
25. **ACM-FR-25** `mismatch_type` is drawn from a fixed set: `missing_headless_argument` for an adapter failure; `arity` and `value` for `expected.args`; `absent` for `expected.required_args`; `length` and `content` for `expected.stdin`; and for a configuration error one of `missing_tool_option`, `unknown_tool`, `missing_scenario_option`, `unknown_option`, `unreadable`, `malformed_json`, `duplicate_key`, `unknown_field`, `bad_version`, `bad_encoding`, `bad_type`, `bad_exit_code`, or `bad_delay`. An `arity` failure reports element counts and no index; a `value` failure reports the index and the two arguments' byte lengths; an `absent` requirement reports its index, its `name` where it has one, and the required value's byte length with no actual length, because nothing was found to measure.
26. **ACM-FR-26** A configuration error and an expectation mismatch both suppress the configured response entirely: no configured stdout byte and no configured stderr byte is written, and the configured exit code is not used.
27. **ACM-FR-27** The process exits `64` for a configuration error, `65` for an expectation mismatch, and with the configured `exit_code` otherwise. The mock never exits `64` or `65` on a successful run, and never produces a classification of its own for being terminated — a caller-enforced timeout or kill is identified by the caller's own termination status.
28. **ACM-FR-28** Adding a tool requires adding one adapter and registering it, and requires no change to option parsing, scenario loading, expectation matching, response emission, exit-code classification, or the failure record. The scenario document contains no tool-specific field, so the same scenario file drives any registered adapter.
29. **ACM-FR-29** The crate ships at least one example scenario per registered tool under `tools/agentic-cli-mock/scenarios/`, each a valid document under this schema and each exercised by the crate's own suite, so an example that drifts from the schema fails the suite rather than misleading a reader.
30. **ACM-FR-30** `tools/agentic-cli-mock/README.md` documents the build command, the resulting executable path, the command-line grammar, the scenario schema including the `delay_ms` bound, the exit codes, the failure record shape, and the stdin-closure requirement of ACM-FR-07 — and states that mounting or copying the executable into a container, or otherwise putting it where a system under test will find it, is the calling harness's responsibility.

## Non-functional requirements
- The mock performs no network access of any kind. It is usable on a machine with no route to anywhere and inside a container with no network namespace.
- A run whose scenario configures no delay completes in single-digit milliseconds, so a suite can afford hundreds of invocations.
- The crate's `cargo check` and `cargo test` run as their own CI lane, separate from the application's Rust lane and needing none of the system packages that lane installs (per `CIP-ci-pipeline.md` CIP-FR-19).
- Behaviour is identical across the platforms the project develops and tests on. The only platform-visible difference is the executable's filename suffix.
- The mock holds the whole received argument vector, the whole stdin stream, and the whole configured response in memory; scenarios are sized for tests, not for streaming.
- Every diagnostic the mock produces goes to stderr as the single failure record. The mock writes no log file, emits no progress output, and honours no verbosity option, so stderr is either the configured payload or exactly one record.
