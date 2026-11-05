/**
 * The waveform that fills the free width of the voice area (`VOM-voice-mode.md`
 * VOM-FR-WFHN, VOM-FR-VQIU, VOM-FR-ZDLY).
 */
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { noteMicLevel, resetMicLevelForTest } from "../../state/voiceLevel";
import { patchVoiceState, resetVoiceStateForTest } from "../../state/voiceSession";
import { installFakeResizeObserver, resizeAll } from "../../test/fakeScroll";
import { MIN_BARS, VadControls, VoiceWaveform, barsFor } from "./VadControls";

let restore: () => void;

beforeEach(() => {
  resetVoiceStateForTest();
  resetMicLevelForTest();
  restore = installFakeResizeObserver();
});

afterEach(() => {
  cleanup();
  restore();
  vi.restoreAllMocks();
});

function bars(): HTMLElement[] {
  return [...screen.getByTestId("voice-waveform").querySelectorAll<HTMLElement>(".voice-waveform__bar")];
}

/** The waveform, laid out at `width`. */
function widthOf(width: number) {
  Object.defineProperty(screen.getByTestId("voice-waveform"), "clientWidth", {
    configurable: true,
    get: () => width,
  });
}

describe("the bar count", () => {
  it("VOM-FR-VQIU: the count follows the width, and is never less than 16", () => {
    expect(MIN_BARS).toBe(16);
    expect(barsFor(0)).toBe(16);
    expect(barsFor(78)).toBe(16);
    expect(barsFor(83)).toBe(17);
    expect(barsFor(498)).toBe(100);
  });

  it("VOM-FR-VQIU, VOM-FR-WFHN: a waveform that fills its row draws as many bars as its width holds", () => {
    patchVoiceState({ vadOpen: true });
    render(<VoiceWaveform fill />);
    expect(screen.getByTestId("voice-waveform")).toHaveClass("voice-waveform--fill");
    widthOf(298);
    act(() => resizeAll());
    expect(bars()).toHaveLength(60);
    widthOf(40);
    act(() => resizeAll());
    expect(bars()).toHaveLength(16);
  });

  it("VOM-FR-VQIU: after a change of width, the newest level is still the last bar", () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    patchVoiceState({ vadOpen: true });
    render(<VoiceWaveform fill />);
    widthOf(198);
    act(() => resizeAll());
    now.mockReturnValue(2_000);
    act(() => noteMicLevel(0.7));
    const all = bars();
    expect(all).toHaveLength(40);
    expect(all[all.length - 1].style.transform).toBe("scaleY(0.7)");
  });

  it("VOM-FR-ZDLY, VOM-FR-VQIU: a flat waveform that fills its row is flat at every bar", () => {
    patchVoiceState({ vadOpen: true, muted: true });
    render(<VoiceWaveform fill />);
    widthOf(148);
    act(() => resizeAll());
    expect(bars()).toHaveLength(30);
    expect(bars().every((bar) => bar.style.transform === "scaleY(0.12)")).toBe(true);
  });

  it("VOM-FR-VGLT, VOM-FR-GPNK: the waveform of the session control keeps its 16 bars", () => {
    patchVoiceState({ vadOpen: true, inputMode: "vad" });
    render(<VadControls />);
    expect(screen.getByTestId("voice-waveform")).not.toHaveClass("voice-waveform--fill");
    widthOf(400);
    act(() => resizeAll());
    expect(bars()).toHaveLength(16);
  });

  it("VOM-FR-TISN: the VAD controls can leave the waveform to the voice area", () => {
    patchVoiceState({ inputMode: "vad" });
    render(<VadControls waveform={false} />);
    expect(screen.queryByTestId("voice-waveform")).toBeNull();
    expect(screen.getByTestId("voice-vad-indicator")).toBeInTheDocument();
  });
});
