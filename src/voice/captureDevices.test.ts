/**
 * The chosen microphone and the echo cancellation of capture
 * (`VOM-voice-mode.md` VOM-FR-EQQV, VOM-FR-OGFM, VOM-FR-GIES, VOM-FR-IQNQ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const log = vi.hoisted(() => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
const prefs = vi.hoisted(() => ({ value: null as null | Record<string, unknown> }));
vi.mock("../logging", () => log);
vi.mock("../state/appPreferences", async (original) => ({
  ...(await original<typeof import("../state/appPreferences")>()),
  peekAppPreferences: () => prefs.value,
}));

import { resetAudioDevicesForTest, setDeviceFallbackListener } from "./audioDevices";
import { openMicrophone } from "./capture";

interface FakeTrack {
  label: string;
  stop: ReturnType<typeof vi.fn>;
  getSettings: () => { deviceId: string };
}

let getUserMedia: ReturnType<typeof vi.fn>;

/** A WebView whose microphone opens the device `opened`, among `devices`. */
function stubMedia(opened: { id: string; label: string }, devices: { id: string; label: string }[]) {
  const track: FakeTrack = {
    label: opened.label,
    stop: vi.fn(),
    getSettings: () => ({ deviceId: opened.id }),
  };
  getUserMedia = vi.fn(async () => ({ getTracks: () => [track], getAudioTracks: () => [track] }));
  vi.stubGlobal("navigator", {
    mediaDevices: {
      getUserMedia,
      enumerateDevices: vi.fn(async () =>
        devices.map((d) => ({ kind: "audioinput", deviceId: d.id, label: d.label })),
      ),
    },
  });
  // The capture graph does not matter here: the open stops after the device.
  vi.stubGlobal(
    "AudioContext",
    class {
      constructor() {
        throw new Error("no graph");
      }
    },
  );
}

function audioOf(): MediaTrackConstraints {
  return (getUserMedia.mock.calls[0] as unknown as [{ audio: MediaTrackConstraints }])[0].audio;
}

function openedRecords(): Record<string, unknown>[] {
  return log.logInfo.mock.calls
    .filter((call) => call[1] === "the microphone opened")
    .map((call) => call[2] as Record<string, unknown>);
}

beforeEach(() => {
  prefs.value = null;
  log.logInfo.mockClear();
  resetAudioDevicesForTest();
});

afterEach(() => vi.unstubAllGlobals());

describe("the chosen microphone", () => {
  it("VOM-FR-EQQV: with no saved microphone, capture asks for no device", async () => {
    stubMedia({ id: "d", label: "Built-in" }, [{ id: "d", label: "Built-in" }]);
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(audioOf().deviceId).toBeUndefined();
    expect(openedRecords()).toEqual([
      { device: "Built-in", saved: false, found: null, echoCancellation: true },
    ]);
  });

  it("VOM-FR-EQQV: the saved microphone is the ideal device", async () => {
    prefs.value = { voiceInputDevice: { id: "usb", label: "USB Mic" } };
    stubMedia({ id: "usb", label: "USB Mic" }, [{ id: "usb", label: "USB Mic" }]);
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(audioOf().deviceId).toEqual({ ideal: "usb" });
    expect(openedRecords()[0]).toMatchObject({ device: "USB Mic", saved: true, found: true });
  });

  it("VOM-FR-GIES: a saved microphone whose id changed is found by its label", async () => {
    prefs.value = { voiceInputDevice: { id: "old", label: "USB Mic" } };
    stubMedia({ id: "new", label: "USB Mic" }, [{ id: "new", label: "USB Mic" }]);
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(audioOf().deviceId).toEqual({ ideal: "new" });
    expect(openedRecords()[0]).toMatchObject({ found: true });
  });

  it("VOM-FR-GIES, VOM-FR-IQNQ: a saved microphone that did not open reports the fallback", async () => {
    const fallback = vi.fn();
    setDeviceFallbackListener(fallback);
    prefs.value = { voiceInputDevice: { id: "gone", label: "Gone Mic" } };
    stubMedia({ id: "d", label: "Built-in" }, [{ id: "d", label: "Built-in" }]);
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(audioOf().deviceId).toEqual({ ideal: "gone" });
    expect(fallback).toHaveBeenCalledWith("microphone");
    expect(openedRecords()[0]).toMatchObject({ device: "Built-in", saved: true, found: false });
  });
});

describe("echo cancellation", () => {
  it("VOM-FR-OGFM: capture asks for echo cancellation as the preference states", async () => {
    prefs.value = { voiceEchoCancellation: false };
    stubMedia({ id: "d", label: "Built-in" }, []);
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(audioOf()).toMatchObject({
      echoCancellation: false,
      noiseSuppression: true,
      autoGainControl: true,
    });
    expect(openedRecords()[0]).toMatchObject({ echoCancellation: false });
  });

  it("VOM-FR-OGFM: with no saved value, echo cancellation is on", async () => {
    stubMedia({ id: "d", label: "Built-in" }, []);
    await openMicrophone(() => undefined).catch(() => undefined);
    expect(audioOf().echoCancellation).toBe(true);
  });
});
