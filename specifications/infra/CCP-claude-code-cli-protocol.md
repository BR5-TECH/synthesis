# Claude Code CLI protocol

**Spec code:** `CCP`

## Intent
The wire contract for driving the Claude Code CLI non-interactively: the exact argument vector, the bytes that go on stdin, the document that comes back on stdout, the exit statuses, and the session identity that lets one turn continue another. It exists because `../tools/EAC-execute-agent-cli.md` treats an agentic CLI as an instrument and needs a contract it can generate and parse without guessing, and because a vendor upgrade should be a change to one pinned protocol rather than a change to the executor. This spec is the sole authority on how Claude Code is invoked and how its answer is extracted. It describes one vendor and no other: it names no flag, output format, credential, or session mechanism belonging to any other agentic CLI, it depends on no other vendor's protocol, and nothing stated here is transferable to one — a reader must not carry any element of this grammar to a different vendor, and a change made here affects no other vendor's contract. Out of scope: launching a container, mounting anything into one, bounding a run, or normalizing a result, all of which are `../tools/EAC-execute-agent-cli.md`'s; the image this CLI is installed in and the manifest that pins it, which are `AVI-agent-vendor-images.md`'s; resolving which vendor, model, or reasoning effort a project uses, which is `../core/AIC-agentic-integrations.md`'s; and the meaning of the task and response documents themselves, which is the executor's protocol rather than the vendor's.

## Contract surface

This spec exposes no operation and registers nothing. Its product is the pinned grammar below, which `../tools/EAC-execute-agent-cli.md`'s Claude Code vendor execution descriptor (EAC-FR-10) is built from and which its tests assert against.

### The pinned version and environment

| Fact | Value |
|---|---|
| CLI | Claude Code |
| Pinned version | `2.1.233` |
| Package | `@anthropic-ai/claude-code` |
| Invoked as | the image entrypoint, per `AVI-agent-vendor-images.md` AVI-FR-04 |
| Working directory | the execution directory's own path, or `/workspace` in the fallback shape (`../tools/EAC-execute-agent-cli.md` EAC-FR-ZKMR) |
| Runs as | the image's non-root user (AVI-FR-03) |
| Network | required — the CLI reaches Anthropic's own service |

The version this protocol was established against is `2.1.233`. `AVI-agent-vendor-images.md`'s manifest (AVI-FR-07) remains the single runtime record of which version is installed, and its `cli_version` for `claude_code` and this pinned version are the same string; a manifest bump that does not match this spec is a protocol change and is resolved here before the image is adopted.

### Sources

Every requirement below traces to one of these, all accessed **2026-08-16** except the gateway page, accessed **2026-10-09**:

| Source | URL |
|---|---|
| Run Claude Code programmatically | `https://code.claude.com/docs/en/headless` |
| CLI reference | `https://code.claude.com/docs/en/cli-reference` |
| Manage sessions | `https://code.claude.com/docs/en/sessions` |
| Connect Claude Code to an LLM gateway | `https://code.claude.com/docs/en/llm-gateway-connect` |
| Get structured output from agents | `https://code.claude.com/docs/en/agent-sdk/structured-outputs` |
| `claude --help` at `2.1.233` | the installed binary |
| Direct observation at `2.1.233` | two probe runs, recorded in CCP-FR-06 and CCP-FR-11 |

### The argument vector

Generated in this exact order. No element is ever omitted except where marked conditional, and nothing else is ever appended:

```
-p
--output-format stream-json
--verbose
--json-schema <envelope-schema>
--permission-mode bypassPermissions
--model <model_id>                     # only when a model resolved
--effort <effort_id>                   # only when an effort resolved
--session-id <uuid>                    # fresh turn only
--resume <session_id>                  # resumed turn only
```

`--session-id` and `--resume` are mutually exclusive: a turn carries exactly one of them. No positional prompt argument is ever generated, in either form.

`<envelope-schema>` is the `AgentResponseEnvelope` JSON Schema as a single inline argument. It is fixed text compiled into the binary — it carries no task data, no project material, and no credential — which is what makes putting it on the argument vector compatible with `../tools/EAC-execute-agent-cli.md` EAC-FR-09.

