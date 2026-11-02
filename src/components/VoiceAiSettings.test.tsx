/** The Voice AI section (`VAI-voice-ai-settings.md`). */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  clearVoiceAiIntegration: vi.fn(),
  listVoiceAiIntegrations: vi.fn(),
  listVoiceAiModels: vi.fn(),
  listVoiceAiVoices: vi.fn(),
  setActiveVoiceAiIntegration: vi.fn(),
  setVoiceAiDefaultVoice: vi.fn(),
  setVoiceAiRoutes: vi.fn(),
  verifyVoiceAiIntegration: vi.fn(),
  loadAppPreferences: vi.fn(),
  saveAppPreferences: vi.fn(),
}));
const voiceChanged = vi.hoisted(() => ({
  emit: null as null | ((list: unknown) => void),
}));

vi.mock("../api", () => api);
const events = vi.hoisted(() => ({
  emitAppPreferencesChanged: vi.fn(async () => undefined),
}));

vi.mock("../events", () => ({
  emitAppPreferencesChanged: events.emitAppPreferencesChanged,
  onVoiceAiChanged: vi.fn(async (handler: (list: unknown) => void) => {
    voiceChanged.emit = handler;
    return () => undefined;
  }),
}));
vi.mock("../logging", () => ({ logInfo: vi.fn(), logWarn: vi.fn() }));

import { resetAppPreferencesCache } from "../state/appPreferences";
import type { VoiceAiIntegrations, VoiceAiRecord, VoiceCapabilities } from "../types";
import { voiceMessage } from "../voice/voiceMessages";
import { VoiceAiSettings, capabilityLine } from "./VoiceAiSettings";

const LIVE: VoiceCapabilities = {
  dialect: "llamacpp",
  realtimeTranscription: "live",
  realtimeInputRate: 16_000,
  httpTranscription: true,
  realtimeSpeech: true,
  openaiSpeech: false,
  llamaTts: true,
};

function record(over: Partial<VoiceAiRecord> & { provider: VoiceAiRecord["provider"] }): VoiceAiRecord {
  return {
    baseUrl: "https://api.example/v1",
    sttModel: "stt",
    ttsModel: "tts",
    transcriptionRoute: "http",
    defaultVoice: "",
    routesChosen: false,
    ...over,
  };
}

function integrations(over: Partial<VoiceAiIntegrations> = {}): VoiceAiIntegrations {
  return {
    active: null,
    records: [
      record({ provider: "openai" }),
      record({ provider: "openrouter" }),
      record({ provider: "custom", baseUrl: "", sttModel: "", ttsModel: "" }),
    ],
    ...over,
  };
}

const verified = record({
  provider: "custom",
  baseUrl: "http://localhost:8080",
  verifiedAt: "2026-10-31T12:00:00Z",
  transcriptionRoute: "realtime",
  speechRoute: "realtime",
  capabilities: LIVE,
});

beforeEach(() => {
  vi.clearAllMocks();
  resetAppPreferencesCache();
  api.listVoiceAiIntegrations.mockResolvedValue(integrations());
  api.listVoiceAiModels.mockResolvedValue({ sttModels: ["voxtral"], ttsModels: ["lfm3"] });
  api.listVoiceAiVoices.mockResolvedValue(null);
  api.loadAppPreferences.mockResolvedValue({ theme: "system" });
  api.saveAppPreferences.mockImplementation(async (prefs: unknown) => prefs);
  api.verifyVoiceAiIntegration.mockResolvedValue(verified);
  api.setActiveVoiceAiIntegration.mockResolvedValue(integrations({ active: "custom" }));
  api.clearVoiceAiIntegration.mockResolvedValue(integrations());
  api.setVoiceAiRoutes.mockResolvedValue({ ...verified, transcriptionRoute: "http" });
  api.setVoiceAiDefaultVoice.mockResolvedValue(verified);
});

afterEach(cleanup);

async function open(tab?: "OpenRouter" | "Custom") {
  render(<VoiceAiSettings />);
  await screen.findByTestId("voice-ai-explanation");
  if (tab) fireEvent.click(screen.getByRole("tab", { name: tab }));
}

