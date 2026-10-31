import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  appendVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello"),
  retryVoiceCapture: vi.fn(async (): Promise<string> => "again"),
  speakVoiceSegment: vi.fn(async () => undefined),
  startVoiceCapture: vi.fn(async () => ({ sessionId: "s1", route: "realtime", live: true })),
}));
const mic = vi.hoisted(() => ({
  stop: vi.fn(),
  open: vi.fn(),
}));
const audio = vi.hoisted(() => ({
  isPlaybackIdle: vi.fn(() => false),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
}));
vi.mock("./playback", () => audio);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerBody,
} from "../state/discussionSession";
import { getVoiceState, resetVoiceStateForTest } from "../state/voiceSession";
import type { Comment, Discussion } from "../types";
import { MicrophoneError } from "./capture";
import { errorCode } from "./voiceMessages";
import {
  RECORDING_LIMIT_MS,
  SHORT_PRESS_MS,
  TAIL_MS,
  bindVoiceToDiscussion,
  endVoiceForTeardown,
  enterVoiceMode,
  leaveVoiceMode,
  noteDiscussionChanged,
  noteProvider,
  noteTranscript,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  resetVoiceControllerForTest,
  retryRecording,
  setResponseMode,
  setVoiceBehaviour,
  stopSpeech,
  voiceComposerMounted,
  type PostOutcome,
  type VoiceComposerHandle,
} from "./voiceController";

const KEY = "d1";

