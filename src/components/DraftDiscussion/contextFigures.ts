/**
 * DDS-FR-KXWQ / DDS-FR-TSPL / DDS-FR-WUCD / DDS-FR-ZQVE: what the context
 * indicator shows, worked out from the figures the backend gives. Pure, so the
 * rules can be tested without a column.
 */
import type {
  ContextSplit,
  DiscussionContext,
  DocumentSize,
  TurnContext,
} from "../../types";

/** DDS-FR-WUCD: how full the window is. */
export type ContextLevel = "normal" | "high" | "critical";

/** The indicator's figures, ready to render. */
export interface ShownContext {
  /** The tokens the value is made of. */
  tokens: number;
  /** True where `tokens` is an estimate rather than what the provider reported. */
  estimate: boolean;
  /** The effective window, which is always known (DDS-FR-TSPL). */
  window: number;
  /** A whole percent of `window`. */
  percent: number;
  /** DDS-FR-WUCD: the level of `percent`. */
  level: ContextLevel;
  split: ContextSplit;
  document: DocumentSize | null;
  limitTokens: number;
  /** DDS-FR-ZQVE: the warning stands. It does not depend on `level`. */
  aboveLimit: boolean;
}

/** The sum of a split, which stands for a total no provider reported. */
export function splitTotal(split: ContextSplit): number {
  return (
    split.instructions +
    split.document +
    split.history +
    split.currentComment +
    split.toolResults
  );
}

/** DDS-FR-CTXI: the share of the window, as a whole percent. */
export function percentOf(tokens: number, window: number): number {
  if (window <= 0) return 0;
  return Math.round((tokens * 100) / window);
}

/**
 * DDS-FR-WUCD: `normal` below 50%, `high` from 50% to below 85%, and
 * `critical` from 85% up.
 */
export function levelOf(percent: number): ContextLevel {
  if (percent >= 85) return "critical";
  if (percent >= 50) return "high";
  return "normal";
}

/** DDS-FR-WCET: the share of the ring that is filled, from 0 to 1. */
export function ringFill(percent: number): number {
  return Math.min(Math.max(percent, 0), 100) / 100;
}

function shown(
  tokens: number,
  estimate: boolean,
  window: number,
  split: ContextSplit,
  sizeSource: TurnContext | DiscussionContext,
): ShownContext {
  const percent = percentOf(tokens, window);
  return {
    tokens,
    estimate,
    window,
    percent,
    level: levelOf(percent),
    split,
    document: sizeSource.document,
    limitTokens: sizeSource.limitTokens,
    aboveLimit: sizeSource.aboveLimit,
  };
}

/**
 * DDS-FR-KXWQ: the latest turn's reported figures win over the estimate. The
 * document size and the warning come from the newer of the two, so a save
 * after a turn shows the size of the saved document (DDS-FR-HJMF).
 */
export function shownContext(
  reported: TurnContext | null,
  estimated: DiscussionContext | null,
  documentFromEstimate: boolean,
): ShownContext | null {
  if (reported) {
    const tokens = reported.inputTokens ?? splitTotal(reported.split);
    const sizeSource = documentFromEstimate && estimated ? estimated : reported;
    // DDS-FR-TSPL: the window is the one of the agent that answered the turn.
    return shown(tokens, reported.inputTokens === null, reported.window, reported.split, sizeSource);
  }
  if (estimated) {
    return shown(estimated.estimatedTokens, true, estimated.window, estimated.split, estimated);
  }
  return null;
}

/** A token count in full, for a tooltip: `84,210`. */
export function fullTokens(n: number): string {
  return n.toLocaleString("en-US");
}

/** DDS-FR-WRNB: the level as the tooltip and the accessible name say it. */
export function levelLabel(level: ContextLevel): string {
  return level.charAt(0).toUpperCase() + level.slice(1);
}

/** DDS-FR-CTXI: the text the header shows. */
export function indicatorText(shown: ShownContext): string {
  return `${shown.estimate ? "~" : ""}${shown.percent}%`;
}

/** DDS-FR-WRNB: the indicator's accessible name, its percent and its level. */
export function indicatorName(shown: ShownContext): string {
  const kind = shown.estimate ? ", estimated" : "";
  return `Context use: ${shown.percent}% of the model's context window, level ${shown.level}${kind}`;
}