async function verifyCustom() {
  await open("Custom");
  fireEvent.change(screen.getByLabelText("Base URL"), {
    target: { value: "http://localhost:8080" },
  });
  fireEvent.click(screen.getByTestId("voice-verify"));
  await screen.findByTestId("voice-capabilities");
}

describe("the section", () => {
  it("VAI-FR-CEJC, VAI-FR-IDIV: one tab per provider, in order", async () => {
    await open();
    expect(api.listVoiceAiIntegrations).toHaveBeenCalled();
    expect(screen.getAllByRole("tab").map((t) => t.textContent)).toEqual([
      "OpenAI",
      "OpenRouter",
      "Custom",
    ]);
  });

  it("VAI-FR-UJNQ: a tab holds the five fields", async () => {
    await open();
    for (const label of ["Base URL", "Key", "STT model", "TTS model", "Default voice"]) {
      expect(screen.getByLabelText(label)).toBeInTheDocument();
    }
  });

  it("VAI-FR-KWOT: states that voice mode needs a provider", async () => {
    await open();
    expect(screen.getByTestId("voice-ai-empty")).toBeInTheDocument();
  });

  it("VAI-FR-QARI, VAI-FR-BSDN: an event refreshes the fields of a tab with no edit", async () => {
    await open();
    const next = integrations({ active: "openai" });
    next.records[0] = record({ provider: "openai", sttModel: "from-the-other-window" });
    voiceChanged.emit?.(next);
    await waitFor(() =>
      expect(screen.getByLabelText("STT model")).toHaveValue("from-the-other-window"),
    );
    expect(screen.queryByTestId("voice-ai-empty")).not.toBeInTheDocument();
  });

  it("VAI-FR-BSDN: a tab with a candidate edit keeps it", async () => {
    await open();
    fireEvent.change(screen.getByLabelText("STT model"), { target: { value: "mine" } });
    voiceChanged.emit?.(integrations({ active: "openai" }));
    await waitFor(() => expect(screen.queryByTestId("voice-ai-empty")).not.toBeInTheDocument());
    expect(screen.getByLabelText("STT model")).toHaveValue("mine");
  });
});

