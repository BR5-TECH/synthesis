/**
 * The translation of a transcript before it posts (`VOM-voice-mode.md`
 * VOM-FR-QWMK, VOM-FR-DLPA).
 */
import { translateVoiceTranscript } from "../api";
import { logInfo, logWarn } from "../logging";
import { peekAppPreferences } from "../state/appPreferences";
import { patchVoiceState } from "../state/voiceSession";
import { isSpokenLanguage } from "./languages";
import { errorCode } from "./voiceMessages";

export interface Translated {
  /** What posts as the body. */
  text: string;
  /** The transcript as the author spoke it, or null where nothing was translated. */
  original: string | null;
  /** VOM-FR-DLPA: a spoken language is set and the translation failed. */
  failed: boolean;
}

/**
 * VOM-FR-QWMK, VOP-FR-BYZC: the spoken language of the preferences, or "" for
 * none. A value that is no spoken language is none, as the backend reads it.
 */
export function spokenLanguage(): string {
  const value = peekAppPreferences()?.voiceSpokenLanguage?.trim() ?? "";
  return isSpokenLanguage(value) ? value : "";
}

/**
 * VOM-FR-QWMK, VOM-FR-DLPA: the transcript in English. With no spoken
 * language, the transcript as it stands. A failure posts the transcript.
 */
export async function translateSpoken(spoken: string, nicknames: string[]): Promise<Translated> {
  if (spokenLanguage() === "") return { text: spoken, original: null, failed: false };
  patchVoiceState({ note: "translating" });
  try {
    const { text } = await translateVoiceTranscript(spoken, nicknames);
    const translated = text.trim();
    if (translated === "") throw new Error("translation_failed");
    logInfo(["frontend", "ai"], "a voice transcript was translated", {
      chars: spoken.length,
      translatedChars: translated.length,
    });
    // VOM-FR-DLPA: a translation equal to the transcript keeps no original.
    return {
      text: translated,
      original: translated === spoken ? null : spoken,
      failed: false,
    };
  } catch (error) {
    logWarn(["frontend", "ai"], "a voice transcript was not translated", {
      code: errorCode(error),
    });
    return { text: spoken, original: null, failed: true };
  }
}
