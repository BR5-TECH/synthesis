/**
 * The voice of a Custom provider as two selectors (`VAI-voice-ai-settings.md`
 * VAI-FR-KWSN, VAI-FR-PMDX; `AGT-agents.md` AGT-FR-HXVQ).
 *
 * The speech route of a local server such as Ollama with LFM3-Audio takes a
 * voice as a sex and an optional language, for example `female-polish`.
 */
import { useEffect, useId, useRef, useState } from "react";

/** VAI-FR-KWSN: the languages that a voice name can carry. */
export const VOICE_LANGUAGES = [
  "american-english",
  "arabic",
  "british-english",
  "chinese",
  "danish",
  "dutch",
  "english",
  "finnish",
  "french",
  "german",
  "greek",
  "hebrew",
  "italian",
  "japanese",
  "korean",
  "polish",
  "portuguese",
  "spanish",
  "swedish",
  "turkish",
] as const;

export type VoiceSex = "" | "female" | "male" | "other";

export interface VoiceChoice {
  /** "" is the default voice, and "other" a free name. */
  sex: VoiceSex;
  /** "" is no language. */
  language: string;
  /** The free name of "other". */
  other: string;
}

/** VAI-FR-PMDX: the two selectors of a stored name. */
export function parseVoiceName(name: string): VoiceChoice {
  const value = name.trim();
  if (value === "") return { sex: "", language: "", other: "" };
  const match = /^(female|male)(?:-(.+))?$/.exec(value);
  if (match) {
    const language = match[2] ?? "";
    if (language === "" || (VOICE_LANGUAGES as readonly string[]).includes(language)) {
      return { sex: match[1] as "female" | "male", language, other: "" };
    }
  }
  return { sex: "other", language: "", other: value };
}

/** VAI-FR-PMDX: the name of two selectors. */
export function composeVoiceName(choice: VoiceChoice): string {
  switch (choice.sex) {
    case "":
      return "";
    case "other":
      return choice.other.trim();
    default:
      return choice.language === "" ? choice.sex : `${choice.sex}-${choice.language}`;
  }
}

export function VoiceNamePicker({
  value,
  defaultLabel,
  disabled = false,
  liveName = false,
  testId,
  onChange,
}: {
  value: string;
  /** "Server default" on the Custom tab, "Provider default" for an agent. */
  defaultLabel: string;
  disabled?: boolean;
  /** AGT-FR-HXVQ: the name field changes the value at each key, not on blur. */
  liveName?: boolean;
  testId: string;
  /** A promise that rejects is a failed save: the picker shows the stored value again. */
  onChange: (name: string) => void | Promise<unknown>;
}) {
  const id = useId();
  const [choice, setChoice] = useState(() => parseVoiceName(value));
  // The last name that this picker sent, or the stored one. The stored value
  // refreshes after a save, so a second change before that is not lost.
  const sent = useRef(value.trim());
  // A new stored value (a save, or another provider) replaces the local one.
  useEffect(() => {
    sent.current = value.trim();
    setChoice((current) =>
      composeVoiceName(current) === value.trim() ? current : parseVoiceName(value),
    );
  }, [value]);

  const apply = (next: VoiceChoice, commit: boolean) => {
    setChoice(next);
    const name = composeVoiceName(next);
    // An "Other name…" with no name yet changes nothing.
    const unnamed = next.sex === "other" && name === "";
    if (commit && !unnamed && name !== sent.current) {
      sent.current = name;
      const saved = onChange(name);
      if (saved instanceof Promise) {
        saved.catch(() => {
          // A failed save sends the same name again on the next choice.
          sent.current = value.trim();
          setChoice(parseVoiceName(value));
        });
      }
    }
  };
  const named = choice.sex === "female" || choice.sex === "male";

  return (
    <div data-testid={testId}>
      <div style={{ display: "flex", gap: 8 }}>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 0" }}>
          <span className="t-ui-sm">Voice</span>
          <select
            className="input"
            style={{ scrollMarginBottom: 72 }}
            data-testid={`${testId}-sex`}
            disabled={disabled}
            value={choice.sex}
            onChange={(e) => {
              const sex = e.target.value as VoiceSex;
              // The language stays only from one sex to the other. A language
              // that the selector does not show is not kept.
              const kept = named && (sex === "female" || sex === "male") ? choice.language : "";
              apply({ ...choice, sex, language: kept }, true);
            }}
          >
            <option value="">{defaultLabel}</option>
            <option value="female">Female</option>
            <option value="male">Male</option>
            <option value="other">Other name…</option>
          </select>
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 0" }}>
          <span className="t-ui-sm">Voice language</span>
          <select
            className="input"
            style={{ scrollMarginBottom: 72 }}
            data-testid={`${testId}-language`}
            disabled={disabled || !named}
            value={named ? choice.language : ""}
            onChange={(e) => apply({ ...choice, language: e.target.value }, true)}
          >
            <option value="">None</option>
            {VOICE_LANGUAGES.map((language) => (
              <option key={language} value={language}>
                {language}
              </option>
            ))}
          </select>
        </label>
      </div>
      {choice.sex === "other" && (
        <label
          htmlFor={id}
          style={{ display: "flex", flexDirection: "column", gap: 4, marginTop: 6 }}
        >
          <span className="t-ui-sm">Voice name</span>
          <input
            id={id}
            className="input input--mono"
            style={{ scrollMarginBottom: 72 }}
            data-testid={`${testId}-name`}
            spellCheck={false}
            disabled={disabled}
            value={choice.other}
            onChange={(e) => apply({ ...choice, other: e.target.value }, liveName)}
            onBlur={() => apply(choice, true)}
          />
        </label>
      )}
    </div>
  );
}
