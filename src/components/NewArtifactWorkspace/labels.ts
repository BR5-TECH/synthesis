/**
 * What a draft's tab says about a refused image and about its History
 * (`../../../specifications/ui/NAW-new-artifact.md`).
 *
 * Pure functions over what the backend returned, kept out of the tab because
 * each answers one question completely and none needs a render to be tested —
 * which is what makes the rail's wording and the refusals checkable without
 * mounting anything.
 */
import type { DraftHistoryEntry, DraftHistoryList } from "../../types";

export function isRefusal(error: unknown, token: string): boolean {
  return String(error).includes(token);
}

/**
 * NAW-FR-51: what a refused image insertion tells the author.
 *
 * The refusals divide in **what kind of thing went wrong**, because the three
 * kinds call for three different responses: the image was not acceptable, so
 * bring a different picture; the draft takes no write while its run holds it,
 * which is the same reason the tab's read-only presentation already states
 * (NAW-FR-44); or the store did not happen, which is nothing the author did and
 * which they may simply try again (per
 * `../../../specifications/core/DAS-draft-assets.md` DAS-FR-03, DAS-FR-04).
 *
 * Exported so the wording is testable on its own, and matched as a substring
 * because a Tauri command's rejection arrives as the error's own text rather
 * than as a typed value.
 */
export function imageRefusal(error: string): string {
  if (error.includes("unsupported_media_type")) {
    return "That file is not an image this application accepts. Try a PNG, a JPEG, a GIF, or a WEBP.";
  }
  if (error.includes("image_too_large")) {
    return "That image is larger than this application accepts. Try a smaller one.";
  }
  if (error.includes("malformed_image")) {
    return "That image could not be read. Try a different one.";
  }
  if (error.includes("draft_locked_by_graduation")) {
    return "This draft takes no changes while its graduation holds it.";
  }
  if (error.includes("asset_store_failed")) {
    return "The image could not be stored. Nothing was written — you can try again.";
  }
  return "The image could not be inserted.";
}

/**
 * NAW-FR-38: what a version's row says about where it stands.
 *
 * `Original` keeps its label for the draft's whole life; the newest version an
 * acceptance produced is **Latest accepted**; every earlier accepted version is
 * **Superseded**. No row is ever marked as the current draft — where the live
 * prompt has moved on, the rail says that of the *live prompt* (NAW-FR-37)
 * rather than withdrawing the newest version's label.
 */
export function standingOf(entry: DraftHistoryEntry, entries: DraftHistoryEntry[]): string {
  if (entry.source.kind === "original") return "Original";
  const newestAccepted = [...entries]
    .reverse()
    .find((e) => e.source.kind === "proposal_accepted");
  return newestAccepted?.id === entry.id ? "Latest accepted" : "Superseded";
}

/** NAW-FR-07: whose version it is — `Original`, or the agent that proposed it. */
export function sourceLabelOf(entry: DraftHistoryEntry): string {
  if (entry.source.kind === "original") return "Original";
  const agent = entry.source.agent;
  return agent.kind === "agent" ? `@${agent.handle}` : "An agent";
}

/**
 * NAW-FR-37: what the live-prompt row says when the author has typed since the
 * newest version — **naming the baseline it has moved on from** rather than
 * asserting an acceptance that may never have happened.
 *
 * Null while the live prompt and the newest version are the same bytes, which is
 * when the row is rendered plainly — and null for a draft holding no version,
 * whose live prompt is the `Original` and has moved on from nothing.
 */
export function liveMarkerFor(history: DraftHistoryList | null): string | null {
  if (!history || history.live.matchesLatest) return null;
  const newest = history.entries[history.entries.length - 1];
  // A draft holding no version has moved on from nothing: its live prompt is the
  // `Original` itself, so there is no baseline for the marker to name.
  if (!newest) return null;
  return newest.source.kind === "proposal_accepted"
    ? "Modified since latest accepted version"
    : "Modified since Original";
}

/**
 * NAW-FR-08 / NAW-FR-38: how many versions the draft holds, the live prompt
 * counted among them.
 *
 * A draft nobody has proposed a change to holds one — the `Original`, which is
 * the live prompt itself (per `../../../specifications/core/DHS-draft-history.md`
 * DHS-FR-07). Reporting nought there would say a draft that has been written in
 * all afternoon has no version of its prompt at all, when what it has is one
 * that has never been superseded.
 *
 * Null where the history has not answered — not yet, or not at all because it
 * could not be reconciled (NAW-FR-41): a count is a claim about the draft, and
 * "1" asserted over a history the application could not confirm is exactly the
 * claim that requirement forbids. The toggle renders without a count instead of
 * with a wrong one.
 */
export function versionCountOf(history: DraftHistoryList | null): number | null {
  if (!history) return null;
  return history.entries.length === 0 ? 1 : history.entries.length;
}

/** A version's timestamp, short in the row and absolute on hover. */
export function timestampOf(iso: string): { short: string; absolute: string } {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return { short: iso, absolute: iso };
  return {
    short: at.toLocaleString(undefined, {
      weekday: "short",
      hour: "2-digit",
      minute: "2-digit",
    }),
    absolute: at.toISOString(),
  };
}
