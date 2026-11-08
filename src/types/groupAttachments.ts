// Attachments that mark the comments of an agent group (CMS-comments-storage.md).
// Every name is re-exported from `./comments`.

/** CMS-FR-TUHL: marks a comment as the Synthesis of an agent group. */
export interface SynthesisMark {
  kind: "synthesis";
}

/** CMS-FR-KVDV: one advisor of a Meta reply, finished or skipped. */
export interface AdvisorEntry {
  handle: string;
  state: "finished" | "skipped";
  /** The reason of a skipped entry. */
  reason?: "timed_out" | "failed" | "cancelled";
  points?: string[];
  questions?: string[];
}

/** CMS-FR-KVDV: the advisors a Meta reply was written with. */
export interface AdvisorsList {
  kind: "advisors";
  advisors: AdvisorEntry[];
}
