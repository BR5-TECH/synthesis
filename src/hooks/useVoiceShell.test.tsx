/** The hotkey of voice mode, at the window (`VOM-voice-mode.md`). */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";

const controller = vi.hoisted(() => ({
  noteActivity: vi.fn(),
  noteDiscussionChanged: vi.fn(),
  noteProvider: vi.fn(),
  noteTranscript: vi.fn(),
  pressRecord: vi.fn(),
  releaseRecord: vi.fn(),
  setVoiceBehaviour: vi.fn(),
  voiceComposerMounted: vi.fn(() => true),
}));
const backend = vi.hoisted(() => ({
  listVoiceAiIntegrations: vi.fn(),
  loadAppPreferences: vi.fn(),
  saveAppPreferences: vi.fn(),
}));
const handlers = vi.hoisted(() => ({
  preferences: null as null | (() => void),
  voice: null as null | ((list: unknown) => void),
  discussion: null as null | ((d: unknown) => void),
  transcript: null as null | ((change: { sessionId: string; text: string }) => void),
  activity: null as null | ((change: unknown) => void),
}));

vi.mock("../voice/voiceController", () => controller);
vi.mock("../api", () => backend);
vi.mock("../logging", () => ({ logWarn: vi.fn() }));
vi.mock("../events", () => ({
  emitAppPreferencesChanged: vi.fn(async () => undefined),
  onAppPreferencesChanged: vi.fn(async (handler: () => void) => {
    handlers.preferences = handler;
    return () => undefined;
  }),
  onDiscussionChanged: vi.fn(async (handler: (d: unknown) => void) => {
    handlers.discussion = handler;
    return () => undefined;
  }),
  onVoiceAiChanged: vi.fn(async (handler: (list: unknown) => void) => {
    handlers.voice = handler;
    return () => undefined;
  }),
  onVoiceActivityChanged: vi.fn(async (handler: (change: unknown) => void) => {
    handlers.activity = handler;
    return () => undefined;
  }),
  onVoiceTranscriptChanged: vi.fn(
    async (handler: (change: { sessionId: string; text: string }) => void) => {
      handlers.transcript = handler;
      return () => undefined;
    },
  ),
}));

import { resetAppPreferencesCache } from "../state/appPreferences";
import { patchVoiceState, resetVoiceStateForTest } from "../state/voiceSession";
import { applyIntegrations, useVoiceShell } from "./useVoiceShell";

const buttonKey = vi.hoisted(() => vi.fn());

function Host() {
  useVoiceShell();
  return (
    <>
      <input data-testid="field" />
      <button data-testid="record" onKeyDown={buttonKey} />
    </>
  );
}

const CHORD = { code: "Space", ctrlKey: true, shiftKey: true };

async function mount() {
  const view = render(<Host />);
  await waitFor(() => expect(controller.setVoiceBehaviour).toHaveBeenCalled());
  return view;
}

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(handlers, {
    preferences: null,
    voice: null,
    discussion: null,
    transcript: null,
    activity: null,
  });
  resetAppPreferencesCache();
  resetVoiceStateForTest();
  controller.voiceComposerMounted.mockReturnValue(true);
  backend.listVoiceAiIntegrations.mockResolvedValue({ active: null, records: [] });
  backend.loadAppPreferences.mockResolvedValue({ theme: "system" });
});

afterEach(cleanup);

describe("the hotkey", () => {
  it("VOM-FR-WPRW: a press and a release of the chord reach the controller", async () => {
    await mount();
    fireEvent.keyDown(window, CHORD);
    expect(controller.pressRecord).toHaveBeenCalledTimes(1);
    fireEvent.keyUp(window, { code: "Space" });
    expect(controller.releaseRecord).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-WPRW: releasing a modifier of the chord also ends the hold", async () => {
    await mount();
    fireEvent.keyDown(window, CHORD);
    fireEvent.keyUp(window, { code: "ShiftLeft", ctrlKey: true, shiftKey: false });
    expect(controller.releaseRecord).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-SAMO: a repeated keydown is ignored", async () => {
    await mount();
    fireEvent.keyDown(window, CHORD);
    fireEvent.keyDown(window, { ...CHORD, repeat: true });
    fireEvent.keyDown(window, { ...CHORD, repeat: true });
    expect(controller.pressRecord).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-HXLV: does nothing in a text field", async () => {
    const view = await mount();
    fireEvent.keyDown(view.getByTestId("field"), CHORD);
    expect(controller.pressRecord).not.toHaveBeenCalled();
  });

  it("VOM-FR-HXLV: does nothing during an IME composition", async () => {
    await mount();
    fireEvent.keyDown(window, { ...CHORD, isComposing: true });
    expect(controller.pressRecord).not.toHaveBeenCalled();
  });

  it("VOM-FR-FHYY: does nothing with no composer mounted", async () => {
    controller.voiceComposerMounted.mockReturnValue(false);
    await mount();
    fireEvent.keyDown(window, CHORD);
    expect(controller.pressRecord).not.toHaveBeenCalled();
  });

  it("VOM-FR-DXJT: does nothing under a foreign overlay", async () => {
    patchVoiceState({ activeKey: "d1" });
    await mount();
    const overlay = document.createElement("div");
    overlay.setAttribute("role", "dialog");
    document.body.append(overlay);
    try {
      fireEvent.keyDown(window, CHORD);
      expect(controller.pressRecord).not.toHaveBeenCalled();
    } finally {
      overlay.remove();
    }
  });

  it("VOM-FR-DXJT: works in an overlay that holds the composer", async () => {
    patchVoiceState({ activeKey: "d1" });
    await mount();
    const overlay = document.createElement("div");
    overlay.setAttribute("role", "dialog");
    overlay.innerHTML = '<div data-composer-key="d1"></div>';
    document.body.append(overlay);
    try {
      fireEvent.keyDown(window, CHORD);
      expect(controller.pressRecord).toHaveBeenCalledTimes(1);
    } finally {
      overlay.remove();
    }
  });

  it("VOM-FR-XQUY: blur ends a hold", async () => {
    await mount();
    fireEvent.keyDown(window, CHORD);
    fireEvent.blur(window);
    expect(controller.releaseRecord).toHaveBeenCalledTimes(1);
  });

  it("VAI-FR-AVBR: a new hotkey applies with no restart", async () => {
    await mount();
    backend.loadAppPreferences.mockResolvedValue({
      theme: "system",
      voiceHotkey: "Ctrl+Alt+B",
      voiceHotkeyBehaviour: "toggle",
    });
    handlers.preferences?.();
    await waitFor(() => expect(controller.setVoiceBehaviour).toHaveBeenCalledWith("toggle"));
    fireEvent.keyDown(window, { code: "KeyB", ctrlKey: true, altKey: true });
    expect(controller.pressRecord).toHaveBeenCalledTimes(1);
  });
});

