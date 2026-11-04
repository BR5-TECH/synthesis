/** The speed and the style of a provider (`VAI-voice-ai-settings.md` VAI-FR-USDR, VAI-FR-EIHC). */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

const api = vi.hoisted(() => ({ setVoiceAiSpeechOptions: vi.fn() }));
vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logInfo: vi.fn(), logWarn: vi.fn() }));

import type { VoiceAiRecord, VoiceCapabilities } from "../types";
import { VoiceSpeechOptions } from "./VoiceSpeechOptions";

const CAPS: VoiceCapabilities = {
  httpTranscription: true,
  realtimeSpeech: false,
  openaiSpeech: true,
  llamaTts: false,
};

function record(over: Partial<VoiceAiRecord> = {}): VoiceAiRecord {
  return {
    provider: "custom",
    baseUrl: "http://localhost:11434",
    sttModel: "stt",
    ttsModel: "tts",
    transcriptionRoute: "http",
    speechRoute: "openai_speech",
    defaultVoice: "",
    routesChosen: false,
    verifiedAt: "2026-11-04T12:00:00Z",
    capabilities: CAPS,
    ...over,
  };
}

const onError = vi.fn();

function show(rec: VoiceAiRecord, verified = true) {
  render(<VoiceSpeechOptions provider="custom" record={rec} verified={verified} onError={onError} />);
}

beforeEach(() => {
  vi.clearAllMocks();
  api.setVoiceAiSpeechOptions.mockResolvedValue(record());
});
afterEach(cleanup);

describe("speed and style", () => {
  it("VAI-FR-USDR, VAI-FR-UJNQ: the speed offers 50% to 200% in steps of 10%, and 100% is the default", () => {
    show(record());
    const speed = screen.getByLabelText("Speed") as HTMLSelectElement;
    const values = Array.from(speed.options).map((o) => Number(o.value));
    expect(values[0]).toBe(50);
    expect(values[values.length - 1]).toBe(200);
    expect(values).toHaveLength(16);
    expect(speed.value).toBe("100");
  });

  it("VAI-FR-USDR: a change of speed saves at once, and 100% saves as no speed", () => {
    show(record({ speechStyle: "calm" }));
    const speed = screen.getByLabelText("Speed");
    fireEvent.change(speed, { target: { value: "150" } });
    expect(api.setVoiceAiSpeechOptions).toHaveBeenCalledWith("custom", 150, "calm");
    fireEvent.change(speed, { target: { value: "100" } });
    expect(api.setVoiceAiSpeechOptions).toHaveBeenLastCalledWith("custom", null, "calm");
  });

  it("VAI-FR-USDR: the style saves when it loses focus with a changed value", () => {
    show(record({ speechSpeedPercent: 120 }));
    const style = screen.getByLabelText("Speaking style");
    fireEvent.blur(style);
    expect(api.setVoiceAiSpeechOptions).not.toHaveBeenCalled();
    fireEvent.change(style, { target: { value: " warm " } });
    fireEvent.blur(style);
    expect(api.setVoiceAiSpeechOptions).toHaveBeenCalledWith("custom", 120, "warm");
    expect(style).toHaveAttribute("maxLength", "200");
  });

  it("VAI-FR-USDR: a save of one control keeps the last value of the other before the record refreshes", () => {
    show(record());
    const style = screen.getByLabelText("Speaking style");
    fireEvent.change(style, { target: { value: "warm" } });
    fireEvent.blur(style);
    fireEvent.change(screen.getByLabelText("Speed"), { target: { value: "150" } });
    expect(api.setVoiceAiSpeechOptions).toHaveBeenLastCalledWith("custom", 150, "warm");
    fireEvent.change(style, { target: { value: "calm" } });
    fireEvent.blur(style);
    expect(api.setVoiceAiSpeechOptions).toHaveBeenLastCalledWith("custom", 150, "calm");
  });

  it("VAI-FR-USDR: a refused save shows the typed code", async () => {
    api.setVoiceAiSpeechOptions.mockRejectedValueOnce("invalid_speech_options");
    show(record());
    fireEvent.change(screen.getByLabelText("Speed"), { target: { value: "150" } });
    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith("invalid_speech_options"));
  });
});

describe("the notes", () => {
  it("VAI-FR-EIHC: a server that cannot change speed or take a style says so", () => {
    show(record({ capabilities: { ...CAPS, speechSpeed: "client", speechInstructions: false } }));
    expect(screen.getByTestId("voice-speed-client")).toHaveTextContent(
      "This server cannot change speed. Playback changes speed and pitch.",
    );
    expect(screen.getByTestId("voice-style-unsupported")).toHaveTextContent(
      "This server does not support a speaking style.",
    );
  });

  it("VAI-FR-EIHC: a capable server shows no note, and neither does an unverified tab", () => {
    show(record({ capabilities: { ...CAPS, speechSpeed: "server", speechInstructions: true } }));
    expect(screen.queryByTestId("voice-speed-client")).toBeNull();
    expect(screen.queryByTestId("voice-style-unsupported")).toBeNull();
    cleanup();
    show(record(), false);
    expect(screen.queryByTestId("voice-speed-client")).toBeNull();
    expect(screen.queryByTestId("voice-style-unsupported")).toBeNull();
    cleanup();
    show(record({ capabilities: undefined }));
    expect(screen.queryByTestId("voice-speed-client")).toBeNull();
    expect(screen.queryByTestId("voice-style-unsupported")).toBeNull();
  });

  it("VAI-FR-EIHC: a speed capability that is absent reads as client", () => {
    show(record({ capabilities: { ...CAPS, speechInstructions: true } }));
    expect(screen.getByTestId("voice-speed-client")).toBeInTheDocument();
    expect(screen.queryByTestId("voice-style-unsupported")).toBeNull();
  });
});
