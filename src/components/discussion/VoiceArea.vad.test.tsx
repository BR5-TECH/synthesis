/**
 * VAD sessions in the shared composer (`VOM-voice-mode.md`). The controller is
 * real, so a press here runs the true path to the mocked commands.
 */
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async (): Promise<{ captureId: string | null }> => ({
    captureId: null,
  })),
  feedVoiceActivity: vi.fn(async () => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello there"),
  // VOM-FR-QWMK: a transcript already in the document language.
  translateVoiceTranscript: vi.fn(async (text: string) => ({ text })),
  openVoiceActivitySession: vi.fn(async () => ({ activityId: "va1" })),
  setVoiceActivityGate: vi.fn(async () => undefined),
  speakVoiceSegment: vi.fn(async () => undefined),
}));
const mic = vi.hoisted(() => ({ stop: vi.fn() }));
/** The record that `load_app_preferences` answers, where a test sets one. */
const stored = vi.hoisted(() => ({ prefs: null as null | Record<string, unknown> }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "load_app_preferences" && stored.prefs) return stored.prefs;
    throw new Error(`unexpected command: ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("../../api", async (original) => ({
  ...(await original<typeof import("../../api")>()),
  ...api,
}));
vi.mock("../../voice/capture", async (original) => ({
  ...(await original<typeof import("../../voice/capture")>()),
  openMicrophone: vi.fn(async () => ({ rate: 48_000, stop: mic.stop })),
}));
vi.mock("../../voice/playback", () => ({
  isPlaybackIdle: vi.fn(() => false),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  setPlaybackFailureListener: vi.fn(),
  beginSegment: vi.fn(),
  endSegmentInput: vi.fn(),
  notePlaybackEntry: vi.fn(),
  stopPlayback: vi.fn(),
}));

import { resetLogBufferForTest } from "../../logging";
import { clearAllDiscussionSessions, getDiscussionSession } from "../../state/discussionSession";
import { noteMicLevel, resetMicLevelForTest } from "../../state/voiceLevel";
import {
  getVoiceState,
  patchVoiceState,
  resetVoiceStateForTest,
  type VoiceState,
} from "../../state/voiceSession";
import type { Discussion, ProjectAgent, VoiceActivityChange } from "../../types";
import {
  endVoiceForTeardown,
  noteActivity,
  noteDiscussionChanged,
  noteProvider,
  resetVoiceControllerForTest,
  setResponseMode,
  noteTranscript,
} from "../../voice/voiceController";
import { voiceMessage } from "../../voice/voiceMessages";
import { loadAppPreferences } from "../../state/appPreferences";
import { DiscussionComposer } from "./DiscussionComposer";
import { VoiceSessionBar } from "./VoiceSessionBar";
import { registerQuestionBlock } from "../../voice/voiceQuestions";

function agent(nickname: string): ProjectAgent {
  return {
    scope: "project",
    agent: { id: nickname, nickname, title: "", modelId: "m", instructions: "", voice: "cedar" },
    availability: "ready",
    shadowed: false,
  } as unknown as ProjectAgent;
}

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
}

const onReply = vi.fn(async () => undefined);

function reply(over: { disabled?: boolean; agents?: ProjectAgent[]; thread?: Partial<Discussion> } = {}) {
  return (
    <DiscussionComposer
      mode="reply"
      discussion={discussion(over.thread)}
      onReply={onReply}
      agents={over.agents ?? [agent("arch")]}
      disabled={over.disabled}
    />
  );
}

function record() {
  return screen.getByTestId("voice-record");
}

function indicator() {
  return screen.getByTestId("voice-vad-indicator");
}

function mute() {
  return screen.getByTestId("voice-mute");
}

function enterVad() {
  fireEvent.click(screen.getByTestId("voice-button"));
  fireEvent.click(screen.getByRole("button", { name: "VAD" }));
}

async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

/** VOM-FR-PDZR: selecting VAD opens the session, with no press. */
async function openSession() {
  enterVad();
  await waitFor(() => expect(getVoiceState().vadOpen).toBe(true));
}

function activity(state: VoiceActivityChange["state"], over: Partial<VoiceActivityChange> = {}) {
  act(() => {
    noteActivity({
      activityId: "va1",
      state,
      captureId: null,
      live: null,
      code: null,
      fatal: null,
      ...over,
    });
  });
}

function lastGate() {
  const calls = api.setVoiceActivityGate.mock.calls as unknown as [string, Record<string, unknown>][];
  return calls[calls.length - 1]?.[1];
}

beforeEach(() => {
  vi.clearAllMocks();
  stored.prefs = null;
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  clearAllDiscussionSessions();
  resetLogBufferForTest();
  resetMicLevelForTest();
  noteProvider(true, true);
});

afterEach(async () => {
  cleanup();
  endVoiceForTeardown();
  // The close of a session must end here, or it runs on in the next test.
  await flush();
  vi.restoreAllMocks();
});

describe("the VAD controls", () => {
  const table: [string, Partial<VoiceState>, string, string, string][] = [
    ["Idle", {}, "idle", "Voice session: Idle", "mic"],
    ["Listening", { vadOpen: true }, "listening", "Voice session: Listening", "mic-listening"],
    [
      "Hearing speech",
      { vadOpen: true, hearing: true },
      "hearing",
      "Voice session: Hearing speech",
      "waveform",
    ],
    [
      "Transcribing",
      { vadOpen: true, phase: "transcribing" },
      "transcribing",
      "Voice session: Transcribing",
      "text-lines",
    ],
    [
      "Posting",
      { vadOpen: true, phase: "posting" },
      "transcribing",
      "Voice session: Transcribing",
      "text-lines",
    ],
    [
      "Paused",
      { vadOpen: true, hearing: true, playing: true, playbackPaused: true },
      "paused",
      "Voice session: Paused",
      "pause",
    ],
  ];

  it.each(table)("VOM-FR-VOUG, VOM-FR-HSKZ: %s shows by icon, state, and name", async (_, change, state, name, icon) => {
    render(reply());
    await openSession();
    act(() => patchVoiceState({ vadOpen: false, ...change }));
    expect(indicator()).toHaveAttribute("data-vad", state);
    expect(indicator()).toHaveAttribute("aria-label", name);
    expect(indicator().querySelector(`[data-icon="${icon}"]`)).not.toBeNull();
  });

  it("VOM-FR-VOUG: the indicator is not a button", async () => {
    render(reply());
    await openSession();
    expect(indicator().tagName).toBe("SPAN");
    expect(indicator()).toHaveAttribute("role", "img");
  });

  it("VOM-FR-VOUG, VOM-FR-XDUY: Paused comes before Transcribing", async () => {
    render(reply());
    await openSession();
    act(() => patchVoiceState({ playbackPaused: true, transcribing: 1 }));
    expect(indicator()).toHaveAttribute("data-vad", "paused");
    act(() => patchVoiceState({ playbackPaused: false }));
    expect(indicator()).toHaveAttribute("data-vad", "transcribing");
  });

  it("VOM-FR-PDZR, VOM-FR-TISN: VAD mode has no record control, and shows the indicator, Mute, and the waveform", async () => {
    render(reply());
    await openSession();
    expect(screen.queryByTestId("voice-record")).toBeNull();
    expect(screen.queryByText(/Press to start listening/)).toBeNull();
    expect(indicator()).toBeInTheDocument();
    expect(mute()).toBeInTheDocument();
    expect(screen.getByTestId("voice-waveform")).toBeInTheDocument();
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-WFHN, VOM-FR-VQIU: the waveform stands after the status and fills the free width, and a transcript takes the width", async () => {
    render(reply());
    await openSession();
    const wave = screen.getByTestId("voice-waveform");
    expect(wave).toHaveClass("voice-waveform--fill");
    expect(wave.previousElementSibling).toBe(screen.getByTestId("voice-transcript"));
    const area = screen.getByTestId("voice-area");
    expect(area).toHaveAttribute("data-vad", "true");
    expect(area).toHaveAttribute("data-transcript", "false");
    act(() => patchVoiceState({ transcript: "the second paragraph" }));
    expect(area).toHaveAttribute("data-transcript", "true");
  });

  it("VOM-FR-HSKZ: Push-to-Talk has a record control with no VAD state", () => {
    render(reply());
    fireEvent.click(screen.getByTestId("voice-button"));
    expect(record()).not.toHaveAttribute("data-vad");
    expect(record()).toHaveAttribute("aria-label", "Record");
    expect(screen.queryByTestId("voice-mute")).toBeNull();
  });

  it("VOM-FR-SRGN, VOM-FR-HUXY: a route that is not live shows the timer, and text comes first", async () => {
    render(reply());
    await openSession();
    act(() =>
      patchVoiceState({
        hearing: true,
        phase: "recording",
        live: false,
        recordingSince: Date.now(),
      }),
    );
    expect(screen.getByTestId("voice-transcript")).toHaveTextContent(/^Recording 0:0/);
    act(() => patchVoiceState({ transcript: "the words" }));
    expect(screen.getByTestId("voice-transcript")).toHaveTextContent("the words");
  });

  it("VOM-FR-GTAQ: a disabled composer still opens the session, and the gate blocks utterances", async () => {
    render(reply({ disabled: true }));
    await openSession();
    expect(lastGate()).toMatchObject({ blocked: true });
  });

  it("VOM-FR-LAQO: in VAD mode the Mute toggle takes the focus of the hidden field", async () => {
    render(reply());
    await openSession();
    expect(mute()).toHaveFocus();
  });

  it("VOM-FR-LAQO: a switch back to Push-to-Talk gives focus to the record control", async () => {
    render(reply());
    await openSession();
    fireEvent.click(screen.getByRole("button", { name: "Push-to-Talk" }));
    await flush();
    expect(record()).toHaveFocus();
  });

  it("VOM-FR-LAQO: a switch leaves focus that the author moved elsewhere", async () => {
    render(
      <>
        <input data-testid="elsewhere" />
        {reply()}
      </>,
    );
    await openSession();
    const elsewhere = screen.getByTestId("elsewhere");
    elsewhere.focus();
    fireEvent.click(screen.getByRole("button", { name: "Push-to-Talk" }));
    await flush();
    expect(elsewhere).toHaveFocus();
  });

  it("VOM-FR-QKTE, VOM-FR-MUZB: Mute keeps the session open, and shows by icon, state, and name", async () => {
    render(reply());
    await openSession();
    expect(mute().tagName).toBe("BUTTON");
    expect(mute()).toHaveAttribute("aria-pressed", "false");
    expect(mute()).toHaveAccessibleName("Mute microphone");
    expect(mute()).toHaveAttribute("title", "Mute microphone");
    fireEvent.click(mute());
    expect(mute()).toHaveAttribute("aria-pressed", "true");
    expect(mute()).toHaveAccessibleName("Unmute microphone");
    expect(mute().querySelector('[data-icon="mic-off"]')).not.toBeNull();
    expect(indicator()).toHaveAccessibleName("Voice session: Listening, muted");
    expect(screen.getByTestId("voice-transcript")).toHaveTextContent(/Muted/);
    expect(getVoiceState()).toMatchObject({ vadOpen: true, muted: true });
    expect(lastGate()).toMatchObject({ blocked: true });
    expect(api.closeVoiceActivitySession).not.toHaveBeenCalled();
  });

  it("VOM-FR-RBWX: Unmute frees the gate again", async () => {
    render(reply());
    await openSession();
    fireEvent.click(mute());
    fireEvent.click(mute());
    expect(mute()).toHaveAttribute("aria-pressed", "false");
    expect(lastGate()).toMatchObject({ blocked: false });
  });

  it("VOM-FR-CUCN: a failed open shows Open again, and it opens one time", async () => {
    api.openVoiceActivitySession.mockRejectedValueOnce("voice_detector_unavailable");
    render(reply());
    enterVad();
    await flush();
    expect(getVoiceState().vadOpen).toBe(false);
    expect(screen.getByTestId("voice-error")).toBeInTheDocument();
    expect(mute()).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(screen.getByTestId("voice-vad-reopen"));
    await flush();
    expect(getVoiceState().vadOpen).toBe(true);
    expect(screen.queryByTestId("voice-vad-reopen")).toBeNull();
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(2);
  });
});

describe("the waveform", () => {
  function bars() {
    return [...screen.getByTestId("voice-waveform").querySelectorAll<HTMLElement>(".voice-waveform__bar")];
  }

  function lastBar() {
    const all = bars();
    return all[all.length - 1];
  }

  it("VOM-FR-WFHN, VOM-FR-PJOC: it shows the live level and is decorative", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    render(reply());
    await openSession();
    const wave = screen.getByTestId("voice-waveform");
    expect(wave).toHaveAttribute("aria-hidden", "true");
    expect(wave).not.toHaveAttribute("aria-live");
    expect(wave).toHaveAttribute("data-flat", "false");
    now.mockReturnValue(2_000);
    act(() => noteMicLevel(0.8));
    expect(lastBar().style.transform).toBe("scaleY(0.8)");
  });

  it("VOM-FR-ZDLY: it is flat while muted and while no session is open", async () => {
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000);
    render(reply());
    enterVad();
    expect(screen.getByTestId("voice-waveform")).toHaveAttribute("data-flat", "true");
    await waitFor(() => expect(getVoiceState().vadOpen).toBe(true));
    now.mockReturnValue(2_000);
    act(() => noteMicLevel(0.8));
    expect(lastBar().style.transform).toBe("scaleY(0.8)");
    fireEvent.click(mute());
    now.mockReturnValue(3_000);
    act(() => noteMicLevel(0.8));
    expect(screen.getByTestId("voice-waveform")).toHaveAttribute("data-flat", "true");
    expect(bars().every((b) => b.style.transform === "scaleY(0.12)")).toBe(true);
  });

  it("VOM-FR-PJOC: with reduced motion it changes at most two times each second", async () => {
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: (query: string) => ({ matches: query.includes("reduce"), media: query }),
    });
    onTestFinished(() => {
      Reflect.deleteProperty(window, "matchMedia");
    });
    const now = vi.spyOn(Date, "now").mockReturnValue(10_000);
    render(reply());
    await openSession();
    expect(screen.getByTestId("voice-waveform")).toHaveAttribute("data-reduced-motion", "true");
    now.mockReturnValue(10_600);
    act(() => noteMicLevel(0.5));
    now.mockReturnValue(10_700);
    act(() => noteMicLevel(0.9));
    expect(lastBar().style.transform).toBe("scaleY(0.5)");
    now.mockReturnValue(11_200);
    act(() => noteMicLevel(0.9));
    expect(lastBar().style.transform).toBe("scaleY(0.9)");
  });
});

describe("the toggles", () => {
  it.each(["opening", "recording", "transcribing", "posting"] as const)(
    "VOM-FR-VGCE: VAD is disabled with its reason during %s",
    (phase) => {
      render(reply());
      fireEvent.click(screen.getByTestId("voice-button"));
      act(() => patchVoiceState({ phase }));
      const vad = screen.getByRole("button", { name: "VAD" });
      expect(vad).toBeDisabled();
      expect(vad).toHaveAccessibleDescription(/not available/);
    },
  );

  it("VOM-FR-VGCE: Push-to-Talk can be selected at any time", () => {
    render(reply());
    enterVad();
    act(() => patchVoiceState({ phase: "recording" }));
    const ptt = screen.getByRole("button", { name: "Push-to-Talk" });
    expect(ptt).toBeEnabled();
    fireEvent.click(ptt);
    expect(getVoiceState().inputMode).toBe("ptt");
  });

  it("VOM-FR-IAVZ: Voice keeps its reason in its tooltip and description", () => {
    noteProvider(true, false);
    render(reply());
    fireEvent.click(screen.getByTestId("voice-button"));
    const voice = screen.getByRole("button", { name: "Voice" });
    expect(voice).toBeDisabled();
    expect(voice).toHaveAttribute("title", "The active provider has no speech route.");
    expect(voice).toHaveAccessibleDescription("The active provider has no speech route.");
    expect(voice.querySelector("svg")).not.toBeNull();
  });

  it("VOM-FR-TISN, VOM-FR-IAVZ: the response options are icon buttons with names", () => {
    render(reply());
    fireEvent.click(screen.getByTestId("voice-button"));
    const text = screen.getByRole("button", { name: "Text" });
    expect(text).toHaveAttribute("aria-pressed", "true");
    expect(text).toHaveAttribute("title", "Text");
    expect(text).toHaveTextContent("");
  });

  it("VOM-FR-RTSU: macOS shows the hint while VAD is selected", () => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
    render(reply());
    fireEvent.click(screen.getByTestId("voice-button"));
    expect(screen.queryByTestId("voice-vad-hint")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "VAD" }));
    const hint = screen.getByTestId("voice-vad-hint");
    expect(hint).toHaveAccessibleName(/sound lower/);
    expect(hint).toHaveAttribute("title", hint.getAttribute("aria-label"));
    expect(hint).toHaveAttribute("tabindex", "0");
    expect(screen.getByTestId("voice-area")).not.toHaveTextContent(/lower/);
  });

  it("VOM-FR-RTSU, VOM-FR-OGFM: with echo cancellation off, macOS shows no hint", async () => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
    stored.prefs = { theme: "system", voiceEchoCancellation: false };
    await loadAppPreferences();
    render(reply());
    enterVad();
    expect(screen.queryByTestId("voice-vad-hint")).toBeNull();
  });

  it("VOM-FR-RTSU: another platform shows no hint", () => {
    vi.spyOn(navigator, "platform", "get").mockReturnValue("Linux x86_64");
    render(reply());
    enterVad();
    expect(screen.queryByTestId("voice-vad-hint")).toBeNull();
  });
});

describe("what a session shows", () => {
  it("VOM-FR-QDWW: Stop stays while a reply stands paused, and it drops the reply", async () => {
    render(reply());
    await openSession();
    act(() => patchVoiceState({ playing: true, playbackPaused: true }));
    fireEvent.click(screen.getByTestId("voice-stop"));
    expect(screen.queryByTestId("voice-stop")).toBeNull();
    expect(getVoiceState().playbackPaused).toBe(false);
  });

  it.each(["voice_detector_unavailable", "voice_detector_failed", "voice_no_audio"])(
    "VOM-FR-CSXI, VOM-FR-CUCN: %s has its own text and no Retry",
    (code) => {
      render(reply());
      enterVad();
      act(() => patchVoiceState({ error: code }));
      expect(screen.getByTestId("voice-error")).toHaveTextContent(voiceMessage(code));
      expect(voiceMessage(code)).not.toBe("The voice operation failed.");
      expect(screen.queryByTestId("voice-retry")).toBeNull();
    },
  );

  it("VOM-FR-LNUH: a new roster reaches the gate", async () => {
    const view = render(reply());
    await openSession();
    expect(lastGate()).toMatchObject({ keyterms: ["arch"] });
    view.rerender(reply({ agents: [agent("arch"), agent("helga")] }));
    expect(lastGate()).toMatchObject({ keyterms: ["arch", "helga"] });
  });

  it("VOM-FR-GTAQ, VOM-FR-ZUWW: a disabled composer blocks the gate, and it comes back with no press", async () => {
    const view = render(reply());
    await openSession();
    view.rerender(reply({ disabled: true }));
    expect(lastGate()).toMatchObject({ blocked: true });
    expect(getVoiceState().vadOpen).toBe(true);
    view.rerender(reply());
    expect(lastGate()).toMatchObject({ blocked: false });
  });
});

describe("the session bar", () => {
  /** VOM-FR-VGLT: the bar shows while a question set stands. */
  function setStands() {
    return registerQuestionBlock("d1", {
      setId: "q1",
      showing: () => ({ position: 1, text: "Which one?", options: [] }),
      answer: vi.fn(),
    });
  }

  async function barWithSession() {
    const view = render(reply());
    await openSession();
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "cedar"} nicknames={() => ["arch"]} />);
    return view;
  }

  it("VOM-FR-VGLT: shows the state and the open microphone", async () => {
    await barWithSession();
    expect(indicator()).toHaveAttribute("data-vad", "listening");
    expect(screen.getByTestId("voice-waveform")).toBeInTheDocument();
    expect(screen.queryByTestId("voice-record")).toBeNull();
    expect(screen.getByTestId("voice-session-bar")).toHaveTextContent(/microphone is open/);
    expect(screen.queryByTestId("voice-stop")).toBeNull();
    act(() => patchVoiceState({ playing: true, error: "voice_no_audio" }));
    expect(screen.getByTestId("voice-stop")).toBeInTheDocument();
    expect(screen.getByTestId("voice-error")).toBeInTheDocument();
  });

  it("VOM-FR-GPNK, VOM-FR-VGLT: the Mute toggle of the bar mutes, and the line says so", async () => {
    await barWithSession();
    fireEvent.click(mute());
    expect(getVoiceState()).toMatchObject({ vadOpen: true, muted: true });
    expect(screen.getByTestId("voice-session-bar")).toHaveTextContent(/microphone is muted/);
    expect(screen.getByTestId("voice-waveform")).toHaveAttribute("data-flat", "true");
    fireEvent.click(mute());
    expect(screen.getByTestId("voice-session-bar")).toHaveTextContent(/microphone is open/);
  });

  it("VOM-FR-GTAQ, VOM-FR-LNUH: a mounted bar leaves the gate open and sends the roster", async () => {
    await barWithSession();
    expect(lastGate()).toMatchObject({ blocked: false, keyterms: ["arch"] });
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-HUXY: a recording at the bar shows its words in place of the line", async () => {
    await barWithSession();
    setStands();
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c1", live: true });
    await flush();
    noteTranscript("c1", "two specs");
    await flush();
    expect(screen.getByTestId("voice-session-bar")).toHaveTextContent("two specs");
    expect(screen.getByTestId("voice-session-bar")).not.toHaveTextContent("Speak to answer");
  });

  it("VOM-FR-LAQO: the bar takes focus from the surface that holds it", async () => {
    render(
      <div data-testid="surface" tabIndex={-1}>
        <input data-testid="elsewhere" />
      </div>,
    );
    screen.getByTestId("surface").focus();
    cleanup();
    const host = document.createElement("div");
    host.tabIndex = -1;
    document.body.append(host);
    host.focus();
    render(<VoiceSessionBar sessionKey="d1" voiceOf={() => null} nicknames={() => []} />, {
      container: host,
    });
    expect(record()).toHaveFocus();
    host.remove();
  });

  it("VOM-FR-LAQO: Stop gives focus to the record control as it goes", async () => {
    render(<VoiceSessionBar sessionKey="d1" voiceOf={() => null} nicknames={() => []} />);
    act(() => patchVoiceState({ playing: true }));
    const stop = screen.getByTestId("voice-stop");
    stop.focus();
    fireEvent.click(stop);
    await flush();
    expect(screen.queryByTestId("voice-stop")).toBeNull();
    expect(record()).toHaveFocus();
  });

  it("VOM-FR-LAQO: the bar takes the focus that went with the composer", async () => {
    await barWithSession();
    expect(mute()).toHaveFocus();
  });

  it("VOM-FR-LAQO: the bar takes no focus that the author holds elsewhere", async () => {
    render(
      <>
        <input data-testid="elsewhere" />
        <VoiceSessionBar sessionKey="d1" voiceOf={() => null} nicknames={() => ["arch"]} />
      </>,
    );
    expect(document.activeElement).toBe(record());
    cleanup();
    render(<input data-testid="elsewhere" />);
    const elsewhere = screen.getByTestId("elsewhere");
    elsewhere.focus();
    render(<VoiceSessionBar sessionKey="d1" voiceOf={() => null} nicknames={() => ["arch"]} />);
    expect(elsewhere).toHaveFocus();
  });

  it("DDS-FR-QPHL: the bar takes the class that holds the measure of the composer", async () => {
    await barWithSession();
    expect(screen.getByTestId("voice-session-bar")).toHaveClass("comment-card__reply");
  });

  it("VOM-FR-ABPU: an utterance that ends at the bar joins the session body", async () => {
    const view = render(reply());
    await openSession();
    activity("speech_start");
    activity("speech_confirmed", { captureId: "c1", live: true });
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "cedar"} nicknames={() => ["arch"]} />);
    setStands();
    activity("speech_end", { captureId: "c1" });
    await flush();
    expect(getDiscussionSession("d1").body).toBe("hello there");
    expect(getVoiceState().note).toBe("kept_in_body");
    expect(onReply).not.toHaveBeenCalled();
  });

  it("VOM-FR-UALP: a reply at the bar takes the newest voice", async () => {
    const view = render(reply());
    await openSession();
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "cedar"} nicknames={() => ["arch"]} />);
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "marin"} nicknames={() => ["arch"]} />);
    setResponseMode("voice");
    noteDiscussionChanged(
      discussion({
        comments: [
          {
            id: "r1",
            author: { kind: "agent", agentId: "a1", handle: "arch" },
            body: "<spoken>A reply.</spoken>",
            quotes: [],
            attachments: [],
            createdAt: "2026-02-01T00:00:00Z",
          },
        ] as never,
      }),
    );
    await flush();
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      expect.any(String),
      "marin",
      expect.any(Function),
    );
  });
});
