## Intent

Replace the existing generic **AI API → Custom** provider contract with the company's OpenAI-compatible LLM Gateway contract, while keeping the existing Custom provider id and tab. The user enters a host/base URL without `/v1` and a required static bearer secret. The provider discovers models from `GET <base>/v1/models` and sends conversation calls through the existing Rig-backed Custom adapter. Rig and the existing adapter own completion routes, serialization, streaming, and response parsing.

Custom must remain a normal AI API provider. The new transport applies to every Custom-provider call, including graduation and conversational Agent turns. Keep the existing provider and model selection rules unchanged: the active project-resolved AI API provider supplies the endpoint and credentials for an Agent, while each Agent stores its model and reasoning. Do not add a provider tab, an Agentic AI integration, a separate Agent provider, or a separate Agent configuration. Do not change the graduation loop's provider/model selection rules or the Agentic AI integrations.

The gateway's model list uses standard model-list fields plus an optional `mode` field: `"chat"`, `"responses"`, or `null`/absent. `null` or absent means that both conversation routes are supported. When both routes are available, preserve the current adapter's route-selection behavior from the existing implementation; do not invent a new route preference or fallback rule in this feature.

## Users and user journey

- The author opens Global settings and selects **AI API**.
- The author opens the existing **Custom** tab.
- The author enters the host/base URL without `/v1` and the required static secret.
- The author selects **Verify**.
- The application sends `Authorization: Bearer <secret>` and retrieves the model list from `<base>/v1/models`.
- The author sees the discovered models and selects the Custom provider as the active AI API provider.
- In Global settings → Agents, the author creates or edits an Agent and selects its model from the models served by the currently active AI API provider. The Agent editor does not select a provider. Custom models do not expose a reasoning selector from model discovery and use the model default.
- The author starts a conversation with that Agent.
- The conversation uses the currently active, project-resolved AI API provider's URL and credentials, together with the Agent's stored model and reasoning.
- If the active provider changes, existing Agents use the new provider on their next conversation. An Agent whose stored model is not available from the new provider is shown as unavailable and cannot start a turn until the author selects an available model.

## Scope

In scope:

- Integrate the existing Custom provider with the project's existing Rig OpenAI-compatible adapter and add Custom Gateway model discovery from `<base>/v1/models`.
- Keep the existing Custom provider tab and provider id. Do not add a Custom Gateway tab or an authentication-mode selector.
- Store the required static secret in the application secret vault. Never store or display the secret in the user-global settings file, logs, errors, events, or UI state.
- Discover standard model records from `<base>/v1/models`, including the optional route `mode` field.
- Configure the existing Rig-backed Custom adapter with the normalized base URL and bearer secret. Keep its current Chat Completions/Responses route selection, streaming, portable tool calls, image input, retries, timeout, cancellation, and error behavior; do not duplicate its wire contract in this feature.
- Use the active AI API provider's endpoint and credentials for conversational Agents while keeping each Agent's model and reasoning in the Agent record.
- Update the AI API and Agent settings UI so the available model list comes from the current active AI API provider.
- Preserve the existing immediate-apply settings behavior and inline error presentation.

Out of scope:

- A new provider tab or a new provider id.
- Agentic AI integrations, Claude Code, OpenCode, Codex, or external agent execution.
- A separate provider or credential per Agent.
- A change to the graduation loop's provider and model selection rules.
- The old bespoke gateway routes `<root>/models` and `<root>/completions`.
- A new Custom reasoning or capability schema beyond the existing provider adapter and standard model route metadata.

## Gateway HTTP contract

### Base URL and authentication

- The entered value is an absolute HTTP or HTTPS host/base URL and must not include `/v1`.
- Remove trailing `/` characters before appending routes.
- Use `<base>/v1/models` for model discovery.
- Pass the normalized base URL to the existing Rig-backed Custom adapter for conversation calls. This feature must not construct a parallel completion client or duplicate the adapter's route paths.
- Do not use the old `<root>/models` or `<root>/completions` routes.
- Every request includes `Authorization: Bearer <secret>`.
- The secret is required. Verify must not send a network request when it is empty.
- Store the secret only in the application secret vault. Never expose it in settings storage, UI state, logs, errors, events, or request diagnostics.
- Use the existing bounded timeout and normalized error handling. A failed verification leaves the last verified Custom configuration unchanged.

