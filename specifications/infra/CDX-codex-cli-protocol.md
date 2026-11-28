# Codex CLI protocol

**Spec code:** `CDX`

## Intent
The wire contract for driving the Codex CLI non-interactively: the exact argument vector for a fresh turn and for a resumed one, the bytes that go on stdin, the files the CLI reads, the event stream that comes back on stdout, the exit statuses, the thread identity that lets one turn continue another, the two ways the CLI authenticates, and the models and reasoning-effort levels it offers. It exists because `../tools/EAC-execute-agent-cli.md` treats an agentic CLI as an instrument and needs a contract it can generate and parse without guessing, and because a vendor upgrade should be a change to one pinned protocol rather than a change to the executor. This spec is the sole authority on how Codex is invoked and how its answer is extracted. It describes one vendor and no other: it names no flag, output format, credential, or session mechanism belonging to any other agentic CLI, it depends on no other vendor's protocol, and nothing stated here is transferable to one — a reader must not carry any element of this grammar to a different vendor, and a change made here affects no other vendor's contract. Out of scope: launching a container, mounting anything into one, writing the files the CLI reads, bounding a run, or normalizing a result, all of which are `../tools/EAC-execute-agent-cli.md`'s; the image this CLI is installed in and the manifest that pins it, which are `AVI-agent-vendor-images.md`'s; resolving which vendor, model, reasoning effort, or authentication mode a project uses, which is `../core/AIC-agentic-integrations.md`'s; and the meaning of the task and response documents themselves, which is the executor's protocol rather than the vendor's.

## Functional requirements

1. **CDX-FR-01** Observation of version `0.172.0` establishes the elements of this protocol. That version names the source of the grammar. It is not a pin, and it restricts no image. An installed version that behaves differently is a change to this spec. An installed version that behaves the same needs no change here.
2. **CDX-FR-02** The fresh-turn argument vector is exactly the ordered sequence the contract surface defines, with the conditional elements present only under their stated conditions. Order is contractual rather than incidental.
3. **CDX-FR-03** `exec` is always the first element and `--json` is always present.
4. **CDX-FR-04** A fresh turn and a resumed turn each carry `--dangerously-bypass-approvals-and-sandbox`. The launch generates no `--sandbox`, no `sandbox_mode` override, and no `approval_policy` override.
   - *Why:* The container is the isolation boundary, and the flag removes every approval request, which a non-interactive turn cannot answer.
5. **CDX-FR-05** `--skip-git-repo-check` is always present, on a fresh turn and on a resumed one alike.
   - *Why:* Without it the CLI refuses to run outside a Git repository, making the outcome depend on whether the execution directory happened to be one.
6. **CDX-FR-06** `--model` is generated only when a model resolved, carrying the resolved identifier unaltered.
7. **CDX-FR-07** A resolved reasoning effort reaches the CLI as the configuration override `-c model_reasoning_effort="<effort_id>"` and never as a dedicated flag. The value is quoted.
   - *Why:* This CLI has no reasoning-effort flag, and a `-c` override's value is parsed as TOML, where an unquoted identifier would not parse as a string.
8. **CDX-FR-08** The trailing `-` positional argument is always present and is always the last element, also when the schema option or the milestone reminder is present. No other positional prompt is generated. The `-` directs the CLI to take the instructions from stdin.
9. **CDX-FR-09** The resumed-turn argument vector is a distinct grammar rather than the fresh vector with `resume` inserted, and the two are specified and generated separately. `resume` follows `exec` as its subcommand, and the session id and the `-` prompt follow the options as positional arguments in that order.
10. **CDX-FR-10** A resumed turn carries the same conditional elements as a fresh turn, under the same conditions: the schema option, the milestone reminder, the model, and the effort. They stand after `--skip-git-repo-check` and before the session id, in the order of the contract surface.
11. **CDX-FR-11** No flag absent from `exec resume`'s option set is generated on a resumed turn. At the observed version, `-s/--sandbox`, `-C/--cd`, `--add-dir`, `--approve-for-me`, `-p/--profile`, `--oss`, `--local-provider`, and `--color` are absent from that subcommand. The CLI rejects a resumed vector that carries one before the run starts.
12. **CDX-FR-12** The envelope is extracted from the last `item.completed` event whose `item.type` is `agent_message`, by parsing its `item.text` as a JSON document. There is exactly one extraction position, and no other event, item type, or field is ever read for an envelope or for any part of one.
13. **CDX-FR-13** A stream carrying no `item.completed` event of type `agent_message`, or one whose `item.text` does not parse as a well-formed envelope, is a protocol failure. Neither is ever reported as an agent-reported outcome.
14. **CDX-FR-14** A turn whose task names a result contract carries `--output-schema /opt/synthesis/vendor/output-schema-<contract>.json`, where `<contract>` is the contract identifier. That file holds the strict schema of CDX-FR-XIGW for the contract. A turn that names no contract carries no `--output-schema`. The task document states the response contract to every turn (`../tools/EAC-execute-agent-cli.md` EAC-FR-35, EAC-FR-43).
    - *Why:* The CLI sends the schema as strict structured output, which refuses the free-form `result` of a turn with no contract (observed).
