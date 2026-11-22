import * as api from "../api";
import { AiApiPercentLimitRow, type PercentLimitConfig } from "./AiApiPercentLimitRow";

/** GLS-FR-GLFQ: the smallest limit, in kilobytes. */
export const EXTRACTED_TEXT_LIMIT_MIN = 1;
/** GLS-FR-GLFQ: the largest limit, in kilobytes. */
export const EXTRACTED_TEXT_LIMIT_MAX = 10240;

const EXTRACTED_TEXT_LIMIT: PercentLimitConfig = {
  label: "Extracted text limit",
  idPrefix: "ai-api-extracted-text-limit",
  note:
    "The most text that one attached PDF or DOCX file may give. A file with more text is refused when you post the comment.",
  logName: "extracted text limit",
  load: () => api.loadExtractedTextLimitKb(),
  save: (kb) => api.saveExtractedTextLimitKb(kb),
  min: EXTRACTED_TEXT_LIMIT_MIN,
  max: EXTRACTED_TEXT_LIMIT_MAX,
  suffix: "KB",
  refusalCode: "kilobytes_out_of_range",
};

/**
 * GLS-FR-GLFQ / GLS-FR-LZLG: the AI API section's **Extracted text limit**,
 * one setting for every provider, below the History size limit.
 */
export function AiApiExtractedTextLimitRow() {
  return <AiApiPercentLimitRow config={EXTRACTED_TEXT_LIMIT} />;
}