function comment(id: string, kind: "agent" | "human", body = "A reply."): Comment {
  return {
    id,
    author:
      kind === "agent"
        ? { kind: "agent", agentId: "a1", handle: "arch" }
        : { kind: "human", login: "me" },
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function discussion(comments: Comment[], id = KEY): Discussion {
  return {
    id,
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

function handle(over: Partial<VoiceComposerHandle> = {}) {
  return {
    post: vi.fn(async (): Promise<PostOutcome> => "posted"),
    tagged: (text: string) => text,
    blocked: () => false,
    nicknames: () => ["arch"],
    voiceOf: () => "cedar",
    ...over,
  } satisfies VoiceComposerHandle;
}

async function settle() {
  await vi.advanceTimersByTimeAsync(0);
}

/** Press, let the microphone open, hold, and release. */
async function record(holdMs = 1000) {
  pressRecord();
  await settle();
  await vi.advanceTimersByTimeAsync(holdMs);
  releaseRecord();
  await vi.advanceTimersByTimeAsync(TAIL_MS);
  await settle();
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  clearAllDiscussionSessions();
  api.finishVoiceCapture.mockImplementation(async () => "hello");
  api.retryVoiceCapture.mockImplementation(async () => "again");
  api.startVoiceCapture.mockImplementation(async () => ({
    sessionId: "s1",
    route: "realtime",
    live: true,
  }));
  api.speakVoiceSegment.mockImplementation(async () => undefined);
  mic.open.mockImplementation(async () => ({ rate: 48_000, stop: mic.stop }));
  audio.isPlaybackIdle.mockReturnValue(false);
  noteProvider(true, true);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("entering and leaving", () => {
  it("VOM-FR-OSTM: a second composer takes voice mode from the first", () => {
    enterVoiceMode("a", null);
    enterVoiceMode("b", null);
    expect(getVoiceState().activeKey).toBe("b");
  });

  it("VOM-FR-SIEO: entering calls no provider", () => {
    enterVoiceMode(KEY, discussion([]));
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
    expect(mic.open).not.toHaveBeenCalled();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-MQHD: nothing leaves memory, and leaving resets the response mode", () => {
    enterVoiceMode(KEY, discussion([]));
    setResponseMode("voice");
    expect(getVoiceState().responseMode).toBe("voice");
    leaveVoiceMode();
    expect(getVoiceState()).toMatchObject({ activeKey: null, responseMode: "text" });
  });

  it("VOM-FR-FHYY: voice mode with no mounted composer is not reachable", () => {
    enterVoiceMode(KEY, null);
    expect(voiceComposerMounted()).toBe(false);
    pressRecord();
    expect(mic.open).not.toHaveBeenCalled();
    const unregister = registerVoiceComposer(KEY, handle());
    expect(voiceComposerMounted()).toBe(true);
    unregister();
    expect(voiceComposerMounted()).toBe(false);
  });

  it("VOM-FR-WMAP: losing the speech route returns the response mode to text", () => {
    enterVoiceMode(KEY, null);
    setResponseMode("voice");
    noteProvider(true, false);
    expect(getVoiceState()).toMatchObject({ providerSpeaks: false, responseMode: "text" });
  });
});

describe("recording", () => {
  beforeEach(() => {
    enterVoiceMode(KEY, discussion([]));
  });

  it("VOM-FR-HHWL, VOM-FR-WYIE, VOM-FR-LNUH: a hold records, then posts the transcript", async () => {
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await record();
    expect(api.startVoiceCapture).toHaveBeenCalledWith(48_000, ["arch"]);
    expect(api.finishVoiceCapture).toHaveBeenCalledWith("s1");
    expect(composer.post).toHaveBeenCalledWith("hello");
    expect(getVoiceState().phase).toBe("idle");
  });

  it("VOM-FR-HHWL: nothing ends while the control is held", async () => {
    registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(getVoiceState().phase).toBe("recording");
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-LIFL: one press starts, the next stops, and a release does nothing", async () => {
    setVoiceBehaviour("toggle");
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    pressRecord();
    await settle();
    releaseRecord();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(getVoiceState().phase).toBe("recording");
    pressRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(composer.post).toHaveBeenCalledWith("hello");
  });

  it("VOM-FR-FTFU, VOM-FR-YIJS: the microphone goes after the tail, and not before", async () => {
    registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS - 1);
    expect(mic.stop).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(mic.stop).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-DGOG: a hold under 300 ms posts nothing", async () => {
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await record(SHORT_PRESS_MS - 1);
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getVoiceState().phase).toBe("idle");
  });

  it("VOM-FR-DGOG: a release before the microphone opens also posts nothing", async () => {
    let open: (m: { rate: number; stop: () => void }) => void = () => undefined;
    mic.open.mockImplementation(() => new Promise((resolve) => (open = resolve)));
    registerVoiceComposer(KEY, handle());
    pressRecord();
    releaseRecord();
    open({ rate: 48_000, stop: mic.stop });
    await settle();
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState().phase).toBe("idle");
  });

  it("VOM-FR-WSXW: a recording ends at 10 minutes", async () => {
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(RECORDING_LIMIT_MS + TAIL_MS);
    await settle();
    expect(composer.post).toHaveBeenCalled();
  });

  it("VOM-FR-VVTY: a press stops playback", async () => {
    registerVoiceComposer(KEY, handle());
    pressRecord();
    expect(audio.stopPlayback).toHaveBeenCalled();
    await settle();
  });

  it("VOM-FR-HUXY: live text shows, also after the release", async () => {
    let finish: (text: string) => void = () => undefined;
    api.finishVoiceCapture.mockImplementation(() => new Promise((r) => (finish = r)));
    registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    noteTranscript("s1", "hel");
    expect(getVoiceState().transcript).toBe("hel");
    noteTranscript("other", "x");
    expect(getVoiceState().transcript).toBe("hel");
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    noteTranscript("s1", "hello");
    expect(getVoiceState().transcript).toBe("hello");
    finish("hello");
    await settle();
  });

  it("VOM-FR-SRGN: a route that is not live records with a timer, then transcribes", async () => {
    api.startVoiceCapture.mockImplementation(async () => ({
      sessionId: "s1",
      route: "http",
      live: false,
    }));
    let finish: (text: string) => void = () => undefined;
    api.finishVoiceCapture.mockImplementation(() => new Promise((r) => (finish = r)));
    registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    expect(getVoiceState()).toMatchObject({ phase: "recording", live: false });
    expect(getVoiceState().recordingSince).not.toBeNull();
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(getVoiceState().phase).toBe("transcribing");
    finish("done");
    await settle();
  });

  it("VOM-FR-RENV: an empty transcript posts nothing, and says so", async () => {
    api.finishVoiceCapture.mockImplementation(async () => "  ");
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await record();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getVoiceState().note).toBe("no_speech");
  });

  it("VOM-FR-TAQJ: the spoken address goes through the composer", async () => {
    const composer = handle({ tagged: (t) => `@arch ${t}` });
    registerVoiceComposer(KEY, composer);
    await record();
    expect(composer.post).toHaveBeenCalledWith("@arch hello");
  });

  it("VOM-FR-BVNX: a post that reaches nobody says so", async () => {
    registerVoiceComposer(KEY, handle({ post: vi.fn(async () => "reached_nobody" as const) }));
    await record();
    expect(getVoiceState().note).toBe("no_agent");
  });

  it("VOM-FR-ABPU: a post in flight keeps the transcript in the session body", async () => {
    setComposerBody(KEY, "typed");
    registerVoiceComposer(KEY, handle({ post: vi.fn(async () => "busy" as const) }));
    await record();
    expect(getDiscussionSession(KEY).body).toBe("typed\nhello");
    expect(getVoiceState().note).toBe("kept_in_body");
  });

  it("VOM-FR-ABPU, VOM-FR-EGNX: a composer that goes ends the recording, and the text stays", async () => {
    const unregister = registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    unregister();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(getDiscussionSession(KEY).body).toBe("hello");
    expect(getVoiceState().note).toBe("kept_in_body");
  });

  it("VOM-FR-ABPU: a question set that arrives keeps the transcript", async () => {
    let blocked = false;
    const composer = handle({ blocked: () => blocked });
    registerVoiceComposer(KEY, composer);
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    blocked = true;
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("hello");
  });

  it("VOM-FR-JBJI: leaving ends the recording as VOM-FR-ABPU states", async () => {
    registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    leaveVoiceMode();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(getDiscussionSession(KEY).body).toBe("hello");
  });

  it("VOM-FR-ZUWW: a disabled composer does not record", () => {
    registerVoiceComposer(KEY, handle({ blocked: () => true }));
    pressRecord();
    expect(mic.open).not.toHaveBeenCalled();
  });
});

describe("failure", () => {
  beforeEach(() => {
    enterVoiceMode(KEY, discussion([]));
  });

  it("VOM-FR-ZFSA: a microphone failure shows and posts nothing, with no retry", async () => {
    mic.open.mockImplementation(async () => {
      throw new MicrophoneError("microphone_denied");
    });
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    pressRecord();
    await settle();
    expect(getVoiceState()).toMatchObject({
      error: "microphone_denied",
      canRetry: false,
      phase: "idle",
    });
    expect(composer.post).not.toHaveBeenCalled();
  });

  it("VOM-FR-ZFSA: a refused start releases the microphone", async () => {
    api.startVoiceCapture.mockImplementation(async () => {
      throw "no_active_provider";
    });
    registerVoiceComposer(KEY, handle());
    pressRecord();
    await settle();
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState().error).toBe("no_active_provider");
  });

  it("VOM-FR-WICV: a provider failure offers Retry, and a success posts", async () => {
    api.finishVoiceCapture.mockImplementation(async () => {
      throw "provider_unreachable";
    });
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await record();
    expect(getVoiceState()).toMatchObject({ error: "provider_unreachable", canRetry: true });
    expect(composer.post).not.toHaveBeenCalled();
    await retryRecording();
    expect(api.retryVoiceCapture).toHaveBeenCalledWith("s1");
    expect(composer.post).toHaveBeenCalledWith("again");
    expect(getVoiceState()).toMatchObject({ error: null, canRetry: false });
  });

  it("VOM-FR-WICV: a failed Retry can run again", async () => {
    api.finishVoiceCapture.mockImplementation(async () => {
      throw "provider_rejected";
    });
    api.retryVoiceCapture.mockImplementation(async () => {
      throw "provider_rejected";
    });
    registerVoiceComposer(KEY, handle());
    await record();
    await retryRecording();
    expect(getVoiceState().canRetry).toBe(true);
    await retryRecording();
    expect(api.retryVoiceCapture).toHaveBeenCalledTimes(2);
  });

  it("VOM-FR-FASJ: the next press discards the held recording", async () => {
    api.finishVoiceCapture.mockImplementationOnce(async () => {
      throw "transcription_failed";
    });
    registerVoiceComposer(KEY, handle());
    await record();
    pressRecord();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(getVoiceState().error).toBeNull();
    await settle();
  });

  it("VOM-FR-FASJ: leaving voice mode discards the held recording", async () => {
    api.finishVoiceCapture.mockImplementationOnce(async () => {
      throw "transcription_failed";
    });
    registerVoiceComposer(KEY, handle());
    await record();
    leaveVoiceMode();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
  });
});

