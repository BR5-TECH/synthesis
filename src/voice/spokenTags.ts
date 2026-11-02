/**
 * VOM-FR-TAQJ, VOM-FR-SBSS, VOM-FR-CXXP: speech has no `@`. At the start of a
 * transcript, a spoken address of a known agent becomes the tag that a typed
 * comment would hold.
 */
import type { AgentRoster } from "../components/agentTags";

function escaped(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Levenshtein distance, without case. */
export function editDistance(a: string, b: string): number {
  const x = a.toLowerCase();
  const y = b.toLowerCase();
  let row = Array.from({ length: y.length + 1 }, (_, i) => i);
  for (let i = 1; i <= x.length; i += 1) {
    const next = [i];
    for (let j = 1; j <= y.length; j += 1) {
      const cost = x[i - 1] === y[j - 1] ? 0 : 1;
      next.push(Math.min(row[j] + 1, next[j - 1] + 1, row[j - 1] + cost));
    }
    row = next;
  }
  return row[y.length];
}

/** VOM-FR-CXXP: the largest distance that a nickname of this length accepts. */
function fuzzyLimit(nickname: string): number {
  if (nickname.length < 4) return 0;
  return nickname.length <= 6 ? 1 : 2;
}

/**
 * VOM-FR-SBSS, VOM-FR-CXXP: the nickname that the first word names. An exact
 * match wins. A fuzzy match needs a comma after the word, and one nickname that
 * is strictly the closest.
 */
export function matchBareNickname(
  word: string,
  comma: boolean,
  nicknames: readonly string[],
): string | null {
  const exact = nicknames.find((n) => n.toLowerCase() === word.toLowerCase());
  if (exact) return exact;
  if (!comma) return null;
  let best: string | null = null;
  let bestDistance = Infinity;
  let tie = false;
  for (const nickname of nicknames) {
    const limit = fuzzyLimit(nickname);
    if (limit === 0) continue;
    const distance = editDistance(word, nickname);
    if (distance > limit) continue;
    if (distance < bestDistance) {
      best = nickname;
      bestDistance = distance;
      tie = false;
    } else if (distance === bestDistance) {
      tie = true;
    }
  }
  return tie ? null : best;
}

export function withSpokenTags(transcript: string, roster: AgentRoster): string {
  const text = transcript.trim();
  const trailing = "(?=$|[\\s,.:;!?])[,.:;!?]*\\s*";
  // The longest name first, so that `arch` does not take the start of `archer`.
  const nicknames = [...roster.nicknames].sort((a, b) => b.length - a.length);
  for (const nickname of nicknames) {
    const pattern = new RegExp(`^(?:at|hey)\\s+${escaped(nickname)}${trailing}`, "i");
    const match = pattern.exec(text);
    if (match) return `@${nickname} ${text.slice(match[0].length)}`.trim();
  }
  const everyone = new RegExp(`^at\\s+all${trailing}`, "i").exec(text);
  if (everyone && roster.nicknames.length > 0) {
    return `@all ${text.slice(everyone[0].length)}`.trim();
  }
  // VOM-FR-SBSS: a bare nickname as the first word, with its punctuation removed.
  const first = /^([^\s,.:;!?]+)([,.:;!?]*)\s*/.exec(text);
  if (first) {
    const named = matchBareNickname(first[1], first[2].startsWith(","), nicknames);
    if (named) return `@${named} ${text.slice(first[0].length)}`.trim();
  }
  return text;
}
