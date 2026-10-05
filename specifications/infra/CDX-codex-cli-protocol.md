# Codex CLI protocol

**Spec code:** `CDX`

## Intent
The wire contract for driving the Codex CLI non-interactively: the exact argument vector for a fresh turn and for a resumed one, the bytes that go on stdin, the event stream that comes back on stdout, the exit statuses, and the thread identity that lets one turn continue another. It exists because `../tools/EAC-execute-agent-cli.md` treats an agentic CLI as an instrument and needs a contract it can generate and parse without guessing, and because a vendor upgrade should be a change to one pinned protocol rather than a change to the executor. This spec is the sole authority on how Codex is invoked and how its answer is extracted. It describes one vendor and no other: it names no flag, output format, credential, or session mechanism belonging to any other agentic CLI, it depends on no other vendor's protocol, and nothing stated here is transferable to one — a reader must not carry any element of this grammar to a different vendor, and a change made here affects no other vendor's contract. Out of scope: launching a container, mounting anything into one, bounding a run, or normalizing a result, all of which are `../tools/EAC-execute-agent-cli.md`'s; the image this CLI is installed in and the manifest that pins it, which are `AVI-agent-vendor-images.md`'s; resolving which vendor, model, or reasoning effort a project uses, which is `../core/AIC-agentic-integrations.md`'s; and the meaning of the task and response documents themselves, which is the executor's protocol rather than the vendor's.

## Contract surface

This spec exposes no operation and registers nothing. Its product is the pinned grammar below, which `../tools/EAC-execute-agent-cli.md`'s Codex vendor execution descriptor (EAC-FR-10) is built from and which its tests assert against.

### The pinned version and environment

| Fact | Value |
|---|---|
| CLI | Codex CLI |
| Pinned version | `0.147.0`, reported as `codex-cli 0.147.0` |
| Package | `@openai/codex` |
| Invoked as | the image entrypoint, per `AVI-agent-vendor-images.md` AVI-FR-04 |
| Working directory | the execution directory's own path, or `/workspace` in the fallback shape (`../tools/EAC-execute-agent-cli.md` EAC-FR-ZKMR) |
| Runs as | the image's non-root user (AVI-FR-03) |
| Network | required — the CLI reaches OpenAI's own service |

The version this protocol was established against is `0.147.0`. `AVI-agent-vendor-images.md`'s manifest (AVI-FR-07) remains the single runtime record of which version is installed, and its `cli_version` for `codex` and this pinned version are the same string; a manifest bump that does not match this spec is a protocol change and is resolved here before the image is adopted.

### Sources

Every requirement below traces to one of these, all accessed **2026-08-16**:

| Source | URL | Standing |
|---|---|---|
| `codex exec --help` at `0.147.0` | the installed binary | primary, pinned version |
| `codex exec resume --help` at `0.147.0` | the installed binary | primary, pinned version |
| `codex --help` at `0.147.0` | the installed binary | primary, pinned version |
| Non-interactive mode | `https://learn.chatgpt.com/docs/non-interactive-mode` | vendor documentation |
| Developer commands | `https://learn.chatgpt.com/docs/developer-commands?surface=cli` | vendor documentation |
| `codex exec --json` event cheatsheet | `https://takopi.dev/reference/runners/codex/exec-json-cheatsheet/` | third-party, unversioned |
| Exec-mode flag experiments | `https://gist.github.com/alexfazio/359c17d84cb6a5af12bac88fa1db9770` | third-party, observed at `0.114.0` |

The last two are not vendor documentation and the last was observed on an earlier version. Everything resting on them alone is marked in its requirement as requiring verification against the pinned version.

### The argument vector — fresh turn

Generated in this exact order:

```
exec
--json
--sandbox danger-full-access
--skip-git-repo-check
--model <model_id>                          # only when a model resolved
-c model_reasoning_effort="<effort_id>"     # only when an effort resolved
-
```

The trailing `-` is the positional `PROMPT` argument, and it is what directs the CLI to read the instructions from stdin.

### The argument vector — resumed turn

Generated in this exact order, and it is **not** the fresh vector with an addition:

```
exec
resume
--json
--skip-git-repo-check
-c sandbox_mode="danger-full-access"
--model <model_id>                          # only when a model resolved
-c model_reasoning_effort="<effort_id>"     # only when an effort resolved
<session_id>
-
```

