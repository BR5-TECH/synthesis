## Intent

Local Models should be usable for the IDE. Today I’m able to configure local Ollama with some local LLM, and use it via Custom AI API integration. It works, but I believe we need to add additional configuration options to better support capabilities of local models.

Scope: the **Custom** tab of the AI API level (Settings → AI API → Custom), the Rig-backed Custom adapter, the conversation loop, and the reasoning selector of the persona editor. Users are developers who run a local OpenAI-compatible host (Ollama first; LM Studio and others work as generic Custom hosts without detection). Out of scope: discovery of local services on the machine (the user always enters the host), sending `num_ctx` or any other limit to the host, and detection for hosts other than Ollama.

## User journey

- User opens the settings window, then the AI API section, then the Custom tab.
- User enters the host (and optional path prefix) and a secret. The tab states that for local deployments such as Ollama any secret value can be used, because the secret is required.
- User activates Verify. The IDE calls `GET <base>/v1/models` and shows the model list. Then, for every model in the list, it calls Ollama `POST <base>/api/show`. Where that works, the host is treated as Ollama and the IDE stores the detected context size and capabilities on each model. Where it fails, the model has no detected values, and Verify is not affected.
- The tab shows the detected context size and capabilities of the selected model, when present.
- User sets provider-level options in the Custom tab: default reasoning effort, max output tokens, max context size. These are manual defaults for every Custom model.
- In the persona editor, the user can set reasoning effort for one agent. It overrides the provider default for that agent only.
- Conversational agents on the Custom provider pass the effective reasoning and output limit to Rig, and use the effective context size as the conversation loop's token budget.

## Requirements

- User must able to specify a host and path prefix, /v1 will be appended on top of what user defines.
- The secret stays required (see the Custom gateway draft). Verify with an empty secret sends no request. No "no key" switch is added.

### Provider-level settings

- The Custom tab renders three optional fields: **Reasoning effort** (default), **Max output tokens**, **Max context size**. They belong to the Custom provider record, not to a model. Each is unset by default. Token fields accept positive integers only, with inline validation. Saving follows the existing reasoning-selector pattern (applies at once, no Verify needed).
- The reasoning selector offers the model-default entry (unset: no `reasoning` parameter is sent), then `none`, `low`, `medium`, `high`, for every Custom model. Detected capabilities do not hide or limit it.
- The provider's reasoning effort is a default. An agent's own reasoning choice in the persona editor wins over it. The persona editor shows the same selector whenever the active provider is Custom. Max output tokens and max context size cannot be overridden by an agent.

### Detection (Ollama)

- Detection runs at Verify and at every model-list refresh, only after `/v1/models` succeeded, and for every model in the list. It sends the same bearer secret, uses the bounded network timeout, and limits concurrent calls.
- On success, read the context length from `model_info.<architecture>.context_length` and the `capabilities` list (when present). Store both with the model. Replace them at each run.
- Any `/api/show` failure (404, network, bad body) means "no detected values" for that model. It is never shown as an error, never fails Verify, and never changes the model list. A failed Verify keeps the last verified configuration, as today.
- `/api/show` gives no max output tokens, so none is detected.
- The secret is never logged or put in an error.

### Effective values

- **Context size** for a model: the detected value; else the provider-level value; else the conversation loop's existing default token budget. The value is app-side only: the conversation loop uses it as its token budget (trim or compact history). Nothing is sent to the host.
- **Max output tokens**: the provider-level value when set, replacing the 128000 constant for every Custom call on both routes; 128000 when unset.
- **Reasoning**: agent choice, else provider default, else unset. A set value is passed to the Rig adapter as the call's reasoning. This replaces the rule that a Custom call carries no reasoning parameter.
- The graduation loop uses the provider-level reasoning default, as it uses the stored reasoning of other providers.

## Specifications to change

- `specifications/ui/AII-ai-integrations.md`: Custom tab controls and order (AII-FR-05), the three new fields, the required-secret hint for local hosts (AII-FR-08), detected values display, and reasoning for Custom (AII-FR-41 now says Custom renders none).
- `specifications/core/AAP-ai-api-integrations.md`: Custom record holds the three provider-level fields; per-model detected context size and capabilities; detection step in Custom verification and refresh (AAP-FR-MDLQ now ignores every field but `id`, `object`, `mode`); a Custom model now offers the fixed reasoning set (AAP-FR-24, AAP-FR-25); the adapter and the endpoint resolution return the effective reasoning, output limit, and context size (AAP-FR-ADPX).
- `specifications/ai/CVL-conversation-loop.md`: CVL-FR-ZPGW (a Custom call may carry reasoning), CVL-FR-FHFM (128000 applies only when no limit is set), and the context budget the loop uses.
- `specifications/ui/AGT-agents.md`: AGT-FR-16 and AGT-FR-RFSH (reasoning selector now shown for Custom models). Check `specifications/core/AGR-agent-registry.md` for any rule that validates a stored reasoning against the catalog.
- Align with the pending Custom gateway draft (required secret, URL rules).