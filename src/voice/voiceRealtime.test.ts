/**
 * Real-time speech in the controller (`VOM-voice-mode.md` VOM-FR-GBCN to
 * VOM-FR-MVLA): chunks, the live bubble, the speech turn, and the Stop control
 * of the status bar.
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

import { getDiscussionSession } from "../state/discussionSession";
import { getLiveBubble, resetLiveBubbleForTest } from "../state/liveBubble";
import { claimSpeechPost, resetSpeechPostsForTest } from "../state/speechPosts";
import { getVoiceState } from "../state/voiceSession";
import type { VoiceChunkChange } from "../types";
import {
  KEY,
  bench,
  change,
  comment,
  confirmed,
  deferred,
  discussion,
  handle,
  lastGate,
  openSession,
  replyPlays,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import {
  TAIL_MS,
  enterVoiceMode,
  leaveVoiceMode,
  noteActivity,
  noteDiscussionChanged,
  setResponseMode,
  noteChunk,
  noteOwnerDeleted,
  noteTranscript,
  pressRecord,
  registerVoiceComposer,
  setInputMode,
  stopVoiceMode,
} from "./voiceController";
import { registerQuestionBlock } from "./voiceQuestions";

useVadBench();

function chunk(
  state: VoiceChunkChange["state"],
  index: number,
  over: Partial<VoiceChunkChange> = {},
): VoiceChunkChange {
  return { captureId: "c1", index, state, text: null, code: null, ...over };
}

/** A composer whose post appends comment `k9`, as a dispatch path claims it. */
function speaker(over: Parameters<typeof handle>[0] = {}) {
  return handle({
    targets: () => ["arch"],
    post: vi.fn(async (spoken: string) => {
      claimSpeechPost({ id: KEY, comments: [{ id: "k9" }] as never }, spoken);
      return "posted" as const;
    }),
    ...over,
  });
}

function said(index: number, text: string) {
  noteChunk(chunk("chunk_end", index));
  noteChunk(chunk("chunk_transcript", index, { text }));
}

async function speaking(over: Parameters<typeof handle>[0] = {}) {
  resetLiveBubbleForTest();
  resetSpeechPostsForTest();
  await confirmed(speaker(over));
}

describe("chunks and the live bubble", () => {
  it("VOM-FR-GBCN, VOP-FR-ERDS: a VAD utterance in a discussion opens chunked", async () => {
    await openSession();
    expect(lastGate()).toMatchObject({ chunked: true });
  });

  it("VOM-FR-NCHC, CVP-FR-TGPV: the bubble joins the chunk transcripts in order", async () => {
    await speaking();
    said(0, "Check the second paragraph.");
    said(1, "The scope is too wide.");
    expect(getLiveBubble()).toMatchObject({
      discussionId: KEY,
      parts: [
        {
          captureId: "c1",
          chunks: [
            { index: 0, text: "Check the second paragraph." },
            { index: 1, text: "The scope is too wide." },
          ],
        },
      ],
    });
  });

  it("VOM-FR-VOUG, VOM-FR-GBCN: a chunk in transcription shows Transcribing with the microphone open", async () => {
    await speaking();
    noteChunk(chunk("chunk_end", 0));
    expect(getVoiceState()).toMatchObject({ transcribing: 1, vadOpen: true, phase: "recording" });
    noteChunk(chunk("chunk_transcript", 0, { text: "One." }));
    expect(getVoiceState().transcribing).toBe(0);
  });

  it("VOM-FR-HUXY: the live text of the open chunk follows the chunk transcripts", async () => {
    await speaking();
    said(0, "One.");
    noteTranscript("c1", "and the");
    expect(getLiveBubble().parts[0]).toMatchObject({ liveText: "and the" });
    said(1, "And the story.");
    expect(getLiveBubble().parts[0].liveText).toBe("");
  });

  it("VOM-FR-MBHB: a failed chunk shows at its place, and later chunks go on", async () => {
    await speaking();
    said(0, "One.");
    noteChunk(chunk("chunk_end", 1));
    noteChunk(chunk("chunk_failed", 1, { code: "provider_unreachable" }));
    said(2, "Three.");
    expect(getLiveBubble().parts[0].chunks).toEqual([
      { index: 0, text: "One.", failed: null },
      { index: 1, text: null, failed: "provider_unreachable" },
      { index: 2, text: "Three.", failed: null },
    ]);
    await settle();
    expect(api.steerSpeechTurn).toHaveBeenCalledWith("c1", "Three.");
  });

  it("VOM-FR-NZXN: a chunk event of a capture that is not held changes nothing", async () => {
    await speaking();
    noteChunk(chunk("chunk_transcript", 0, { captureId: "c9", text: "Other." }));
    expect(getLiveBubble().parts[0].chunks).toEqual([]);
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
  });

  it("VOM-FR-WYIE, CVP-FR-GHWN: the posted comment replaces the part of the utterance", async () => {
    await speaking();
    said(0, "One.");
    api.finishVoiceCapture.mockResolvedValueOnce("One.");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(bench.composer.post).toHaveBeenCalledWith("One.");
    expect(getLiveBubble().parts).toEqual([]);
  });
});

