/** The VAD part of the Voice AI section (`VAI-voice-ai-settings.md`). */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  loadAppPreferences: vi.fn(),
  saveAppPreferences: vi.fn(),
}));

vi.mock("../api", () => api);
vi.mock("../events", () => ({ emitAppPreferencesChanged: vi.fn(async () => undefined) }));
const log = vi.hoisted(() => ({ logWarn: vi.fn() }));
vi.mock("../logging", () => log);

import { resetAppPreferencesCache } from "../state/appPreferences";
import { VoiceVadSettings, clampSilenceMs } from "./VoiceVadSettings";

beforeEach(() => {
  vi.clearAllMocks();
  resetAppPreferencesCache();
  api.loadAppPreferences.mockResolvedValue({ theme: "system" });
  api.saveAppPreferences.mockImplementation(async (prefs: unknown) => prefs);
});

afterEach(cleanup);

function slider(): HTMLInputElement {
  return screen.getByTestId("voice-vad-silence") as HTMLInputElement;
}

describe("the VAD settings", () => {
  it("VAI-FR-MHHE: the range is 0.5 s to 5 s in steps of 0.1 s, and the default is 1.5 s", async () => {
    render(<VoiceVadSettings />);
    await waitFor(() => expect(api.loadAppPreferences).toHaveBeenCalled());
    expect(slider().min).toBe("500");
    expect(slider().max).toBe("5000");
    expect(slider().step).toBe("100");
    expect(slider().value).toBe("1500");
    expect(slider()).toHaveAttribute("aria-valuetext", "1.5 s");
  });

  it("VAI-FR-ACFG: three levels, and the default is Medium", async () => {
    render(<VoiceVadSettings />);
    await waitFor(() => expect(api.loadAppPreferences).toHaveBeenCalled());
    expect(screen.getByRole("radio", { name: "Low" })).not.toBeChecked();
    expect(screen.getByRole("radio", { name: "Medium" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "High" })).not.toBeChecked();
  });

  it("VAI-FR-NWOW: a sensitivity change saves at once", async () => {
    render(<VoiceVadSettings />);
    await waitFor(() => expect(api.loadAppPreferences).toHaveBeenCalled());
    fireEvent.click(screen.getByRole("radio", { name: "High" }));
    await waitFor(() =>
      expect(api.saveAppPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ voiceVadSensitivity: "high" }),
      ),
    );
  });

  it("VAI-FR-NWOW: a drag saves once, when it ends", async () => {
    render(<VoiceVadSettings />);
    await waitFor(() => expect(api.loadAppPreferences).toHaveBeenCalled());
    fireEvent.change(slider(), { target: { value: "800" } });
    fireEvent.change(slider(), { target: { value: "900" } });
    expect(api.saveAppPreferences).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider());
    fireEvent.blur(slider());
    await waitFor(() => expect(api.saveAppPreferences).toHaveBeenCalledTimes(1));
    expect(api.saveAppPreferences).toHaveBeenCalledWith(
      expect.objectContaining({ voiceVadSilenceMs: 900 }),
    );
  });

  it("VAI-FR-NWOW: the keyboard saves too", async () => {
    render(<VoiceVadSettings />);
    await waitFor(() => expect(api.loadAppPreferences).toHaveBeenCalled());
    fireEvent.change(slider(), { target: { value: "1600" } });
    fireEvent.keyUp(slider(), { key: "ArrowRight" });
    await waitFor(() =>
      expect(api.saveAppPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ voiceVadSilenceMs: 1600 }),
      ),
    );
  });

  it("VAI-FR-NWOW: a refused save is logged, and the next commit tries again", async () => {
    render(<VoiceVadSettings />);
    await waitFor(() => expect(api.loadAppPreferences).toHaveBeenCalled());
    api.saveAppPreferences.mockRejectedValueOnce(new Error("no"));
    fireEvent.change(slider(), { target: { value: "1600" } });
    fireEvent.pointerUp(slider());
    await waitFor(() => expect(log.logWarn).toHaveBeenCalled());
    fireEvent.pointerUp(slider());
    await waitFor(() => expect(api.saveAppPreferences).toHaveBeenCalledTimes(2));
  });

  it("VAI-FR-MZZU: the controls state that they apply to VAD only", async () => {
    render(<VoiceVadSettings />);
    expect(screen.getByTestId("voice-vad-scope")).toHaveTextContent(/VAD only/);
  });

  it("VAI-FR-WVEV: a stored value out of range shows clamped", async () => {
    api.loadAppPreferences.mockResolvedValue({ theme: "system", voiceVadSilenceMs: 90_000 });
    render(<VoiceVadSettings />);
    await waitFor(() => expect(slider().value).toBe("5000"));
    expect(api.saveAppPreferences).not.toHaveBeenCalled();
    expect(clampSilenceMs(10)).toBe(500);
    expect(clampSilenceMs(undefined)).toBe(1500);
  });

  it("VAI-FR-ACFG: a stored level shows", async () => {
    api.loadAppPreferences.mockResolvedValue({ theme: "system", voiceVadSensitivity: "low" });
    render(<VoiceVadSettings />);
    await waitFor(() => expect(screen.getByRole("radio", { name: "Low" })).toBeChecked());
  });
});
