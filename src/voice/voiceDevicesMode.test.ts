/**
 * Voice mode and the chosen devices (`VOM-voice-mode.md` VOM-FR-GIES,
 * VOM-FR-IQNQ, VOM-FR-EPBL).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const prefs = vi.hoisted(() => ({
  cached: null as Record<string, unknown> | null,
  load: vi.fn(async () => ({}) as Record<string, unknown>),
  patch: vi.fn(async (change: Record<string, unknown>) => change),
}));

const playback = vi.hoisted(() => ({ entry: vi.fn() }));

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
  notePlaybackEntry: playback.entry,
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
import { reportDeviceFallback } from "./audioDevices";
import { patchVoiceState } from "../state/voiceSession";
import { logDebug } from "../logging";
import { logSegment, SegmentStats } from "./playbackDiagnostics";
import {
  endVoiceForTeardown,
  enterVoiceMode,
  leaveVoiceMode,
  noteProvider,
  resetVoiceControllerForTest,
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
  endVoiceForTeardown();
  await vi.waitFor(() => expect(getVoiceState().phase).toBe("idle"));
  for (let tick = 0; tick < 10; tick += 1) await Promise.resolve();
});

const DRAFT = { kind: "draft", draftId: "x" } as const;

describe("a device that is not connected", () => {
  it("VOM-FR-GIES: the note shows one time for each device kind and each entry", () => {
    enterVoiceMode("d1", null, DRAFT);
    reportDeviceFallback("microphone");
    expect(getVoiceState().note).toBe("microphone_fallback");
    patchVoiceState({ note: null });
    reportDeviceFallback("microphone");
    expect(getVoiceState().note).toBeNull();
    reportDeviceFallback("speaker");
    expect(getVoiceState().note).toBe("speaker_fallback");

    leaveVoiceMode();
    enterVoiceMode("d1", null, DRAFT);
    reportDeviceFallback("microphone");
    expect(getVoiceState().note).toBe("microphone_fallback");
  });

  it("VOM-FR-GIES: with voice mode off, a fallback shows no note", () => {
    enterVoiceMode("d1", null, DRAFT);
    leaveVoiceMode();
    reportDeviceFallback("speaker");
    expect(getVoiceState().note).toBeNull();
  });

  it("VOM-FR-IQNQ: each entry asks playback to log the speaker again", () => {
    enterVoiceMode("d1", null, DRAFT);
    leaveVoiceMode();
    enterVoiceMode("d1", null, DRAFT);
    expect(playback.entry).toHaveBeenCalledTimes(2);
  });
});

describe("the microphone in the playback log", () => {
  it("VOM-FR-EPBL: voice mode tells playback whether the microphone and a VAD session are open", async () => {
    prefs.cached = { voiceInputMode: "vad" };
    enterVoiceMode("d1", null, DRAFT);
    await vi.waitFor(() => expect(getVoiceState().vadOpen).toBe(true));
    const stats = new SegmentStats("reply");
    stats.inputDone = true;
    logSegment(stats);
    expect(logDebug).toHaveBeenCalledWith(
      ["frontend"],
      "a voice segment ended",
      expect.objectContaining({ microphoneOpen: true, vadOpen: true }),
    );
  });
});