describe("spoken replies", () => {
  const old = comment("c1", "agent", "Old reply.");

  beforeEach(() => {
    enterVoiceMode(KEY, discussion([old]));
    registerVoiceComposer(KEY, handle());
    setResponseMode("voice");
  });

  it("VOM-FR-ZLAY, VOM-FR-UALP: speaks a new agent comment with its agent's voice", async () => {
    noteDiscussionChanged(discussion([old, comment("c2", "agent", "New reply.")]));
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      "New reply.",
      "cedar",
      expect.any(Function),
    );
    expect(getVoiceState().playing).toBe(true);
  });

  it("VOM-FR-UALP: an agent with no voice uses the provider default", async () => {
    resetVoiceControllerForTest();
    enterVoiceMode(KEY, discussion([old]));
    registerVoiceComposer(KEY, handle({ voiceOf: () => null }));
    setResponseMode("voice");
    noteDiscussionChanged(discussion([old, comment("c2", "agent")]));
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      "A reply.",
      null,
      expect.any(Function),
    );
  });

  it("VOM-FR-ILJW: a comment from before voice mode is never spoken", async () => {
    noteDiscussionChanged(discussion([old]));
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-ILJW: a comment that came in Text mode is not spoken later", async () => {
    setResponseMode("text");
    const seen = comment("c2", "agent");
    noteDiscussionChanged(discussion([old, seen]));
    setResponseMode("voice");
    noteDiscussionChanged(discussion([old, seen]));
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-ILJW: an opening composer takes its baseline from the new discussion", async () => {
    leaveVoiceMode();
    enterVoiceMode("target-key", null);
    registerVoiceComposer("d2", handle());
    setResponseMode("voice");
    const opened = discussion([comment("c9", "human")], "d2");
    bindVoiceToDiscussion("target-key", opened);
    expect(getVoiceState().activeKey).toBe("d2");
    noteDiscussionChanged(opened);
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
    noteDiscussionChanged(discussion([...opened.comments, comment("c10", "agent")], "d2"));
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-VSCQ: a person's comment and a question comment are not spoken", async () => {
    noteDiscussionChanged(
      discussion([old, comment("c2", "human"), comment("set-1:01-1q", "agent", "Which one?")]),
    );
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-TRWN: a comment with no speakable text is not spoken", async () => {
    noteDiscussionChanged(discussion([old, comment("c2", "agent", "   ")]));
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
    expect(getVoiceState().playing).toBe(false);
  });

  it("VOM-FR-RKOG: a new comment stops the old speech, and it is not queued", async () => {
    const c2 = comment("c2", "agent", "First.");
    noteDiscussionChanged(discussion([old, c2]));
    await settle();
    audio.stopPlayback.mockClear();
    noteDiscussionChanged(discussion([old, c2, comment("c3", "agent", "Second.")]));
    await settle();
    expect(audio.stopPlayback).toHaveBeenCalled();
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    expect(api.speakVoiceSegment).toHaveBeenLastCalledWith(
      expect.any(String),
      "Second.",
      "cedar",
      expect.any(Function),
    );
  });

  it("VOM-FR-RKOG: of two new comments in one event, the newest is spoken", async () => {
    noteDiscussionChanged(
      discussion([old, comment("c2", "agent", "One."), comment("c3", "agent", "Two.")]),
    );
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      "Two.",
      "cedar",
      expect.any(Function),
    );
  });

  it("VOM-FR-QDWW: Stop cancels the speech, and later segments do not start", async () => {
    let finishFirst: () => void = () => undefined;
    api.speakVoiceSegment.mockImplementation(
      () => new Promise<undefined>((resolve) => (finishFirst = () => resolve(undefined))),
    );
    const long = Array.from({ length: 30 }, (_, i) => `Sentence number ${i} here.`).join(" ");
    noteDiscussionChanged(discussion([old, comment("c2", "agent", long)]));
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
    stopSpeech();
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    expect(getVoiceState().playing).toBe(false);
    finishFirst();
    await settle();
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-QPFQ: a failed segment shows an error, with no Retry", async () => {
    api.speakVoiceSegment.mockImplementation(async () => {
      throw "speech_failed";
    });
    noteDiscussionChanged(discussion([old, comment("c2", "agent")]));
    await settle();
    expect(getVoiceState()).toMatchObject({
      error: "speech_failed",
      canRetry: false,
      playing: false,
    });
    expect(audio.stopPlayback).toHaveBeenCalled();
  });

  it("VOM-FR-EGNX: a composer that goes stops playback, and a new comment is not spoken", async () => {
    resetVoiceControllerForTest();
    enterVoiceMode(KEY, discussion([old]));
    const unregister = registerVoiceComposer(KEY, handle());
    setResponseMode("voice");
    const c2 = comment("c2", "agent");
    noteDiscussionChanged(discussion([old, c2]));
    await settle();
    audio.stopPlayback.mockClear();
    unregister();
    expect(audio.stopPlayback).toHaveBeenCalled();
    api.speakVoiceSegment.mockClear();
    noteDiscussionChanged(discussion([old, c2, comment("c3", "agent")]));
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
  });

  it("VOM-FR-KQAI: the teardown ends voice mode, and keeps the provider state", async () => {
    pressRecord();
    await settle();
    endVoiceForTeardown();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(getVoiceState()).toMatchObject({ activeKey: null, providerReady: true });
  });
});

