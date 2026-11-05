/**
 * The filler of a long turn (`VOM-voice-mode.md` "Long turns"), on the VAD
 * bench. Turn events come straight to the controller, as the shell gives them.
 */
import { describe, expect, it, vi } from "vitest";

const { api, audio, mic } = await vi.hoisted(() => import("../test/voiceVadMocks"));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => audio);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import type { AgentTurn, TurnToolCall } from "../types";
import {
  KEY,
  change,
  comment,
  confirmed,
  deferred,
  discussion,
  handle,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import {
  TAIL_MS,
  enterVoiceMode,
  noteActivity,
  noteAgentTurn,
  noteDiscussionChanged,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  setInputMode,
  setResponseMode,
  stopSpeech,
} from "./voiceController";
import { FILLER_DELAY_MS } from "./voiceFiller";
import { FILLER_PHRASES, fillerPhrase } from "./voiceFillerPhrases";

useVadBench();

/** A running turn of the voice discussion with `calls` tool calls. */
function turn(calls: number, over: Partial<AgentTurn> = {}): AgentTurn {
  return {
    id: "t1",
    agentId: "a1",
    nickname: "arch",
    origin: { discussionId: KEY, target: { kind: "artifact", artifactId: "a.md" } },
    triggerCommentId: "c-1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: null,
    activeToolCalls: [],
    toolCalls: Array.from({ length: calls }, (_, i): TurnToolCall => ({
      id: `call-${i + 1}`,
      tool: "read_file",
      activationSeq: i + 1,
      state: "done",
      keyInput: null,
    })),
    spokenLanguage: "English",
    ...over,
  } as AgentTurn;
}

const ALL_PHRASES = new Set(Object.values(FILLER_PHRASES).flat());

/** The texts of the segments that went to speech, in order. */
function spoken(): string[] {
  return (api.speakVoiceSegment.mock.calls as unknown as unknown[][]).map((c) => c[1] as string);
}

function fillers(): string[] {
  return spoken().filter((text) => ALL_PHRASES.has(text));
}

/** An utterance in response mode Voice that ends and posts. */
async function spokeAndPosted() {
  await confirmed();
  setResponseMode("voice");
  noteActivity(change("speech_end", { captureId: "c1" }));
  await settle();
}

describe("when the filler is due", () => {
  it("VOM-FR-VTNW, VOM-FR-YGAR, VOM-FR-UFBT: the first tool call after the end speaks one filler", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    expect(fillers()).toEqual([]);
    noteAgentTurn(turn(1));
    expect(fillers()).toHaveLength(1);
    expect(FILLER_PHRASES.english).toContain(fillers()[0]);
    const call = api.speakVoiceSegment.mock.calls[0] as unknown as unknown[];
    expect(call[2]).toBe("cedar");
    expect(getVoiceState().playing).toBe(true);
    noteAgentTurn(turn(2));
    expect(fillers()).toHaveLength(1);
  });

  it("VOM-FR-VTNW: a tool call that started before the end does not count", async () => {
    noteAgentTurn(turn(1));
    await spokeAndPosted();
    noteAgentTurn(turn(1));
    expect(fillers()).toEqual([]);
    noteAgentTurn(turn(2));
    expect(fillers()).toHaveLength(1);
  });

  it("VOM-FR-VTNW: 10 s after the end with no reply, a filler speaks", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS - 1);
    expect(fillers()).toEqual([]);
    await vi.advanceTimersByTimeAsync(1);
    expect(fillers()).toHaveLength(1);
  });

  it("VOM-FR-VTNW: with no running turn, nothing speaks", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteAgentTurn(turn(0, { state: "delivered", endedAt: "2026-02-01T00:00:05Z" }));
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS);
    expect(fillers()).toEqual([]);
  });

  it("VOM-FR-VTNW: a turn of another discussion does not make it due", async () => {
    await spokeAndPosted();
    noteAgentTurn(turn(1, { origin: { discussionId: "other", target: { kind: "artifact", artifactId: "b.md" } } }));
    expect(fillers()).toEqual([]);
  });
});

describe("when a due filler waits", () => {
  it("VOM-FR-LKXN: a due filler waits until the utterance settles", async () => {
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    noteAgentTurn(turn(0));
    await confirmed();
    setResponseMode("voice");
    noteActivity(change("speech_end", { captureId: "c1" }));
    noteAgentTurn(turn(1));
    expect(fillers()).toEqual([]);
    finish.resolve("hello");
    await settle();
    expect(fillers()).toHaveLength(1);
  });

  it("VOM-FR-LKXN: an utterance that posted nothing speaks no filler", async () => {
    api.finishVoiceCapture.mockResolvedValueOnce("");
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteAgentTurn(turn(1));
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS);
    expect(fillers()).toEqual([]);
  });

  it("VOM-FR-LKXN: a due filler waits while the author speaks, and speaks after a misfire", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteActivity(change("speech_start"));
    noteAgentTurn(turn(1));
    expect(fillers()).toEqual([]);
    noteActivity(change("misfire"));
    expect(fillers()).toHaveLength(1);
  });

  it("VOM-FR-LKXN: a due filler does not speak while a reply plays", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    patchVoiceState({ playing: true });
    noteAgentTurn(turn(1));
    expect(fillers()).toEqual([]);
  });
});

