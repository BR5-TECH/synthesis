/**
 * DDS-FR-KXWQ / DDS-FR-TSPL / DDS-FR-ZQVE: what the context indicator shows,
 * worked out from the figures the backend gives. Pure, so the rules can be
 * tested without a column.
 */
import type {
  ContextSplit,
  DiscussionContext,
  DocumentSize,
  TurnContext,
} from "../../types";

/** The indicator's figures, ready to render. */
export interface ShownContext {
  /** The tokens the value is made of. */
  tokens: number;
  /** True where `tokens` is an estimate rather than what the provider reported. */
  estimate: boolean;
  window: number | null;
  /** A whole percent of `window`, or `null` where no window is known. */
  percent: number | null;
  split: ContextSplit;
  document: DocumentSize | null;
  limitTokens: number | null;
  /** DDS-FR-ZQVE: the warning stands. Never true without a known window. */
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
export function percentOf(tokens: number, window: number | null): number | null {
  if (window === null || window <= 0) return null;
  return Math.round((tokens * 100) / window);
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
    const window = reported.window;
    return {
      tokens,
      estimate: reported.inputTokens === null,
      window,
      percent: percentOf(tokens, window),
      split: reported.split,
      document: sizeSource.document,
      limitTokens: sizeSource.limitTokens,
      aboveLimit: window !== null && sizeSource.aboveLimit,
    };
  }
  if (estimated) {
    return {
      tokens: estimated.estimatedTokens,
      estimate: true,
      window: estimated.window,
      percent: percentOf(estimated.estimatedTokens, estimated.window),
      split: estimated.split,
      document: estimated.document,
      limitTokens: estimated.limitTokens,
      aboveLimit: estimated.window !== null && estimated.aboveLimit,
    };
  }
  return null;
}

/** A token count in full, for a tooltip: `84,210`. */
export function fullTokens(n: number): string {
  return n.toLocaleString("en-US");
}

/** A token count in short, for the header: `840`, `8.4k`, `84k`. */
export function shortTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1)}k`;
  return `${Math.round(n / 1000)}k`;
}

/** DDS-FR-CTXI / DDS-FR-TSPL: the text the header shows. */
export function indicatorText(shown: ShownContext): string {
  const mark = shown.estimate ? "~" : "";
  return shown.percent === null
    ? `${mark}${shortTokens(shown.tokens)} tokens`
    : `${mark}${shown.percent}%`;
}

/** DDS-FR-WRNB: the indicator's accessible name, which states its value. */
export function indicatorName(shown: ShownContext): string {
  const kind = shown.estimate ? ", estimated" : "";
  return shown.percent === null
    ? `Context use: ${fullTokens(shown.tokens)} tokens${kind}`
    : `Context use: ${shown.percent}% of the model's context window${kind}`;
}
