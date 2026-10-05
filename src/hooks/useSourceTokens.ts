import { useEffect, useRef, useState } from "react";
import { logDebug } from "../logging";
import {
  resolveLanguage,
  tokenize,
  type TokenSpan,
} from "../state/syntaxHighlight";

/**
 * ESH-FR-MJRH: how long the surface waits before tokenising what it holds.
 *
 * Short enough that a pause in typing brings the colour back before the author
 * has read the line they just wrote, and long enough that a burst of typing
 * tokenises once at the end of it rather than once per character. The same
 * shape as the rest-after-typing an artifact writes itself on (EDT-FR-70), for
 * the same reason.
 */
export const HIGHLIGHT_DELAY_MS = 60;

/** What was tokenised, and what came back for it. */
interface Tokenised {
  name: string;
  text: string;
  spans: TokenSpan[] | null;
}

/**
 * ESH-FR-GCEN: the most spans a surface will draw before giving up and rendering
 * the file plain.
 *
 * Every span becomes an element on the layer, so a file with hundreds of
 * thousands of them costs a DOM the size of the file's own token count on every
 * pass — and a 40 000-line source file reaches that easily. Past this point the
 * colouring is what would make the editor unusable rather than what makes it
 * readable, and ESH-FR-GCEN's answer to that is the plain text, which is a
 * complete and fully editable reading of the file.
 *
 * Set well above any file a person edits by hand: the whole of this project's
 * largest source file tokenises to a few thousand spans.
 */
export const MAX_TOKEN_SPANS = 20_000;

/**
 * Tokenise, and refuse a result too large to draw (ESH-FR-GCEN).
 *
 * A refusal reads to the caller exactly as a file with no language does — the
 * plain text — and is told apart from one in the record the caller emits, so a
 * reader who wonders why one enormous file is plain while everything else is
 * coloured can find the reason.
 */
function tokeniseWithin(text: string, language: string): TokenSpan[] | null {
  const spans = tokenize(text, language);
  if (spans === null) return null;
  if (spans.length > MAX_TOKEN_SPANS) return null;
  return spans;
}

/**
 * ESH-FR-BABL/ESH-FR-VUVO: the token spans of a source surface's text, or `null`
 * while there are none to show.
 *
 * `null` is the plain rendering, and it is the answer in every case the surface
 * has to survive: a Markdown file (never highlighted), a file no language
 * resolves for, a tokenisation that failed, and — this is the one that recurs —
 * the interval between the text changing and the result for *that* text
 * arriving. The spans are offsets into a specific string, so returning a result
 * computed for an earlier one would colour the wrong ranges of what the author
 * is now looking at; the text is therefore carried alongside the spans and
 * compared before they are handed out.
 *
 * The work itself runs on a timer rather than during render, which is what keeps
 * it off the typing path: a keystroke re-renders the plain text and returns, and
 * the tokenising happens after the author stops. A file whose tokens are not
 * ready is fully editable in the meantime — it simply has no colour yet.
 */
