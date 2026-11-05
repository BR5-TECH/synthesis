/**
 * The Audio devices group of the Voice AI section (`VAI-voice-ai-settings.md`
 * VAI-FR-WNCL to VAI-FR-NLYF). It needs no verified provider: the test sound
 * and the level meter check the devices alone, so a device fault and a
 * provider fault look different.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { logInfo, logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences } from "../state/appPreferences";
import { levelOfPcm } from "../state/voiceLevel";
import type { AppPreferences, VoiceDevice } from "../types";
import {
  hasLabels,
  listAudioDevices,
  matchDevice,
  onDeviceChange,
  sinkIdSupported,
  type AudioDeviceOption,
} from "../voice/audioDevices";
import { MicrophoneError, openMicrophone, type Microphone } from "../voice/capture";
import { playTestTone } from "../voice/playback";
import { voiceMessage } from "../voice/voiceMessages";

const DEFAULT = "";
const SAVED_ABSENT = "__saved__";

type DeviceField = "voiceInputDevice" | "voiceOutputDevice";

/** VAI-FR-XADO: the value of a selector for a saved device. */
function choiceOf(saved: VoiceDevice | null, devices: AudioDeviceOption[]): string {
  if (!saved) return DEFAULT;
  return matchDevice(saved, devices)?.id ?? SAVED_ABSENT;
}

function DeviceSelect({
  id,
  label,
  saved,
  devices,
  onChoose,
}: {
  id: string;
  label: string;
  saved: VoiceDevice | null;
  devices: AudioDeviceOption[];
  onChoose: (device: VoiceDevice | null) => void;
}) {
  const value = choiceOf(saved, devices);
  return (
    <div className="picker-field" style={{ marginBottom: 10, maxWidth: 360 }}>
      <label className="picker-field__label" htmlFor={id}>
        {label}
      </label>
      <select
        id={id}
        className="input"
        data-testid={id}
        value={value}
        onChange={(event) => {
          const next = event.target.value;
          if (next === SAVED_ABSENT) return;
          const device = devices.find((d) => d.id === next);
          onChoose(device ? { id: device.id, label: device.label } : null);
        }}
      >
        <option value={DEFAULT}>System default</option>
        {value === SAVED_ABSENT && saved && (
          <option value={SAVED_ABSENT}>{`${saved.label || saved.id} (not connected)`}</option>
        )}
        {devices
          .filter((device) => device.id !== "" && device.id !== "default")
          .map((device, index) => (
            <option key={device.id} value={device.id}>
              {device.label || `Device ${index + 1}`}
            </option>
          ))}
      </select>
    </div>
  );
}

/** VAI-FR-ZIFX: the live level of the selected microphone, as a bar. */
function LevelMeter({ level }: { level: number }) {
  const percent = Math.round(level * 100);
  return (
    <div
      role="meter"
      aria-label="Microphone level"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent}
      data-testid="voice-device-level"
      style={{
        width: 140,
        height: 6,
        borderRadius: 3,
        background: "var(--bg-sunken)",
        border: "1px solid var(--border-1)",
        overflow: "hidden",
      }}
    >
      <div style={{ width: `${percent}%`, height: "100%", background: "var(--accent)" }} />
    </div>
  );
}