describe("the speech turn", () => {
  it("VOM-FR-RNBS, CTA-FR-QWLB, CTA-FR-SZOC: the first chunk starts a speech turn for each agent it reaches", async () => {
    await speaking({ tagged: (t) => `@arch ${t}` });
    said(0, "Check this.");
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);
    expect(api.dispatchSpeechTurn).toHaveBeenCalledWith({
      nickname: "arch",
      origin: expect.objectContaining({ discussionId: KEY }),
      speechId: "c1",
      text: "@arch Check this.",
    });
    expect(getDiscussionSession(KEY).turns.map((t) => t.id)).toContain("t-arch");
  });

  it("VOM-FR-OLFT: each later chunk steers the speech, and an empty chunk does nothing", async () => {
    await speaking();
    said(0, "One.");
    said(1, "  ");
    said(2, "Two.");
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);
    expect(api.steerSpeechTurn).toHaveBeenCalledTimes(1);
    expect(api.steerSpeechTurn).toHaveBeenCalledWith("c1", "Two.");
  });

  it("VOM-FR-OLFT: a steer waits for the dispatch, so the calls keep their order", async () => {
    await speaking();
    const dispatched = deferred<never>();
    api.dispatchSpeechTurn.mockReturnValueOnce(dispatched.promise);
    said(0, "One.");
    said(1, "Two.");
    await settle();
    expect(api.steerSpeechTurn).not.toHaveBeenCalled();
    dispatched.reject("agent_unavailable");
    await settle();
    expect(api.steerSpeechTurn).toHaveBeenCalledWith("c1", "Two.");
  });

  it("VOM-FR-MVLA, CTA-FR-SZOC: the posted comment ends the speech with its id", async () => {
    await speaking();
    said(0, "One.");
    api.finishVoiceCapture.mockResolvedValueOnce("One.");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", "k9");
  });

  it("VOM-FR-REJN: an utterance that posts nothing ends the speech with no comment", async () => {
    await speaking();
    said(0, "One.");
    api.finishVoiceCapture.mockResolvedValueOnce("   ");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(bench.composer.post).not.toHaveBeenCalled();
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", null);
    expect(getLiveBubble().parts).toEqual([]);
  });

  it("VOM-FR-REJN, VOM-FR-ZFSA: a failed utterance ends the speech with no comment", async () => {
    await speaking();
    said(0, "One.");
    api.finishVoiceCapture.mockRejectedValueOnce("provider_unreachable");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", null);
    expect(getVoiceState().error).toBe("provider_unreachable");
  });

  it("VOM-FR-RNBS: an utterance with no chunk text calls no speech turn", async () => {
    await speaking();
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
    expect(api.endSpeechTurn).not.toHaveBeenCalled();
    expect(bench.composer.post).toHaveBeenCalledWith("hello");
  });

  it("VOM-FR-PINR, VOP-FR-ERDS: a spoken answer is not chunked and starts no turn", async () => {
    await openSession(speaker());
    registerQuestionBlock(KEY, {
      setId: "q1",
      showing: () => ({ position: 1, text: "Which?", options: [] }),
      answer: vi.fn(),
    });
    noteActivity(change("speech_start"));
    noteActivity(change("speech_confirmed", { captureId: "c1", live: true }));
    expect(lastGate()).toMatchObject({ chunked: false });
    said(0, "The first.");
    expect(getLiveBubble().parts).toEqual([]);
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
  });

  it("VOM-FR-NERG: an opening composer records with no chunks", async () => {
    enterVoiceMode("draft:x", null, { kind: "draft", draftId: "x" });
    registerVoiceComposer("draft:x", speaker());
    pressRecord();
    await settle();
    expect(api.startVoiceCapture).toHaveBeenCalledWith(48_000, ["arch"], false);
  });
});