### Model discovery

`GET <base>/v1/models` must return the standard model-list shape:

```json
{
  "object": "list",
  "data": [
    {
      "id": "model-id",
      "object": "model",
      "mode": "chat"
    }
  ]
}
```

- `object` must be `list`.
- `data` must be an array. Every entry must contain a string `id` and `object: "model"`.
- `mode` is optional and, when present, is `"chat"`, `"responses"`, or `null`. An absent or null value means that both routes are supported.
- Do not require a gateway-specific capability object. Do not infer reasoning levels, image support, tool support, or streaming support from the model-list response.
- Custom models use the existing Custom adapter and common conversation-loop behavior for streaming, tools, images, retries, timeouts, cancellation, and errors. Custom models expose no reasoning selector from model discovery and use the model default, preserving the settled Custom-provider behavior.
- A malformed response is a typed verification failure and does not replace the last verified configuration. A successful verification replaces the stored Custom model list only after the complete response is validated.
- The model selector shows the discovered models, supports the existing filtering and keyboard behavior, and states that the list came from the Custom gateway. Where useful, it may show the model's supported route or routes.

### Route selection

- Pass the discovered `mode` metadata to the existing Rig-backed Custom adapter.
- `mode: "chat"` restricts the adapter to Chat Completions; `mode: "responses"` restricts it to Responses; null or absent mode permits both.
- When both routes are permitted, preserve the current adapter route-selection and fallback behavior. Do not add a new preference rule or guess from the model id.
- The stored Agent record contains no route. A conversation resolves the current active project AI API provider and passes its current model metadata to the adapter.

### Conversation transport

- Use the existing Rig-backed Custom adapter and the existing common conversation-loop caller contract. Do not add a gateway-specific completion protocol.
- Preserve the adapter's current mapping for messages, portable tools, image content, reasoning, streaming, tool-call rounds, usage, retries, timeout, cancellation, empty replies, and normalized errors.
- This feature owns only Custom adapter configuration and model metadata. Rig owns completion serialization, route paths, response parsing, SSE parsing, tool-call delta assembly, image serialization, and structured provider-error parsing.

## Functional requirements

### URL and authentication

- The entered URL is an absolute gateway root URL without `/v1`.
- Remove trailing `/` characters before appending `/v1/models` for discovery and before passing the base URL to the existing Rig-backed Custom adapter.
- Every request made by discovery or the configured adapter includes `Authorization: Bearer <secret>` through the existing authentication configuration.
- The secret is required. Verify must not send a network request when it is empty.
- Use the existing bounded network timeout and normalized error handling. Do not expose the secret in any failure detail.

### Model discovery

`GET <root>/v1/models` returns the standard model-list shape defined above. Parse and validate only `object`, `data`, each model's `id` and `object`, and the optional `mode` field. Do not define or duplicate completion request or response JSON in this feature.

```json
{
  "object": "list",
  "data": [
    {
      "id": "model-id",
      "object": "model",
      "created": 0,
      "owned_by": "gateway",
      "capabilities": {
        "streaming": true,
        "tool_calls": true,
        "image_input": true,
        "reasoning": {
          "mandatory": false,
          "default_enabled": true,
          "supported_efforts": ["low", "medium", "high"],
          "default_effort": "medium"
        }
      }
    }
  ]
}
```

- `object` must be `list`.
- `data` must be an array. Every entry must contain a string `id` and `object: "model"`.
- `mode` is optional and, when present, is `"chat"`, `"responses"`, or `null`; absent or null means both routes are supported.
- Do not require a gateway-specific capability object. Rig and the existing adapter own completion capabilities and wire behavior.
- The discovered list replaces the Custom provider's stored model list only after successful verification. A malformed response is a typed verification failure and does not replace the last verified configuration.
- The model selector must show the discovered models, support the existing filter and keyboard behavior, and state that the list came from the gateway.

### Rig adapter boundary