describe("keys and verification", () => {
  it("VAI-FR-LLAE: the key field is never filled, and shows the hint", async () => {
    api.listVoiceAiIntegrations.mockResolvedValue({
      active: null,
      records: [record({ provider: "openai", maskedHint: "3f9a" })],
    });
    await open();
    const key = screen.getByLabelText("Key");
    expect(key).toHaveValue("");
    expect(key.getAttribute("placeholder")).toContain("3f9a");
  });

  it("VAI-FR-ZYEZ, VAI-FR-ZEZK: Custom says that its key is optional", async () => {
    await open("Custom");
    expect(screen.getByLabelText("Key (optional)")).toBeInTheDocument();
    expect(screen.getByLabelText("Base URL").getAttribute("placeholder")).toBe(
      "http://localhost:8080",
    );
  });

  it("VAI-FR-MJCA: sends the values of the tab, and is disabled while it runs", async () => {
    let finish: (r: VoiceAiRecord) => void = () => undefined;
    api.verifyVoiceAiIntegration.mockImplementation(() => new Promise((r) => (finish = r)));
    await open("Custom");
    fireEvent.change(screen.getByLabelText("Base URL"), { target: { value: "http://h:1" } });
    fireEvent.change(screen.getByLabelText("Key (optional)"), { target: { value: "k" } });
    fireEvent.change(screen.getByLabelText("STT model"), { target: { value: "s" } });
    fireEvent.change(screen.getByLabelText("TTS model"), { target: { value: "t" } });
    fireEvent.click(screen.getByTestId("voice-verify"));
    expect(api.verifyVoiceAiIntegration).toHaveBeenCalledWith("custom", "http://h:1", "k", "s", "t");
    expect(screen.getByTestId("voice-verify")).toBeDisabled();
    // VAI-FR-MFGW.
    expect(screen.getByTestId("voice-waiting")).toBeInTheDocument();
    finish(verified);
    await screen.findByTestId("voice-capabilities");
    expect(screen.getByTestId("voice-verify")).not.toBeDisabled();
  });

  it("VAI-FR-JSNS: states that verification can cost money", async () => {
    await open();
    expect(screen.getByTestId("voice-cost")).toHaveTextContent(/can charge/);
  });

  it("VAI-FR-TOLC: an edit returns the tab to unverified and calls nothing", async () => {
    await verifyCustom();
    api.verifyVoiceAiIntegration.mockClear();
    expect(screen.getByTestId("voice-activate")).not.toBeDisabled();
    fireEvent.change(screen.getByLabelText("TTS model"), { target: { value: "other" } });
    expect(screen.getByTestId("voice-status")).toHaveTextContent("Not verified");
    expect(screen.getByTestId("voice-activate")).toBeDisabled();
    expect(api.verifyVoiceAiIntegration).not.toHaveBeenCalled();
  });

  it("VAI-FR-HJHX: shows the time, or the typed failure", async () => {
    await verifyCustom();
    expect(screen.getByTestId("voice-status")).toHaveTextContent(/^Verified/);
    api.verifyVoiceAiIntegration.mockRejectedValue("no_transcription_route");
    fireEvent.click(screen.getByTestId("voice-verify"));
    expect(await screen.findByTestId("voice-settings-error")).toHaveTextContent(
      /no transcription route/,
    );
    api.verifyVoiceAiIntegration.mockRejectedValue("tls_untrusted:unknown_issuer:h");
    fireEvent.click(screen.getByTestId("voice-verify"));
    await waitFor(() =>
      expect(screen.getByTestId("voice-settings-error")).toHaveTextContent(/certificate/),
    );
  });

  it("VAI-FR-OFNT: shows one line for the capabilities", async () => {
    await verifyCustom();
    expect(screen.getByTestId("voice-capabilities")).toHaveTextContent(
      "Live transcription, 16 kHz. Speech: Realtime.",
    );
    expect(capabilityLine({ ...verified, capabilities: undefined })).toBe("");
    expect(
      capabilityLine({
        ...verified,
        speechRoute: "llama_tts",
        capabilities: { ...LIVE, realtimeTranscription: "turn" },
      }),
    ).toBe("Turn-based transcription, 16 kHz. Speech: /tts.");
    expect(
      capabilityLine({
        ...verified,
        speechRoute: undefined,
        capabilities: { ...LIVE, realtimeTranscription: undefined, realtimeInputRate: undefined },
      }),
    ).toBe("File transcription. No spoken replies.");
  });

  it("VAI-FR-LWCD: shows the server's refusal", async () => {
    api.verifyVoiceAiIntegration.mockResolvedValue({
      ...verified,
      capabilityNote: "The current model does not support realtime transcription.",
    });
    await verifyCustom();
    expect(screen.getByTestId("voice-capability-note")).toHaveTextContent(/does not support/);
  });

  it("VAI-FR-GZOH: is enabled only while the tab is verified", async () => {
    await open("Custom");
    expect(screen.getByTestId("voice-activate")).toBeDisabled();
    fireEvent.click(screen.getByTestId("voice-verify"));
    await screen.findByTestId("voice-capabilities");
    fireEvent.click(screen.getByTestId("voice-activate"));
    await waitFor(() => expect(api.setActiveVoiceAiIntegration).toHaveBeenCalledWith("custom"));
  });

  it("VAI-FR-AWEA: calls the clear operation", async () => {
    await open();
    fireEvent.click(screen.getByTestId("voice-clear"));
    await waitFor(() => expect(api.clearVoiceAiIntegration).toHaveBeenCalledWith("openai"));
  });
});