15. **CDX-FR-XIGW** The strict schema is the envelope schema with the contract at `result` as `anyOf` of `null` and the contract, changed by the strict-dialect rules of the contract surface. The CLI refuses a schema that breaks a rule of strict structured output before the agent runs, as a `turn.failed` event (observed).
16. **CDX-FR-BDZW** In the strict schema, `escalation.kind` carries a `description`. The description says that the value is `null` for an escalation that asks a question, and `plan_update` only for a plan update.
    - *Why:* Observed: without the description, the agent writes `plan_update` into the nullable field for an escalation that asks a question.
17. **CDX-FR-TPFM** A `null` that the agent writes at a position the original schema marks as optional means that the field is absent. The strict schema lists every field as required, so a `null` is the only way the agent can leave out an optional field.
18. **CDX-FR-15** An event whose `type` is `error`, and an `item.completed` event whose item type is `error`, are not fatal alone. Each is a diagnostic to keep. What decides the outcome is whether the extraction position of CDX-FR-12 yielded an envelope. At the observed version, the bypass flags of CDX-FR-04 and CDX-FR-OQVA each make the CLI write such an item (observed).
19. **CDX-FR-16** A `turn.failed` event carrying `error.message` is a failed run rather than an agent-reported failure, and a run reporting one yields no envelope even if an earlier `agent_message` item is present in the stream.
20. **CDX-FR-17** A line in the stream that is not valid JSON, and an event carrying a `type` this protocol does not recognize, are each ignored rather than treated as a failure. What is never ignored is the absence of the extraction position of CDX-FR-12.
21. **CDX-FR-18** stderr is captured separately from stdout and never merged with it, and nothing on stderr is ever parsed for an envelope or for any part of one. It carries the configuration banner naming working directory, model, sandbox mode, and approval policy, along with MCP startup messages, progress indicators, and error messages — all worth preserving for a reader and none of it part of the answer.
22. **CDX-FR-19** Exit status `0` means that the run completed. A non-zero status means that the run failed. Status `2` means that an argument was not recognized (observed). The executor does not classify a non-zero status more finely.
23. **CDX-FR-20** The session identifier is the `thread_id` carried by the `thread.started` event, which is the first event of a fresh run. The executor cannot preassign it: this CLI has no argument that supplies a session id for a new session.
24. **CDX-FR-21** A resumed turn passes the recorded identifier as `exec resume`'s `SESSION_ID` positional argument. That argument takes a UUID or a thread name, and a value that parses as a UUID is read as one. `SESSION_ID` accepts the `thread_id` that `thread.started` reports (observed).
25. **CDX-FR-22** `--ephemeral` is never generated. Resuming a session that was never persisted is documented to create a fresh session silently rather than to fail.
    - *Why:* The flag prevents session rollout files from being written, so a run generating it would report success having lost the conversation it was asked to continue.
26. **CDX-FR-HVLL** The CLI authenticates in one of two modes, `login_folder` and `access_token`. `../core/AIC-agentic-integrations.md` decides the mode of a launch. The mode decides the credential and the session-state directory of the authentication handoff table. It changes nothing else in the argument vector, the stdin payload, or the stream.
27. **CDX-FR-23** In the `login_folder` mode, session state and authentication live in the same directory. The CLI reads `auth.json` from `CODEX_HOME` and writes session rollout files below it. So the mount that carries the login directory also carries session state, and it is read/write.
    - *Why:* A read-only mount leaves the CLI unable to persist a session, making resumption unreachable, and unable to refresh an expired token.
