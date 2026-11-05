/** The Audio devices group (`VAI-voice-ai-settings.md` VAI-FR-WNCL to VAI-FR-KXFA). */
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const prefs = vi.hoisted(() => ({
  load: vi.fn(async (): Promise<Record<string, unknown>> => ({ theme: "system" })),
  patch: vi.fn(async (change: Record<string, unknown>) => change),
}));
const mic = vi.hoisted(() => ({
  open: vi.fn(),
  stop: vi.fn(),
  onChunk: null as null | ((pcm: ArrayBuffer) => void),
}));
const tone = vi.hoisted(() => ({ play: vi.fn() }));
vi.mock("../state/appPreferences", () => ({
  loadAppPreferences: prefs.load,
  patchAppPreferences: prefs.patch,
  resetAppPreferencesCache: vi.fn(),
}));
vi.mock("../voice/capture", async (original) => ({
  ...(await original<typeof import("../voice/capture")>()),
  openMicrophone: mic.open,
}));
vi.mock("../voice/playback", () => ({ playTestTone: tone.play }));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));

import { MicrophoneError } from "../voice/capture";
import { VoiceDeviceSettings } from "./VoiceDeviceSettings";

type Device = { kind: string; deviceId: string; label: string };

let devices: Device[];
let deviceChange: (() => void) | null;
let getUserMedia: ReturnType<typeof vi.fn>;
let enumerateDevices: ReturnType<typeof vi.fn>;
let accessTrack: { stop: ReturnType<typeof vi.fn> };

function stubMedia() {
  accessTrack = { stop: vi.fn() };
  getUserMedia = vi.fn(async () => ({ getTracks: () => [accessTrack] }));
  enumerateDevices = vi.fn(async () => devices);
  vi.stubGlobal("navigator", {
    mediaDevices: {
      enumerateDevices,
      getUserMedia,
      addEventListener: (_: string, listener: () => void) => (deviceChange = listener),
      removeEventListener: () => (deviceChange = null),
    },
  });
}

class SinkContext {
  setSinkId() {
    return Promise.resolve();
  }
}

const LABELLED: Device[] = [
  { kind: "audioinput", deviceId: "in-1", label: "Built-in Microphone" },
  { kind: "audioinput", deviceId: "in-2", label: "USB Mic" },
  { kind: "audiooutput", deviceId: "out-1", label: "Built-in Speakers" },
  { kind: "audiooutput", deviceId: "out-2", label: "Headset" },
];

function pcm(value: number): ArrayBuffer {
  return new Int16Array(480).fill(value).buffer;
}

/** The group, after its record and its device lists were read. */
async function renderGroup() {
  render(<VoiceDeviceSettings />);
  await waitFor(() => expect(prefs.load).toHaveBeenCalled());
  await waitFor(() => expect(enumerateDevices).toHaveBeenCalledTimes(2));
  // The two reads resolve, and their state lands.
  await act(async () => undefined);
}

beforeEach(() => {
  devices = LABELLED;
  deviceChange = null;
  prefs.load.mockReset();
  prefs.load.mockImplementation(async () => ({ theme: "system" }));
  prefs.patch.mockReset();
  prefs.patch.mockImplementation(async (change) => change);
  mic.open.mockReset();
  mic.stop.mockReset();
  mic.onChunk = null;
  mic.open.mockImplementation(async (onChunk: (pcm: ArrayBuffer) => void) => {
    mic.onChunk = onChunk;
    return { rate: 48_000, stop: mic.stop };
  });
  tone.play.mockReset();
  stubMedia();
  vi.stubGlobal("AudioContext", class {});
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the group", () => {
  it("VAI-FR-WNCL: the group holds the microphone, its test, the speaker, the test sound, and echo cancellation", async () => {
    await renderGroup();
    expect(screen.getByRole("heading", { name: "Audio devices" })).toBeInTheDocument();
    expect(screen.getByLabelText("Microphone")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test microphone" })).toBeInTheDocument();
    expect(screen.getByLabelText("Speaker")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Play test sound" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Echo cancellation" })).toBeInTheDocument();
  });
});

