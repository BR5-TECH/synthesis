# agentic-cli-mock

A standalone stand-in for an agentic CLI — Claude Code or Codex — that a test
launches instead of the real tool. It asserts the executable contract itself:
which arguments were built, what reached stdin, what came back on stdout and
stderr, what the exit status meant, and how long the caller waited.

It is test infrastructure. It is not a production agent backend, not a
drop-in replacement for either tool, and it simulates nothing about what an
agent *does* — only about how one is *invoked*.

The specification is `specifications/infra/ACM-agentic-cli-mock.md`.

## Build

```sh
cargo build --manifest-path tools/agentic-cli-mock/Cargo.toml
```

The executable lands at:

```text
tools/agentic-cli-mock/target/debug/agentic-cli-mock
```

Add `--release` for `target/release/agentic-cli-mock`. On Windows both carry a
`.exe` suffix. The build never installs anything onto `PATH`.

The crate is its own workspace root — the repository root holds no Cargo
manifest — and depends only on `serde`, `serde_json` and `base64`. It compiles
in a checkout with no `src-tauri/` directory at all, and needs none of the
system libraries the application links against.

Its tests:

```sh
cd tools/agentic-cli-mock && cargo check && cargo test
```

## Invoke

```text
agentic-cli-mock --tool <claude|codex|docker|docker+claude|docker+codex> \
                 --scenario <path-to-json> -- <tool arguments...>
```

- `--tool` and `--scenario` are both required and both take a **space-separated**
  value. The `--tool=claude` spelling is not defined.
- `--` terminates the mock's own options. Every token after it is treated as an
  argument the simulated CLI received — including empty strings, repeats, and
  tokens that look like the mock's own options. An invocation with no `--`
  supplies an empty received vector, which is not itself an error.
- Because `--` terminates options, it is never taken as an option's *value*:
  `--tool claude --scenario -- -p` reports a missing `--scenario` rather than
  treating `--` as the path.
- Repeating either option is an error, including as the final token with no
  value after it.

```sh
agentic-cli-mock --tool claude --scenario scenarios/claude-headless.json -- \
    -p "Summarise the staged diff in one sentence." < /dev/null
```

> **stdin must reach EOF.** The mock reads stdin to EOF on every run that gets
> past parsing its own options, whether or not the scenario validates it, so a
> caller can always write its payload and close without risking a broken pipe.
> The cost is that a run which inherits a terminal's stdin never proceeds:
> redirect from `/dev/null`, or close the stream, when you are supplying no
> input.

## Scenario schema

```json
{
  "version": 1,
  "expected": {
    "args": ["-p", "expected prompt"],
    "stdin": null
  },
  "response": {
    "stdout": {"encoding": "utf8", "value": "{\"result\":\"ok\"}\n"},
    "stderr": {"encoding": "utf8", "value": ""},
    "exit_code": 0,
    "delay_ms": 0
  }
}
```

| Field | Required | Rules |
|---|---|---|
| `version` | yes | The integer `1`. Not `"1"`, not `1.0`. |
| `expected.args` | no | Exact ordered list of strings — the **complete** received vector. An empty list requires an empty vector. |
| `expected.required_args` | no | Positional-independent requirements, `{"value": …}` or `{"name": …, "value": …}`. `name` is diagnostic metadata and is never matched against anything. An empty list requires nothing. |
| `expected.stdin` | no | Absent = not validated. `null` = the caller must supply zero bytes. A payload = those exact bytes. |
| `response.stdout` | yes | Payload, emitted verbatim. |
| `response.stderr` | yes | Payload, emitted verbatim. |
| `response.exit_code` | yes | Integer `0..=255`, never `64` or `65`. |
| `response.delay_ms` | no | Integer `0..=10000`. Defaults to `0`. |

A **payload** is a bare JSON string, `{"encoding": "utf8", "value": …}`, or
`{"encoding": "base64", "value": …}`. The bare string form is exactly the
`utf8` object form. Bytes are decoded once and thereafter compared and emitted
verbatim — no Unicode normalisation, no line-ending translation, and no newline
added or removed. A test that needs structured JSON supplies its exact
serialized text; the mock never parses or re-serializes a payload.

Anything else is rejected: an unknown field at any level, a repeated key in any
object, an unknown encoding, undecodable base64, and any value outside the
stated types or ranges.

`version`, `exit_code` and `delay_ms` accept only JSON *integer* literals.
Spellings that JSON's data model treats as the same number but writes as a
float — `1.0`, `1E2`, `0e0`, `-0` — are rejected, in the same way `"version":
1.0` is.

Both `args` and `required_args` may be present, and both must then pass.

### Adapter validation

Each tool's headless convention is validated on top of whatever the scenario
says, and neither can waive the other:

| `--tool` | requires the token |
|---|---|
| `claude` | `-p` |
| `codex` | `exec` |
| `docker` | `run` |
| `docker+claude` | `run` and `-p` |
| `docker+codex` | `run` and `exec` |