describe("the events of the shell", () => {
  it("VOM-FR-ZLAY: a discussion change reaches the controller", async () => {
    await mount();
    await waitFor(() => expect(handlers.discussion).not.toBeNull());
    const thread = { id: "d1" };
    handlers.discussion?.(thread);
    expect(controller.noteDiscussionChanged).toHaveBeenCalledWith(thread);
  });

  it("VOM-FR-HUXY: a transcript change reaches the controller", async () => {
    await mount();
    await waitFor(() => expect(handlers.transcript).not.toBeNull());
    handlers.transcript?.({ sessionId: "s1", text: "words" });
    expect(controller.noteTranscript).toHaveBeenCalledWith("s1", "words");
  });

  it("VOM-FR-LIFL: the hotkey does not reach a focused record control", async () => {
    const view = await mount();
    buttonKey.mockClear();
    const record = view.getByTestId("record");
    record.focus();
    fireEvent.keyDown(record, { ...CHORD, key: " " });
    expect(controller.pressRecord).toHaveBeenCalledTimes(1);
    expect(buttonKey).not.toHaveBeenCalled();
  });
});

describe("VAD mode", () => {
  it("VOM-FR-WPRW: the hotkey does nothing, and it does not reach the record control", async () => {
    const view = await mount();
    patchVoiceState({ inputMode: "vad" });
    view.getByTestId("record").focus();
    const stopped = !fireEvent.keyDown(view.getByTestId("record"), CHORD);
    expect(stopped).toBe(true);
    expect(controller.pressRecord).not.toHaveBeenCalled();
    expect(buttonKey).not.toHaveBeenCalled();
    fireEvent.keyUp(window, CHORD);
    expect(controller.releaseRecord).not.toHaveBeenCalled();
  });

  it("VOM-FR-WPRW, VOM-FR-SAMO: a repeated key does nothing", async () => {
    await mount();
    patchVoiceState({ inputMode: "vad" });
    fireEvent.keyDown(window, { ...CHORD, repeat: true });
    expect(controller.pressRecord).not.toHaveBeenCalled();
  });

  it("VOM-FR-HXLV: in a text field, the event is not stopped", async () => {
    const view = await mount();
    patchVoiceState({ inputMode: "vad" });
    expect(fireEvent.keyDown(view.getByTestId("field"), CHORD)).toBe(true);
  });

  it("VOM-FR-NZXN: an activity event reaches the controller", async () => {
    await mount();
    await waitFor(() => expect(handlers.activity).not.toBeNull());
    const change = { activityId: "va1", state: "misfire" };
    handlers.activity?.(change);
    expect(controller.noteActivity).toHaveBeenCalledWith(change);
  });
});

describe("the provider state", () => {
  it("VOM-FR-WMAP, VOM-FR-UXKL: an active verified record makes voice ready", () => {
    applyIntegrations({
      active: "custom",
      records: [
        {
          provider: "custom",
          baseUrl: "",
          sttModel: "",
          ttsModel: "",
          transcriptionRoute: "http",
          defaultVoice: "",
          routesChosen: false,
          verifiedAt: "t",
          speechRoute: "llama_tts",
        },
      ],
    });
    expect(controller.noteProvider).toHaveBeenCalledWith(true, true);
    applyIntegrations({ active: null, records: [] });
    expect(controller.noteProvider).toHaveBeenLastCalledWith(false, false);
  });

  it("VOM-FR-WMAP: an event updates the voice controls", async () => {
    await mount();
    controller.noteProvider.mockClear();
    handlers.voice?.({ active: null, records: [] });
    expect(controller.noteProvider).toHaveBeenCalledWith(false, false);
  });
});
