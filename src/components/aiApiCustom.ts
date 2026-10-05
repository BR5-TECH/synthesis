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
