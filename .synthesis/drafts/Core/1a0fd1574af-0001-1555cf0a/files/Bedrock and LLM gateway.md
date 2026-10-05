## Intent

Add native AWS Bedrock as a new AI API provider. The provider is configured in Global settings and can serve both the graduation loop and conversational Agents. OpenRouter, Anthropic, OpenAI, Custom, and all existing behaviour remain unchanged.

This feature uses only the native `bedrock-mantle` endpoint. It does not add third-party gateways, custom Bedrock URLs, or Bedrock support through the existing OpenAI or Anthropic provider tabs.

## Users and user journey

The user:

1. Opens **Global settings**.
2. Opens **AI API**.
3. Selects the **AWS Bedrock** tab.
4. Selects the AWS region.
5. Selects one authentication method:
   - AWS SDK default credential chain;
   - entered AWS access key ID, secret access key, and optional session token; or
   - a Bedrock bearer API key.
6. Activates **Verify**.
7. The application validates the endpoint, credentials, and region, discovers the available Bedrock models, and discovers the API route each model supports.
8. The user selects a model for the AI API integration, or keeps the provider default where the backend supports one.
9. The user can open **Global settings → Agents**, create or edit an Agent, select AWS Bedrock as its provider, and select a supported Bedrock conversational model.
10. The graduation loop or an Agent conversation sends requests through the selected Bedrock model and its selected route.

## Scope

### In scope

- A dedicated **AWS Bedrock** tab in the AI API section.
- Native `bedrock-mantle` inference only.
- Region configuration.
- AWS SDK default credentials, entered AWS credentials, and Bedrock bearer API key authentication.
- Live model discovery from the configured Bedrock endpoint and region.
- Discovery of whether each model supports OpenAI Chat Completions or Anthropic Messages.
- Bedrock model selection for the AI API integration and conversational Agents.
- Runtime requests through OpenAI Chat Completions and Anthropic Messages.
- Clear verification, discovery, authentication, and inference errors in the settings UI and conversation flows.
- Persistence of non-secret configuration and storage of credentials in the application secret vault.
- Frontend, backend, IPC, persistence, runtime, and automated-test changes required by the above.

### Out of scope

- Third-party LLM gateways or custom gateway URLs.
- Bedrock through the existing OpenAI or Anthropic tabs.
- Bedrock endpoints other than `bedrock-mantle`.
- A bundled model catalogue, cached catalogue, or manually entered model IDs when live discovery fails.
- Non-conversational inference calls. Non-chat models may be returned by discovery, but this feature does not add a call path for them.
- Voice, embedding, image-generation, fine-tuning, or other non-conversational modalities.
- Changes to existing providers or their current authentication and model-discovery behaviour.

## Functional requirements

### Provider configuration and authentication

- Add `bedrock` as a dedicated AI API provider. Keep the existing providers and their current tabs and behaviour unchanged.
- The AWS Bedrock tab must render a region field and an authentication-method selector.
- The authentication selector must offer:
  1. **AWS default credentials**, resolved through the AWS SDK default credential chain;
  2. **AWS access keys**, with access key ID, secret access key, and optional session token fields; and
  3. **Bedrock API key**, with a masked bearer-token field.
- The tab must use the regional native `bedrock-mantle` endpoint derived from the configured AWS region. The user must not enter a custom endpoint URL.
- AWS credentials and the Bedrock API key are secrets. Store them only in the application secret vault. Never return, persist, log, or display secret values; display only a masked hint where the existing AI API UI supports one.
- Default-chain mode must not require credential fields. Entered-credentials mode must validate the required access key ID and secret access key, and accept an optional session token. API-key mode must work without AWS credentials when the selected Bedrock endpoint supports the requested operation.
- Verify must be explicit. It must validate the region, selected authentication method, endpoint access, credentials, and model discovery before marking the integration verified.
- A failed verification must not replace the last verified configuration. The UI must keep the last verified state and show a typed, actionable error.
- Changing the region or authentication method invalidates the previous verification until Verify succeeds again.

### Model discovery and route capability

- Verify must discover the models available to the configured Bedrock account, region, and endpoint. The result must include all models exposed by the endpoint, across model families.
- The discovery result must include, or be joined with authoritative AWS compatibility data for, whether each model supports:
  - OpenAI Chat Completions;
  - Anthropic Messages; or
  - neither supported route.
- If model discovery is unavailable, fails, or returns no usable model catalogue, the provider must not become verified. The UI must show the discovery error and must show no selectable Bedrock model choices. Do not use a bundled fallback, the last successful catalogue, or manual model entry.
- Models that support neither OpenAI Chat Completions nor Anthropic Messages must remain visible as disabled entries in both the AI API model selector and the Agent model selector. They must not be selectable or usable for inference.
- Every model that supports at least one supported route must be selectable in both selectors. The selector must preserve the existing filterable, keyboard-accessible model-selector behaviour defined by the AI integration specifications.
- When a model supports both routes, infer one route from the discovered compatibility metadata: prefer Anthropic Messages for Claude models and prefer OpenAI Chat Completions for other compatible models.
- When a model supports only one route, use that route. A model with no confirmed route is disabled.
- Store the selected route with the selected Bedrock model, or derive it deterministically from the stored compatibility metadata. The runtime must never guess a route from an identifier when authoritative compatibility data is available.
- A selected model that disappears from a later successful discovery must be cleared or marked unavailable according to the existing provider-selection rules. It must not be sent to Bedrock.

