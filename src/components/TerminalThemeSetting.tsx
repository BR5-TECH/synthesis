/**
 * The Terminal theme selector of the Appearance section
 * (`../../specifications/ui/GLS-global-settings.md` GLS-FR-SOJD, GLS-FR-VTVK).
 *
 * **Match app theme** and the five terminal themes, each shown as a small
 * terminal in its own colours. A choice applies at once and saves at once, so
 * the selector holds no dirty state.
 */
import { useEffect, useRef, useState } from "react";
import { logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences } from "../state/appPreferences";
import {
  TERMINAL_THEMES,
  adoptStoredTerminalTheme,
  appTerminalTheme,
  getTerminalThemePreference,
  setTerminalThemePreference,
  type TerminalPalette,
} from "../state/terminalThemes";
import type { TerminalThemePreference } from "../types";

interface Choice {
  value: TerminalThemePreference;
  name: string;
  palette: Pick<TerminalPalette, "background" | "foreground" | "red" | "green" | "yellow" | "blue">;
}

function choices(): Choice[] {
  const app = appTerminalTheme();
  return [
    {
      value: "app",
      name: "Match app theme",
      palette: {
        background: app.background ?? "",
        foreground: app.foreground ?? "",
        red: app.red ?? "",
        green: app.green ?? "",
        yellow: app.yellow ?? "",
        blue: app.blue ?? "",
      },
    },
    ...TERMINAL_THEMES.map((t) => ({ value: t.id, name: t.name, palette: t.palette })),
  ];
}

/** A terminal in miniature: a prompt and four ANSI colours on the theme's background. */
function Swatch({ palette }: { palette: Choice["palette"] }) {
  return (
    <span className="terminal-theme-tile__screen" aria-hidden="true" style={{ background: palette.background }}>
      <span className="terminal-theme-tile__prompt" style={{ color: palette.foreground }}>
        ~ $ ls
      </span>
      <span className="terminal-theme-tile__ansi">
        {[palette.red, palette.green, palette.yellow, palette.blue].map((color, i) => (
          <span key={i} style={{ background: color }} />
        ))}
      </span>
    </span>
  );
}

export function TerminalThemeSetting() {
  const [value, setValue] = useState<TerminalThemePreference | null>(null);
  const [error, setError] = useState("");
  const saved = useRef<TerminalThemePreference>("app");
  /** Counts the choices, so the answer of an older save changes nothing. */
  const latest = useRef(0);

  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (cancelled) return;
      // GSS-FR-FRBK: a missing or unknown stored value reads as `app`.
      adoptStoredTerminalTheme(prefs);
      const stored = getTerminalThemePreference();
      saved.current = stored;
      setValue(stored);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const select = async (next: TerminalThemePreference) => {
    if (next === value) return;
    const choice = ++latest.current;
    setValue(next);
    setError("");
    setTerminalThemePreference(next);
    try {
      await patchAppPreferences({ terminalTheme: next });
      saved.current = next;
    } catch (err) {
      logWarn(["frontend"], "terminal theme save failed", { theme: next });
      // A later choice is on screen already: its own save decides what stays.
      if (choice !== latest.current) return;
      setValue(saved.current);
      setTerminalThemePreference(saved.current);
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const list = choices();
  const focused = Math.max(0, list.findIndex((c) => c.value === (value ?? "app")));

  return (
    <div className="terminal-theme-setting">
      <h3 className="t-eyebrow" style={{ margin: "28px 0 10px" }}>
        Terminal theme
      </h3>
      <p className="t-p" style={{ marginBottom: 16 }}>
        The colours of terminal tabs. A terminal can also have a theme of its own, from its menu.
      </p>
      <div
        role="radiogroup"
        aria-label="Terminal theme"
        className="terminal-theme-setting__tiles"
        onKeyDown={(e) => {
          const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
          if (step === 0) return;
          e.preventDefault();
          const next = list[(focused + step + list.length) % list.length];
          void select(next.value);
          const buttons = e.currentTarget.querySelectorAll<HTMLElement>('[role="radio"]');
          buttons[(focused + step + list.length) % list.length]?.focus();
        }}
      >
        {list.map((choice, i) => {
          const checked = (value ?? "app") === choice.value;
          return (
            <button
              key={choice.value}
              type="button"
              role="radio"
              aria-checked={checked}
              tabIndex={i === focused ? 0 : -1}
              className="terminal-theme-tile"
              data-active={checked || undefined}
              disabled={value === null}
              onClick={() => void select(choice.value)}
            >
              <Swatch palette={choice.palette} />
              <span className="terminal-theme-tile__name">{choice.name}</span>
            </button>
          );
        })}
      </div>
      {error && (
        <span className="picker-error" role="alert" style={{ display: "block", marginTop: 8 }}>
          ✗ {error}
        </span>
      )}
    </div>
  );
}