28. **CDX-FR-EUBZ** In the `access_token` mode, the container environment holds `CODEX_ACCESS_TOKEN`, a Codex access token of a ChatGPT Business or Enterprise workspace. `CODEX_HOME` addresses a dedicated read/write directory, and no login directory is mounted. This mode rests on vendor documentation. It is verified against the observed version before a release depends on it.
    - *Why:* In `exec`, `CODEX_ACCESS_TOKEN` comes before a stored `auth.json`, so the token alone authenticates the turn.
29. **CDX-FR-24** `CODEX_HOME=/home/agent/.codex` in the container environment addresses the session-state directory in both modes. The launch never sets `CODEX_API_KEY` or `OPENAI_API_KEY`.
    - *Why:* `CODEX_API_KEY` comes first in the authentication order of `exec`, so a key in the environment would replace the mode that `../core/AIC-agentic-integrations.md` selects.
30. **CDX-FR-25** An authentication failure is the CLI's own non-zero exit rather than a protocol failure this spec classifies further.
31. **CDX-FR-26** `--ignore-user-config` and `--ignore-rules` are never generated, so the CLI loads `$CODEX_HOME/config.toml` and the user and project execpolicy `.rules` files, including any the mounted working tree carries. This is a property of the pinned invocation rather than an accident of it, and it is what makes the container — not the CLI — the isolation boundary the arrangement depends on.
32. **CDX-FR-LJBK** The launch generates no trust setting: no `--trust`, and no `projects.<path>.trust_level` override. At the observed version, `exec` reads the `AGENTS.md` of a project that is not trusted, also in a linked worktree (observed).
    - *Why:* `--trust` writes into `$CODEX_HOME/config.toml`, which in the `login_folder` mode is the author's own file, and the CLI ignores a `projects` override (observed).
33. **CDX-FR-27** No flag outside this protocol's grammar is generated. `--trust`, `--ephemeral`, `-s/--sandbox`, `--approve-for-me`, `-o/--output-last-message`, `-i/--image`, `--enable`, `--disable`, `--strict-config`, `--worktree`, `--remote`, `--last`, `--all`, and every other argument the CLI accepts are absent. The generated vector is derivable from this spec alone.
34. **CDX-FR-OQVA** A launch that carries the milestone reminder (`../tools/EAC-execute-agent-cli.md` EAC-FR-WBIF) carries `--dangerously-bypass-hook-trust` and the `hooks.PreToolUse` override of the contract surface, in a fresh turn and in a resumed turn. A launch without the reminder carries neither element.
    - *Why:* Observed: a `PreToolUse` hook's `additionalContext` reaches the model in `exec`, a `PostToolUse` hook's does not, and an override hook without stored trust runs only with the bypass flag.
35. **CDX-FR-WJZY** The reminder hook's command prints `/opt/synthesis/vendor/milestone-reminder.json`. That file holds one fixed object, `{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"<reminder>"}}`, where `<reminder>` is the fixed reminder text of the contract surface. The file holds nothing that the task supplies.
36. **CDX-FR-RJPF** The descriptor's final event is a stdout line whose `type` is `turn.completed` or `turn.failed`. At the observed version, that line is the last line that a turn writes (observed).
37. **CDX-FR-SDOV** The CLI accepts six reasoning-effort identifiers: `low`, `medium`, `high`, `xhigh`, `max`, and `ultra`. They are the levels of the contract surface's effort table, in that order.
38. **CDX-FR-OOCJ** Each model declares its own ladder of effort levels, which is a subset of the six of CDX-FR-SDOV. The CLI accepts a level that the model does not declare and reports no error (observed: `gpt-6-luna` with `ultra`). So the CLI never refuses an unsupported level, and a caller that needs the ladder enforces it.
39. **CDX-FR-BQNL** `codex debug models` prints the model catalog as one JSON document `{"models":[...]}` on stdout and exits `0`, also with an empty `CODEX_HOME` (observed). The fields read are those of the contract surface's model-listing table. A model whose `visibility` is not `list` is not offered.
40. **CDX-FR-28** Each line of the event stream is readable as one thing the agent did, for a reader watching the turn (`../tools/EAC-execute-agent-cli.md` EAC-FR-33). The reading uses the activity vocabulary of the contract surface. Nothing in this reading decides an outcome, and an event or item type outside that vocabulary is reported as unread rather than dropped.

