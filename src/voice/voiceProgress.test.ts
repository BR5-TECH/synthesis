/**
 * The progress updates of a long turn (`VOM-voice-mode.md` VOM-FR-JTWB,
 * VOM-FR-NKCG, VOM-FR-ZAHM, VOM-FR-RUDX, VOM-FR-LQYS, VOM-FR-BGXE), on the VAD
 * bench. The timers are fake, and no test waits on real time.
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

import { noteGroupDispatched } from "../state/groupSpeech";
import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import type { AgentGroup, AgentTurn, GroupRole } from "../types";
import {
  KEY,
  change,
  comment,
  confirmed,
  deferred,
  discussion,
  handle,
  setGroupSwitch,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import {
  TAIL_MS,
  enterVoiceMode,
  leaveVoiceMode,
  noteActivity,
  noteAgentTurn,
  noteDiscussionChanged,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  setInputMode,
  setResponseMode,
  stopSpeech,
  stopVoiceMode,
} from "./voiceController";
import { FILLER_DELAY_MS, cancelFiller } from "./voiceFiller";
import { FILLER_PHRASES } from "./voiceFillerPhrases";
import { PROGRESS_INTERVAL_MS } from "./voiceProgress";

useVadBench();

const UPDATE = "Checking existing specifications.";
const ALL_PHRASES = new Set(Object.values(FILLER_PHRASES).flat());

interface TurnOver {
  id?: string;
  agentId?: string;
  nickname?: string;
  role?: GroupRole | null;
  tools?: Array<{ tool: string; advisor?: string }>;
  state?: AgentTurn["state"];
  language?: string;
}

/** A turn of the voice discussion. */
function turn({
  id = "t1",
  agentId = "a1",
  nickname = "arch",
  role = null,
  tools = [],
  state = "running",
  language = "English",
}: TurnOver = {}): AgentTurn {
  return {
    id,
    agentId,
    nickname,
    origin: { discussionId: KEY, target: { kind: "artifact", artifactId: "a.md" } },
    triggerCommentId: "c-1",
    state,
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: state === "running" ? null : "2026-02-01T00:00:05Z",
    activeToolCalls: tools.map((t, i) => ({ id: `call-${i}`, activationSeq: i + 1, ...t })),
    toolCalls: [],
    spokenLanguage: language,
    imagesOmitted: false,
    group: role === null ? null : { groupId: "g1", role },
  } as AgentTurn;
}

function spoken(): string[] {
  return (api.speakVoiceSegment.mock.calls as unknown as unknown[][]).map((c) => c[1] as string);
}

function updates(): string[] {
  return spoken().filter((text) => !ALL_PHRASES.has(text));
}

/**
 * An utterance in response mode Voice that ends and posts. The filler of the
 * utterance is cancelled, so the speech of a test is the updates alone, except
 * where `keepFiller` is set.
 */
async function spokeAndPosted(keepFiller = false) {
  await confirmed(handle({ voiceOf: (agentId: string) => agentId }));
  setResponseMode("voice");
  noteActivity(change("speech_end", { captureId: "c1" }));
  await settle();
  if (!keepFiller) cancelFiller("test");
}

/** Playback ends at once, so an update that spoke does not hold the next one. */
function playbackEndsAtOnce() {
  audio.isPlaybackIdle.mockReturnValue(true);
}

async function tick(ms: number) {
  await vi.advanceTimersByTimeAsync(ms);
}