describe("the values of voice mode", () => {
  it("VOM-FR-DGOG, VOM-FR-FTFU, VOM-FR-WSXW: the limits have their stated values", () => {
    expect(SHORT_PRESS_MS).toBe(300);
    expect(TAIL_MS).toBe(200);
    expect(RECORDING_LIMIT_MS).toBe(600_000);
  });

  it("VOM-FR-ZFSA: a rejection reaches a log only as a code", () => {
    expect(errorCode("provider_rejected")).toBe("provider_rejected");
    expect(errorCode("tls_untrusted:unknown_issuer:host")).toBe("tls_untrusted:unknown_issuer:host");
    expect(errorCode("Bearer sk-secret was refused")).toBe("unknown");
    expect(errorCode(new Error("the words of a transcript"))).toBe("unknown");
    // A shape that makes a nested pattern backtrack without end.
    expect(errorCode(`_:${"!:".repeat(5000)} `)).toBe("unknown");
    expect(errorCode(`_:${"!:".repeat(5000)}`)).toBe(`_:${"!:".repeat(5000)}`);
  });
});

describe("the end of voice mode while text is on its way", () => {
  let finish: (text: string) => void = () => undefined;
  let refuse: (code: string) => void = () => undefined;

  beforeEach(() => {
    api.finishVoiceCapture.mockImplementation(
      () =>
        new Promise<string>((resolve, reject) => {
          finish = resolve;
          refuse = reject;
        }),
    );
    enterVoiceMode(KEY, discussion([]));
  });

  async function untilTranscribing() {
    await record();
    expect(getVoiceState().phase).toBe("transcribing");
  }

  it("VOM-FR-JBJI, VOM-FR-ABPU: leaving during transcription keeps the text and posts nothing", async () => {
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await untilTranscribing();
    leaveVoiceMode();
    finish("hello");
    await settle();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("hello");
  });

  it("VOM-FR-OSTM, VOM-FR-ABPU: a second composer takes voice mode, and the first keeps its text", async () => {
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await untilTranscribing();
    enterVoiceMode("other", null);
    finish("hello");
    await settle();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("hello");
    expect(getVoiceState().activeKey).toBe("other");
  });

  it("VOM-FR-ABPU: a composer that unmounts during the tail keeps the text", async () => {
    const composer = handle();
    const unregister = registerVoiceComposer(KEY, composer);
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS - 1);
    unregister();
    await vi.advanceTimersByTimeAsync(1);
    await settle();
    finish("hello");
    await settle();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("hello");
  });

  it("VOM-FR-FASJ: a finish that fails after a leave holds nothing for Retry", async () => {
    registerVoiceComposer(KEY, handle());
    await untilTranscribing();
    leaveVoiceMode();
    refuse("provider_unreachable");
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(getVoiceState()).toMatchObject({ error: null, canRetry: false, phase: "idle" });
    await retryRecording();
    expect(api.retryVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-WICV, VOM-FR-ABPU: a Retry that lands after a leave keeps the text", async () => {
    let answer: (text: string) => void = () => undefined;
    api.retryVoiceCapture.mockImplementation(() => new Promise((resolve) => (answer = resolve)));
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    await untilTranscribing();
    refuse("provider_rejected");
    await settle();
    const retrying = retryRecording();
    await settle();
    leaveVoiceMode();
    answer("again");
    await retrying;
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("again");
  });

  it("VOM-FR-WICV: a Retry with the composer gone keeps the text", async () => {
    const composer = handle();
    const unregister = registerVoiceComposer(KEY, composer);
    await untilTranscribing();
    refuse("provider_unreachable");
    await settle();
    unregister();
    await retryRecording();
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("again");
  });

  it("VOM-FR-DGOG: a release after 300 ms, before the microphone opens, still posts", async () => {
    let open: (m: { rate: number; stop: () => void }) => void = () => undefined;
    mic.open.mockImplementation(() => new Promise((resolve) => (open = resolve)));
    const composer = handle();
    registerVoiceComposer(KEY, composer);
    pressRecord();
    await vi.advanceTimersByTimeAsync(SHORT_PRESS_MS);
    releaseRecord();
    open({ rate: 48_000, stop: mic.stop });
    await settle();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    finish("hello");
    await settle();
    expect(composer.post).toHaveBeenCalledWith("hello");
  });

  it("VOM-FR-EGNX: one of two surfaces going changes nothing", async () => {
    registerVoiceComposer(KEY, handle());
    const second = registerVoiceComposer(KEY, handle());
    audio.stopPlayback.mockClear();
    second();
    expect(voiceComposerMounted()).toBe(true);
    expect(audio.stopPlayback).not.toHaveBeenCalled();
  });

  it("VOM-FR-HUXY: a late delta of a discarded recording does not show", async () => {
    registerVoiceComposer(KEY, handle());
    await record(SHORT_PRESS_MS - 1);
    noteTranscript("s1", "late");
    expect(getVoiceState().transcript).toBe("");
  });
});