describe("routes and models", () => {
  it("VAI-FR-HQCG: offers only detected routes, and a change applies at once", async () => {
    await verifyCustom();
    const speech = screen.getByTestId("voice-speech-route");
    const options = Array.from(speech.querySelectorAll("option")).map((o) => o.textContent);
    expect(options).toEqual(["Realtime", "/tts"]);
    fireEvent.change(screen.getByTestId("voice-transcription-route"), {
      target: { value: "http" },
    });
    expect(api.setVoiceAiRoutes).toHaveBeenCalledWith("custom", "http", "realtime");
    await waitFor(() => expect(screen.getByTestId("voice-not-live")).toBeInTheDocument());
  });

  it("VAI-FR-OLNG: before verification the selectors are disabled and say why", async () => {
    await open();
    expect(screen.getByTestId("voice-transcription-route")).toBeDisabled();
    expect(screen.getByTestId("voice-speech-route")).toBeDisabled();
    expect(screen.getByTestId("voice-routes-hint")).toBeInTheDocument();
  });

  it("VAI-FR-PKZY: says that a file route gives text after recording", async () => {
    api.verifyVoiceAiIntegration.mockResolvedValue({
      ...verified,
      transcriptionRoute: "http",
      capabilities: { ...LIVE, realtimeTranscription: undefined },
    });
    await verifyCustom();
    expect(screen.getByTestId("voice-not-live")).toBeInTheDocument();
  });

  it("VAI-FR-WNVT: suggests models, and a failed list leaves a plain field", async () => {
    await open();
    await waitFor(() =>
      expect(document.querySelector('#voice-stt-models option[value="voxtral"]')).not.toBeNull(),
    );
    cleanup();
    api.listVoiceAiModels.mockClear();
    api.listVoiceAiModels.mockRejectedValue(new Error("no"));
    await open();
    await waitFor(() => expect(api.listVoiceAiModels).toHaveBeenCalled());
    expect(screen.getByLabelText("STT model")).toBeInTheDocument();
    await waitFor(() => expect(document.querySelector("#voice-stt-models option")).toBeNull());
  });

  it("VOP-FR-TOCX: a model list follows a finished URL edit, not each key press", async () => {
    await open("Custom");
    api.listVoiceAiModels.mockClear();
    const url = screen.getByLabelText("Base URL");
    fireEvent.change(url, { target: { value: "http://l" } });
    fireEvent.change(url, { target: { value: "http://lo" } });
    expect(api.listVoiceAiModels).not.toHaveBeenCalled();
    fireEvent.blur(url);
    await waitFor(() =>
      expect(api.listVoiceAiModels).toHaveBeenCalledWith("custom", "http://lo", null),
    );
    expect(api.listVoiceAiModels).toHaveBeenCalledTimes(1);
  });

  it("VAI-FR-OJBE: offers the voices of the provider of the tab", async () => {
    api.listVoiceAiVoices.mockResolvedValue(["marin", "cedar"]);
    await open();
    expect(api.listVoiceAiVoices).toHaveBeenCalledWith("openai");
    await waitFor(() =>
      expect(document.querySelector('#voice-voices option[value="cedar"]')).not.toBeNull(),
    );
  });

  it("VAI-FR-EEMC: on Custom an empty TTS model is valid and says so", async () => {
    await open("Custom");
    expect(screen.getByTestId("voice-no-tts")).toBeInTheDocument();
  });

  it("VAI-FR-NIWH: says that a llama.cpp Realtime route takes no voice", async () => {
    await verifyCustom();
    expect(screen.getByTestId("voice-no-agent-voice")).toBeInTheDocument();
  });

  it("VAI-FR-OJBE: saves when it loses focus with a changed value", async () => {
    await open();
    const field = screen.getByLabelText("Default voice");
    fireEvent.blur(field);
    expect(api.setVoiceAiDefaultVoice).not.toHaveBeenCalled();
    fireEvent.change(field, { target: { value: "cedar" } });
    fireEvent.blur(field);
    expect(api.setVoiceAiDefaultVoice).toHaveBeenCalledWith("openai", "cedar");
  });
});

describe("messages", () => {
  it("VAI-FR-HJHX: each verification code has its own message", () => {
    const codes = [
      "base_url_empty",
      "base_url_invalid",
      "key_missing",
      "unreachable",
      "rejected",
      "no_transcription_route",
      "no_speech_route",
      "keychain_unavailable",
      "tls_untrusted:unknown_issuer:h",
    ];
    const messages = codes.map(voiceMessage);
    expect(new Set(messages).size).toBe(codes.length);
    messages.forEach((m) => expect(m).not.toBe(voiceMessage("not_a_code")));
  });
});