## Contract surface

This spec exposes no operation and registers nothing. Its product is the pinned grammar below, which `../tools/EAC-execute-agent-cli.md`'s Codex vendor execution descriptor (EAC-FR-10) is built from and which its tests assert against.

### The observed version and environment

| Fact | Value |
|---|---|
| CLI | Codex CLI |
| Observed version | `0.172.0`, reported as `codex-cli 0.172.0` |
| Package | `@openai/codex` |
| Invoked as | the image entrypoint, per `AVI-agent-vendor-images.md` AVI-FR-04 |
| Working directory | the execution directory's own path, or `/workspace` in the fallback shape (`../tools/EAC-execute-agent-cli.md` EAC-FR-ZKMR) |
| Runs as | the image's non-root user (AVI-FR-03) |
| Network | required — the CLI reaches OpenAI's own service |

The version this protocol was established against is `0.172.0`. The version an image installs is the choice of its Dockerfile's author (`AVI-agent-vendor-images.md` AVI-FR-QWVT), and `AVI-agent-vendor-images.md`'s manifest (AVI-FR-07) remains the single record of the version a shipped image holds. Neither has to equal the version above.

### Sources

Every requirement below traces to one of these, all accessed **2026-11-28**:

| Source | URL | Standing |
|---|---|---|
| `codex exec --help` at `0.172.0` | the installed binary | primary, observed version |
| `codex exec resume --help` at `0.172.0` | the installed binary | primary, observed version |
| `codex debug models --help` and its output at `0.172.0` | the installed binary | primary, observed version |
| Turns run against `0.172.0` with a ChatGPT login | the installed binary | primary, observed version |
| `codex-rs/exec/src/exec_events.rs` at `rust-v0.172.0` | `https://github.com/openai/codex` | vendor source code |
| Non-interactive mode | `https://learn.chatgpt.com/docs/non-interactive-mode` | vendor documentation |
| Developer commands | `https://learn.chatgpt.com/docs/developer-commands?surface=cli` | vendor documentation |
| Authentication | `https://learn.chatgpt.com/docs/auth` | vendor documentation |
| Codex access tokens | `https://learn.chatgpt.com/docs/enterprise/access-tokens` | vendor documentation |
| Hooks | `https://learn.chatgpt.com/docs/hooks` | vendor documentation |
| Configuration reference | `https://learn.chatgpt.com/docs/config-file/config-reference` | vendor documentation |
| Models | `https://learn.chatgpt.com/docs/models` | vendor documentation |

A requirement that rests on a turn run against the observed version says "(observed)". The `access_token` mode rests on vendor documentation alone, because no Codex access token was available to observe it.

### The argument vector — fresh turn

Generated in this exact order:

```
exec
--json
--dangerously-bypass-approvals-and-sandbox
--skip-git-repo-check
--output-schema /opt/synthesis/vendor/output-schema-<contract>.json      # only when the task names a result contract
--dangerously-bypass-hook-trust                                # only when the launch carries the milestone reminder
-c hooks.PreToolUse=<reminder-hook>                            # only when the launch carries the milestone reminder
--model <model_id>                                             # only when a model resolved
-c model_reasoning_effort="<effort_id>"                        # only when an effort resolved
-
```

`<reminder-hook>` is this one TOML value:

```
[{matcher="*",hooks=[{type="command",command="cat /opt/synthesis/vendor/milestone-reminder.json"}]}]
```

The trailing `-` is the positional `PROMPT` argument, and it is what directs the CLI to read the instructions from stdin.

### The argument vector — resumed turn

Generated in this exact order, and it is **not** the fresh vector with an addition:

```
exec
resume
--json
--dangerously-bypass-approvals-and-sandbox
--skip-git-repo-check
--output-schema /opt/synthesis/vendor/output-schema-<contract>.json      # only when the task names a result contract
--dangerously-bypass-hook-trust                                # only when the launch carries the milestone reminder
-c hooks.PreToolUse=<reminder-hook>                            # only when the launch carries the milestone reminder
--model <model_id>                                             # only when a model resolved
-c model_reasoning_effort="<effort_id>"                        # only when an effort resolved
<session_id>
-
```

`resume` is a subcommand of `exec`, taking `[SESSION_ID] [PROMPT]` positionally in that order. Its option set is a strict subset of `exec`'s (CDX-FR-11).

