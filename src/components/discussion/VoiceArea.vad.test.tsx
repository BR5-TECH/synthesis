/**
 * VAD sessions in the shared composer (`VOM-voice-mode.md`). The controller is
 * real, so a press here runs the true path to the mocked commands.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

const api = vi.hoisted(() => ({
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  closeVoiceActivitySession: vi.fn(async (): Promise<{ captureId: string | null }> => ({
    captureId: null,
  })),
  feedVoiceActivity: vi.fn(async () => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello there"),
  openVoiceActivitySession: vi.fn(async () => ({ activityId: "va1" })),
  setVoiceActivityGate: vi.fn(async () => undefined),
  speakVoiceSegment: vi.fn(async () => undefined),
}));
const mic = vi.hoisted(() => ({ stop: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
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
  stopPlayback: vi.fn(),
}));

import { resetLogBufferForTest } from "../../logging";
import { clearAllDiscussionSessions, getDiscussionSession } from "../../state/discussionSession";
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
} from "../../voice/voiceController";
import { voiceMessage } from "../../voice/voiceMessages";
import { DiscussionComposer } from "./DiscussionComposer";
import { VoiceSessionBar } from "./VoiceSessionBar";

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

async function openSession() {
  enterVad();
  fireEvent.pointerDown(record());
  fireEvent.pointerUp(record());
  await flush();
  expect(getVoiceState().vadOpen).toBe(true);
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
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  clearAllDiscussionSessions();
  resetLogBufferForTest();
  noteProvider(true, true);
});

afterEach(async () => {
  cleanup();
  endVoiceForTeardown();
  // The close of a session must end here, or it runs on in the next test.
  await flush();
  vi.restoreAllMocks();
});

describe("the record control", () => {
  const table: [string, Partial<VoiceState>, string, string, string][] = [
    ["Idle", {}, "idle", "Idle. Open a voice session", "mic"],
    [
      "Listening",
      { vadOpen: true },
      "listening",
      "Listening. Close the voice session",
      "mic-listening",
    ],
    [
      "Hearing speech",
      { vadOpen: true, hearing: true },
      "hearing",
      "Hearing speech. Close the voice session",
      "waveform",
    ],
    [
      "Transcribing",
      { vadOpen: true, phase: "transcribing" },
      "transcribing",
      "Transcribing. Close the voice session",
      "text-lines",
    ],
    [
      "Posting",
      { vadOpen: true, phase: "posting" },
      "transcribing",
      "Transcribing. Close the voice session",
      "text-lines",
    ],
    [
      "Paused",
      { vadOpen: true, hearing: true, playing: true, playbackPaused: true },
      "paused",
      "Paused. Close the voice session",
      "pause",
    ],
  ];

  it.each(table)("VOM-FR-VOUG: %s shows by icon, state, and name", (_, change, state, name, icon) => {
    render(reply());
    enterVad();
    act(() => patchVoiceState(change));
    expect(record()).toHaveAttribute("data-vad", state);
    expect(record()).toHaveAttribute("aria-label", name);
    expect(record().querySelector(`[data-icon="${icon}"]`)).not.toBeNull();
  });

  it("VOM-FR-OTRF, VOM-FR-VOUG: Transcribing shows while a reply stands paused", () => {
    render(reply());
    enterVad();
    act(() => patchVoiceState({ vadOpen: true, playbackPaused: true, phase: "transcribing" }));
    expect(record()).toHaveAttribute("data-vad", "transcribing");
  });

  it("VOM-FR-VOUG: after a close, the name offers no close", () => {
    render(reply());
    enterVad();
    act(() => patchVoiceState({ vadOpen: false, phase: "transcribing" }));
    expect(record()).toHaveAttribute("aria-label", "Transcribing");
  });

  it("VOM-FR-HSKZ: Push-to-Talk has no VAD state", () => {
    render(reply());
    fireEvent.click(screen.getByTestId("voice-button"));
    expect(record()).not.toHaveAttribute("data-vad");
    expect(record()).toHaveAttribute("aria-label", "Record");
  });

  it("VOM-FR-SRGN, VOM-FR-HUXY: a route that is not live shows the timer, and text comes first", () => {
    render(reply());
    enterVad();
    act(() =>
      patchVoiceState({
        vadOpen: true,
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

  it("VOM-FR-ZUWW: a disabled composer opens nothing", () => {
    render(reply({ disabled: true }));
    enterVad();
    expect(record()).toBeDisabled();
  });

  it("VOM-FR-ZUWW: a disabled composer can still close an open session", async () => {
    const view = render(reply());
    await openSession();
    view.rerender(reply({ disabled: true }));
    expect(record()).toBeEnabled();
    fireEvent.pointerDown(record());
    await flush();
    expect(getVoiceState().vadOpen).toBe(false);
  });

  it("VOM-FR-PDZR: a click with no pointer, as assistive technology sends, opens a session", async () => {
    render(reply());
    enterVad();
    fireEvent.click(record(), { detail: 0 });
    await flush();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-PDZR: the click after a pointer press does not press twice", async () => {
    render(reply());
    enterVad();
    fireEvent.pointerDown(record());
    fireEvent.pointerUp(record());
    fireEvent.click(record(), { detail: 1 });
    await flush();
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-XEIA: an open session stays closable while it transcribes", () => {
    render(reply());
    enterVad();
    act(() => patchVoiceState({ vadOpen: true, phase: "transcribing" }));
    expect(record()).toHaveAttribute("aria-disabled", "false");
  });

  it("VOM-FR-PDZR, VOM-FR-PECJ: a press opens, a release does not close, and a press closes", async () => {
    render(reply());
    await openSession();
    fireEvent.pointerDown(record());
    fireEvent.pointerUp(record());
    await flush();
    expect(getVoiceState().vadOpen).toBe(false);
    expect(mic.stop).toHaveBeenCalledTimes(1);
  });

  it.each([" ", "Enter"])("VOM-FR-PDZR, VOM-FR-PECJ: key %j opens and closes once", async (key) => {
    render(reply());
    enterVad();
    fireEvent.keyDown(record(), { key });
    fireEvent.keyDown(record(), { key, repeat: true });
    fireEvent.keyUp(record(), { key });
    await flush();
    expect(getVoiceState().vadOpen).toBe(true);
    fireEvent.keyDown(record(), { key });
    await flush();
    expect(getVoiceState().vadOpen).toBe(false);
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
  async function barWithSession() {
    const view = render(reply());
    await openSession();
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "cedar"} />);
    return view;
  }

  it("VOM-FR-VGLT: shows the state and the open microphone", async () => {
    await barWithSession();
    expect(record()).toHaveAttribute("data-vad", "listening");
    expect(screen.getByTestId("voice-session-bar")).toHaveTextContent(/microphone is open/);
    expect(screen.queryByTestId("voice-stop")).toBeNull();
    act(() => patchVoiceState({ playing: true, error: "voice_no_audio" }));
    expect(screen.getByTestId("voice-stop")).toBeInTheDocument();
    expect(screen.getByTestId("voice-error")).toBeInTheDocument();
  });

  it("VOM-FR-GPNK: the record control closes the session, and it does not open one", async () => {
    await barWithSession();
    fireEvent.pointerDown(record());
    await flush();
    expect(getVoiceState().vadOpen).toBe(false);
    fireEvent.pointerDown(record());
    await flush();
    expect(getVoiceState().vadOpen).toBe(false);
    expect(api.openVoiceActivitySession).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-VGLT, VOM-FR-GTAQ: a mounted bar blocks the gate", async () => {
    await barWithSession();
    expect(lastGate()).toMatchObject({ blocked: true, keyterms: [] });
    expect(getVoiceState().vadOpen).toBe(true);
  });

  it("VOM-FR-LAQO: the bar takes the focus that went with the composer", async () => {
    await barWithSession();
    expect(record()).toHaveFocus();
  });

  it("VOM-FR-LAQO: the bar takes no focus that the author holds elsewhere", async () => {
    render(
      <>
        <input data-testid="elsewhere" />
        <VoiceSessionBar sessionKey="d1" voiceOf={() => null} />
      </>,
    );
    expect(document.activeElement).toBe(record());
    cleanup();
    render(<input data-testid="elsewhere" />);
    const elsewhere = screen.getByTestId("elsewhere");
    elsewhere.focus();
    render(<VoiceSessionBar sessionKey="d1" voiceOf={() => null} />);
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
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "cedar"} />);
    activity("speech_end", { captureId: "c1" });
    await flush();
    expect(getDiscussionSession("d1").body).toBe("hello there");
    expect(getVoiceState().note).toBe("kept_in_body");
    expect(onReply).not.toHaveBeenCalled();
  });

  it("VOM-FR-UALP: a reply at the bar takes the newest voice", async () => {
    const view = render(reply());
    await openSession();
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "cedar"} />);
    view.rerender(<VoiceSessionBar sessionKey="d1" voiceOf={() => "marin"} />);
    setResponseMode("voice");
    noteDiscussionChanged(
      discussion({
        comments: [
          {
            id: "r1",
            author: { kind: "agent", agentId: "a1", handle: "arch" },
            body: "A reply.",
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
