/**
 * The test of a tab in the Voice AI section (`VAI-voice-ai-settings.md`
 * VAI-FR-HTVQ, VAI-FR-NWRL, VAI-FR-SKOF; `VOP-voice-ai-providers.md`
 * VOP-FR-TPXA). The section, the control, and the test controller are real;
 * the backend, the microphone, and playback are mocks.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  appendVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  clearVoiceAiIntegration: vi.fn(),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello there"),
  listVoiceAiIntegrations: vi.fn(),
  listVoiceAiModels: vi.fn(async () => ({ sttModels: [], ttsModels: [] })),
  listVoiceAiVoices: vi.fn(async () => null),
  loadAppPreferences: vi.fn(async () => ({ theme: "system" })),
  saveAppPreferences: vi.fn(async (prefs: unknown) => prefs),
  setActiveVoiceAiIntegration: vi.fn(),
  setVoiceAiDefaultVoice: vi.fn(),
  setVoiceAiRoutes: vi.fn(),
  speakVoiceSegment: vi.fn((..._args: unknown[]) => new Promise<void>(() => undefined)),
  startVoiceCapture: vi.fn(async () => ({ sessionId: "s1", route: "http", live: false })),
  verifyVoiceAiIntegration: vi.fn(),
}));
const mic = vi.hoisted(() => ({ stop: vi.fn() }));

vi.mock("../api", () => api);
vi.mock("../events", () => ({
  emitAppPreferencesChanged: vi.fn(async () => undefined),
  onVoiceAiChanged: vi.fn(async () => () => undefined),
}));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("../voice/capture", async (original) => ({
  ...(await original<typeof import("../voice/capture")>()),
  openMicrophone: vi.fn(async () => ({ rate: 48_000, stop: mic.stop })),
}));
vi.mock("../voice/playback", () => ({
  isPlaybackIdle: () => false,
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  setDrainedListener: vi.fn(),
  setPlaybackFailureListener: vi.fn(),
  beginSegment: vi.fn(),
  endSegmentInput: vi.fn(),
  notePlaybackEntry: vi.fn(),
  stopPlayback: vi.fn(),
  playTestTone: vi.fn(async () => undefined),
}));

import { resetAppPreferencesCache } from "../state/appPreferences";
import type { VoiceAiRecord } from "../types";
import { VoiceAiSettings } from "./VoiceAiSettings";

function record(over: Partial<VoiceAiRecord> & { provider: VoiceAiRecord["provider"] }): VoiceAiRecord {
  return {
    baseUrl: "https://api.example/v1",
    sttModel: "stt",
    ttsModel: "tts",
    transcriptionRoute: "http",
    defaultVoice: "",
    routesChosen: false,
    verifiedAt: "2026-11-05T00:00:00Z",
    ...over,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  resetAppPreferencesCache();
  // OpenAI is active. Custom is verified and is not.
  api.listVoiceAiIntegrations.mockResolvedValue({
    active: "openai",
    records: [
      record({ provider: "openai" }),
      record({ provider: "openrouter", verifiedAt: undefined }),
      record({
        provider: "custom",
        baseUrl: "http://localhost:11434",
        defaultVoice: "male-french",
        speechSpeedPercent: 120,
        speechStyle: "warm",
      }),
    ],
  });
});

afterEach(cleanup);

async function openCustom() {
  render(<VoiceAiSettings />);
  await screen.findByTestId("voice-ai-explanation");
  fireEvent.click(screen.getByRole("tab", { name: "Custom" }));
}

describe("the test of a tab that is not active", () => {
  it("VAI-FR-HTVQ, VOP-FR-TPXA: the capture and the speech use the provider and the saved voice of the tab", async () => {
    await openCustom();
    const hold = screen.getByTestId("voice-test-hold");
    fireEvent.pointerDown(hold, { pointerId: 1 });
    await waitFor(() =>
      expect(api.startVoiceCapture).toHaveBeenCalledWith(48_000, [], false, "custom"),
    );
    await waitFor(() => expect(screen.getByTestId("voice-test-status")).toHaveTextContent("Listening…"));
    fireEvent.pointerUp(window, { pointerId: 1 });
    await waitFor(() =>
      expect(api.speakVoiceSegment).toHaveBeenCalledWith(
        expect.any(String),
        "hello there",
        "male-french",
        expect.any(Function),
        { speedPercent: 120, style: "warm" },
        "custom",
      ),
    );
    expect(screen.getByTestId("voice-test-transcript")).toHaveTextContent("hello there");
  });

  it("VAI-FR-NWRL: a move to another tab while listening closes the microphone and cancels the capture", async () => {
    await openCustom();
    fireEvent.pointerDown(screen.getByTestId("voice-test-hold"), { pointerId: 1 });
    await waitFor(() => expect(screen.getByTestId("voice-test-status")).toHaveTextContent("Listening…"));
    fireEvent.click(screen.getByRole("tab", { name: /^OpenAI/ }));
    expect(mic.stop).toHaveBeenCalled();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(screen.queryByTestId("voice-test-status")).toBeNull();
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VAI-FR-NWRL, VAI-FR-SKOF: a move to another tab while speaking cancels the speech, and the transcript goes", async () => {
    await openCustom();
    fireEvent.pointerDown(screen.getByTestId("voice-test-hold"), { pointerId: 1 });
    await waitFor(() => expect(screen.getByTestId("voice-test-status")).toHaveTextContent("Listening…"));
    fireEvent.pointerUp(window, { pointerId: 1 });
    await screen.findByTestId("voice-test-transcript");
    fireEvent.click(screen.getByRole("tab", { name: /^OpenAI/ }));
    expect(api.cancelVoiceSpeech).toHaveBeenCalledWith(expect.stringMatching(/^voice-test-/));
    fireEvent.click(screen.getByRole("tab", { name: "Custom" }));
    expect(screen.queryByTestId("voice-test-transcript")).toBeNull();
  });
});