`<effort_id>` is one of `low`, `medium`, `high`, `xhigh`, `max`.

### The stdin payload

The serialized `AgentTaskRequest` as UTF-8 JSON, written to the container's stdin and nothing else. Because no positional prompt is generated, the CLI takes the whole of stdin as the prompt. stdin is closed once the payload is written.

### The stdout event stream

JSON Lines: one JSON object per line, written as the turn happens rather than at exit. Exactly one of those lines carries `"type": "result"`, and it is the same document the single-document output format would have printed on its own.

The event types observed at `2.1.233`:

| `type` | `subtype` | What it says |
|---|---|---|
| `system` | `init` | the session, the model, the tools, and the servers this turn can reach |
| `system` | `api_retry` | the CLI is retrying its own service: the attempt, the ceiling, the delay, and the status and error that caused it |
| `assistant` | — | what the agent produced: `message.content` blocks of `text`, `thinking`, and `tool_use` |
| `user` | — | what came back: `message.content` blocks of `tool_result` |
| `result` | as below | the turn ended, and the document extraction reads |

### The result event

The line carrying `"type": "result"`. Observed at `2.1.233`, its keys are:

```
api_error_status  duration_api_ms  duration_ms  fast_mode_disabled_reason
fast_mode_state   is_error         modelUsage   num_turns
permission_denials result          session_id   stop_reason
structured_output subtype          terminal_reason time_to_request_ms
total_cost_usd    ttft_ms          ttft_stream_ms type
usage             uuid
```

The four this protocol reads:

| Field | Type | Meaning |
|---|---|---|
| `subtype` | string | `success`, `error_max_turns`, `error_during_execution`, or `error_max_structured_output_retries` |
| `is_error` | boolean | whether the run itself failed |
| `structured_output` | object | the schema-validated envelope; **absent** unless `--json-schema` was passed and validation succeeded |
| `result` | string | the final response as text; when `--json-schema` is in force this is the same document serialized as a JSON string |
| `session_id` | string (UUID) | the session this turn ran in |

### The extraction positions

The result event is found first: the **last** line of the stream whose `type` is `result`. Within it the positions are ordered, and the order is part of the contract:

1. `structured_output`, when present — used as the envelope directly, no parsing step.
2. otherwise `result`, parsed as a JSON document.

### stderr

Diagnostics only: startup warnings, invalid-flag errors, and skipped-configuration notices. It never carries the envelope, never carries a fragment of it, and is never merged with stdout.

### Exit statuses

| Status | Meaning |
|---|---|
| `0` | the run completed; the result document is on stdout |
| non-zero | the run failed; an invalid flag is reported on stderr before the run starts |
| `143` | the process received `SIGTERM` and aborted the in-progress turn |

### Authentication and configuration handoff

| Item | Form | Supplied by |
|---|---|---|
| Subscription OAuth token | `CLAUDE_CODE_OAUTH_TOKEN` in the container environment | `../core/AIC-agentic-integrations.md` AIC-FR-30 |
| Custom gateway | `CLAUDE_CODE_USE_BEDROCK=1`, `CLAUDE_CODE_SKIP_BEDROCK_AUTH=1`, `ANTHROPIC_BEDROCK_BASE_URL`, and the gateway token under the author's variable name (default `ANTHROPIC_AUTH_TOKEN`), in the container environment | `../core/AIC-agentic-integrations.md` AIC-FR-XZCS |
| Author variables | further `NAME=value` entries in the container environment, which replace a variable of the same name above | `../core/AIC-agentic-integrations.md` AIC-FR-XZCS |
| Session state | writable directory mounted in the container, addressed by `CLAUDE_CONFIG_DIR` | `../tools/EAC-execute-agent-cli.md` |

Nothing else is required. This CLI needs no configuration file mount and no login directory.

## Functional requirements

