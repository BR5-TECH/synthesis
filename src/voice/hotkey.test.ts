import { describe, expect, it } from "vitest";

import { DEFAULT_VOICE_HOTKEY, chordOf, hotkeyProblem, mainKeyOf, matchesHotkey } from "./hotkey";

const key = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  ...mods,
});

describe("hotkey chords", () => {
  it("VAI-FR-RASA: the default is Ctrl+Shift+Space", () => {
    expect(DEFAULT_VOICE_HOTKEY).toBe("Ctrl+Shift+Space");
    const event = key("Space", { ctrlKey: true, shiftKey: true });
    expect(chordOf(event)).toBe("Ctrl+Shift+Space");
    expect(matchesHotkey(event, DEFAULT_VOICE_HOTKEY)).toBe(true);
    expect(matchesHotkey(key("Space", { ctrlKey: true }), DEFAULT_VOICE_HOTKEY)).toBe(false);
  });

  it("VAI-FR-FRJY: a modifier alone is no chord, and codes have short names", () => {
    expect(chordOf(key("ShiftLeft", { shiftKey: true }))).toBeNull();
    expect(chordOf(key("KeyB", { ctrlKey: true }))).toBe("Ctrl+B");
    expect(chordOf(key("Digit3", { altKey: true }))).toBe("Alt+3");
    expect(chordOf(key("ArrowLeft", { altKey: true }))).toBe("Alt+Left");
    expect(mainKeyOf("Ctrl+Shift+Space")).toBe("Space");
  });

  it("VAI-FR-FRJY: a chord holds a modifier or is a function key", () => {
    expect(hotkeyProblem("B", "hold")).toBe("no_modifier");
    expect(hotkeyProblem("Space", "toggle")).toBe("no_modifier");
    expect(hotkeyProblem("F8", "hold")).toBeNull();
    expect(hotkeyProblem("Ctrl+B", "hold")).toBeNull();
  });

  it("VAI-FR-BGBL: refuses what the application binds", () => {
    for (const chord of ["Cmd+K", "Ctrl+K", "Ctrl+Z", "Cmd+Shift+Z", "Ctrl+J", "Cmd+Backslash", "Ctrl+Enter", "Alt+Up", "Alt+Down"]) {
      expect(hotkeyProblem(chord, "toggle")).toBe("reserved");
    }
    expect(hotkeyProblem("Ctrl+Shift+K", "toggle")).toBeNull();
  });

  it("VAI-FR-WOCD: refuses Cmd in Hold behaviour only", () => {
    expect(hotkeyProblem("Cmd+Shift+Space", "hold")).toBe("holds_cmd");
    expect(hotkeyProblem("Cmd+Shift+Space", "toggle")).toBeNull();
  });
});
