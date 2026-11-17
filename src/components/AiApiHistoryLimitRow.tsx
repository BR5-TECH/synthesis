import * as api from "../api";
import { AiApiPercentLimitRow, type PercentLimitConfig } from "./AiApiPercentLimitRow";

const HISTORY_LIMIT: PercentLimitConfig = {
  label: "History size limit",
  idPrefix: "ai-api-history-limit",
  note:
    "How much of the model's context window the discussion history may fill. In a longer discussion the agent gets the newest comments, and older comments are left out.",
  logName: "history size limit",
  load: () => api.loadHistoryLimitPercent(),
  save: (percent) => api.saveHistoryLimitPercent(percent),
};

/**
 * GLS-FR-UTNL / GLS-FR-QHNI: the AI API section's **History size limit**, one
 * setting for every provider, below the Document size limit.
 */
export function AiApiHistoryLimitRow() {
  return <AiApiPercentLimitRow config={HISTORY_LIMIT} />;
}