### The files the CLI reads

| Path in the container | Content | Present when |
|---|---|---|
| `/opt/synthesis/vendor/output-schema-review_verdict.json` | the strict schema of CDX-FR-XIGW for `review_verdict` | every launch |
| `/opt/synthesis/vendor/output-schema-graduation_verdict.json` | the strict schema of CDX-FR-XIGW for `graduation_verdict` | every launch |
| `/opt/synthesis/vendor/output-schema-pull_request_summary.json` | the strict schema of CDX-FR-XIGW for `pull_request_summary` | every launch |
| `/opt/synthesis/vendor/output-schema-work_result.json` | the strict schema of CDX-FR-XIGW for `work_result` | every launch |
| `/opt/synthesis/vendor/milestone-reminder.json` | the fixed hook output of CDX-FR-WJZY | every launch |

The directory holds the same files for every Codex launch of one build. A turn names only the files it uses: the schema of its own contract, and the reminder file only when it carries the milestone reminder. The directory is read-only in the container. Writing and mounting it is `../tools/EAC-execute-agent-cli.md`'s.

### The strict dialect

The strict schema of CDX-FR-XIGW applies these rules to every schema object of the envelope and of the contract:

| Rule | Effect |
|---|---|
| Every property is required | an object's `required` lists every key of its `properties` |
| An optional property becomes nullable | `null` is added to its `type`, and to its `enum` where it has one; a property with no `type` becomes `anyOf` of itself and `{"type":"null"}` |
| Every object is closed | every object has `additionalProperties: false` |
| `oneOf` becomes `anyOf` | the branches do not change |
| Unsupported keywords are dropped | `if`, `then`, `else`, `maxProperties`, and `$schema` are removed |
| Every array declares its items | an array with no `items` gets `items: {"type":"null"}` |
| `metadata` holds nothing | `metadata` is `{"type":"null"}` |
| `result` holds the contract | `result` is `{"anyOf":[{"type":"null"}, <contract>]}` |

The rule that a question escalation asks at least one question, and the rule that `metadata` holds no key, leave the strict schema. The executor's own validation still applies them after the turn.

### The fixed reminder text

```
Milestone reminder: if you are now in a different milestone, write its start marker in your next text message. A start marker marks every earlier milestone done.
```

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

Item types that appear: `agent_message` (`text`), `reasoning` (`text`), `command_execution` (`command`, `aggregated_output`, `exit_code`, `status`), `file_change` (`changes[]`, `status`), `mcp_tool_call` (`server`, `tool`, `arguments`, `result`, `error`, `status`), `collab_tool_call` (`tool`, `receiver_thread_ids`, `status`), `web_search` (`query`), `todo_list` (`items[]`), and `error` (`message`).

### The activity vocabulary

| Event or item | What a reader sees |
|---|---|
| `thread.started`, `turn.started` | the turn beginning |
| `turn.completed` | the turn ending, with its input, cached, and output token counts |
| `turn.failed` | the turn failing, with its message |
| top-level `error` | what went wrong, not the turn ending (CDX-FR-15) |
| `agent_message` | what the agent wrote |
| `reasoning` | the agent's thinking |
| `command_execution` | a shell command, with its status and exit status |
| `file_change` | how many paths the agent changed |
| `mcp_tool_call` | the server and the tool |
| `collab_tool_call` | the collaboration tool and how many agents it reaches |
| `web_search` | the query |
| `todo_list` | how many items the list holds |
| item `error` | its message |

An `item.started` or `item.updated` is the same item mid-flight, and a reader sees it as unfinished.

### The extraction position

One position, and only one: the **last** `item.completed` event whose `item.type` is `agent_message`, taking `item.text` and parsing it as a JSON document.

### stderr

Diagnostics only: the configuration banner naming working directory, model, sandbox mode, and approval policy; MCP server startup messages; progress indicators; and error messages. It never carries the envelope, never carries a fragment of it, and is never merged with stdout.

### Exit statuses

| Status | Meaning |
|---|---|
| `0` | the run completed |
| `2` | an argument the CLI does not recognize (observed) |
| any other non-zero | the run failed |

### Authentication and configuration handoff

