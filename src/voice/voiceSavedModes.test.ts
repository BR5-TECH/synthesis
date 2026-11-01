/**
 * The saved input mode and response mode (`VOM-voice-mode.md` VOM-FR-KEMU,
 * VOM-FR-MQHD, VOM-FR-KQAI; `GSS-global-settings-storage.md` GSS-FR-WYHC).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const prefs = vi.hoisted(() => ({
  cached: null as Record<string, unknown> | null,
  load: vi.fn(async () => ({}) as Record<string, unknown>),
  patch: vi.fn(async (change: Record<string, unknown>) => change),
}));

vi.mock("../api", () => ({
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async () => ({ captureId: null })),
}));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => ({
  isPlaybackIdle: vi.fn(() => true),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));
vi.mock("../state/appPreferences", () => ({
  peekAppPreferences: () => prefs.cached,
  loadAppPreferences: prefs.load,
  patchAppPreferences: prefs.patch,
  // The shared setup resets the cache before each test.
  resetAppPreferencesCache: vi.fn(),
}));

import { getVoiceState, resetVoiceStateForTest } from "../state/voiceSession";
import {
  endVoiceForTeardown,
  enterVoiceMode,
  leaveVoiceMode,
  noteProvider,
  resetVoiceControllerForTest,
  setInputMode,
  setResponseMode,
} from "./voiceController";

beforeEach(() => {
  vi.clearAllMocks();
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  prefs.cached = null;
  prefs.load.mockImplementation(async () => ({}));
  noteProvider(true, true);
});

describe("the saved modes", () => {
  it("VOM-FR-KEMU, GSS-FR-WYHC: entry applies the saved input and response modes", () => {
    prefs.cached = { voiceInputMode: "vad", voiceResponseMode: "voice" };
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    expect(getVoiceState()).toMatchObject({ inputMode: "vad", responseMode: "voice", vadOpen: false });
  });

  it("VOM-FR-KEMU: a record that is not read yet applies when it comes", async () => {
    prefs.load.mockImplementation(async () => ({ voiceInputMode: "vad", voiceResponseMode: "voice" }));
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    expect(getVoiceState().inputMode).toBe("ptt");
    await vi.waitFor(() => expect(getVoiceState().inputMode).toBe("vad"));
    expect(getVoiceState().responseMode).toBe("voice");
  });

  it("VOM-FR-KEMU: a toggle before the read comes wins over the read", async () => {
    let answer: (value: Record<string, unknown>) => void = () => undefined;
    prefs.load.mockImplementation(() => new Promise((resolve) => (answer = resolve)));
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    setResponseMode("voice");
    answer({ voiceResponseMode: "text" });
    await Promise.resolve();
    await Promise.resolve();
    expect(getVoiceState().responseMode).toBe("voice");
  });

  it("GSS-FR-WYHC: a missing or unknown value reads as the default", () => {
    prefs.cached = { voiceInputMode: "loud", voiceResponseMode: undefined };
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    expect(getVoiceState()).toMatchObject({ inputMode: "ptt", responseMode: "text" });
  });

  it("VOM-FR-KEMU: saved Voice with no speech route is Text, and the saved value stays", () => {
    noteProvider(true, false);
    prefs.cached = { voiceResponseMode: "voice" };
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    expect(getVoiceState().responseMode).toBe("text");
    expect(prefs.patch).not.toHaveBeenCalled();
    noteProvider(true, true);
    expect(getVoiceState().responseMode).toBe("voice");
  });

  it("VOM-FR-KEMU: a change of either toggle saves at once", () => {
    prefs.cached = {};
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    setInputMode("vad");
    expect(prefs.patch).toHaveBeenLastCalledWith({ voiceInputMode: "vad" });
    setResponseMode("voice");
    expect(prefs.patch).toHaveBeenLastCalledWith({ voiceResponseMode: "voice" });
  });

  it("VOM-FR-MQHD, VOM-FR-KQAI: voice mode is not saved, and the saved modes stay after a teardown", () => {
    prefs.cached = { voiceInputMode: "vad", voiceResponseMode: "voice" };
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    endVoiceForTeardown();
    expect(getVoiceState()).toMatchObject({ activeKey: null, inputMode: "vad", responseMode: "voice" });
    leaveVoiceMode();
    const saved = prefs.patch.mock.calls.flatMap((c) => Object.keys(c[0] as object));
    expect(saved.every((k) => k === "voiceInputMode" || k === "voiceResponseMode")).toBe(true);
  });
});