- The existing Rig seam is the test boundary for completion calls. Tests must verify that Custom supplies the normalized base URL, bearer authentication, selected model metadata, and reasoning to the adapter, and that the existing adapter route-selection behavior is preserved.
- Do not add application-owned snapshots for completion request or response JSON, SSE chunks, tool-call deltas, image serialization, or structured provider errors. Those wire details belong to the existing Rig adapter tests.

`POST <root>/completions` uses JSON and includes:

```json
{
  "model": "model-id",
  "messages": [
    {
      "role": "user",
      "content": "text or an array of content parts"
    }
  ],
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "tool_name",
        "description": "tool description",
        "parameters": {}
      }
    }
  ],
  "tool_choice": "auto",
  "stream": true,
  "reasoning": {
    "enabled": true,
    "effort": "medium"
  }
}
```

- `model` is the Agent's selected model.
- `messages` uses the existing conversation exchange. Preserve message order and roles.
- `content` may be a string or an array of text and image parts. Image parts use the OpenAI-compatible form `{ "type": "image_url", "image_url": { "url": "data:<media-type>;base64,<bytes>" } }`.
- Omit `tools` and `tool_choice` when the request has no tools. Otherwise use the existing portable tool definitions translated to the standard function-tool shape.
- `tool_choice` supports the standard values `"auto"`, `"none"`, and a named function choice where the existing loop requires one.
- Always set `stream: true` when the existing loop requests streaming. The gateway must also accept `stream: false` for the non-streaming path.
- Omit `reasoning` when the Agent uses the model default. For `off`, send `{ "enabled": false }`. For `on`, send `{ "enabled": true }`. For an effort choice, send `{ "enabled": true, "effort": "<declared-effort>" }`.
- Do not send OpenRouter-native tool entries to the Custom Gateway. Custom uses the portable OpenAI-compatible function-tool contract only; provider-native entries remain specific to providers that declare them.

### Rig-owned completion behavior

Completion request and response shapes, streaming events, tool-call delta assembly, image serialization, reasoning serialization, usage mapping, and structured provider-error parsing belong to the existing Rig-backed Custom adapter. This feature must configure and invoke that adapter and must not define a parallel wire contract.For `stream: false`, return HTTP 200 with:

```json
{
  "id": "completion-id",
  "object": "chat.completion",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "answer or null",
        "tool_calls": [
          {
            "id": "call-id",
            "type": "function",
            "function": {
              "name": "tool_name",
              "arguments": "{\"key\":\"value\"}"
            }
          }
        ]
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 0,
    "completion_tokens": 0,
    "total_tokens": 0
  }
}
```

For `stream: true`, return `Content-Type: text/event-stream` and one JSON chunk per `data:` line, followed by `data: [DONE]`. Each chunk uses the standard delta shape:

```json
{
  "id": "completion-id",
  "object": "chat.completion.chunk",
  "choices": [
    {
      "index": 0,
      "delta": {
        "role": "assistant",
        "content": "partial text",
        "tool_calls": [
          {
            "index": 0,
            "id": "call-id",
            "type": "function",
            "function": {
              "name": "tool_name",
              "arguments": "partial-json"
            }
          }
        ]
      },
      "finish_reason": null
    }
  ],
  "usage": null
}
```

- The client must assemble streamed text and streamed tool-call deltas using the existing conversation-loop behavior.
- A response may contain assistant text, tool calls, or both according to the existing loop contract.
- `usage` is optional. When present, pass the reported token values through the existing usage-record path. Never estimate missing values.
- HTTP errors use the OpenAI-compatible shape `{ "error": { "message": string, "type": string, "param": string | null, "code": string | null } }`. Keep the existing distinction between transport failures, timeouts, rejected credentials, structured provider errors, empty replies, and tool-call failures.

## Functional requirements

