/**
 * VOM-FR-TAQJ: speech has no `@`. At the start of a transcript, a spoken address
 * of a known agent becomes the tag that a typed comment would hold.
 */
import type { AgentRoster } from "../components/agentTags";

function escaped(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
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
  return text;
}