describe("when an update is due", () => {
  it("VOM-FR-JTWB, VOM-FR-NKCG: the first update speaks 30 s after the end, and each later one 30 s after", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn({ tools: [{ tool: "search_specifications" }] }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS - 1);
    expect(updates()).toEqual([]);
    await tick(1);
    expect(updates()).toEqual([UPDATE]);
    await tick(PROGRESS_INTERVAL_MS - 1);
    expect(updates()).toHaveLength(1);
    await tick(1);
    expect(updates()).toHaveLength(2);
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toHaveLength(3);
  });

  it("VOM-FR-NKCG: the call gets the active tool calls, the advisor names, and the spoken language", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(
      turn({
        language: "German",
        tools: [{ tool: "search_specifications" }, { tool: "search_drafts", advisor: "sec" }],
      }),
    );
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).toHaveBeenCalledWith(
      [{ tool: "search_specifications" }, { tool: "search_drafts", advisor: "sec" }],
      "German",
    );
  });

  it("VOM-FR-JTWB: the interval does not depend on the agent group switch or on a change of activity", async () => {
    await setGroupSwitch(true);
    playbackEndsAtOnce();
    noteAgentTurn(turn({ tools: [{ tool: "read_file" }] }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toHaveLength(1);
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toHaveLength(2);
    expect(api.writeVoiceProgressUpdate).toHaveBeenCalledTimes(2);
  });

  it("VOM-FR-JTWB: an update starts after the filler and does not replace it", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn());
    await spokeAndPosted(true);
    await tick(FILLER_DELAY_MS);
    expect(spoken()).toHaveLength(1);
    expect(ALL_PHRASES.has(spoken()[0])).toBe(true);
    await tick(PROGRESS_INTERVAL_MS - FILLER_DELAY_MS);
    expect(spoken()).toHaveLength(2);
    expect(spoken()[1]).toBe(UPDATE);
  });

  it("VOM-FR-JTWB: response mode Text arms no stream", async () => {
    noteAgentTurn(turn());
    await confirmed();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-JTWB, VOM-FR-LKXN: an utterance that posted nothing gets no update", async () => {
    api.finishVoiceCapture.mockResolvedValueOnce("");
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });
});

describe("when an update waits or is dropped", () => {
  it("VOM-FR-RUDX: a due update waits until the utterance settles", async () => {
    playbackEndsAtOnce();
    const finish = deferred<string>();
    api.finishVoiceCapture.mockReturnValueOnce(finish.promise);
    noteAgentTurn(turn());
    await confirmed();
    setResponseMode("voice");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
    // A reply with nothing to say cancels the filler, so only the update is left.
    noteDiscussionChanged(discussion([comment("r0", "agent", "")]));
    finish.resolve("hello");
    await settle();
    expect(updates()).toEqual([UPDATE]);
  });

  it("VOM-FR-RUDX: a due update does not speak while a reply plays, and it is not queued", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn());
    await spokeAndPosted();
    patchVoiceState({ playing: true });
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
    patchVoiceState({ playing: false });
    await tick(1);
    expect(updates()).toEqual([]);
    // The next one is due 30 s after the dropped one.
    await tick(PROGRESS_INTERVAL_MS - 1);
    expect(updates()).toEqual([UPDATE]);
  });

  it("VOM-FR-RUDX: a due update does not speak while a reply is paused", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    patchVoiceState({ playbackPaused: true });
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-RUDX: the rules apply again when the text arrives", async () => {
    playbackEndsAtOnce();
    const written = deferred<{ text: string }>();
    api.writeVoiceProgressUpdate.mockReturnValueOnce(written.promise);
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).toHaveBeenCalledTimes(1);
    patchVoiceState({ playing: true });
    written.resolve({ text: UPDATE });
    await settle();
    expect(updates()).toEqual([]);
  });

  it("VOM-FR-NKCG: a failed call speaks nothing and shows no error, and the next update is due as before", async () => {
    playbackEndsAtOnce();
    api.writeVoiceProgressUpdate.mockRejectedValueOnce("progress_update_failed");
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toEqual([]);
    expect(getVoiceState().error).toBeNull();
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toEqual([UPDATE]);
  });

  it("VOM-FR-NKCG: an empty text speaks nothing", async () => {
    api.writeVoiceProgressUpdate.mockResolvedValueOnce({ text: "  " });
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).toHaveBeenCalledTimes(1);
    expect(updates()).toEqual([]);
    expect(getVoiceState().error).toBeNull();
  });
});

