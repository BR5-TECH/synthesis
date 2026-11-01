/**
 * What a proposal reference says about how much it changes
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-HVXK,
 * DCR-FR-OYRR; `PCR-prompt-change-review.md` PCR-FR-RFJC; `CMT-comments.md`
 * CMT-FR-48).
 *
 * One module, so the card in the rail and the row in the discussion column
 * say the same words for the same reference.
 */
import type { ProposalUpdateCounts } from "../types";

/** DCR-FR-HVXK: the count of changes a recorded proposal holds. */
export function changeCountSummary(changes: number): string {
  return `${changes} ${changes === 1 ? "change" : "changes"}`;
}

/**
 * DCR-FR-OYRR: the one line an update comment's reference carries in place of
 * the count of changes. It names all three numbers, a zero included, so the
 * line reads the same for every update.
 */
export function updateSummary(update: ProposalUpdateCounts): string {
  return `${update.changed} changed, ${update.added} added, ${update.withdrawn} withdrawn`;
}
