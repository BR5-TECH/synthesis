/**
 * What an in-flight operation's activation destination means to the status bar
 * (`STB-status-bar.md` STB-FR-RWPD, STB-FR-DNLC, STB-FR-YQFE).
 *
 * The destination is data the producer supplies (`PRG-progress-reporting.md`
 * PRG-FR-KXQW). Nothing here reads `kind` or `label` to find one
 * (PRG-FR-TBZN): an operation with no `activation`, or with a `type` this build
 * does not know, has no destination and is not actionable.
 */
import type { Activation, Operation } from "../types";

/** The destination of an operation, or `null` where it has none this build can open. */
export function destinationOf(operation: Operation): Activation | null {
  const activation: unknown = operation.activation;
  if (typeof activation !== "object" || activation === null) return null;
  const candidate = activation as Record<string, unknown>;
  switch (candidate.type) {
    case "graduation_run":
      return typeof candidate.runId === "string" && candidate.runId !== ""
        ? (activation as Activation)
        : null;
    case "discussion":
      return typeof candidate.discussionId === "string" &&
        candidate.discussionId !== ""
        ? (activation as Activation)
        : null;
    case "git_push":
      return typeof candidate.branch === "string" && candidate.branch !== ""
        ? (activation as Activation)
        : null;
    default:
      return null;
  }
}

/**
 * STB-FR-YQFE: the target an actionable row opens, in words, for its accessible
 * name. The label itself is the producer's and is never altered.
 */
export function targetPhrase(activation: Activation): string {
  switch (activation.type) {
    case "graduation_run":
      return "open graduation run";
    case "discussion":
      return "open discussion";
    case "git_push":
      return `open branch ${activation.branch} in Git`;
  }
}

/** STB-FR-YQFE: `<label> — <target phrase>`. */
export function actionableName(
  operation: Operation,
  activation: Activation,
): string {
  return `${operation.label} — ${targetPhrase(activation)}`;
}