describe("what cancels the stream", () => {
  it("VOM-FR-LQYS: the end of the last turn of the utterance cancels it, and so does a cancel", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    noteAgentTurn(turn({ state: "delivered" }));
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: a cancelled last turn cancels the stream", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    noteAgentTurn(turn({ state: "cancelled" }));
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: with no running turn at the due time, nothing speaks and the stream ends", async () => {
    playbackEndsAtOnce();
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    noteAgentTurn(turn());
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: a turn of another discussion does not keep the stream", async () => {
    await spokeAndPosted();
    noteAgentTurn({
      ...turn(),
      origin: { discussionId: "other", target: { kind: "artifact", artifactId: "b.md" } },
    } as AgentTurn);
    await tick(PROGRESS_INTERVAL_MS);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: a new utterance cancels it", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS - 1000);
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c2", live: true }));
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: Stop cancels it", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    stopSpeech();
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: the Stop control of the status bar cancels it", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    stopVoiceMode();
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: response mode Text cancels it", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    setResponseMode("text");
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: the end of voice mode cancels it", async () => {
    noteAgentTurn(turn());
    await spokeAndPosted();
    leaveVoiceMode();
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(api.writeVoiceProgressUpdate).not.toHaveBeenCalled();
  });

  it("VOM-FR-LQYS: a reply of one agent of a group does not cancel it while another turn runs", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant" }));
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec", role: "participant" }));
    await spokeAndPosted();
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant", state: "delivered" }));
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toEqual([UPDATE]);
  });

  it("VOM-FR-LQYS: the stream waits for the synthesis turn after the last participant ended", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant" }));
    await spokeAndPosted();
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant", state: "delivered" }));
    noteAgentTurn(turn({ id: "t3", agentId: "a1", nickname: "arch", role: "synthesis" }));
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toEqual([UPDATE]);
    noteAgentTurn(turn({ id: "t3", agentId: "a1", nickname: "arch", role: "synthesis", state: "delivered" }));
    await tick(PROGRESS_INTERVAL_MS * 2);
    expect(updates()).toHaveLength(1);
  });
});

describe("an update is not a reply", () => {
  it("VOM-FR-BGXE: it shows Speaking while it plays, and Stop stops it", async () => {
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(getVoiceState().playing).toBe(true);
    stopSpeech();
    expect(getVoiceState().playing).toBe(false);
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
    segment.resolve(undefined);
  });

  it("VOM-FR-BGXE, VOM-FR-UCQN: a barge-in drops a playing update, and a misfire does not resume it", async () => {
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(getVoiceState().playing).toBe(true);
    noteActivity(change("speech_start"));
    expect(audio.pausePlayback).not.toHaveBeenCalled();
    expect(getVoiceState().playing).toBe(false);
    expect(getVoiceState().playbackPaused).toBe(false);
    noteActivity(change("misfire"));
    expect(audio.resumePlayback).not.toHaveBeenCalled();
    segment.resolve(undefined);
  });

  it("VOM-FR-BGXE, VOM-FR-RKOG: a reply stops an update that plays", async () => {
    const segment = deferred<undefined>();
    api.speakVoiceSegment.mockReturnValueOnce(segment.promise);
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    const updateId = (api.speakVoiceSegment.mock.calls[0] as unknown as unknown[])[0];
    noteDiscussionChanged(discussion([comment("r1")]));
    await settle();
    expect(api.cancelVoiceSpeech).toHaveBeenCalledWith(updateId);
    expect(spoken()).toEqual([UPDATE, "A reply."]);
    segment.resolve(undefined);
  });

  it("VOM-FR-BGXE: a failed update stops and shows no error", async () => {
    api.speakVoiceSegment.mockRejectedValueOnce({ code: "provider_unreachable" });
    noteAgentTurn(turn());
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(updates()).toEqual([UPDATE]);
    expect(getVoiceState().playing).toBe(false);
    expect(getVoiceState().error).toBeNull();
  });
});

