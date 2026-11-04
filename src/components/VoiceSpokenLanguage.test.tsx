/** The spoken language control (`VAI-voice-ai-settings.md` VAI-FR-RGPV, VAI-FR-JPZK). */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  loadAppPreferences: vi.fn(),
  saveAppPreferences: vi.fn(),
}));
vi.mock("../api", () => api);
vi.mock("../events", () => ({ emitAppPreferencesChanged: vi.fn(async () => undefined) }));
vi.mock("../logging", () => ({ logInfo: vi.fn(), logWarn: vi.fn() }));

import { resetAppPreferencesCache } from "../state/appPreferences";
import { VoiceSpokenLanguage } from "./VoiceSpokenLanguage";

function stored(voiceSpokenLanguage?: string) {
  api.loadAppPreferences.mockResolvedValue({ theme: "system", voiceSpokenLanguage });
}

function lastSaved(): string | undefined {
  const calls = api.saveAppPreferences.mock.calls as unknown as [{ voiceSpokenLanguage?: string }][];
  return calls[calls.length - 1]?.[0].voiceSpokenLanguage;
}

beforeEach(() => {
  vi.clearAllMocks();
  resetAppPreferencesCache();
  api.saveAppPreferences.mockImplementation(async (prefs: unknown) => prefs);
});
afterEach(cleanup);

describe("the spoken language", () => {
  it("VAI-FR-RGPV: Auto first, the table by name, and Other last; Auto states nothing", async () => {
    stored(undefined);
    render(<VoiceSpokenLanguage />);
    const select = (await screen.findByLabelText("Spoken language")) as HTMLSelectElement;
    const names = Array.from(select.options).map((o) => o.textContent);
    expect(names[0]).toBe("Auto (no translation)");
    expect(names).toContain("Russian");
    expect(names[names.length - 1]).toBe("Other…");
    expect(names).toHaveLength(17);
    expect(select.value).toBe("");
    expect(screen.queryByTestId("voice-spoken-language-note")).toBeNull();
  });

  it("VAI-FR-RGPV, VAI-FR-JPZK: a preset saves its code and states the translation", async () => {
    stored("de");
    render(<VoiceSpokenLanguage />);
    const select = (await screen.findByLabelText("Spoken language")) as HTMLSelectElement;
    // The read of the preference settles before the change.
    await waitFor(() => expect(select.value).toBe("de"));
    fireEvent.change(select, { target: { value: "ru" } });
    await waitFor(() => expect(lastSaved()).toBe("ru"));
    expect(screen.getByTestId("voice-spoken-language-note")).toHaveTextContent(
      "Your speech is translated into English before it posts. Agents speak to you in this language.",
    );
  });

  it("VAI-FR-RGPV: Other shows a field of at most 40 characters that saves on blur", async () => {
    stored("de");
    render(<VoiceSpokenLanguage />);
    const select = (await screen.findByLabelText("Spoken language")) as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("de"));
    fireEvent.change(select, { target: { value: "other" } });
    const field = screen.getByLabelText("Language name");
    expect(field).toHaveAttribute("maxLength", "40");
    fireEvent.change(field, { target: { value: " Basque " } });
    fireEvent.blur(field);
    await waitFor(() => expect(lastSaved()).toBe("Basque"));
  });

  it("VAI-FR-RGPV: a stored free name opens on Other with its name, and Auto clears it", async () => {
    stored("Basque");
    render(<VoiceSpokenLanguage />);
    const field = await screen.findByLabelText("Language name");
    expect(field).toHaveValue("Basque");
    fireEvent.change(screen.getByLabelText("Spoken language"), { target: { value: "" } });
    await waitFor(() => expect(lastSaved()).toBe(""));
    expect(screen.queryByTestId("voice-spoken-language-note")).toBeNull();
  });

  it("VAI-FR-JPZK, VOP-FR-BYZC: a stored free name states the translation, and a value that is no language does not", async () => {
    stored("Basque");
    render(<VoiceSpokenLanguage />);
    expect(await screen.findByTestId("voice-spoken-language-note")).toBeInTheDocument();
    cleanup();
    resetAppPreferencesCache();
    stored("Klingon 2");
    render(<VoiceSpokenLanguage />);
    await waitFor(() => expect(screen.getByLabelText("Language name")).toHaveValue("Klingon 2"));
    expect(screen.queryByTestId("voice-spoken-language-note")).toBeNull();
  });
});