export function useSourceTokens(
  name: string,
  text: string,
  enabled: boolean,
): TokenSpan[] | null {
  const [done, setDone] = useState<Tokenised | null>(null);
  /**
   * Bumped for every scheduled pass, so a result whose generation is no longer
   * current is dropped rather than applied. The timer's own `clearTimeout`
   * covers a pass that has not started; this covers the rest — a pass that ran
   * and is now resolving into a surface showing another file (ESH-FR-VUVO).
   */
  const generation = useRef(0);
  /**
   * The last resolution reported for this surface, so the record below is
   * emitted when the answer *changes* rather than on every pass.
   *
   * Without it a burst of typing would put one record per pass into a bounded
   * ring buffer, and the evidence a reader actually came for — anything else the
   * session recorded — would be pushed out by a hundred copies of the same line.
   */
  const reported = useRef<string | null>(null);

  useEffect(() => {
    generation.current += 1;
    const mine = generation.current;
    if (!enabled) {
      // Not merely "no spans": dropping the record too is what stops a later
      // re-enable from flashing the previous file's colouring over this one's
      // text for the frame before the new pass lands.
      setDone(null);
      reported.current = null;
      return;
    }
    const timer = setTimeout(() => {
      if (generation.current !== mine) return;
      const resolved = resolveLanguage(name, text);
      const spans = resolved
        ? tokeniseWithin(text, resolved.language)
        : null;
      if (generation.current !== mine) return;
      // "Why is this file not coloured?" is the one question this feature
      // prompts, and the answer — no language resolved, one did and the
      // tokenising came back empty, or the file is too large to draw — is
      // otherwise invisible. The file's path is named; its contents never are.
      const outcome = resolved
        ? spans === null
          ? `${resolved.language} (not drawn)`
          : `${resolved.language} (${resolved.source})`
        : "none";
      // Keyed by the file as well as the answer: two files that resolve the same
      // way are two facts, and a surface pointed at a second one has to record
      // its own rather than inherit the first's silence.
      if (reported.current !== `${name}\u0000${outcome}`) {
        reported.current = `${name}\u0000${outcome}`;
        logDebug(["frontend"], "source language resolved", {
          file: name,
          language: outcome,
          tokens: spans?.length ?? 0,
        });
      }
      setDone({ name, text, spans });
    }, HIGHLIGHT_DELAY_MS);
    return () => clearTimeout(timer);
  }, [name, text, enabled]);

  if (!enabled || !done) return null;
  if (done.name !== name || done.text !== text) return null;
  return done.spans;
}

/** Two revisions' spans, each in its own revision's coordinates. */
export interface RevisionSpans {
  old: TokenSpan[] | null;
  new: TokenSpan[] | null;
}

/** What was tokenised for a pair, and what came back. */
interface TokenisedPair extends RevisionSpans {
  name: string;
  oldText: string;
  newText: string;
}

const NO_PAIR: RevisionSpans = { old: null, new: null };

/**
 * DFV-FR-57: the tokens of **both revisions** of one file, under **one**
 * resolved language.
 *
 * The single resolution is the whole point of this being its own hook rather
 * than two calls to `useSourceTokens`. A file the extension map names resolves
 * the same way from either revision, but one it does not — an extensionless
 * script, an unmapped extension — is resolved from its *text*, and the two
 * revisions of a file under edit are two different texts. Resolved separately
 * they can disagree, and a comparison whose left pane reads as one language and
 * whose right reads as another is exactly what "the original and the target read
 * in one language rather than in two" rules out.
 *
 * The **new** revision decides, because it is the file as it will land; a
 * comparison that deletes the file has no new revision and falls back to the
 * old, which is the only text it has.
 */
export function useRevisionSpans(
  name: string,
  oldText: string,
  newText: string,
  enabled: boolean,
): RevisionSpans {
  const [done, setDone] = useState<TokenisedPair | null>(null);
  const generation = useRef(0);

  useEffect(() => {
    generation.current += 1;
    const mine = generation.current;
    if (!enabled) {
      setDone(null);
      return;
    }
    const timer = setTimeout(() => {
      if (generation.current !== mine) return;
      // One resolution, from the revision that is the file as it will land.
      const resolved = resolveLanguage(name, newText !== "" ? newText : oldText);
      const spans: RevisionSpans = resolved
        ? {
            old: oldText === "" ? null : tokeniseWithin(oldText, resolved.language),
            new: newText === "" ? null : tokeniseWithin(newText, resolved.language),
          }
        : NO_PAIR;
      if (generation.current !== mine) return;
      setDone({ name, oldText, newText, ...spans });
    }, HIGHLIGHT_DELAY_MS);
    return () => clearTimeout(timer);
  }, [name, oldText, newText, enabled]);

  if (!enabled || !done) return NO_PAIR;
  // The spans index these exact strings, so a result for either revision the
  // comparison has moved on from is not handed out for it (ESH-FR-VUVO).
  if (done.name !== name || done.oldText !== oldText || done.newText !== newText) {
    return NO_PAIR;
  }
  return done;
}
