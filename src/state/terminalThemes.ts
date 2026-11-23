/**
 * The terminal themes (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-CFXM, TRM-FR-PXDQ, TRM-FR-IUMC, TRM-FR-WRVB).
 *
 * A terminal shows its own override where it has one, else the Terminal theme
 * of Settings (`GLS-global-settings.md` GLS-FR-SOJD), else the colours of the
 * app theme.
 */
import type { ITheme } from "ghostty-web";
import { currentTheme } from "../components/markdownMermaidRender";
import type { AppPreferences, TerminalThemeId, TerminalThemePreference } from "../types";

/** A full terminal palette: the four surface colours and the 16 ANSI colours. */
export type TerminalPalette = Required<
  Pick<
    ITheme,
    | "background"
    | "foreground"
    | "cursor"
    | "selectionBackground"
    | "black"
    | "red"
    | "green"
    | "yellow"
    | "blue"
    | "magenta"
    | "cyan"
    | "white"
    | "brightBlack"
    | "brightRed"
    | "brightGreen"
    | "brightYellow"
    | "brightBlue"
    | "brightMagenta"
    | "brightCyan"
    | "brightWhite"
  >
>;

export interface TerminalThemeInfo {
  id: TerminalThemeId;
  name: string;
  dark: boolean;
  palette: TerminalPalette;
}

/** TRM-FR-CFXM: the five themes, the light ones first. */
export const TERMINAL_THEMES: readonly TerminalThemeInfo[] = [
  {
    id: "paper",
    name: "Paper",
    dark: false,
    palette: {
      background: "#fbfaf7", foreground: "#1f2124", cursor: "#2f5fc4", selectionBackground: "#d3def4",
      black: "#24272b", red: "#b3261e", green: "#2a6b2f", yellow: "#7a5600",
      blue: "#2156b8", magenta: "#8a3f9e", cyan: "#13707a", white: "#8b9096",
      brightBlack: "#5a6068", brightRed: "#a1361c", brightGreen: "#3b6e1a", brightYellow: "#6b4f00",
      brightBlue: "#3a5fc0", brightMagenta: "#9b3a7c", brightCyan: "#1f6680", brightWhite: "#b5b9be",
    },
  },
  {
    id: "sand",
    name: "Sand",
    dark: false,
    palette: {
      background: "#f5eedf", foreground: "#2c2418", cursor: "#a64b24", selectionBackground: "#e6d3ae",
      black: "#2c2418", red: "#a8321f", green: "#4b6316", yellow: "#7a5200",
      blue: "#2e5a8a", magenta: "#8c3e6e", cyan: "#2d6a62", white: "#8a7f6c",
      brightBlack: "#5e5444", brightRed: "#9a3b12", brightGreen: "#56661a", brightYellow: "#6e4c00",
      brightBlue: "#3b5f9a", brightMagenta: "#7f4486", brightCyan: "#2f6678", brightWhite: "#b3a891",
    },
  },
  {
    id: "mist",
    name: "Mist",
    dark: false,
    palette: {
      background: "#edf1f4", foreground: "#1b2530", cursor: "#1f6f80", selectionBackground: "#c9d9e5",
      black: "#1b2530", red: "#a8303a", green: "#2c6a3e", yellow: "#6f5a00",
      blue: "#2a57a0", magenta: "#7b4596", cyan: "#16687a", white: "#7d8893",
      brightBlack: "#4f5c69", brightRed: "#9c3a2a", brightGreen: "#38663a", brightYellow: "#665200",
      brightBlue: "#3b5aad", brightMagenta: "#8a3f84", brightCyan: "#1c6470", brightWhite: "#a9b3bc",
    },
  },
  {
    id: "night",
    name: "Night",
    dark: true,
    palette: {
      background: "#10141c", foreground: "#d9dfe8", cursor: "#7cb2ff", selectionBackground: "#2a3652",
      black: "#2a313d", red: "#f07a7a", green: "#7ccf8a", yellow: "#e3c070",
      blue: "#79aaf5", magenta: "#c99af0", cyan: "#6fcdd6", white: "#c3cad4",
      brightBlack: "#5c6676", brightRed: "#ff9a9a", brightGreen: "#9be0a7", brightYellow: "#f2d58f",
      brightBlue: "#9dc2ff", brightMagenta: "#ddb8ff", brightCyan: "#93e0e6", brightWhite: "#eef2f7",
    },
  },
  {
    id: "ember",
    name: "Ember",
    dark: true,
    palette: {
      background: "#1b1512", foreground: "#ece1d4", cursor: "#ff9f5a", selectionBackground: "#4a3429",
      black: "#34291f", red: "#f2785e", green: "#a9c46b", yellow: "#eab35c",
      blue: "#8fb0e0", magenta: "#dc93b8", cyan: "#82c4b4", white: "#cfc2b2",
      brightBlack: "#665648", brightRed: "#ff9a80", brightGreen: "#c2d98a", brightYellow: "#f5cb80",
      brightBlue: "#aec8ef", brightMagenta: "#ecb1cf", brightCyan: "#a2d8ca", brightWhite: "#f7eee4",
    },
  },
];

