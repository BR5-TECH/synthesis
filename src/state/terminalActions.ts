/**
 * The rename and the theme change of a terminal
 * (`../../specifications/ui/TRM-terminals.md` TRM-FR-LIRN, TRM-FR-QXIY).
 * The row of the Terminals panel and the terminal tab both use them.
 */
import * as api from "../api";
import { logDebug, logWarn } from "../logging";
import type { TerminalThemeId } from "../types";
import { errorCode, findTerminal, upsertTerminal } from "./terminals";

/** The text of a refused rename, for the line under the label field. */
function renameMessage(code: string): string {
  if (code === "invalid_label") return "A name has 1 to 64 characters.";
  if (code === "unknown_terminal") return "This terminal is gone.";
  return "The terminal could not be renamed.";
}

/**
 * TRM-FR-LIRN: renames a terminal to the trimmed text. Empty or unchanged text
 * makes no call. Returns null when the field can close, else the error to show.
 */
export async function renameTerminal(id: string, text: string): Promise<string | null> {
  const label = text.trim();
  const current = findTerminal(id)?.label;
  if (label === "" || label === current) return null;
  try {
    const summary = await api.setTerminalLabel(id, label);
    upsertTerminal(summary);
    logDebug(["frontend"], "terminal renamed", { terminalId: id });
    return null;
  } catch (err) {
    const code = errorCode(err);
    logWarn(["frontend"], "terminal rename failed", { terminalId: id, code });
    return renameMessage(code);
  }
}

/** TRM-FR-QXIY: sets or clears the theme override of a terminal. */
export async function setTerminalThemeOverride(id: string, theme: TerminalThemeId | null): Promise<void> {
  try {
    const summary = await api.setTerminalTheme(id, theme);
    upsertTerminal(summary);
    logDebug(["frontend"], "terminal theme changed", { terminalId: id, theme: theme ?? "default" });
  } catch (err) {
    logWarn(["frontend"], "terminal theme change failed", { terminalId: id, code: errorCode(err) });
  }
}