describe("what cancels or stops the filler", () => {
  it("VOM-FR-POBZ: a reply before the tool call cancels the filler", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteDiscussionChanged(discussion([comment("r1", "agent", "")]));
    noteAgentTurn(turn(1));
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS);
    expect(fillers()).toEqual([]);
  });

  it("VOM-FR-POBZ, VOM-FR-RKOG: a reply stops a filler that plays", async () => {
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteAgentTurn(turn(1));
    const fillerId = (api.speakVoiceSegment.mock.calls[0] as unknown as unknown[])[0];
    noteDiscussionChanged(discussion([comment("r1")]));
    await settle();
    expect(api.cancelVoiceSpeech).toHaveBeenCalledWith(fillerId);
    expect(spoken()).toEqual([fillers()[0], "A reply."]);
    segment.resolve(undefined);
  });

  it("VOM-FR-POBZ: a new utterance cancels a filler that did not speak", async () => {
    api.finishVoiceCapture.mockResolvedValueOnce("hello").mockResolvedValueOnce("");
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c2", live: true }));
    noteAgentTurn(turn(1));
    noteActivity(change("speech_end", { captureId: "c2" }));
    await settle();
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS);
    expect(fillers()).toEqual([]);
  });

  it("VOM-FR-QDWW, VOM-FR-UFBT: Stop stops a filler", async () => {
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteAgentTurn(turn(1));
    expect(getVoiceState().playing).toBe(true);
    stopSpeech();
    expect(getVoiceState().playing).toBe(false);
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    segment.resolve(undefined);
  });

  it("VOM-FR-POBZ, VOM-FR-EWOK: response mode Text speaks no filler", async () => {
    noteAgentTurn(turn(0));
    await confirmed();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    noteAgentTurn(turn(1));
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS);
    expect(fillers()).toEqual([]);
  });

  it("VOM-FR-POBZ: a switch to response mode Text cancels the filler", async () => {
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    setResponseMode("text");
    noteAgentTurn(turn(1));
    await vi.advanceTimersByTimeAsync(FILLER_DELAY_MS);
    expect(fillers()).toEqual([]);
  });
});

describe("a filler is not a reply", () => {
  it("VOM-FR-UCQN: a barge-in drops the filler, and a misfire does not resume it", async () => {
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteAgentTurn(turn(1));
    noteActivity(change("speech_start"));
    expect(audio.pausePlayback).not.toHaveBeenCalled();
    expect(getVoiceState().playing).toBe(false);
    expect(getVoiceState().playbackPaused).toBe(false);
    noteActivity(change("misfire"));
    expect(audio.resumePlayback).not.toHaveBeenCalled();
    segment.resolve(undefined);
  });

  it("VOM-FR-UCQN: a failed filler stops and shows no error", async () => {
    api.speakVoiceSegment.mockRejectedValueOnce({ code: "provider_unreachable" });
    noteAgentTurn(turn(0));
    await spokeAndPosted();
    noteAgentTurn(turn(1));
    await settle();
    expect(fillers()).toHaveLength(1);
    expect(getVoiceState().playing).toBe(false);
    expect(getVoiceState().error).toBeNull();
  });
});

describe("the voice", () => {
  it("VOM-FR-UFBT, VOM-FR-UALP: the filler uses the voice and the options of the agent of its turn", async () => {
    const options = { speedPercent: 120, style: "calm" };
    const composer = handle({
      voiceOf: (agentId: string) => (agentId === "a2" ? "marin" : "cedar"),
      voiceOptionsOf: (agentId: string) => (agentId === "a2" ? options : null),
    });
    noteAgentTurn(turn(0, { id: "t2", agentId: "a2" }));
    await confirmed(composer);
    setResponseMode("voice");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    noteAgentTurn(turn(1, { id: "t2", agentId: "a2" }));
    const call = api.speakVoiceSegment.mock.calls[0] as unknown as unknown[];
    expect(FILLER_PHRASES.english).toContain(call[1]);
    expect(call[2]).toBe("marin");
    expect(call[4]).toEqual(options);
  });
});

describe("the phrase", () => {
  it("VOM-FR-VQGI: German speaks the German set, and another language speaks English", async () => {
    noteAgentTurn(turn(0, { spokenLanguage: "German" }));
    await spokeAndPosted();
    noteAgentTurn(turn(1, { spokenLanguage: "German" }));
    expect(FILLER_PHRASES.german).toContain(fillers()[0]);

    expect(fillerPhrase("Esperanto", () => 0)).toBe(FILLER_PHRASES.english[0]);
    expect(fillerPhrase(null, () => 0.99)).toBe(FILLER_PHRASES.english[2]);
    expect(fillerPhrase("japanese", () => 0.5)).toBe(FILLER_PHRASES.japanese[1]);
  });

  it("VOM-FR-VQGI: each language of the table has a set", () => {
    const table = ["Spanish", "French", "German", "Italian", "Portuguese", "Russian", "Ukrainian"];
    const more = ["Polish", "Dutch", "Turkish", "Arabic", "Hindi", "Chinese", "Japanese", "Korean"];
    for (const name of [...table, ...more]) {
      expect(FILLER_PHRASES[name.toLowerCase()]?.length, name).toBeGreaterThan(0);
    }
    expect(FILLER_PHRASES.english).toEqual([
      "Let me think about this.",
      "One moment, I will check.",
      "Let me look into this.",
    ]);
  });
});

describe("Push-to-Talk", () => {
  it("VOM-FR-YGAR: the end of a recording arms the filler", async () => {
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ providerReady: true, providerSpeaks: true });
    registerVoiceComposer(KEY, handle());
    setInputMode("ptt");
    setResponseMode("voice");
    noteAgentTurn(turn(0));
    pressRecord();
    await settle();
    await vi.advanceTimersByTimeAsync(1000);
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    noteAgentTurn(turn(1));
    expect(fillers()).toHaveLength(1);
  });
});