describe("the microphone", () => {
  it("VAI-FR-XADO: the selector offers the system default and each input by label, and a change saves at once", async () => {
    await renderGroup();
    const select = screen.getByLabelText("Microphone") as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.text)).toEqual([
      "System default",
      "Built-in Microphone",
      "USB Mic",
    ]);
    fireEvent.change(select, { target: { value: "in-2" } });
    expect(prefs.patch).toHaveBeenCalledWith({ voiceInputDevice: { id: "in-2", label: "USB Mic" } });
    fireEvent.change(select, { target: { value: "" } });
    expect(prefs.patch).toHaveBeenLastCalledWith({ voiceInputDevice: null });
  });

  it("VAI-FR-XADO: a saved microphone that is not present shows selected as not connected", async () => {
    prefs.load.mockImplementation(async () => ({
      theme: "system",
      voiceInputDevice: { id: "gone", label: "Studio Mic" },
    }));
    await renderGroup();
    const select = screen.getByLabelText("Microphone") as HTMLSelectElement;
    await waitFor(() => expect(select.selectedOptions[0]?.text).toBe("Studio Mic (not connected)"));
  });

  it("VAI-FR-XADO: a saved microphone whose id changed is found by its label", async () => {
    prefs.load.mockImplementation(async () => ({
      theme: "system",
      voiceInputDevice: { id: "old", label: "USB Mic" },
    }));
    await renderGroup();
    const select = screen.getByLabelText("Microphone") as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("in-2"));
  });

  it("VAI-FR-XADO: the list reads again on devicechange", async () => {
    await renderGroup();
    devices = [...LABELLED, { kind: "audioinput", deviceId: "in-3", label: "New Mic" }];
    act(() => deviceChange?.());
    expect(await screen.findByRole("option", { name: "New Mic" })).toBeInTheDocument();
  });

  it("VAI-FR-XOOE: with no labels, Allow microphone access opens the microphone one time and reads the list again", async () => {
    devices = LABELLED.map((d) => ({ ...d, label: "" }));
    await renderGroup();
    const allow = await screen.findByRole("button", { name: "Allow microphone access" });
    devices = LABELLED;
    fireEvent.click(allow);
    await waitFor(() => expect(getUserMedia).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("option", { name: "USB Mic" })).toBeInTheDocument();
    // It closes the microphone it opened.
    expect(accessTrack.stop).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("button", { name: "Allow microphone access" })).toBeNull();
  });

  it("VAI-FR-XOOE: with labels, no access action shows", async () => {
    await renderGroup();
    await screen.findByRole("option", { name: "USB Mic" });
    expect(screen.queryByRole("button", { name: "Allow microphone access" })).toBeNull();
  });

  it("VAI-FR-XOOE: a refused access shows the denied message", async () => {
    devices = LABELLED.map((d) => ({ ...d, label: "" }));
    getUserMedia.mockRejectedValueOnce(new DOMException("no", "NotAllowedError"));
    await renderGroup();
    fireEvent.click(await screen.findByRole("button", { name: "Allow microphone access" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The microphone is not allowed");
  });
});

describe("the microphone test", () => {
  it("VAI-FR-ZIFX: the toggle opens the microphone and shows its live level, and a second press closes it", async () => {
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    await waitFor(() => expect(mic.open).toHaveBeenCalledTimes(1));
    const meter = await screen.findByRole("meter", { name: "Microphone level" });
    expect(meter).toHaveAttribute("aria-valuenow", "0");
    act(() => mic.onChunk?.(pcm(8000)));
    expect(Number(meter.getAttribute("aria-valuenow"))).toBeGreaterThan(0);
    const stop = screen.getByRole("button", { name: "Stop microphone test" });
    expect(stop).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(stop);
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("meter")).toBeNull();
  });

  it("VAI-FR-ZIFX: a change of the microphone ends the test", async () => {
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    await screen.findByRole("meter");
    await waitFor(() => expect(mic.onChunk).not.toBeNull());
    fireEvent.change(screen.getByLabelText("Microphone"), { target: { value: "in-2" } });
    expect(mic.stop).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Test microphone" })).toHaveAttribute("aria-pressed", "false");
  });

  it("VAI-FR-ZIFX: the close of the section closes the microphone", async () => {
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    await screen.findByRole("meter");
    await waitFor(() => expect(mic.onChunk).not.toBeNull());
    cleanup();
    expect(mic.stop).toHaveBeenCalledTimes(1);
  });

  it("VAI-FR-ZIFX: a microphone that opens after the test stopped closes at once", async () => {
    let finish: (value: unknown) => void = () => undefined;
    mic.open.mockImplementationOnce(() => new Promise((resolve) => (finish = resolve)));
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    fireEvent.click(screen.getByRole("button", { name: "Stop microphone test" }));
    await act(async () => finish({ rate: 48_000, stop: mic.stop }));
    expect(mic.stop).toHaveBeenCalledTimes(1);
  });

  it("VAI-FR-ZIFX: a microphone that does not open shows its message", async () => {
    mic.open.mockRejectedValueOnce(new MicrophoneError("microphone_missing"));
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("No microphone was found.");
    expect(screen.getByRole("button", { name: "Test microphone" })).toHaveAttribute("aria-pressed", "false");
  });
});

