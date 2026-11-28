import type { AgenticIntegration, AgenticTurnKind, EffortOption } from "../types";

/**
 * The reasoning-effort levels an agentic selector offers —
 * `specifications/core/AIC-agentic-integrations.md` AIC-FR-SFMS and AIC-FR-TPKB,
 * as `specifications/ui/AII-ai-integrations.md` AII-FR-24 and AII-FR-BMGM render
 * them.
 *
 * The backend applies the same rules when it reads a record and when it accepts
 * a selection. The section applies them again so that a list follows its model
 * at once — in the same render as the model it follows, with no request of its
 * own — and so that a value the record still carries is never shown beside a
 * list that does not hold it.
 */

/**
 * AIC-FR-SFMS: the effective model of a selection — the kind's own override,
 * else the default model. `null` for the default selector's own row and for a
 * kind with no override when no default is chosen: the backend default.
 */
export function effectiveModel(
  integration: AgenticIntegration,
  kind: AgenticTurnKind | null,
): string | null {
  const override = kind ? (integration.modelOverrides ?? {})[kind] : undefined;
  return override ?? integration.selectedModel ?? null;
}

/** A model's own ladder, or null when it declares none. */
function ladderOf(integration: AgenticIntegration, modelId: string): string[] | null {
  const model = integration.models.find((m) => m.id === modelId);
  const ladder = model?.reasoning?.supportedEfforts;
  return Array.isArray(ladder) ? ladder : null;
}

/**
 * AIC-FR-SFMS: the levels offered for a selection whose effective model is
 * `modelId`.
 *
 * - With no model of the record declaring a ladder (Claude Code, every API
 *   agent), the record's own `reasoningEfforts`, unchanged.
 * - With a listed model that declares one, that model's ladder; with a listed
 *   model that declares none, the record's `reasoningEfforts`.
 * - With no effective model, or one the record does not list, the levels every
 *   ladder of the record shares, in the order `reasoningEfforts` gives them.
 *
 * Each level takes its label from `reasoningEfforts`, and a level that list
 * does not name falls back to its id.
 */
export function offeredEfforts(
  integration: AgenticIntegration,
  modelId: string | null,
): EffortOption[] {
  const vendorLevels = integration.reasoningEfforts ?? [];
  const ladders = integration.models
    .map((m) => m.reasoning?.supportedEfforts)
    .filter((l): l is string[] => Array.isArray(l));
  if (ladders.length === 0) return vendorLevels;

  const label = (id: string) => vendorLevels.find((e) => e.id === id)?.label ?? id;
  const listed = modelId ? integration.models.some((m) => m.id === modelId) : false;
  if (listed && modelId) {
    const own = ladderOf(integration, modelId);
    // A listed model with no ladder, in a list where others carry one, offers
    // the vendor list, as a vendor with no ladders at all does.
    return own ? own.map((id) => ({ id, label: label(id) })) : vendorLevels;
  }

  const shared = (id: string) => ladders.every((l) => l.includes(id));
  // The vendor's order first, then any shared level it does not name.
  const ordered = [
    ...vendorLevels.map((e) => e.id),
    ...ladders[0].filter((id) => !vendorLevels.some((e) => e.id === id)),
  ];
  return ordered.filter(shared).map((id) => ({ id, label: label(id) }));
}

/**
 * AII-FR-BMGM / AIC-FR-TPKB: the value a selector shows. A stored level the
 * list does not offer renders as the selector's first, default entry — `""` —
 * exactly as the record reads once the backend drops it.
 */
export function shownEffort(value: string | null | undefined, offered: EffortOption[]): string {
  if (!value) return "";
  return offered.some((e) => e.id === value) ? value : "";
}
