/**
 * The saved input mode and response mode (`VOM-voice-mode.md` VOM-FR-KEMU,
 * VOM-FR-MQHD, VOM-FR-KQAI; `GSS-global-settings-storage.md` GSS-FR-WYHC).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const prefs = vi.hoisted(() => ({
  cached: null as Record<string, unknown> | null,
  load: vi.fn(async () => ({}) as Record<string, unknown>),
  patch: vi.fn(async (change: Record<string, unknown>) => change),
}));

const vad = vi.hoisted(() => ({
  open: vi.fn(async () => ({ activityId: "va1" })),
  mic: vi.fn(async () => ({ rate: 48_000, stop: () => undefined })),
}));

vi.mock("../api", () => ({
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async () => ({ captureId: null })),
  feedVoiceActivity: vi.fn(async () => undefined),
  openVoiceActivitySession: vad.open,
  setVoiceActivityGate: vi.fn(async () => undefined),
}));
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: vad.mic,
}));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => ({
  isPlaybackIdle: vi.fn(() => true),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  setPlaybackFailureListener: vi.fn(),
  beginSegment: vi.fn(),
  endSegmentInput: vi.fn(),
  notePlaybackEntry: vi.fn(),
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

afterEach(async () => {
  // A session that a test opened closes, so nothing goes to the next test. The
  // close and an open on its way end in microtasks of the mocked commands.
  endVoiceForTeardown();
  await vi.waitFor(() => expect(getVoiceState().phase).toBe("idle"));
  for (let tick = 0; tick < 10; tick += 1) await Promise.resolve();
});

describe("the saved modes", () => {
  it("VOM-FR-KEMU, VOM-FR-PDZR, GSS-FR-WYHC: entry applies the saved modes, and saved VAD opens a session with no press", async () => {
    prefs.cached = { voiceInputMode: "vad", voiceResponseMode: "voice" };
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    expect(getVoiceState()).toMatchObject({ inputMode: "vad", responseMode: "voice" });
    await vi.waitFor(() => expect(getVoiceState().vadOpen).toBe(true));
    expect(vad.open).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-PDZR: entry with saved Push-to-Talk opens no session", () => {
    prefs.cached = { voiceInputMode: "ptt" };
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    // An open starts in the same call as the entry, so nothing that comes later can start one.
    expect(getVoiceState().phase).toBe("idle");
    expect(vad.mic).not.toHaveBeenCalled();
    expect(getVoiceState().vadOpen).toBe(false);
  });

  it("VOM-FR-PDZR: leaving before saved VAD is read opens nothing", async () => {
    let answer: (value: Record<string, unknown>) => void = () => undefined;
    prefs.load.mockImplementation(() => new Promise((resolve) => (answer = resolve)));
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    leaveVoiceMode();
    answer({ voiceInputMode: "vad" });
    await vi.waitFor(() => expect(prefs.load).toHaveBeenCalled());
    await Promise.resolve();
    await Promise.resolve();
    expect(getVoiceState().phase).toBe("idle");
    expect(vad.mic).not.toHaveBeenCalled();
  });

  it("VOM-FR-KEMU, VOM-FR-PDZR: a record that is not read yet applies when it comes, and opens the session", async () => {
    prefs.load.mockImplementation(async () => ({ voiceInputMode: "vad", voiceResponseMode: "voice" }));
    enterVoiceMode("d1", null, { kind: "draft", draftId: "x" });
    expect(getVoiceState().inputMode).toBe("ptt");
    await vi.waitFor(() => expect(getVoiceState().inputMode).toBe("vad"));
    expect(getVoiceState().responseMode).toBe("voice");
    await vi.waitFor(() => expect(getVoiceState().vadOpen).toBe(true));
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
