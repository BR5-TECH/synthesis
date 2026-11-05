/**
 * The translation of a transcript before it posts (`VOM-voice-mode.md`
 * VOM-FR-QWMK, VOM-FR-DLPA).
 */
import { translateVoiceTranscript } from "../api";
import { logInfo, logWarn } from "../logging";
import { patchVoiceState } from "../state/voiceSession";
import type { DiscussionTarget } from "../types";
import { errorCode } from "./voiceMessages";

export interface Translated {
  /** What posts as the body. */
  text: string;
  /** The transcript as the author spoke it, or null where nothing was translated. */
  original: string | null;
  /** VOM-FR-DLPA: the translation failed. */
  failed: boolean;
}

/**
 * VOM-FR-QWMK, VOM-FR-DLPA: the transcript in the document language of
 * `target`, also with no spoken language. The backend gives a transcript that
 * is already in that language as it stands (AGC-FR-WSKP). With no target, the
 * transcript as it stands. A failure posts the transcript.
 */
export async function translateSpoken(
  spoken: string,
  nicknames: string[],
  target: DiscussionTarget | null,
): Promise<Translated> {
  if (target === null) {
    logWarn(["frontend", "ai"], "a voice transcript has no target to translate for", {});
    return { text: spoken, original: null, failed: false };
  }
  patchVoiceState({ note: "translating" });
  try {
    const { text } = await translateVoiceTranscript(spoken, nicknames, target);
    const translated = text.trim();
    if (translated === "") throw new Error("translation_failed");
    logInfo(["frontend", "ai"], "a voice transcript was translated", {
      chars: spoken.length,
      translatedChars: translated.length,
      changed: translated !== spoken,
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