`resume` is a subcommand of `exec`, taking `[SESSION_ID] [PROMPT]` positionally in that order. Its option set is a strict subset of `exec`'s: at `0.147.0` it accepts neither `-s/--sandbox`, nor `-C/--cd`, nor `-a/--ask-for-approval`, nor `--add-dir`, nor `--approve-for-me`, nor `-p/--profile`, nor `--oss`, nor `--color`. The sandbox policy therefore reaches a resumed turn as a configuration override rather than as a flag.

### The stdin payload

The serialized `AgentTaskRequest` as UTF-8 JSON, written to the container's stdin and nothing else. The trailing `-` makes it the whole of the instructions rather than context appended to a prompt. stdin is closed once the payload is written.

### The stdout event stream

JSON Lines: one JSON object per line, each carrying a `type`. The event and item vocabulary this protocol recognizes:

| `type` | Fields read |
|---|---|
| `thread.started` | `thread_id` |
| `turn.started` | — |
| `turn.completed` | `usage.input_tokens`, `usage.cached_input_tokens`, `usage.output_tokens` |
| `turn.failed` | `error.message` |
| `item.started` / `item.updated` / `item.completed` | `item.id`, `item.type`, and the fields of that item type |
| `error` | `message` |

Item types that appear: `agent_message` (`text`), `reasoning` (`text`), `command_execution` (`command`, `aggregated_output`, `exit_code`, `status`), `file_change` (`changes[]`, `status`), `mcp_tool_call` (`server`, `tool`, `arguments`, `result`, `error`, `status`), `web_search` (`query`), `todo_list` (`items[]`), and `error` (`message`).

### The extraction position

One position, and only one: the **last** `item.completed` event whose `item.type` is `agent_message`, taking `item.text` and parsing it as a JSON document.

### stderr

Diagnostics only: the configuration banner naming working directory, model, sandbox mode, and approval policy; MCP server startup messages; progress indicators; and error messages. It never carries the envelope, never carries a fragment of it, and is never merged with stdout.

### Exit statuses

| Status | Meaning |
|---|---|
| `0` | the run completed |
| `1` | the run failed — authentication failure, a required MCP server failing to initialize, or the Git repository check failing |
| `2` | an argument the CLI does not recognize |

### Authentication and configuration handoff

| Item | Form | Supplied by |
|---|---|---|
| Login directory and session state | one read/write directory mounted in the container, addressed by `CODEX_HOME` | `../core/AIC-agentic-integrations.md` AIC-FR-30 for the source and target; `../tools/EAC-execute-agent-cli.md` for the mount |

This CLI holds no credential in the application's keychain. It authenticates from `auth.json` inside `CODEX_HOME`, written by its own `codex login` outside Synthesis, and it writes session rollout files into the same directory.

## Functional requirements

1. **CDX-FR-01** The pinned version is `0.147.0`, and every element of this protocol is asserted against that version alone. A different installed version is an unverified protocol rather than a compatible one, and adopting it is a change to this spec made before the image that carries it is adopted.
2. **CDX-FR-02** The fresh-turn argument vector is exactly the ordered sequence the contract surface defines, with the conditional elements present only under their stated conditions. Order is contractual rather than incidental.
3. **CDX-FR-03** `exec` is always the first element and `--json` is always present.
4. **CDX-FR-04** A fresh turn carries `--sandbox danger-full-access`. `--dangerously-bypass-approvals-and-sandbox` is not generated in its place, so the sandbox policy remains an explicit, named value rather than a blanket bypass.
   - *Why:* The container is already the isolation boundary, and a narrower sandbox would refuse the writes a code-modifying turn depends on.
5. **CDX-FR-05** `--skip-git-repo-check` is always present, on a fresh turn and on a resumed one alike.
   - *Why:* Without it the CLI refuses to run outside a Git repository, making the outcome depend on whether the execution directory happened to be one.
6. **CDX-FR-06** `--model` is generated only when a model resolved, carrying the resolved identifier unaltered.
7. **CDX-FR-07** A resolved reasoning effort reaches the CLI as the configuration override `-c model_reasoning_effort="<effort_id>"` and never as a dedicated flag. The value is quoted.
   - *Why:* This CLI has no reasoning-effort flag, and a `-c` override's value is parsed as TOML, where an unquoted identifier would not parse as a string.