export function VoiceDeviceSettings() {
  const [inputs, setInputs] = useState<AudioDeviceOption[]>([]);
  const [outputs, setOutputs] = useState<AudioDeviceOption[]>([]);
  const [prefs, setPrefs] = useState<AppPreferences | null>(null);
  const [testing, setTesting] = useState(false);
  const [level, setLevel] = useState(0);
  const [micError, setMicError] = useState<string | null>(null);
  const [tone, setTone] = useState<"idle" | "playing" | "failed">("idle");
  const mic = useRef<Microphone | null>(null);
  const testRun = useRef(0);
  const mounted = useRef(true);
  const canRoute = sinkIdSupported();

  const readDevices = useCallback(async () => {
    const [nextInputs, nextOutputs] = await Promise.all([
      listAudioDevices("audioinput"),
      listAudioDevices("audiooutput"),
    ]);
    if (!mounted.current) return;
    setInputs(nextInputs);
    setOutputs(nextOutputs);
  }, []);

  const stopTest = useCallback(() => {
    testRun.current += 1;
    mic.current?.stop();
    mic.current = null;
    if (mounted.current) {
      setTesting(false);
      setLevel(0);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    loadAppPreferences()
      .then((loaded) => {
        // A choice made while the read ran wins over the read.
        if (mounted.current) setPrefs((current) => ({ ...loaded, ...current }));
      })
      .catch(() => logWarn(["frontend"], "the audio device settings could not be read", {}));
    void readDevices();
    // VAI-FR-XADO: the list reads again when a device comes or goes.
    const unsubscribe = onDeviceChange(() => void readDevices());
    return () => {
      mounted.current = false;
      unsubscribe();
      // VAI-FR-ZIFX: the close of the section closes the microphone.
      testRun.current += 1;
      mic.current?.stop();
      mic.current = null;
    };
  }, [readDevices]);

  const save = (patch: Partial<AppPreferences>, what: string) => {
    // A choice before the record is read, or after a failed read, still shows.
    setPrefs((current) => ({ theme: "system", ...current, ...patch }));
    patchAppPreferences(patch).catch(() => {
      logWarn(["frontend"], "an audio device setting was not saved", { setting: what });
    });
  };

  const chooseDevice = (field: DeviceField, device: VoiceDevice | null) => {
    logInfo(["frontend"], "an audio device was chosen", {
      kind: field === "voiceInputDevice" ? "microphone" : "speaker",
      device: device?.label ?? "system default",
    });
    // VAI-FR-ZIFX: a change of the microphone ends the test.
    if (field === "voiceInputDevice") stopTest();
    save({ [field]: device }, field);
  };

  // VAI-FR-ZIFX.
  const toggleTest = async () => {
    if (testing) {
      stopTest();
      return;
    }
    const run = ++testRun.current;
    setTesting(true);
    setMicError(null);
    try {
      const opened = await openMicrophone((pcm) => {
        if (testRun.current === run && mounted.current) setLevel(levelOfPcm(pcm));
      });
      if (testRun.current !== run) {
        opened.stop();
        return;
      }
      mic.current = opened;
      // VAI-FR-XOOE: an open microphone gives the labels.
      void readDevices();
    } catch (error) {
      if (testRun.current !== run || !mounted.current) return;
      const code = error instanceof MicrophoneError ? error.message : "microphone_unavailable";
      logWarn(["frontend"], "the microphone test could not open", { code });
      setMicError(code);
      setTesting(false);
    }
  };

  // VAI-FR-XOOE.
  const allowAccess = async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      stream.getTracks().forEach((track) => track.stop());
    } catch (error) {
      const name = error instanceof DOMException || error instanceof Error ? error.name : "";
      logWarn(["frontend"], "microphone access was not allowed", { error: name });
      setMicError(name === "NotAllowedError" ? "microphone_denied" : "microphone_unavailable");
    }
    await readDevices();
  };

  // VAI-FR-BPUR.
  const playTone = () => {
    setTone("playing");
    playTestTone()
      .then(() => {
        if (mounted.current) setTone("idle");
      })
      .catch(() => {
        if (mounted.current) setTone("failed");
      });
  };

  const savedInput = prefs?.voiceInputDevice ?? null;
  const savedOutput = prefs?.voiceOutputDevice ?? null;
  const echo = prefs?.voiceEchoCancellation !== false;

  return (
    <div data-testid="voice-device-settings" style={{ marginTop: 20 }}>
      <h3 className="t-h3">Audio devices</h3>
      <div className="t-ui-sm t-muted" style={{ marginBottom: 10 }}>
        These devices apply to voice mode and to Hold to test.
      </div>

      {!hasLabels(inputs) && (
        <div style={{ marginBottom: 10 }}>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            data-testid="voice-device-allow"
            onClick={() => void allowAccess()}
          >
            Allow microphone access
          </button>
        </div>
      )}

      <DeviceSelect
        id="voice-device-microphone"
        label="Microphone"
        saved={savedInput}
        devices={inputs}
        onChoose={(device) => chooseDevice("voiceInputDevice", device)}
      />
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 6 }}>
        <button
          type="button"
          className="btn btn--sm"
          aria-pressed={testing}
          data-testid="voice-device-test-microphone"
          onClick={() => void toggleTest()}
        >
          {testing ? "Stop microphone test" : "Test microphone"}
        </button>
        {testing && <LevelMeter level={level} />}
      </div>
      {micError && (
        <div className="t-ui-sm" role="alert" data-testid="voice-device-mic-error" style={{ color: "var(--danger)" }}>
          {voiceMessage(micError)}
        </div>
      )}

      <div style={{ marginTop: 12 }}>
        {canRoute ? (
          <DeviceSelect
            id="voice-device-speaker"
            label="Speaker"
            saved={savedOutput}
            devices={outputs}
            onChoose={(device) => chooseDevice("voiceOutputDevice", device)}
          />
        ) : (
          <div className="picker-field" style={{ marginBottom: 10, maxWidth: 360 }}>
            <label className="picker-field__label" htmlFor="voice-device-speaker">
              Speaker
            </label>
            <select id="voice-device-speaker" className="input" data-testid="voice-device-speaker" disabled value={DEFAULT}>
              <option value={DEFAULT}>System default</option>
            </select>
            <div className="t-ui-sm t-muted" data-testid="voice-device-speaker-note" style={{ marginTop: 4 }}>
              Voice plays through the system output. Change it in the system sound settings.
            </div>
          </div>
        )}
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <button
            type="button"
            className="btn btn--sm"
            data-testid="voice-device-play-tone"
            disabled={tone === "playing"}
            onClick={playTone}
          >
            Play test sound
          </button>
          <span className="t-ui-sm t-muted" role="status" data-testid="voice-device-tone-status">
            {tone === "playing" && "Playing…"}
            {tone === "failed" && "The test sound could not play. See the log for details."}
          </span>
        </div>
      </div>

      <div style={{ marginTop: 14 }}>
        <label style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
          <input
            type="checkbox"
            data-testid="voice-device-echo"
            checked={echo}
            onChange={(event) =>
              save({ voiceEchoCancellation: event.target.checked }, "voiceEchoCancellation")
            }
          />
          <span className="t-label">Echo cancellation</span>
        </label>
        <div className="t-ui-sm t-muted" data-testid="voice-device-echo-note" style={{ marginTop: 4 }}>
          Turn off when you use headphones and the agent's voice is too quiet. With speakers, the
          agent can hear itself.
        </div>
      </div>
    </div>
  );
}
