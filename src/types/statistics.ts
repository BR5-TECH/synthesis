/**
 * A draft's captured lifetime totals
 * (`specifications/core/DSS-draft-statistics-storage.md` DSS-FR-YOVS,
 * rendered by `specifications/ui/DFI-draft-information.md`).
 *
 * Every figure carries its own availability rather than being a bare number,
 * because an absent figure and a figure of zero are different facts: a total the
 * application did not capture is `unavailable`, one whose captured record is
 * damaged or partial is `incomplete`, and neither is ever rendered as a `0`
 * (DSS-FR-NCLP, DFI-FR-HSYB).
 */

/** DSS-FR-NCLP: what the fold found of a statistic's own source family. */
export type Availability = "available" | "incomplete" | "unavailable";

/** One statistic: a value, and what the fold knows about the record behind it. */
export interface Measure {
  /** Null exactly where `availability` is `unavailable`. */
  value: number | null;
  availability: Availability;
}

/** DSS-FR-DZQF: the two directions, kept apart and decided independently. */
export interface TokenPair {
  input: Measure;
  output: Measure;
}

/**
 * DSS-FR-IYRG: the six buckets and their total, as durations in milliseconds.
 *
 * The order the surface renders them in is fixed in the surface rather than
 * derived from this shape (DFI-FR-MODK), so two drafts read the same way.
 */
export interface BucketTimes {
  refinement: Measure;
  authoring: Measure;
  validationHandoffPublication: Measure;
  implementation: Measure;
  reviews: Measure;
  reconciliation: Measure;
  total: Measure;
}

/** DSS-FR-CMTP: the same six buckets and their total, as reported tokens. */
export interface BucketTokens {
  refinement: TokenPair;
  authoring: TokenPair;
  validationHandoffPublication: TokenPair;
  implementation: TokenPair;
  reviews: TokenPair;
  reconciliation: TokenPair;
  total: TokenPair;
}

/** DSS-FR-YOVS: what `read_draft_statistics` answers with. */
export interface DraftStatistics {
  draftId: string;
  /** DSS-FR-JRSY: null where the draft has captured nothing at all. */
  boundaryAt: string | null;
  editingTimeMs: Measure;
  aiInteractions: Measure;
  draftEdits: Measure;
  acceptedProposals: Measure;
  rejectedProposals: Measure;
  conversationTokens: TokenPair;
  agentTimeMs: BucketTimes;
  agentTokens: BucketTokens;
  /** DSS-FR-LDFK: how many lines the fold could not read and left in place. */
  unreadableLines: number;
}

/** DSS-FR-TWNA: what `"draft statistics changed"` carries — the draft, and no totals. */
export interface DraftStatisticsChangedPayload {
  draftId: string;
}