describe("a refused speech dispatch", () => {
  /** A composer whose post records which agents the dispatch skips. */
  function skipping() {
    const seen: { skipped: ReadonlySet<string> | null } = { skipped: null };
    const composer = speaker({
      post: vi.fn(async (spoken: string) => {
        seen.skipped = claimSpeechPost({ id: KEY, comments: [{ id: "k9" }] as never }, spoken);
        return "posted" as const;
      }),
    });
    return { seen, composer };
  }

  it("VOM-FR-RNBS, CTA-FR-SZOC: the comment reaches an agent whose speech turn was refused", async () => {
    const { seen, composer } = skipping();
    resetLiveBubbleForTest();
    await confirmed(composer);
    api.dispatchSpeechTurn.mockRejectedValueOnce("agent_unavailable");
    said(0, "One.");
    await settle();
    api.finishVoiceCapture.mockResolvedValueOnce("One.");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(composer.post).toHaveBeenCalledWith("One.");
    expect(seen.skipped).toEqual(new Set());
  });

  it("CTA-FR-SZOC: the comment skips an agent whose speech turn was dispatched", async () => {
    const { seen, composer } = skipping();
    await confirmed(composer);
    said(0, "One.");
    await settle();
    api.finishVoiceCapture.mockResolvedValueOnce("One.");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(seen.skipped).toEqual(new Set(["arch"]));
  });
});

describe("the Voice button during a speech", () => {
  it("VOM-FR-JBJI, VOM-FR-REJN, VOM-FR-ABPU: the speech stops at once, late chunks do nothing, and the transcript joins the body", async () => {
    await speaking();
    said(0, "One.");
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);
    api.closeVoiceActivitySession.mockResolvedValueOnce({ captureId: "c1" });
    api.finishVoiceCapture.mockResolvedValueOnce("One. Two.");
    leaveVoiceMode();
    await settle();
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", null);
    expect(getLiveBubble().parts).toEqual([]);
    said(1, "Two.");
    await settle();
    expect(api.steerSpeechTurn).not.toHaveBeenCalled();
    expect(getLiveBubble().parts).toEqual([]);
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    await settle();
    expect(bench.composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("One. Two.");
    expect(api.endSpeechTurn).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-JBJI: a first chunk after the Voice button starts no speech turn", async () => {
    await speaking();
    leaveVoiceMode();
    said(0, "One.");
    await settle();
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
    expect(api.endSpeechTurn).not.toHaveBeenCalled();
  });
});

describe("the held reply", () => {
  it("VOM-FR-DHUU, VOM-FR-ZPOD: a reply that comes while the author speaks plays after the comment posts", async () => {
    await speaking();
    setResponseMode("voice");
    said(0, "One.");
    await settle();
    noteDiscussionChanged(discussion([comment("r1")]));
    await settle();
    expect(api.speakVoiceSegment).not.toHaveBeenCalled();
    api.finishVoiceCapture.mockResolvedValueOnce("One.");
    noteActivity(change("speech_end", { captureId: "c1" }));
    await settle();
    expect(bench.composer.post).toHaveBeenCalledWith("One.");
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", "k9");
    expect(api.speakVoiceSegment).toHaveBeenCalledTimes(1);
    const posted = vi.mocked(bench.composer.post).mock.invocationCallOrder[0];
    const spoken = api.speakVoiceSegment.mock.invocationCallOrder[0];
    expect(posted).toBeLessThan(spoken);
  });
});