Every token an adapter lists must appear, at any position. Nothing else about
the vector is constrained by the adapter.

The composite entries exist because a container runtime with an entrypoint adds
no process to stand in for: it passes everything after the image straight to the
program it launches and forwards stdin to it. Replacing `docker` with this
executable therefore replaces the vendor CLI too, and `docker+claude` /
`docker+codex` are what validate both halves of that vector in one run. Use the
single-layer entries to check one layer in isolation.

## Exit codes

| Code | Meaning |
|---|---|
| `64` | Mock configuration error — invalid invocation, unreadable scenario, malformed JSON, duplicate key, or invalid scenario data. |
| `65` | Expectation mismatch — adapter validation or a configured expectation failed. |
| `0..255` otherwise | The configured simulated tool exit code, passed through unchanged. |

A caller-enforced timeout or kill gets no classification from the mock: it is
identified by the caller's own termination status. A configured `delay_ms`
keeps the process alive and silent for its duration, which is what makes
timeout handling testable.

Both failure codes suppress the configured response entirely — no configured
stdout byte, no configured stderr byte, and the configured exit code unused.

## The failure record

A configuration error or an expectation mismatch writes exactly one JSON object
followed by exactly one newline to stderr, and nothing else:

```json
{"kind":"expectation_mismatch","code":65,"tool":"claude","check":"args","index":1,"expected_length":26,"actual_length":5,"mismatch_type":"value"}
```

Fields always appear in that order. `kind`, `code` and `check` are always
present; `tool` is present once `--tool` has resolved to a registered adapter;
everything else appears only where it applies. The record is byte-identical
across runs for the same input and failure.

**It never contains an expected or actual value, a decoded payload, or the
scenario path** — a mismatch is described by location, length and category
alone, so a scenario holding a token or a prompt can fail without disclosing it,
and without disclosing where on disk it lives. `expected_length` and
`actual_length` are element counts when two vectors are compared and byte
counts when two arguments or two stdin streams are.

`check` is one of `scenario`, `adapter`, `args`, `required_args`, `stdin`.
Every configuration error reports `check: "scenario"`; which one it was lives in
`mismatch_type`.

`mismatch_type` is drawn from a fixed set:

| `check` | categories |
|---|---|
| `adapter` | `missing_headless_argument` |
| `args` | `arity`, `value` |
| `required_args` | `absent` |
| `stdin` | `length`, `content` |
| `scenario` | `missing_tool_option`, `unknown_tool`, `missing_scenario_option`, `unknown_option`, `unreadable`, `malformed_json`, `duplicate_key`, `unknown_field`, `bad_version`, `bad_encoding`, `bad_type`, `bad_exit_code`, `bad_delay` |

An `arity` failure reports element counts and no index. A `value` failure
reports the lowest differing index and the two arguments' byte lengths. An
`absent` requirement reports its index, its `name` where it has one, and the
required value's byte length — with no actual length, because nothing was found
to measure.

### Which check fails first

Checks run in a fixed order and the first failure is the one reported:

1. adapter validation
2. `expected.args`
3. `expected.required_args`
4. `expected.stdin`

Invocation problems are resolved before any of that, in this order: the tool is
resolved first (so every later record can name it), then an undefined or
repeated option, then a missing `--scenario`. A repeated `--tool` or
`--scenario` reports `unknown_option`, since neither spelling is an invocation
the mock defines.

## Using it from a harness or a container

Putting the executable where a system under test will look for it is the calling
harness's responsibility — this crate builds a binary and documents its path,
and does nothing else about placement.

The binary is self-contained apart from the platform's C runtime, so the usual
approaches all work:

- **Direct launch** — spawn the path above with the tool arguments after `--`.
- **PATH injection** — symlink or copy it into a directory placed ahead of the
  real tool on `PATH`, under the name the system under test invokes (`claude`,
  `codex`). Note that the mock still requires its own `--tool` and `--scenario`
  options, so this only works where the harness controls the argument vector; a
  wrapper script that prepends them is the usual shape.
- **Container** — mount or `COPY` the built binary in, along with the scenario
  file. Build for the container's target first (`cargo build --release --target
  x86_64-unknown-linux-musl`) when the container's libc differs from the host's.

The scenario path is read-only input and may live anywhere the process can read.

## Adding a tool

Add one adapter and register it in `src/adapter.rs`:

```rust
static OPENCODE: HeadlessTokenAdapter = HeadlessTokenAdapter::new("opencode", "--headless");

pub static REGISTRY: &[&(dyn ToolAdapter + 'static)] = &[&CLAUDE, &CODEX, &OPENCODE];
```

An adapter whose validation is not "one token must be present" implements
`ToolAdapter` directly. Nothing else changes: option parsing, scenario loading,
expectation matching, response emission, exit-code classification and the
failure record are all shared and know nothing about any particular tool, and
the scenario document carries no tool-specific field, so one scenario file
drives every registered adapter.