describe("the voice of the update", () => {
  function groupOf(mode: AgentGroup["mode"], driver: string | null): AgentGroup {
    return {
      mode,
      groupId: "g1",
      driver,
      turns: [
        turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant" }),
        turn({ id: "t2", agentId: "a2", nickname: "sec", role: "participant" }),
      ],
      refusals: [],
    };
  }

  function voiceOfFirstUpdate(): unknown {
    return (api.speakVoiceSegment.mock.calls[0] as unknown as unknown[])[2];
  }

  it("VOM-FR-ZAHM: with the switch on and a driver picked, the driver speaks", async () => {
    playbackEndsAtOnce();
    await setGroupSwitch(true);
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant" }));
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec", role: "participant" }));
    noteGroupDispatched(KEY, "u1", groupOf("synthesis", "sec"));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(voiceOfFirstUpdate()).toBe("a2");
  });

  it("VOM-FR-ZAHM: a driver of an earlier group that no turn runs does not speak", async () => {
    playbackEndsAtOnce();
    await setGroupSwitch(true);
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant" }));
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec", role: "participant" }));
    noteGroupDispatched(KEY, "u1", groupOf("synthesis", "sec"));
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", state: "delivered" }));
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec", state: "delivered" }));
    noteAgentTurn(turn({ id: "t4", agentId: "a3", nickname: "ops" }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(voiceOfFirstUpdate()).toBe("a3");
  });

  it("VOM-FR-ZAHM: for one tagged agent, Meta speaks", async () => {
    playbackEndsAtOnce();
    await setGroupSwitch(true);
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch" }));
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec", role: "meta" }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(voiceOfFirstUpdate()).toBe("a2");
  });

  it("VOM-FR-ZAHM: the synthesis turn speaks once it runs", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch", role: "participant" }));
    noteAgentTurn(turn({ id: "t3", agentId: "a2", nickname: "sec", role: "synthesis" }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(voiceOfFirstUpdate()).toBe("a2");
  });

  it("VOM-FR-ZAHM: with no driver, the agent of the oldest running turn speaks", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec" }));
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch" }));
    noteAgentTurn(turn({ id: "t2", agentId: "a2", nickname: "sec", tools: [{ tool: "read_file" }] }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    expect(voiceOfFirstUpdate()).toBe("a2");
  });

  it("VOM-FR-ZAHM: the later updates of an utterance keep the agent of the first", async () => {
    playbackEndsAtOnce();
    noteAgentTurn(turn({ id: "t1", agentId: "a1", nickname: "arch" }));
    await spokeAndPosted();
    await tick(PROGRESS_INTERVAL_MS);
    noteAgentTurn(turn({ id: "t0", agentId: "a0", nickname: "ops", role: "meta" }));
    await tick(PROGRESS_INTERVAL_MS);
    const voices = (api.speakVoiceSegment.mock.calls as unknown as unknown[][]).map((c) => c[2]);
    expect(voices).toEqual(["a1", "a1"]);
  });
});

describe("Push-to-Talk", () => {
  it("VOM-FR-JTWB: the end of a recording starts the stream", async () => {
    playbackEndsAtOnce();
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ providerReady: true, providerSpeaks: true });
    registerVoiceComposer(KEY, handle());
    setInputMode("ptt");
    setResponseMode("voice");
    noteAgentTurn(turn());
    pressRecord();
    await settle();
    await tick(1000);
    releaseRecord();
    await tick(TAIL_MS);
    await settle();
    cancelFiller("test");
    await tick(PROGRESS_INTERVAL_MS - 1);
    expect(updates()).toEqual([]);
    await tick(1);
    expect(updates()).toEqual([UPDATE]);
  });
});