1. **CCP-FR-01** The pinned version is `2.1.233`, and every element of this protocol is asserted against that version alone. A different installed version is an unverified protocol rather than a compatible one, and adopting it is a change to this spec made before the image that carries it is adopted.
2. **CCP-FR-02** The generated argument vector is exactly the ordered sequence the contract surface defines, with the conditional elements present only under their stated conditions. Order is contractual rather than incidental.
3. **CCP-FR-03** `-p` is always present, and so is `--output-format stream-json`, which makes the answer machine-readable and makes it arrive while the turn is happening rather than only at its end (CCP-FR-25).
4. **CCP-FR-04** `--permission-mode bypassPermissions` is always present. No weaker mode is generated: `acceptEdits` auto-approves file edits and a short list of filesystem commands but leaves arbitrary shell commands needing an allow rule, which a code-modifying turn cannot complete under.
5. **CCP-FR-05** `--model` is generated only when a model resolved, and `--effort` only when an effort resolved, each carrying the resolved identifier unaltered. This CLI has a reasoning-effort argument taking one of `low`, `medium`, `high`, `xhigh`, `max`, and an effort outside that set is a resolution this protocol rejects rather than passes through.
6. **CCP-FR-06** No positional prompt argument is ever generated, and the serialized task reaches the CLI only on stdin. With `-p` present and no positional prompt, the CLI takes the whole of piped stdin as the prompt in the default `text` input format, observed directly at `2.1.233`. The vendor caps piped stdin at 10 MiB, comfortably above `../tools/EAC-execute-agent-cli.md` EAC-FR-08's 1 MiB task limit, so the cap is never the binding constraint.
7. **CCP-FR-07** `--json-schema` is always present, carrying the `AgentResponseEnvelope` JSON Schema inline. The CLI validates the final response against it and re-prompts the agent on a mismatch, so a malformed envelope is corrected inside the run rather than surfacing as a parse failure outside it. The `result` position of that schema holds whatever shape the task named there and a free-form object where it named none (CCP-FR-28). The schema states every **structural** constraint `../tools/EAC-execute-agent-cli.md` enforces on the envelope: the field set of the envelope and of each of its objects is closed exactly as the decoder closes it, and EAC-FR-20's empty `metadata` allowlist is stated as such. No structural rule may be weaker here than there. Three classes of rule are deliberately **not** transcribed, because this schema cannot express them faithfully: EAC-FR-08's byte limits, which JSON Schema can bound only in characters; EAC-FR-18's exclusivity between `outcome` and its payload; and the **readable shapes** EAC-FR-18 holds a proposed response's `summary` and `description` to, which are counts of words and sentences rather than of characters and which a pattern could only approximate. What the schema does state of those values is that each is present and not blank, which is structural and exact. Both stay with the decoder, and both remain able to refuse a turn after it ends. This schema is this vendor's **enforcement** of the response contract the executor states in words for every vendor (EAC-FR-35) rather than a second contract beside it, the two carrying the same structural rules. What this vendor adds is that the rules are checked before the answer is returned.
   - *Why:* A field the schema offered more freely than the decoder accepts is a trap: the agent fills it in, this validation passes it, and the turn is refused afterwards for doing what it was told it could do.
8. **CCP-FR-08** The schema passed to `--json-schema` is built from **fixed text compiled into the binary and nothing else**: the envelope schema, and at its `result` position the schema document of whichever result contract the task named, itself fixed text selected by name from a closed set (per `../tools/EAC-execute-agent-cli.md` EAC-FR-43). No part of it is derived from the task, composed from its `input`, or varied by anything but that one identifier; it carries no project material and no credential. Two turns naming one contract generate byte-identical schema arguments however their tasks differ.
9. **CCP-FR-09** The envelope is extracted from the result document at the first position that yields one: `structured_output` when that key is present, and otherwise `result` parsed as a JSON document. Both positions carry the same document at `2.1.233` — verified directly — so the fallback is a redundancy against the schema path being unavailable rather than a second contract.
10. **CCP-FR-10** A result document in which neither extraction position yields a well-formed envelope is a protocol failure. `structured_output` absent together with a `result` that does not parse as JSON, and a `subtype` of `error_max_structured_output_retries`, are each such a failure, and neither is ever reported as an agent-reported outcome.
11. **CCP-FR-11** `structured_output` is present only when `--json-schema` was passed and validation succeeded; it is absent from the result document otherwise. Observed directly at `2.1.233`: the same probe run without the flag produced a document carrying no `structured_output` key at all, and with the flag produced both the parsed object and its string form in `result`.
12. **CCP-FR-12** A `subtype` other than `success`, and an `is_error` of `true`, are each a failed run rather than an agent-reported failure, whatever the exit status.
   - *Why:* A failure arising inside the run — a missing credential being the documented case — is printed as the result on stdout rather than raising a non-zero exit, so exit status alone establishes nothing.
