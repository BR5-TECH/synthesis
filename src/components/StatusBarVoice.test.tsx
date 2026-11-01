import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

const stop = vi.hoisted(() => vi.fn());
vi.mock("../voice/voiceController", () => ({ stopVoiceMode: stop }));

import {
  INITIAL_VOICE_STATE,
  patchVoiceState,
  resetVoiceStateForTest,
  voiceIndicatorState,
  type VoiceState,
} from "../state/voiceSession";
import { StatusBarVoice } from "./StatusBarVoice";

const ON: Partial<VoiceState> = { activeKey: "d1", discussionId: "d1", ownerName: "onboarding.md" };

beforeEach(() => {
  vi.clearAllMocks();
  resetVoiceStateForTest();
});

afterEach(() => {
  cleanup();
});

function indicator() {
  return screen.getByTestId("status-bar-voice-indicator");
}

describe("the voice indicator", () => {
  it("STB-FR-OSOL, STB-FR-ZBLQ: it shows only while voice mode is on, and follows the state", () => {
    render(<StatusBarVoice onReveal={() => undefined} />);
    expect(screen.queryByTestId("status-bar-voice")).toBeNull();
    act(() => patchVoiceState(ON));
    expect(indicator()).toHaveTextContent("onboarding.md");
    act(() => patchVoiceState({ vadOpen: true }));
    expect(indicator()).toHaveAttribute("data-state", "listening");
    act(() => patchVoiceState({ activeKey: null }));
    expect(screen.queryByTestId("status-bar-voice")).toBeNull();
  });

  it("STB-FR-UXGE, STB-FR-PYRC: the names state the state, the discussion, and the route", () => {
    act(() => patchVoiceState({ ...ON, vadOpen: true }));
    render(<StatusBarVoice onReveal={() => undefined} />);
    expect(indicator()).toHaveAccessibleName(
      "Voice mode Listening in onboarding.md — open discussion",
    );
    expect(screen.getByTestId("status-bar-voice-stop")).toHaveAccessibleName("Stop voice mode");
  });

  it("STB-FR-OSOL, VOM-FR-XDUY: activating it opens the discussion and stops nothing", () => {
    const reveal = vi.fn();
    act(() => patchVoiceState(ON));
    render(<StatusBarVoice onReveal={reveal} />);
    fireEvent.click(indicator());
    expect(reveal).toHaveBeenCalledTimes(1);
    expect(stop).not.toHaveBeenCalled();
  });

  it("STB-FR-JHZF, STB-FR-UXGE: the separate Stop control ends voice mode, by keyboard too", () => {
    act(() => patchVoiceState(ON));
    render(<StatusBarVoice onReveal={() => undefined} />);
    const control = screen.getByTestId("status-bar-voice-stop");
    expect(control.tagName).toBe("BUTTON");
    expect(indicator().tagName).toBe("BUTTON");
    expect(control).not.toBe(indicator());
    fireEvent.click(control);
    expect(stop).toHaveBeenCalledTimes(1);
  });

  it("STB-FR-KWMV: each state shows by an icon and a word", () => {
    act(() => patchVoiceState({ ...ON, hearing: true }));
    render(<StatusBarVoice onReveal={() => undefined} />);
    expect(indicator()).toHaveTextContent("Hearing speech");
    expect(indicator().querySelector("[data-icon]")).not.toBeNull();
  });
});

describe("the state precedence", () => {
  const on = { ...INITIAL_VOICE_STATE, ...ON } as VoiceState;

  it.each<[string, Partial<VoiceState>, string]>([
    ["a paused reply", { playbackPaused: true, hearing: true, transcribing: 1, playing: true }, "paused"],
    ["speech", { hearing: true, transcribing: 1, playing: true, vadOpen: true }, "hearing"],
    ["a recording", { phase: "recording", vadOpen: true }, "hearing"],
    ["a chunk in transcription", { transcribing: 2, playing: true, vadOpen: true }, "transcribing"],
    ["a reply", { playing: true, vadOpen: true }, "speaking"],
    ["an open session", { vadOpen: true }, "listening"],
    ["nothing", {}, "idle"],
  ])("STB-FR-KWMV, VOM-FR-XDUY: %s", (_, change, state) => {
    expect(voiceIndicatorState({ ...on, ...change })).toBe(state);
  });
});
