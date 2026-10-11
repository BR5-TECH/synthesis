import { AI_ERRORS, type ModelsOrigin } from "../types";

/**
 * What the two levels of `specifications/ui/AII-ai-integrations.md` share: the
 * default-entry labels, the status line shape, and the helpers both levels use
 * to name a failure and colour a status.
 */

/** The entry meaning "whatever the provider itself would pick" (AII-FR-12). */
export const PROVIDER_DEFAULT_LABEL = "Provider default";
/** The entry meaning "whatever the backend itself would pick" (AII-FR-23/24). */
export const BACKEND_DEFAULT_LABEL = "Backend default";
/** The entry a per-task selector shows while it follows the default (AII-FR-23/24). */
export const SAME_AS_DEFAULT_LABEL = "Same as default";

/** The typed distinctions this module knows how to name (AII-FR-28). */
const KNOWN_AI_ERRORS = new Set<string>(Object.values(AI_ERRORS));

/**
 * The typed distinction behind a rejection, as a value that is safe to log.
 *
 * A backend rejection carries a code from a closed set, but an error that
 * reached here by another route can carry anything — a URL, a request body, a
 * credential the client echoed back. Nothing downstream redacts a log record, so
 * a code outside the set is reported by its *shape* rather than its value: which
 * failure it was is what a reader needs, and an unrecognised one only needs to
 * be distinguishable from the rest.
 */
export function typedFailureCode(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : "";
  return KNOWN_AI_ERRORS.has(raw) ? raw : "unrecognised";
}

/** What a verification status line says, and how it reads. */
export interface Status {
  text: string;
  tone: "ok" | "warn" | "muted";
}

/** AII-FR-12 / FR-23: where a model list came from, said plainly. */
export function modelsOriginLabel(origin: ModelsOrigin, source: string): string {
  return origin === "probed"
    ? `from ${source}`
    : "from Synthesis's bundled list";
}

export type Busy = "idle" | "detecting" | "verifying" | "working";

export function toneColor(tone: Status["tone"]): string {
  return tone === "ok"
    ? "var(--ok, var(--accent))"
    : tone === "warn"
      ? "var(--danger, var(--fg-2))"
      : "var(--fg-3)";
}