### Inference

- Implement Bedrock inference through `bedrock-mantle` using only the route selected by the model capability data:
  - OpenAI-compatible models use OpenAI Chat Completions.
  - Anthropic-compatible models use Anthropic Messages.
- The runtime must translate the application's common conversational request and response contract to the selected Bedrock route and back without changing the existing caller contract.
- The graduation loop and Agent conversations must be able to resolve a verified Bedrock provider, selected model, selected route, region, and authentication material through the existing internal AI API resolution paths.
- Agents may use only Bedrock models that support one of the two implemented conversational routes. A disabled or unavailable model cannot be saved to an Agent.
- Runtime failures must use the existing typed provider error model where possible and must preserve the provider's actionable error text where the existing conversation UI displays provider failures.
- The feature must not change how the existing OpenRouter, Anthropic, OpenAI, or Custom providers build or send requests.

### Frontend behaviour

- Add the AWS Bedrock tab beside the existing AI API provider tabs. The tab must use the existing AI API layout, verification status treatment, masked-secret rules, model selector, activation control, and inline-error pattern.
- The tab must show the selected region, authentication method, relevant credential fields, Verify, verification/discovery status, model selector, and Clear action in a stable order.
- While verification or discovery is running, show a busy state on Verify and keep the settings surface responsive. Do not silently show stale model choices as current discovery results.
- Show the model-list origin and discovery state. If discovery fails, show the error as a first-class status state and render an explicit empty model state rather than an empty selector.
- Show unsupported models as disabled with a reason such as “This model has no supported conversational API route.” Disabled options must be skipped by keyboard selection and cannot be committed.
- Show the inferred route where it helps the user distinguish models with different API contracts. Do not add a second route selector when the route is uniquely determined by compatibility metadata.
- In Global settings → Agents, include verified AWS Bedrock as a provider and apply the same model filtering, disabled-model, keyboard, and inline-error rules. Existing provider choices remain unchanged.
- Preserve the existing distinction between AI API model selections used by the graduation loop and Agent model selections stored on each Agent. Selecting a Bedrock model in the AI API tab must not change an Agent's model.
- Keep the UI usable with no Bedrock configuration, no credentials, no network, and no discovered models. Render an explicit unconfigured or discovery-failed state, not a broken control.

### Persistence and compatibility

- Extend the AI API provider registry with the Bedrock provider's region, authentication mode, non-secret configuration, discovered model metadata, selected model, selected route, verification state, and verification timestamp.
- Do not store AWS secrets or Bedrock API keys in `synthesis.toml`, project files, logs, IPC responses, or error messages. Use the existing application secret-vault conventions and migration rules.
- Keep the existing single-active-provider and project-override rules. A verified Bedrock provider can be activated or selected as a project override under the same rules as other AI API providers.
- Clearing Bedrock must remove its stored secrets and return its region, model selection, route selection, discovery state, verification state, and activation to the provider's unconfigured state.
- Existing configurations for all other providers must load and behave exactly as before.

## Verification and error cases

Define typed errors and matching frontend messages for at least:

- missing or invalid region;
- missing AWS access key ID or secret access key;
- invalid entered AWS credentials;
- missing or rejected Bedrock API key;
- unavailable or invalid default credentials;
- endpoint unreachable;
- request timeout;
- model discovery unavailable;
- model catalogue empty or unusable;
- model has no supported route;
- selected model unavailable;
- Bedrock route request rejected;
- Bedrock response invalid or empty.

Errors must state what the user can correct. Credentials must never appear in an error.

## Required specifications and project changes

Create or update the following specifications and cross-reference them consistently:

- `specifications/ui/AII-ai-integrations.md` — add the AWS Bedrock tab, fields, authentication selector, verification/discovery states, disabled model entries, route presentation, activation, clear, keyboard behaviour, and frontend empty/error states.
- `specifications/core/AAP-ai-api-integrations.md` — add the Bedrock provider descriptor, registry record, secret-vault entries, authentication modes, `bedrock-mantle` endpoint resolution, model discovery, route capability metadata, typed errors, verification rules, model selection, and runtime resolution contract.
- `specifications/core/GSS-global-settings-storage.md` — add persistence for Bedrock's non-secret configuration and active-provider record while preserving the no-secrets-in-store rule.
- `specifications/ui/GLS-global-settings.md` — keep AI API as a Global settings section and cross-reference the expanded Bedrock tab without changing the window or save model.
- `specifications/ui/AGT-agents.md` — allow verified Bedrock as an Agent provider and define disabled unsupported Bedrock models in the Agent editor.
- `specifications/core/AGC-agent-conversations.md` — define resolution and request handling for Bedrock Chat Completions and Messages routes without changing existing providers.
- `specifications/ai/GRL-graduation-loop.md` and the shared conversation/provider specifications — define the Bedrock provider resolution and route-specific request path used by graduation and conversational calls.

Add or update frontend, backend, IPC, persistence, discovery, authentication, route-selection, inference, regression, and end-to-end tests. Tests must prove that existing providers are unchanged; each Bedrock authentication mode works or fails with the correct typed error; discovery failure shows no model choices; unsupported models are disabled; route inference follows compatibility metadata; Claude models prefer Messages when both routes are available; other models prefer Chat Completions; selected models work in AI API calls and Agents; secrets never cross the UI or persistence boundary; and project overrides and activation follow existing rules.