describe("the Stop control of the status bar", () => {
  it("VOM-FR-UXZT, STB-FR-JHZF, VOM-FR-REJN: Stop discards the utterance, drops the bubble, and ends voice mode", async () => {
    await speaking();
    said(0, "One.");
    await settle();
    stopVoiceMode();
    await settle();
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
    expect(getLiveBubble().parts).toEqual([]);
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", null);
    expect(bench.composer.post).not.toHaveBeenCalled();
    expect(getVoiceState()).toMatchObject({ activeKey: null, vadOpen: false });
  });

  it("VOM-FR-UXZT: Stop stops playback", async () => {
    await openSession();
    await replyPlays();
    stopVoiceMode();
    expect(getVoiceState().playing).toBe(false);
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
  });

  it("VOM-FR-LSBQ, CVP-FR-51: deleting the owner ends voice mode, and another owner does not", async () => {
    await openSession();
    noteOwnerDeleted({ kind: "draft", draftId: "other" });
    expect(getVoiceState().activeKey).toBe(KEY);
    noteOwnerDeleted({ kind: "artifact", artifactId: "a.md" });
    await settle();
    expect(getVoiceState().activeKey).toBeNull();
    expect(api.closeVoiceActivitySession).toHaveBeenCalledWith("va1", true);
  });

  it("VOM-FR-LSBQ, CVP-FR-51: deleting a folder that holds the owner file ends voice mode", async () => {
    await openSession();
    noteOwnerDeleted({ kind: "artifact", artifactId: "a" });
    expect(getVoiceState().activeKey).toBe(KEY);
    noteOwnerDeleted({ kind: "note", noteId: "a.md" });
    expect(getVoiceState().activeKey).toBe(KEY);
    leaveVoiceMode();
    enterVoiceMode(KEY, discussion([]), { kind: "artifact", artifactId: "specs/ui/a.md" });
    noteOwnerDeleted({ kind: "artifact", artifactId: "specs" });
    expect(getVoiceState().activeKey).toBeNull();
  });

  it("VOM-FR-LSBQ, CVP-FR-51: deleting the draft that owns the discussion ends voice mode and discards the session", async () => {
    await openSession();
    leaveVoiceMode();
    await settle();
    enterVoiceMode(KEY, discussion([]), { kind: "draft", draftId: "x" });
    setInputMode("vad");
    pressRecord();
    await settle();
    expect(getVoiceState().vadOpen).toBe(true);
    noteOwnerDeleted({ kind: "draft", draftId: "y" });
    expect(getVoiceState().activeKey).toBe(KEY);
    noteOwnerDeleted({ kind: "draft", draftId: "x" });
    await settle();
    expect(getVoiceState().activeKey).toBeNull();
    expect(api.closeVoiceActivitySession).toHaveBeenLastCalledWith(expect.any(String), true);
  });

  it("VOM-FR-LSBQ, CVP-FR-61: deleting the note that owns the discussion ends voice mode", async () => {
    enterVoiceMode(KEY, null, { kind: "note", noteId: "n1" });
    noteOwnerDeleted({ kind: "note", noteId: "n2" });
    expect(getVoiceState().activeKey).toBe(KEY);
    noteOwnerDeleted({ kind: "note", noteId: "n1" });
    expect(getVoiceState().activeKey).toBeNull();
  });

  it("VOM-FR-BCRD: Stop works with no composer mounted", async () => {
    await openSession();
    bench.unregister();
    stopVoiceMode();
    await settle();
    expect(getVoiceState().activeKey).toBeNull();
    expect(discussion([]).id).toBe(KEY);
  });
});