describe("the teardown", () => {
  beforeEach(() => {
    enterVoiceMode(KEY, discussion([]));
    registerVoiceComposer(KEY, handle());
  });

  it("VOM-FR-KQAI: during opening, no session starts", async () => {
    let open: (m: { rate: number; stop: () => void }) => void = () => undefined;
    mic.open.mockImplementation(() => new Promise((resolve) => (open = resolve)));
    pressRecord();
    endVoiceForTeardown();
    open({ rate: 48_000, stop: mic.stop });
    await settle();
    expect(api.startVoiceCapture).not.toHaveBeenCalled();
    expect(mic.stop).toHaveBeenCalled();
    expect(getVoiceState().activeKey).toBeNull();
  });

  it("VOM-FR-KQAI: during the tail, the recording is discarded", async () => {
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    endVoiceForTeardown();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
  });

  it("VOM-FR-KQAI: during transcription, nothing reaches the cleared store", async () => {
    let finish: (text: string) => void = () => undefined;
    api.finishVoiceCapture.mockImplementation(() => new Promise((r) => (finish = r)));
    await record();
    endVoiceForTeardown();
    clearAllDiscussionSessions();
    finish("hello");
    await settle();
    expect(getDiscussionSession(KEY).body).toBe("");
    expect(getVoiceState()).toMatchObject({ activeKey: null, note: null, phase: "idle" });
  });
});