export function findTerminalTheme(id: string | null | undefined): TerminalThemeInfo | undefined {
  return TERMINAL_THEMES.find((t) => t.id === id);
}

function cssVar(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value || fallback;
}

const LIGHT_ANSI: ITheme = {
  black: "#24292f", red: "#cf222e", green: "#116329", yellow: "#4d2d00",
  blue: "#0969da", magenta: "#8250df", cyan: "#1b7c83", white: "#6e7781",
  brightBlack: "#57606a", brightRed: "#a40e26", brightGreen: "#1a7f37", brightYellow: "#633c01",
  brightBlue: "#218bff", brightMagenta: "#a475f9", brightCyan: "#3192aa", brightWhite: "#8c959f",
};

const DARK_ANSI: ITheme = {
  black: "#484f58", red: "#ff7b72", green: "#3fb950", yellow: "#d29922",
  blue: "#58a6ff", magenta: "#bc8cff", cyan: "#39c5cf", white: "#b1bac4",
  brightBlack: "#6e7681", brightRed: "#ffa198", brightGreen: "#56d364", brightYellow: "#e3b341",
  brightBlue: "#79c0ff", brightMagenta: "#d2a8ff", brightCyan: "#56d4dd", brightWhite: "#f0f6fc",
};

/** TRM-FR-WRVB: the terminal colors of the app theme. */
export function appTerminalTheme(): ITheme {
  const dark = currentTheme() === "dark";
  return {
    ...(dark ? DARK_ANSI : LIGHT_ANSI),
    background: cssVar("--bg-canvas", dark ? "#16181d" : "#ffffff"),
    foreground: cssVar("--fg-1", dark ? "#e6e6e6" : "#1f2328"),
    cursor: cssVar("--accent", dark ? "#7aa2f7" : "#0969da"),
    selectionBackground: cssVar("--bg-selection", dark ? "#2d3a55" : "#cfe3ff"),
  };
}

/** The theme a terminal shows, with a key that changes when its colours change. */
export interface ResolvedTerminalTheme {
  key: string;
  theme: ITheme;
}

/** TRM-FR-IUMC: the override, else the Terminal theme of Settings, else the app theme. */
export function resolveTerminalTheme(
  override: TerminalThemeId | null | undefined,
  preference: TerminalThemePreference = getTerminalThemePreference(),
): ResolvedTerminalTheme {
  const chosen = findTerminalTheme(override) ?? findTerminalTheme(preference);
  if (chosen) return { key: chosen.id, theme: { ...chosen.palette } };
  const theme = appTerminalTheme();
  return { key: `app:${currentTheme()}:${theme.background}`, theme };
}

// GLS-FR-SOJD, GLS-FR-VTVK: the Terminal theme of Settings, as this window last
// read it, and who follows it.
let preference: TerminalThemePreference = "app";
const listeners = new Set<() => void>();

export function getTerminalThemePreference(): TerminalThemePreference {
  return preference;
}

export function setTerminalThemePreference(next: TerminalThemePreference): void {
  const valid: TerminalThemePreference = next === "app" || findTerminalTheme(next) ? next : "app";
  if (valid === preference) return;
  preference = valid;
  for (const listener of [...listeners]) listener();
}

/** GSS-FR-FRBK: a missing or unknown stored value reads as `app`. */
export function adoptStoredTerminalTheme(prefs: Pick<AppPreferences, "terminalTheme">): void {
  setTerminalThemePreference(prefs.terminalTheme ?? "app");
}

export function subscribeTerminalThemePreference(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Resets the Terminal theme of Settings between tests. The listeners stay:
 * `resetTerminalViewsForTest` takes away the one that the views hold.
 */
export function resetTerminalThemePreferenceForTest(): void {
  preference = "app";
}