13. **CCP-FR-13** stdout carries one JSON object per line. The envelope is recovered from the last line whose `type` is `result` and from nowhere else, so a line the reader cannot parse and a line carrying an event this protocol does not know are each skipped rather than treated as a failure. Two result events is the turn ending twice, and the turn that ended last is the turn. A stream carrying no result event at all yields no envelope, however many complete events came before it. Bytes that are not valid UTF-8 remain a protocol failure.
14. **CCP-FR-14** stderr is captured separately from stdout and never merged with it, and nothing on stderr is ever parsed for an envelope or for any part of one. It carries diagnostics — startup warnings, invalid-flag errors, skipped-configuration notices — which are worth preserving for a reader and are never part of the answer.
15. **CCP-FR-15** Exit status `0` means the run completed and the result document is on stdout; a non-zero status means it did not. An invalid flag is reported on stderr before the run starts, so a rejected vector is distinguishable from a run that started and failed. Exit status `143` means the process received `SIGTERM` and aborted the in-progress turn, which is what a termination on timeout or cancellation produces.
16. **CCP-FR-16** A fresh turn carries `--session-id <uuid>` with a UUID the executor generates, so the session's identity is known before the CLI runs rather than recovered from its output afterwards. The `session_id` the result document reports back is asserted to equal the one supplied, and a mismatch is a protocol failure.
17. **CCP-FR-17** A resumed turn carries `--resume <session_id>` and no `--session-id`. Resumption restores the conversation history, tool calls, and results of the named session.
18. **CCP-FR-18** Resumption requires the session's transcript to be readable in the container, and this CLI writes transcripts to `<config-dir>/projects/<project>/<session-id>.jsonl` inside whatever filesystem it is running on. This protocol therefore requires the mount of CCP-FR-19 for resumption to be reachable at all.
   - *Why:* A container without a session-state mount destroys every session it creates.
19. **CCP-FR-19** Session state lives in a writable directory mounted into the container, and `CLAUDE_CONFIG_DIR` in the container environment addresses it. The directory is Synthesis-owned and holds transcripts rather than credentials; it carries no token, authentication reaching the container through the environment variables of CCP-FR-20 instead.
20. **CCP-FR-20** Authentication is by environment variables alone, which `../core/AIC-agentic-integrations.md`'s executor-only handoff supplies (AIC-FR-30). Subscription mode supplies `CLAUDE_CODE_OAUTH_TOKEN`. Custom gateway mode supplies `CLAUDE_CODE_USE_BEDROCK=1`, `CLAUDE_CODE_SKIP_BEDROCK_AUTH=1`, `ANTHROPIC_BEDROCK_BASE_URL`, and the gateway token under the author's variable name, and no `CLAUDE_CODE_OAUTH_TOKEN`, so the CLI sends Amazon Bedrock runtime requests to the gateway and signs none of them with AWS credentials. No mode supplies `ANTHROPIC_BASE_URL`. No configuration file, login directory, or keychain is required, and this protocol reads none of them itself.
21. **CCP-FR-21** `--bare` is never generated.
   - *Why:* Bare mode authenticates strictly by `ANTHROPIC_API_KEY` or an `apiKeyHelper` and never reads OAuth credentials, so it is incompatible with the subscription handoff of CCP-FR-20.
22. **CCP-FR-22** Because `--bare` is absent and `-p` skips the workspace trust dialog, the CLI loads configuration from the mounted working tree: hooks declared in the tree's `.claude/settings.json` run, and MCP servers declared in its `.mcp.json` connect, with no trust prompt. This is a property of the pinned invocation rather than an accident of it, and it is what makes the container — not the CLI — the isolation boundary.
23. **CCP-FR-23** No flag outside this protocol's grammar is generated. `--continue`, `--fork-session`, `--no-session-persistence`, `--dangerously-skip-permissions`, `--max-turns`, `--input-format`, `--add-dir`, `--append-system-prompt`, `--agents`, `--mcp-config`, `--settings`, and every other flag the CLI accepts are absent, so the generated vector is derivable from this spec alone.
24. **CCP-FR-24** A background Bash task the agent starts is terminated by the CLI about five seconds after the final result, and a background subagent is waited for up to a default ten-minute ceiling. Both are the vendor's own behavior inside the run and neither is something this protocol configures, but each can extend the wall-clock time between the agent answering and the process exiting.