describe("the speaker", () => {
  it("VAI-FR-YTLU: where setSinkId exists, the selector offers each output, and a change saves at once", async () => {
    vi.stubGlobal("AudioContext", SinkContext);
    await renderGroup();
    const select = screen.getByLabelText("Speaker") as HTMLSelectElement;
    await waitFor(() =>
      expect(Array.from(select.options).map((o) => o.text)).toEqual([
        "System default",
        "Built-in Speakers",
        "Headset",
      ]),
    );
    expect(select).not.toBeDisabled();
    fireEvent.change(select, { target: { value: "out-2" } });
    expect(prefs.patch).toHaveBeenCalledWith({ voiceOutputDevice: { id: "out-2", label: "Headset" } });
  });

  it("VAI-FR-YTLU: with no setSinkId, the selector shows the system default, disabled, with its line", async () => {
    await renderGroup();
    const select = screen.getByLabelText("Speaker") as HTMLSelectElement;
    expect(select).toBeDisabled();
    expect(Array.from(select.options).map((o) => o.text)).toEqual(["System default"]);
    expect(screen.getByTestId("voice-device-speaker-note")).toHaveTextContent(
      "Voice plays through the system output. Change it in the system sound settings.",
    );
  });
});

describe("the test sound", () => {
  it("VAI-FR-BPUR: Play test sound plays the tone and states Playing… while it plays", async () => {
    let finish: () => void = () => undefined;
    tone.play.mockImplementation(() => new Promise<void>((resolve) => (finish = resolve)));
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Play test sound" }));
    expect(tone.play).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("voice-device-tone-status")).toHaveTextContent("Playing…");
    expect(screen.getByRole("button", { name: "Play test sound" })).toBeDisabled();
    await act(async () => finish());
    expect(screen.getByTestId("voice-device-tone-status")).toHaveTextContent("");
  });

  it("VAI-FR-BPUR: a tone that cannot play states the failure", async () => {
    tone.play.mockRejectedValue("playback_failed");
    await renderGroup();
    fireEvent.click(screen.getByRole("button", { name: "Play test sound" }));
    expect(await screen.findByText("The test sound could not play. See the log for details.")).toBeInTheDocument();
  });
});

describe("a choice before the record is read", () => {
  it("VAI-FR-KXFA, VAI-FR-XADO: a choice made before the read, or after a failed read, shows and saves", async () => {
    prefs.load.mockRejectedValue(new Error("unreadable"));
    await renderGroup();
    const box = screen.getByRole("checkbox", { name: "Echo cancellation" });
    fireEvent.click(box);
    expect(prefs.patch).toHaveBeenCalledWith({ voiceEchoCancellation: false });
    expect(box).not.toBeChecked();
    fireEvent.change(screen.getByLabelText("Microphone"), { target: { value: "in-2" } });
    expect((screen.getByLabelText("Microphone") as HTMLSelectElement).value).toBe("in-2");
  });

  it("VAI-FR-KXFA: a choice made while the read runs wins over the read", async () => {
    let finish: (value: Record<string, unknown>) => void = () => undefined;
    prefs.load.mockImplementation(() => new Promise((resolve) => (finish = resolve)));
    render(<VoiceDeviceSettings />);
    const box = screen.getByRole("checkbox", { name: "Echo cancellation" });
    fireEvent.click(box);
    await act(async () => finish({ theme: "system", voiceEchoCancellation: true }));
    expect(box).not.toBeChecked();
  });
});

describe("echo cancellation", () => {
  it("VAI-FR-KXFA: the toggle is on by default, states its hint, and saves at once", async () => {
    await renderGroup();
    const box = screen.getByRole("checkbox", { name: "Echo cancellation" });
    await waitFor(() => expect(box).toBeChecked());
    expect(screen.getByTestId("voice-device-echo-note")).toHaveTextContent(
      "Turn off when you use headphones and the agent's voice is too quiet.",
    );
    fireEvent.click(box);
    expect(prefs.patch).toHaveBeenCalledWith({ voiceEchoCancellation: false });
    expect(box).not.toBeChecked();
  });

  it("VAI-FR-KXFA: a saved off shows off, so the record was read", async () => {
    prefs.load.mockImplementation(async () => ({ theme: "system", voiceEchoCancellation: false }));
    await renderGroup();
    await waitFor(() =>
      expect(screen.getByRole("checkbox", { name: "Echo cancellation" })).not.toBeChecked(),
    );
  });
});
