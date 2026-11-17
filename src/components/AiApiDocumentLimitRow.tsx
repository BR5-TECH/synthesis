import * as api from "../api";
import {
  AiApiPercentLimitRow,
  PERCENT_LIMIT_MAX,
  PERCENT_LIMIT_MIN,
  percentLimitToCommit,
  type PercentLimitConfig,
} from "./AiApiPercentLimitRow";

/** GLS-FR-DLMP: the smallest percent the field commits. */
export const DOCUMENT_LIMIT_MIN = PERCENT_LIMIT_MIN;
/** GLS-FR-DLMP: the largest percent the field commits. */
export const DOCUMENT_LIMIT_MAX = PERCENT_LIMIT_MAX;

/** GLS-FR-WQHT: what the typed text commits (see `percentLimitToCommit`). */
export const documentLimitToCommit = percentLimitToCommit;

const DOCUMENT_LIMIT: PercentLimitConfig = {
  label: "Document size limit",
  idPrefix: "ai-api-document-limit",
  note:
    "How much of the model's context window the document under discussion may fill. A larger document is given to the agent in part, and the agent reads the rest when it needs it.",
  logName: "document size limit",
  load: () => api.loadDocumentLimitPercent(),
  save: (percent) => api.saveDocumentLimitPercent(percent),
};

/**
 * GLS-FR-DLMP / GLS-FR-WQHT: the AI API section's **Document size limit**, one
 * setting for every provider, above the provider tabs.
 */
export function AiApiDocumentLimitRow() {
  return <AiApiPercentLimitRow config={DOCUMENT_LIMIT} />;
}
