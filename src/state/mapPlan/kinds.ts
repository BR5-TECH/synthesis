/**
 * MPV-FR-SNCR: the kind chip of a new issue comes from the Type mapping of the
 * delivery settings (`DSR-delivery-source.md` DSR-FR-ZJMC).
 */
import type { DeliverySettings, IssueKind } from "../../types";

/** MPV-FR-SNCR: the kind the Type mapping gives a Type name, or null. */
export function kindOfType(settings: DeliverySettings | null, type: string | null): IssueKind | null {
  if (!settings) return null;
  if (type === null) return settings.untyped === "exclude" ? null : settings.untyped;
  const entry = settings.readEntries.find((e) => e.type.toLowerCase() === type.toLowerCase());
  if (entry) return entry.kind === "exclude" ? null : entry.kind;
  const created = (Object.entries(settings.createTypes) as [IssueKind, string][]).find(
    ([, name]) => name.toLowerCase() === type.toLowerCase(),
  );
  return created ? created[0] : null;
}