8. **CDX-FR-08** The trailing `-` positional argument is always present, and no other positional prompt is ever generated. It directs the CLI to take the instructions from stdin.
9. **CDX-FR-09** The resumed-turn argument vector is a distinct grammar rather than the fresh vector with `resume` inserted, and the two are specified and generated separately. `resume` follows `exec` as its subcommand, and the session id and the `-` prompt follow the options as positional arguments in that order.
10. **CDX-FR-10** A resumed turn carries the sandbox policy as `-c sandbox_mode="danger-full-access"` rather than as `--sandbox`. Generating `--sandbox` on a resumed turn is a rejected vector rather than a working one.
   - *Why:* `exec resume` does not accept `-s/--sandbox` at the pinned version.
11. **CDX-FR-11** No flag absent from `exec resume`'s option set is generated on a resumed turn. `-C/--cd`, `-a/--ask-for-approval`, `--add-dir`, `--approve-for-me`, `-p/--profile`, `--oss`, and `--color` are each absent from that subcommand at the pinned version, so a resumed vector carrying one is rejected by the CLI before the run starts.
12. **CDX-FR-12** The envelope is extracted from the last `item.completed` event whose `item.type` is `agent_message`, by parsing its `item.text` as a JSON document. There is exactly one extraction position, and no other event, item type, or field is ever read for an envelope or for any part of one.
13. **CDX-FR-13** A stream carrying no `item.completed` event of type `agent_message`, or one whose `item.text` does not parse as a well-formed envelope, is a protocol failure. Neither is ever reported as an agent-reported outcome.
14. **CDX-FR-14** The envelope's shape is not enforced by the CLI. `--output-schema` is not generated — it takes a filesystem path and would require mounting a schema file into the container — so nothing in this vendor's invocation validates an answer before returning it. What the agent's conformance rests on instead is the response contract the executor states in the task document (`../tools/EAC-execute-agent-cli.md` EAC-FR-35), which is where the shape of an answer is written down for every vendor — this protocol carries the response and does not define it. A result contract a task names is carried on the same terms: its schema document reaches the agent in the task document like every other part of that document (`../tools/EAC-execute-agent-cli.md` EAC-FR-43), no flag of this grammar carries it, and nothing here validates an answer against it. A malformed envelope is therefore a failure mode this vendor can reach where a validating one cannot, and it lands as the executor's `invalid_structured_output` rather than as anything this spec classifies.
15. **CDX-FR-15** A `type` of `error` in the stream is not by itself fatal: it is a diagnostic to preserve rather than a signal to abandon extraction, and what determines the outcome is whether the extraction position of CDX-FR-12 yielded an envelope.
16. **CDX-FR-16** A `turn.failed` event carrying `error.message` is a failed run rather than an agent-reported failure, and a run reporting one yields no envelope even if an earlier `agent_message` item is present in the stream.
17. **CDX-FR-17** A line in the stream that is not valid JSON, and an event carrying a `type` this protocol does not recognize, are each ignored rather than treated as a failure. What is never ignored is the absence of the extraction position of CDX-FR-12.
18. **CDX-FR-18** stderr is captured separately from stdout and never merged with it, and nothing on stderr is ever parsed for an envelope or for any part of one. It carries the configuration banner naming working directory, model, sandbox mode, and approval policy, along with MCP startup messages, progress indicators, and error messages — all worth preserving for a reader and none of it part of the answer.
19. **CDX-FR-19** Exit status `0` means the run completed, `1` means it failed, and `2` means an argument was not recognized. The `1` and `2` values are established by third-party observation at version `0.114.0` rather than by vendor documentation, so each is verified against the pinned version before the executor distinguishes on it, and until then a non-zero status is treated as a failed run without finer classification.
20. **CDX-FR-20** The session identifier is the `thread_id` carried by the `thread.started` event, which is the first event of a fresh run. The executor cannot preassign it: this CLI has no argument that supplies a session id for a new session.
21. **CDX-FR-21** A resumed turn passes the recorded identifier as `exec resume`'s `SESSION_ID` positional argument. That argument accepts a UUID or a thread name, and UUIDs take precedence where a value parses as one. That the `thread_id` reported by `thread.started` is accepted by `SESSION_ID` is established by third-party observation rather than by vendor documentation, and is verified against the pinned version before the executor relies on it.
22. **CDX-FR-22** `--ephemeral` is never generated. Resuming a session that was never persisted is documented to create a fresh session silently rather than to fail.
   - *Why:* The flag prevents session rollout files from being written, so a run generating it would report success having lost the conversation it was asked to continue.