describe("Push-to-Talk", () => {
  async function capture(init: KeyboardEventInit & { code: string }) {
    const button = screen.getByTestId("voice-hotkey");
    fireEvent.click(button);
    fireEvent.keyDown(button, init);
  }

  it("VAI-FR-HVNJ: Hold is the default, and a change saves", async () => {
    await open();
    expect(screen.getByRole("radio", { name: "Hold" })).toBeChecked();
    fireEvent.click(screen.getByRole("radio", { name: "Toggle" }));
    await waitFor(() =>
      expect(api.saveAppPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ voiceHotkeyBehaviour: "toggle" }),
      ),
    );
  });

  it("VAI-FR-HVNJ, VAI-FR-RASA: the controls state that they apply to Push-to-Talk only", async () => {
    await open();
    expect(screen.getByTestId("voice-ptt-scope")).toHaveTextContent(/Push-to-Talk only/);
    expect(screen.getByTestId("voice-vad-settings")).toBeInTheDocument();
  });

  it("VAI-FR-AVBR: a save tells the other window", async () => {
    await open();
    fireEvent.click(screen.getByRole("radio", { name: "Toggle" }));
    await waitFor(() => expect(events.emitAppPreferencesChanged).toHaveBeenCalled());
  });

  it("VAI-FR-RASA, VAI-FR-AVBR: captures a chord, saves it, and restores the default", async () => {
    await open();
    expect(screen.getByTestId("voice-hotkey")).toHaveTextContent("Ctrl+Shift+Space");
    await capture({ code: "KeyB", ctrlKey: true, altKey: true });
    await waitFor(() =>
      expect(api.saveAppPreferences).toHaveBeenCalledWith(
        expect.objectContaining({ voiceHotkey: "Ctrl+Alt+B" }),
      ),
    );
    expect(screen.getByTestId("voice-hotkey")).toHaveTextContent("Ctrl+Alt+B");
    fireEvent.click(screen.getByTestId("voice-hotkey-default"));
    expect(screen.getByTestId("voice-hotkey")).toHaveTextContent("Ctrl+Shift+Space");
  });

  it("VAI-FR-FRJY: refuses a key with no modifier, and says so", async () => {
    await open();
    await capture({ code: "KeyB" });
    expect(screen.getByTestId("voice-hotkey-message")).toHaveTextContent(/modifier/);
    expect(api.saveAppPreferences).not.toHaveBeenCalled();
  });

  it("VAI-FR-BGBL: refuses a key that the application binds", async () => {
    await open();
    await capture({ code: "KeyK", ctrlKey: true });
    expect(screen.getByTestId("voice-hotkey-message")).toHaveTextContent(/already uses/);
  });

  it("VAI-FR-WOCD: Hold refuses Cmd, and choosing Hold restores the default", async () => {
    await open();
    await capture({ code: "Space", metaKey: true, shiftKey: true });
    expect(screen.getByTestId("voice-hotkey-message")).toHaveTextContent(/cannot use Cmd/);
    cleanup();
    resetAppPreferencesCache();
    api.loadAppPreferences.mockResolvedValue({
      theme: "system",
      voiceHotkeyBehaviour: "toggle",
      voiceHotkey: "Cmd+Shift+Space",
    });
    await open();
    await waitFor(() =>
      expect(screen.getByTestId("voice-hotkey")).toHaveTextContent("Cmd+Shift+Space"),
    );
    fireEvent.click(screen.getByRole("radio", { name: "Hold" }));
    expect(screen.getByTestId("voice-hotkey")).toHaveTextContent("Ctrl+Shift+Space");
    expect(screen.getByTestId("voice-hotkey-message")).toHaveTextContent(/default hotkey/);
  });

  it("VAI non-functional: Escape leaves the hotkey unchanged", async () => {
    await open();
    await capture({ code: "Escape", key: "Escape" });
    expect(api.saveAppPreferences).not.toHaveBeenCalled();
    expect(screen.getByTestId("voice-hotkey")).toHaveTextContent("Ctrl+Shift+Space");
  });
});
