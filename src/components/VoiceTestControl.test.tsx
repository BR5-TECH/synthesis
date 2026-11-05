/**
 * The **Hold to test** control (`VAI-voice-ai-settings.md` VAI-FR-HTVQ,
 * VAI-FR-ZCMB, VAI-FR-NWRL, VAI-FR-YBQE). The test itself is a mock: these
 * tests check the control and what it shows.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

const fake = vi.hoisted(() => ({
  press: vi.fn(),
  release: vi.fn(),
  cancel: vi.fn(),
  provider: null as string | null,
  voiceOf: null as null | (() => unknown),
  onState: null as null | ((state: unknown) => void),
}));

vi.mock("../voice/voiceTest", async (original) => ({
  ...(await original<typeof import("../voice/voiceTest")>()),
  createVoiceTest: (provider: string, voiceOf: () => unknown, onState: (s: unknown) => void) => {
    fake.provider = provider;
    fake.voiceOf = voiceOf;
    fake.onState = onState;
    return { press: fake.press, release: fake.release, cancel: fake.cancel };
  },
}));

import type { VoiceAiRecord } from "../types";
import { IDLE_TEST } from "../voice/voiceTest";
import { VoiceTestControl } from "./VoiceTestControl";

const RECORD: VoiceAiRecord = {
  provider: "custom",
  baseUrl: "http://localhost:11434",
  sttModel: "lfm3",
  ttsModel: "lfm3",
  transcriptionRoute: "http",
  defaultVoice: "male-french",
  speechSpeedPercent: 120,
  speechStyle: "warm",
  routesChosen: false,
  verifiedAt: "2026-11-05T00:00:00Z",
};

function show(state: Partial<typeof IDLE_TEST>) {
  act(() => fake.onState!({ ...IDLE_TEST, ...state }));
}

beforeEach(() => {
  vi.clearAllMocks();
  fake.onState = null;
});

afterEach(cleanup);

describe("the Hold to test control", () => {
  it("VAI-FR-HTVQ: a verified tab enables it and states what it does", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    const hold = screen.getByTestId("voice-test-hold");
    expect(hold).toBeEnabled();
    expect(hold).toHaveTextContent("Hold to test");
    expect(screen.getByTestId("voice-test")).toHaveTextContent(
      "Hold, speak, and release. You hear what you said in the voice of this tab.",
    );
    expect(fake.provider).toBe("custom");
  });

  it("VAI-FR-HTVQ: a tab that is not verified disables it", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled={false} />);
    expect(screen.getByTestId("voice-test-hold")).toBeDisabled();
    expect(screen.getByTestId("voice-test")).toHaveTextContent("Verify this provider first.");
  });

  it("VAI-FR-HTVQ: the test takes the saved voice, speed, and style of the tab at the time it speaks", () => {
    const { rerender } = render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    expect(fake.voiceOf!()).toEqual({
      name: "male-french",
      options: { speedPercent: 120, style: "warm" },
    });
    rerender(
      <VoiceTestControl
        provider="custom"
        record={{ ...RECORD, defaultVoice: "", speechSpeedPercent: undefined, speechStyle: "" }}
        enabled
      />,
    );
    expect(fake.voiceOf!()).toEqual({ name: null, options: { speedPercent: null, style: "" } });
  });

  it("VAI-FR-NWRL: a press and a release anywhere in the window make one hold", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    fireEvent.pointerDown(screen.getByTestId("voice-test-hold"), { pointerId: 1 });
    fireEvent.pointerDown(screen.getByTestId("voice-test-hold"), { pointerId: 1 });
    expect(fake.press).toHaveBeenCalledTimes(1);
    fireEvent.pointerUp(window, { pointerId: 1 });
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(fake.release).toHaveBeenCalledTimes(1);
  });

  it("VAI-FR-NWRL: a lost pointer capture and a window pointercancel end the hold", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    const hold = screen.getByTestId("voice-test-hold");
    fireEvent.pointerDown(hold, { pointerId: 1 });
    fireEvent.lostPointerCapture(hold, { pointerId: 1 });
    expect(fake.release).toHaveBeenCalledTimes(1);
    fireEvent.pointerDown(hold, { pointerId: 1 });
    fireEvent.pointerCancel(window, { pointerId: 1 });
    expect(fake.release).toHaveBeenCalledTimes(2);
  });

  it("VAI-FR-NWRL: Space and Enter hold, and a key repeat is no new press", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    const hold = screen.getByTestId("voice-test-hold");
    fireEvent.keyDown(hold, { key: " " });
    fireEvent.keyDown(hold, { key: " ", repeat: true });
    expect(fake.press).toHaveBeenCalledTimes(1);
    fireEvent.keyUp(window, { key: " " });
    expect(fake.release).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(hold, { key: "Enter" });
    fireEvent.keyUp(hold, { key: "Enter" });
    expect(fake.press).toHaveBeenCalledTimes(2);
    expect(fake.release).toHaveBeenCalledTimes(2);
  });

  it("VAI-FR-NWRL: the unmount of the control cancels the test", () => {
    const { unmount } = render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    fireEvent.pointerDown(screen.getByTestId("voice-test-hold"), { pointerId: 1 });
    unmount();
    expect(fake.cancel).toHaveBeenCalled();
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(fake.release).not.toHaveBeenCalled();
  });

  it("VAI-FR-NWRL: a control that becomes disabled cancels the test", () => {
    const { rerender } = render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    fireEvent.pointerDown(screen.getByTestId("voice-test-hold"), { pointerId: 1 });
    fake.cancel.mockClear();
    rerender(<VoiceTestControl provider="custom" record={RECORD} enabled={false} />);
    expect(fake.cancel).toHaveBeenCalled();
  });
});

describe("what the control shows", () => {
  it("VAI-FR-ZCMB: Listening, Transcribing, then the transcript while it speaks", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    expect(screen.queryByTestId("voice-test-status")).toBeNull();
    show({ phase: "listening" });
    expect(screen.getByTestId("voice-test-status")).toHaveTextContent("Listening…");
    expect(screen.getByTestId("voice-test-hold")).toHaveAttribute("aria-pressed", "true");
    show({ phase: "transcribing" });
    expect(screen.getByTestId("voice-test-status")).toHaveTextContent("Transcribing…");
    show({ phase: "speaking", transcript: "Hello, can you hear me?" });
    expect(screen.getByTestId("voice-test-transcript")).toHaveTextContent("Hello, can you hear me?");
    expect(screen.getByTestId("voice-test-status")).toHaveTextContent("Speaking…");
    show({ phase: "idle", transcript: "Hello, can you hear me?" });
    expect(screen.getByTestId("voice-test-status")).not.toHaveTextContent("Speaking…");
  });

  it("VAI-FR-ZCMB: an empty transcript states that nothing was heard", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    show({ phase: "idle", transcript: "", nothingHeard: true });
    expect(screen.getByTestId("voice-test-status")).toHaveTextContent(
      "Nothing was heard. Hold the control while you speak.",
    );
  });

  it("VAI-FR-YBQE: a failure shows its message", () => {
    render(<VoiceTestControl provider="custom" record={RECORD} enabled />);
    show({ phase: "idle", error: "voice_session_active" });
    expect(screen.getByTestId("voice-test-error")).toHaveTextContent(
      "A recording is already in progress.",
    );
  });
});