25. **CCP-FR-25** The output format is the streaming one rather than the single-document one, and this is a requirement rather than a preference. Nothing is given up for it: the final event of the stream is the same result document the single format would have printed, carrying the same keys at the same positions, so CCP-FR-09's extraction is unchanged.
   - *Why:* The single-document format writes nothing until the process exits, which is exactly the silence `../tools/EAC-execute-agent-cli.md` EAC-FR-32 exists to prevent.
26. **CCP-FR-26** `--verbose` is always present, the pinned CLI refusing `--print` together with `--output-format stream-json` without it. Verified directly at `2.1.233`: the same invocation without the flag answers `When using --print, --output-format=stream-json requires --verbose` and exits without running. It is a precondition of the format rather than a request for more output.
27. **CCP-FR-27** Each line of the event stream is readable as one thing the agent did, for a reader watching the turn (`../tools/EAC-execute-agent-cli.md` EAC-FR-33). The vocabulary this protocol reads for that purpose is: `system`/`init` as the turn starting, naming the model and how many tools it can reach; `system`/`api_retry` as a retry, naming the attempt, the ceiling, the status, and the error — the single most useful line this CLI writes when a run is going nowhere, because a wrong or expired credential is a series of these and then a failure, and without them the whole delay looks like thinking; an `assistant` or `user` event read through its content blocks, where `text` is what the agent wrote, `thinking` is its reasoning, `tool_use` is a tool call naming the tool and what it is about, and `tool_result` is what came back and whether it failed; and the `result` event as the turn ending, carrying its subtype, its turn count, and its duration, and read as a failure where `is_error` is true. Nothing in this reading decides an outcome — extraction is CCP-FR-09's and is not affected by it — and an event outside this vocabulary is reported as unread rather than dropped.
28. **CCP-FR-28** Where the task names a result contract, that contract's schema document stands at the envelope schema's `result` position beside the null that position already admits, and nothing else about the envelope schema changes. The null stays because it is the **envelope's** rule rather than the contract's — an outcome other than `success` carries no result — and a position that dropped it would refuse a correctly reported failure inside the run, leaving the agent no shape to move to but an invented verdict. The CLI then validates the whole answer — the envelope and the result body together — and re-prompts the agent on a mismatch, so an answer written to the wrong shape is corrected **inside** the turn rather than refused by the caller after the turn is paid for. This is what the position is for: the agent works with a tool set of its own, and where one of those tools takes findings of its own shape, an unenforced `result` invites an answer written in that tool's field names — a whole turn's work refused for a renamed field the agent was never told about. The rule of CCP-FR-07 holds at this position exactly as it holds above it: the document states no constraint the caller's own validation does not enforce and none more freely than it does, and a rule the document cannot express faithfully stays with the caller (per `../tools/EAC-execute-agent-cli.md` EAC-FR-44) rather than being approximated here. Where the task names no contract the position holds a free-form object, and the schema is the envelope's alone.

## Non-functional requirements

- Every fact in this spec traces to a source in the contract surface's table, accessed on the date it states, or to a probe run recorded in CCP-FR-06, CCP-FR-09, or CCP-FR-11. Behavior established only by observation is marked as observed rather than presented as documented.
- This spec is self-contained. It reads no other vendor's protocol, is not written to be compared with one, and holds no statement whose truth depends on what another vendor does; a reader needs nothing but this file and the sources it names to generate a correct invocation.
- The complete argument vector is derivable from this spec without running anything, which is what lets a test assert it exactly.
- The full result document carries cost, token usage, and per-model accounting. This protocol reads none of it beyond the five fields the contract surface names, so a vendor addition to the document is not a protocol change.
- This protocol pins the invocation only. How long a run may take, when it is terminated, how its streams are bounded, and what is logged are all `../tools/EAC-execute-agent-cli.md`'s.
