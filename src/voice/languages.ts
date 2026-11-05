/**
 * The spoken languages (`VOP-voice-ai-providers.md` VOP-FR-IRZC, VOP-FR-BYZC).
 * This mirrors the table of `src-tauri/src/voice_ai/languages.rs`.
 */

export const SPOKEN_LANGUAGES: readonly { code: string; name: string }[] = [
  { code: "es", name: "Spanish" },
  { code: "fr", name: "French" },
  { code: "de", name: "German" },
  { code: "it", name: "Italian" },
  { code: "pt", name: "Portuguese" },
  { code: "ru", name: "Russian" },
  { code: "uk", name: "Ukrainian" },
  { code: "pl", name: "Polish" },
  { code: "nl", name: "Dutch" },
  { code: "tr", name: "Turkish" },
  { code: "ar", name: "Arabic" },
  { code: "hi", name: "Hindi" },
  { code: "zh", name: "Chinese" },
  { code: "ja", name: "Japanese" },
  { code: "ko", name: "Korean" },
];

/** GSS-FR-DNAW: the longest stored value. */
export const SPOKEN_LANGUAGE_LIMIT = 40;

/**
 * VOP-FR-BYZC: whether a stored value is a spoken language — a code or a name
 * of the table, or a free name of letters, spaces, hyphens, and brackets. The
 * backend reads each other value as no spoken language.
 */
export function isSpokenLanguage(value: string): boolean {
  const v = value.trim();
  if (v === "" || v.length > SPOKEN_LANGUAGE_LIMIT) return false;
  return tableCodeOf(v) !== null || /^[\p{L} ()-]+$/u.test(v);
}

/** The code of the table that a stored value names, or null for a free name. */
export function tableCodeOf(value: string): string | null {
  const v = value.trim().toLowerCase();
  const found = SPOKEN_LANGUAGES.find((l) => l.code === v || l.name.toLowerCase() === v);
  return found?.code ?? null;
}
