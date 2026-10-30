## Intent

Context window visualization was added recently.

But looks like few changes needed:

### Change 1:

We should assume default context window limit of 1M if model API doesn’t provide true limit.

The default is 1,000,000 tokens. It applies when the model reports no context size and no override is set.

### Change 2:

I would imagine visualization to have some nice round circle which shows the utilization. percentage is nice, but nice round icon around it would be nice. it should stay small to visually fit into the button size around. just small nice touch.

The indicator shows a small progress ring that carries the percentage. The ring colour changes by level.

### Change 3:

All moderls/providers we have in the AI API settings must have the max context size override value. it should have the default of 1M if undefined.

The field is one per provider tab (OpenRouter, Anthropic, OpenAI, Custom), not one per model. It applies to every model of that provider.

## Requirements

- **Effective window.** For a model, the window is, in order: the provider's max context size override, else the context size the API or detection reports, else 1,000,000. The window is never null. Override > API/detected > 1M.
- **Used everywhere.** The effective window is the one value for the context indicator, the document size limit, and the conversation loop's budget (history trim and compaction). No consumer keeps its own fallback. A provider that is not configured, or a model the provider does not list, also gets the default.
- **Override field.** Every AI API provider tab shows a `Max context size` field (today only Custom shows it). Empty means unset. The placeholder shows `1000000`. The helper text says: the value wins over the size the model reports, it is the window for the size limit and the history budget, and it is never sent to the host. The field keeps the token-field validation of `AII-FR-TWIR` (empty, or a whole number from 1 to 4,294,967,295), inline errors, and revert on blur. Max output tokens stays Custom-only.
- **Storage and operations.** `set_ai_api_max_context_size` accepts every provider, persists at once, and clears on null. `limit_unsupported` no longer applies to it. `clear_ai_api_integration` clears it for the provider. Existing Custom values stay valid and now also win over a detected size.
- **Ring.** A small circular progress ring surrounds or sits beside the percentage inside the existing indicator. It must not enlarge the indicator or the column header row. The arc length is the used share of the window, filled at 100% and above. Below 50% the ring uses the neutral/accent tone. From 50% up to below 85% it uses the warning tone. From 85% up it uses the danger tone. The tones are theme tokens, and the geometry is the same in both themes.
- **Ring accessibility.** Colour is not the only carrier of the level: the percentage text stays visible, and the accessible name and the tooltip state the percentage and the level (normal, high, critical). The estimate marker, the tooltip split, and the focus behaviour stay as they are. No animation is required.
- **Document warning.** The warning icon for a document above the size limit stays as it is and is separate from the ring level. With a window that is always known, the no-window state (percentage missing, no limit) no longer occurs.

## Specifications to change

- `specifications/core/AAP-ai-api-integrations.md`: AAP-FR-AYKU (max context size is no longer Custom-only), AAP-FR-ITBL (no `limit_unsupported` for context size; clear), AAP-FR-RANS (override > detected > 1M), AAP-FR-BKBF (resolve never returns null).
- `specifications/core/AGC-agent-conversations.md`: the context figures (`window` is never null; `limit_tokens` is never null), AGC-FR-GMSF, AGC-FR-BCHZ, AGC-FR-JFBZ.
- `specifications/ai/CVL-conversation-loop.md`: CVL-FR-OPTI and the budget rule (CVL-FR-PGII) read the effective window.
- `specifications/ui/AII-ai-integrations.md`: AII-FR-05 (field order on every tab), AII-FR-FBVE (new helper text), the Custom wireframe, and a wireframe or note for the other tabs.
- `specifications/ui/DDS-draft-discussion.md`: DDS-FR-TSPL (no unknown window), and the indicator requirements (ring, thresholds, accessible name, DDS-FR-WRNB).
- The draft `Partial changes` (archived) and `Local Models` say "no window known, no limit" and "Custom-only max context size". Align them with this draft.