| Mode | Credential | Session state | Supplied by |
|---|---|---|---|
| `login_folder` | `auth.json` in the host login directory, written by `codex login` outside Synthesis | the same directory, mounted read/write at `/home/agent/.codex` | `../core/AIC-agentic-integrations.md` AIC-FR-30 for the source and target; `../tools/EAC-execute-agent-cli.md` for the mount |
| `access_token` | `CODEX_ACCESS_TOKEN` in the container environment | a dedicated directory, mounted read/write at `/home/agent/.codex` | `../core/AIC-agentic-integrations.md` for the token; `../tools/EAC-execute-agent-cli.md` for the directory and the environment |

In both modes, `CODEX_HOME=/home/agent/.codex`. The authentication order of `exec` is `CODEX_API_KEY`, then external ephemeral tokens, then `CODEX_ACCESS_TOKEN`, then the stored `auth.json`.

### Reasoning-effort levels

| Identifier | Label |
|---|---|
| `low` | Low |
| `medium` | Medium |
| `high` | High |
| `xhigh` | Extra high |
| `max` | Max |
| `ultra` | Ultra |

### Model listing

`codex debug models` writes `{"models":[...]}`. The fields read from each model:

| Field | Meaning |
|---|---|
| `slug` | the model identifier that `--model` takes |
| `display_name` | the label |
| `visibility` | `list` for a model that is offered, `hide` for one that is not |
| `default_reasoning_level` | the level the model uses when no effort resolved |
| `supported_reasoning_levels[].effort` | the model's ladder |

Each model also carries large fields this protocol does not read, such as its instruction template. The bundled catalog at the observed version, without the hidden models:

| `slug` | `display_name` | Default level | Ladder |
|---|---|---|---|
| `gpt-6.2-sol` | GPT-6.2-Sol | `low` | `low` to `ultra` |
| `gpt-6.2-astra` | GPT-6.2-Astra | `low` | `low` to `ultra` |
| `gpt-6-astra` | GPT-6-Astra | `low` | `low` to `ultra` |
| `gpt-6.1-sol` | GPT-6.1-Sol | `low` | `low` to `ultra` |
| `gpt-6.1-terra` | GPT-6.1 Terra | `low` | `low` to `ultra` |
| `gpt-6-sol` | GPT-6-Sol | `medium` | `low` to `ultra` |
| `gpt-6-luna` | GPT-6-Luna | `medium` | `low` to `max` |
| `gpt-5.6-sol` | GPT-5.6-Sol | `low` | `low` to `ultra` |
| `gpt-5.6-terra` | GPT-5.6-Terra | `medium` | `low` to `ultra` |
| `gpt-5.6-luna` | GPT-5.6-Luna | `medium` | `low` to `max` |
| `gpt-5.5` | GPT-5.5 | `medium` | `low` to `xhigh` |

The hidden models at the observed version are `gpt-daybreak-blue-latest`, `gpt-daybreak-red-latest`, and `codex-auto-review`.

## Non-functional requirements

- Every fact in this spec traces to a source in the contract surface's table, accessed 2026-11-28. A fact that rests on a turn run against the observed version is marked as observed. The `access_token` mode (CDX-FR-EUBZ) rests on vendor documentation alone.
- This spec is self-contained. It reads no other vendor's protocol, is not written to be compared with one, and holds no statement whose truth depends on what another vendor does; a reader needs nothing but this file and the sources it names to generate a correct invocation.
- Both argument vectors are derivable from this spec without running anything, which is what lets a test assert them exactly.
- The event stream carries the agent's full working transcript — every command it ran, every file it changed, every tool it called. This protocol reads one item from it, and the rest is captured output rather than protocol input, so a vendor addition to the stream is not a protocol change.
- The CLI holds the answer to the strict schema only on a turn that names a result contract (CDX-FR-14). A turn with no contract returns an envelope that the CLI did not validate. The strict schema also leaves out two rules of the envelope, which the executor checks only after the turn.
- In the `login_folder` mode, resumption depends on a mount that is read/write and shared with the login directory (CDX-FR-23). A container that can write session state can also write the login directory it sits beside. This widening of what the container can reach is the cost of resumption in that mode. The `access_token` mode mounts no login directory.
- The bypass flags of CDX-FR-04 and CDX-FR-OQVA give the agent no sandbox of the CLI's own and run hooks without stored trust. The container is the only boundary around the turn.
- This protocol pins the invocation only. How long a run may take, when it is terminated, how its streams are bounded, and what is logged are all `../tools/EAC-execute-agent-cli.md`'s.
