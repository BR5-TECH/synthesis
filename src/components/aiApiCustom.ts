/**
 * Custom gateway presentation helpers for the AI API level and the Agents
 * editor (`AII-ai-integrations.md` AII-FR-06, AII-FR-08, AII-FR-12 and
 * `AGT-agents.md` AGT-FR-15).
 */
import type { AiApiProviderId, ModelOption } from "../types";

/** AAP-FR-RTMZ: the routes a model supports, said plainly. */
export function routeLabel(model: Pick<ModelOption, "mode">): string {
  switch (model.mode) {
    case "chat":
      return "chat";
    case "responses":
      return "responses";
    default:
      // Absent or null means the model works on both routes.
      return "chat + responses";
  }
}

/**
 * AII-FR-12: the label a model option shows. A Custom gateway model names its
 * route or routes; the models of the other providers carry none.
 */
export function modelOptionLabel(
  provider: AiApiProviderId | null,
  model: ModelOption,
): string {
  return provider === "custom"
    ? `${model.label} · ${routeLabel(model)}`
    : model.label;
}

/** AII-FR-12 / AGT-FR-15: the filterable options for a provider's models. */
export function modelSelectOptions(
  provider: AiApiProviderId | null,
  models: ModelOption[],
): { id: string; label: string }[] {
  return models.map((m) => ({ id: m.id, label: modelOptionLabel(provider, m) }));
}

/** AII-FR-12 / AGT-FR-15: where a Custom model list came from. */
export const CUSTOM_GATEWAY_ORIGIN = "from the Custom gateway";

/** AII-FR-XGIM / AGT-FR-SOIO: the levels every Custom model offers, in order. */
export const CUSTOM_REASONING_LEVELS = ["none", "low", "medium", "high"] as const;

/**
 * AII-FR-XGIM / AGT-FR-SOIO: the entries of a Custom reasoning selector — the
 * default entry, then the fixed levels. The entry ids are the ones
 * `reasoningChoiceFor` reads, so an entry maps to `{ kind: "effort" }`.
 */
export function customReasoningOptions(
  defaultLabel: string,
): { id: string; label: string }[] {
  return [
    { id: "", label: defaultLabel },
    ...CUSTOM_REASONING_LEVELS.map((level) => ({
      id: `effort:${level}`,
      label: level,
    })),
  ];
}

/**
 * AII-FR-ZIEG: the detected values of a Custom model as one line, or `null`
 * where the model has none. A null value is not shown.
 */
export function detectedValuesText(
  model: Pick<ModelOption, "detectedContextSize" | "detectedCapabilities"> | null,
): string | null {
  if (!model) return null;
  const parts: string[] = [];
  if (model.detectedContextSize != null) {
    parts.push(`context ${model.detectedContextSize} tokens`);
  }
  if (model.detectedCapabilities && model.detectedCapabilities.length > 0) {
    parts.push(model.detectedCapabilities.join(", "));
  }
  return parts.length > 0 ? `detected · ${parts.join(" · ")}` : null;
}

/** AII-FR-TWIR: the largest value a Custom token field accepts. */
export const TOKEN_LIMIT_MAX = 4294967295;

/**
 * AII-FR-UQLF / AII-FR-TWIR: what the typed text of a token field commits —
 * `null` to clear, a whole number from 1 to `TOKEN_LIMIT_MAX`, or `undefined`
 * for an entry that commits nothing.
 */
export function tokenLimitToCommit(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  if (!/^\d+$/.test(trimmed)) return undefined;
  const value = Number(trimmed);
  if (value < 1 || value > TOKEN_LIMIT_MAX) return undefined;
  return value;
}
