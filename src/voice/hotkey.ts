/**
 * VAI-FR-FRJY, VAI-FR-BGBL, VAI-FR-WOCD: a hotkey as text, such as
 * `Ctrl+Shift+Space`. It is built from `event.code`, so that a keyboard layout
 * and the Option key do not change what a chord is called.
 */
import type { VoiceHotkeyBehaviour } from "../types";

export const DEFAULT_VOICE_HOTKEY = "Ctrl+Shift+Space";

export interface ChordEvent {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const MODIFIER_CODE = /^(Control|Alt|Shift|Meta|OS)(Left|Right)?$/;

function keyName(code: string): string | null {
  if (MODIFIER_CODE.test(code)) return null;
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1];
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1];
  if (code.startsWith("Arrow")) return code.slice("Arrow".length);
  return code;
}

/** The chord of a key event, or `null` for a modifier alone. */
export function chordOf(event: ChordEvent): string | null {
  const key = keyName(event.code);
  if (key === null) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Cmd");
  parts.push(key);
  return parts.join("+");
}

export function matchesHotkey(event: ChordEvent, chord: string): boolean {
  return chordOf(event) === chord;
}

/** The main key of a chord. Its `keyup` ends a hold. */
export function mainKeyOf(chord: string): string {
  const parts = chord.split("+");
  return parts[parts.length - 1] ?? "";
}

/** VAI-FR-BGBL: chords that the application already binds, after Ctrl or Cmd. */
const RESERVED_AFTER_COMMAND = [
  "Z",
  "Shift+Z",
  "X",
  "C",
  "V",
  "A",
  "S",
  "F",
  "R",
  "Q",
  "J",
  "K",
  "Backslash",
  "Enter",
];
const RESERVED_EXACT = ["Alt+Up", "Alt+Down"];

export type HotkeyProblem = "no_modifier" | "reserved" | "holds_cmd";

/** Why a chord cannot be the hotkey, or `null` when it can. */
export function hotkeyProblem(chord: string, behaviour: VoiceHotkeyBehaviour): HotkeyProblem | null {
  const parts = chord.split("+");
  const key = parts[parts.length - 1] ?? "";
  const modifiers = parts.slice(0, -1);
  if (modifiers.length === 0 && !/^F\d{1,2}$/.test(key)) return "no_modifier";
  if (RESERVED_EXACT.includes(chord)) return "reserved";
  for (const command of ["Ctrl", "Cmd"]) {
    if (modifiers.includes(command)) {
      const rest = parts.filter((p) => p !== command).join("+");
      if (RESERVED_AFTER_COMMAND.includes(rest)) return "reserved";
    }
  }
  if (behaviour === "hold" && modifiers.includes("Cmd")) return "holds_cmd";
  return null;
}