- The existing Custom tab labels the URL as a gateway root and states that the secret is required. It must no longer describe the key as optional.
- Verify calls the gateway's `GET /models`, commits the URL, masked secret hint, verification timestamp, and discovered model list only on success, and leaves the previous verified configuration unchanged on failure.
- Clear removes the stored Custom configuration and secret using the existing Clear behavior.
- The active AI API provider remains selected in the AI API tab. The active provider is the provider resolved for the open project under the existing global/project override rules.
- The Agent editor removes its provider selector. It offers models and reasoning from the current active AI API provider only.
- The Agent record stores `model_id`, `reasoning`, and its existing persona fields, but does not store an AI API provider id. The provider shown in Agent lists is derived from the current active AI API provider, if the UI needs to display it.
- Creating or updating an Agent is disabled when no AI API provider is verified, and the empty state names Global settings → AI API as the place to configure one.
- Changing the active AI API provider refreshes the model and reasoning data available to Agent editors. Existing Agents are not silently rewritten. An Agent whose model is not present in the active provider's model list is retained and shown as unavailable until edited.
- A conversational Agent turn resolves the current active, project-resolved AI API provider for its URL and credential, then passes the Agent's model and reasoning to the existing conversation loop. It must not use the AI API provider's stored model or reasoning selection for the Agent.
- The Custom Gateway must use the same existing conversation-loop behavior as the other conversational providers: streaming, portable tool calls, tool-result rounds, image handling, reasoning, retries, timeout handling, cancellation, and safe error normalization.
- Capability data from `/models` controls UI and request behavior. The UI must not offer image input or reasoning choices that the selected model does not declare. Unsupported images use the existing safe-metadata and notice behavior; they must not cause a second downgraded retry.
- All settings actions remain immediate. Verification, model selection, reasoning selection, activation, and Clear show typed failures inline and do not use a save-before-close flow.
- Add offline tests for URL construction, bearer authentication and redaction, `/v1/models` parsing including absent/null/explicit `mode`, Rig adapter configuration and current route selection, active-provider Agent resolution, and an unavailable Agent model after provider switching.
- Add frontend coverage for the required-secret state, verification and failure states, model discovery, filtered model selection, removal of the Agent provider selector, provider-switch refresh, and the unavailable-model state.

## Specifications that this work must change

Update these specifications together so they no longer contradict the settled Custom-provider contract:

- `specifications/core/AAP-ai-api-integrations.md` — replace the optional-key, generic Custom contract with the required bearer secret, base URL normalization, `GET /v1/models`, the Custom model `mode` metadata, Rig adapter configuration, verification commit rules, and the existing adapter's route selection; do not duplicate completion request or response schemas here.
- `specifications/ui/AII-ai-integrations.md` — keep one Custom tab, make the secret required, describe the host/base URL without `/v1`, route-aware model discovery, no Custom reasoning selector, and the active-provider endpoint used by Agent conversations.
- `specifications/core/AGR-agent-registry.md` — remove provider ownership from Agent records and validate Agent model/reasoning against the current active AI API provider.
- `specifications/ui/AGT-agents.md` — remove the Agent provider selector, source models from the current active AI API provider, and define provider-switch and unavailable-model behavior.
- `specifications/core/AGC-agent-conversations.md` — resolve the active project AI API provider for the endpoint and credential while retaining the Agent's model and reasoning, and preserve the existing adapter route selection for Custom models.
- `specifications/core/GSS-global-settings-storage.md` — update the persisted Agent record shape and Custom provider record references, with no secret material in the user-global store.
- `specifications/ai/CVL-conversation-loop.md` — define Custom as an existing Rig-adapter integration, preserve current route selection when both modes are supported, and preserve common streaming, tool, image, retry, timeout, usage, cancellation, and error behavior without duplicating the adapter's wire contract.

Do not change the existing Agentic AI specifications or the graduation-loop provider/model selection rules. The Custom transport change applies to graduation calls, but the graduation loop must retain its existing selection rules.

Add offline tests for base-URL construction, bearer authentication and redaction, `/v1/models` parsing including absent/null/explicit `mode`, Rig adapter integration and current route selection, active-provider Agent resolution, and an unavailable Agent model after provider switching. Add frontend coverage for the required-secret state, verification and failure states, route-aware model discovery, filtered model selection, removal of the Agent provider selector, provider-switch refresh, no Custom reasoning selector, and the unavailable-model state.