23. **CDX-FR-23** Session state and authentication live in the same directory. This CLI reads `auth.json` from `CODEX_HOME` and writes session rollout files beneath it, so the mount that carries the login directory is also the mount that carries session state, and it is read/write rather than read-only.
   - *Why:* A read-only mount leaves the CLI unable to persist a session, making resumption unreachable, and unable to refresh an expired token.
24. **CDX-FR-24** `CODEX_HOME` in the container environment addresses that mount. No API key is placed in the environment: `CODEX_API_KEY` and `OPENAI_API_KEY` are supported by this CLI but are not the mechanism this protocol uses, the application storing no Codex credential of its own (per `../core/AIC-agentic-integrations.md` AIC-FR-20).
25. **CDX-FR-25** An authentication failure is the CLI's own non-zero exit rather than a protocol failure this spec classifies further.
26. **CDX-FR-26** `--ignore-user-config` and `--ignore-rules` are never generated, so the CLI loads `$CODEX_HOME/config.toml` and the user and project execpolicy `.rules` files, including any the mounted working tree carries. This is a property of the pinned invocation rather than an accident of it, and it is what makes the container — not the CLI — the isolation boundary the arrangement depends on.
27. **CDX-FR-27** No flag outside this protocol's grammar is generated. `--full-auto`, `--dangerously-bypass-approvals-and-sandbox`, `--dangerously-bypass-hook-trust`, `--output-schema`, `-o/--output-last-message`, `-i/--image`, `--enable`, `--disable`, `--strict-config`, `--last`, `--all`, and every other argument the CLI accepts are absent, so the generated vector is derivable from this spec alone.

28. **CDX-FR-28** Each line of the event stream is readable as one thing the agent did, for a reader watching the turn (`../tools/EAC-execute-agent-cli.md` EAC-FR-33). This CLI already writes its output as it works, so nothing about the invocation changes for it; what this requirement adds is what each event means to a reader. The vocabulary read for that purpose is: `thread.started` and `turn.started` as the turn beginning; `turn.completed` as it ending, carrying its input, cached, and output token counts; `turn.failed` and a top-level `error` as a failure carrying its message, the latter reported as what went wrong rather than as the turn ending (CDX-FR-15); and an item event read through its item, where `agent_message` is what the agent wrote, `reasoning` is its thinking, `command_execution` is a shell command with its status and exit status, `file_change` is how many paths it changed, `mcp_tool_call` names the server and the tool, `web_search` names the query, `todo_list` names how many items it holds, and an item `error` names its message. An `item.started` or `item.updated` is the same item mid-flight and is marked as unfinished. Nothing in this reading decides an outcome — extraction is CDX-FR-12's and is not affected by it — and an event or item type outside this vocabulary is reported as unread rather than dropped.

## Non-functional requirements

- Every fact in this spec traces to a source in the contract surface's table, accessed 2026-08-16. The two third-party sources are marked as such there, and every requirement resting on one alone — CDX-FR-19 and CDX-FR-21 — says in its own text that it is verified against the pinned version before being relied on.
- This spec is self-contained. It reads no other vendor's protocol, is not written to be compared with one, and holds no statement whose truth depends on what another vendor does; a reader needs nothing but this file and the sources it names to generate a correct invocation.
- Both argument vectors are derivable from this spec without running anything, which is what lets a test assert them exactly.
- The event stream carries the agent's full working transcript — every command it ran, every file it changed, every tool it called. This protocol reads one item from it, and the rest is captured output rather than protocol input, so a vendor addition to the stream is not a protocol change.
- The envelope arrives unvalidated by the CLI (CDX-FR-14), which is a standing difference in reliability between what this vendor returns and what any other might. It is a property of this protocol. The agent is told the contract in words wherever it runs, so it is never left to guess the shape of an answer, but being told is weaker than being held to it and nothing here closes that gap.
- Resumption depends on a mount that is read/write and shared with the login directory (CDX-FR-23). A container that can write session state can also write the login directory it sits beside, which is a real widening of what the container can reach and is accepted here as the cost of resumption at this pinned version.
- This protocol pins the invocation only. How long a run may take, when it is terminated, how its streams are bounded, and what is logged are all `../tools/EAC-execute-agent-cli.md`'s.