describe("playback edges", () => {
  const old = comment("c1", "agent", "Old.");

  beforeEach(() => {
    enterVoiceMode(KEY, discussion([old]));
    registerVoiceComposer(KEY, handle());
    setResponseMode("voice");
  });

  it("VOM-FR-QDWW: a frame that arrives after Stop does not play", async () => {
    let frame: (bytes: ArrayBuffer) => void = () => undefined;
    let done: () => void = () => undefined;
    const speakWithFrames = (
      _id: string,
      _text: string,
      _voice: string | null,
      onFrame: (b: ArrayBuffer) => void,
    ) =>
      new Promise<undefined>((resolve) => {
        frame = onFrame;
        done = () => resolve(undefined);
      });
    api.speakVoiceSegment.mockImplementation(speakWithFrames as never);
    noteDiscussionChanged(discussion([old, comment("c2", "agent")]));
    await settle();
    frame(new ArrayBuffer(8));
    expect(audio.playFrame).toHaveBeenCalledTimes(1);
    stopSpeech();
    frame(new ArrayBuffer(8));
    expect(audio.playFrame).toHaveBeenCalledTimes(1);
    done();
    await settle();
  });

  it("VOM-FR-QDWW: a reply that gives no frames does not leave Stop on screen", async () => {
    audio.isPlaybackIdle.mockReturnValue(true);
    noteDiscussionChanged(discussion([old, comment("c2", "agent")]));
    await settle();
    expect(getVoiceState().playing).toBe(false);
  });

  it("VOM-FR-QDWW: the end of the audio takes Stop away", async () => {
    noteDiscussionChanged(discussion([old, comment("c2", "agent")]));
    await settle();
    expect(getVoiceState().playing).toBe(true);
    const calls = audio.setDrainedListener.mock.calls as unknown as [() => void][];
    const drained = calls[calls.length - 1][0];
    drained();
    expect(getVoiceState().playing).toBe(false);
  });

  it("VOM-FR-QPFQ: a failure that lands after Stop shows nothing", async () => {
    let fail: (code: string) => void = () => undefined;
    api.speakVoiceSegment.mockImplementation(
      () => new Promise<undefined>((_, reject) => (fail = reject)),
    );
    noteDiscussionChanged(discussion([old, comment("c2", "agent")]));
    await settle();
    stopSpeech();
    fail("speech_failed");
    await settle();
    expect(getVoiceState().error).toBeNull();
  });
});
