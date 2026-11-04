/**
 * The spoken language of the author (`VAI-voice-ai-settings.md` VAI-FR-RGPV,
 * VAI-FR-JPZK). It saves at once, and the main window reads it again through
 * the app-preferences change event (VAI-FR-AVBR).
 */
import { useEffect, useState } from "react";

import { logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences } from "../state/appPreferences";
import {
  SPOKEN_LANGUAGES,
  SPOKEN_LANGUAGE_LIMIT,
  isSpokenLanguage,
  tableCodeOf,
} from "../voice/languages";

const AUTO = "";
const OTHER = "other";

export function VoiceSpokenLanguage() {
  const [stored, setStored] = useState("");
  const [choice, setChoice] = useState(AUTO);
  const [free, setFree] = useState("");

  useEffect(() => {
    let cancelled = false;
    loadAppPreferences()
      .then((prefs) => {
        if (cancelled) return;
        const value = prefs.voiceSpokenLanguage?.trim() ?? "";
        setStored(value);
        const code = tableCodeOf(value);
        setChoice(value === "" ? AUTO : (code ?? OTHER));
        setFree(code === null ? value : "");
      })
      .catch(() => logWarn(["frontend"], "the spoken language could not be read", {}));
    return () => {
      cancelled = true;
    };
  }, []);

  const save = (value: string) => {
    const next = value.trim().slice(0, SPOKEN_LANGUAGE_LIMIT);
    if (next === stored) return;
    const before = stored;
    setStored(next);
    patchAppPreferences({ voiceSpokenLanguage: next }).catch(() => {
      setStored(before);
      logWarn(["frontend"], "the spoken language was not saved", {});
    });
  };

  return (
    <div data-testid="voice-spoken-language" style={{ marginTop: 20 }}>
      <h3 className="t-h3">Spoken language</h3>
      <div className="picker-field" style={{ marginBottom: 10, maxWidth: 360 }}>
        <label className="picker-field__label" htmlFor="voice-spoken-language-choice">
          Spoken language
        </label>
        <select
          id="voice-spoken-language-choice"
          className="input"
          data-testid="voice-spoken-language-choice"
          value={choice}
          onChange={(e) => {
            const next = e.target.value;
            setChoice(next);
            if (next === OTHER) {
              if (free.trim() !== "") save(free);
              return;
            }
            save(next);
          }}
        >
          <option value={AUTO}>Auto (no translation)</option>
          {SPOKEN_LANGUAGES.map((language) => (
            <option key={language.code} value={language.code}>
              {language.name}
            </option>
          ))}
          <option value={OTHER}>Other…</option>
        </select>
      </div>
      {choice === OTHER && (
        <div className="picker-field" style={{ marginBottom: 10, maxWidth: 360 }}>
          <label className="picker-field__label" htmlFor="voice-spoken-language-other">
            Language name
          </label>
          <input
            id="voice-spoken-language-other"
            className="input"
            data-testid="voice-spoken-language-other"
            maxLength={SPOKEN_LANGUAGE_LIMIT}
            placeholder="Basque"
            value={free}
            onChange={(e) => setFree(e.target.value)}
            onBlur={() => save(free)}
          />
        </div>
      )}
      {/* VAI-FR-JPZK. */}
      {isSpokenLanguage(stored) && (
        <div className="t-ui-sm t-muted" data-testid="voice-spoken-language-note">
          Your speech is translated into English before it posts. Agents speak to you in this
          language.
        </div>
      )}
    </div>
  );
}
