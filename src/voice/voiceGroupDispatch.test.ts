/**
 * The dispatch of the chunks of an utterance with the agent group switch on and
 * off (`VOM-voice-mode.md` VOM-FR-SWHT, VOM-FR-ETCK, VOM-FR-RNBS, VOM-FR-OLFT).
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

import { resetLiveBubbleForTest } from "../state/liveBubble";
import { claimSpeechPost, resetSpeechPostsForTest } from "../state/speechPosts";
import type { VoiceChunkChange } from "../types";
import {
  KEY,
  change,
  confirmed,
  handle,
  setGroupSwitch,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import { noteActivity, noteChunk } from "./voiceController";

useVadBench();

function chunk(
  state: VoiceChunkChange["state"],
  index: number,
  over: Partial<VoiceChunkChange> = {},
): VoiceChunkChange {
  return { captureId: "c1", index, state, text: null, code: null, ...over };
}

function said(index: number, text: string) {
  noteChunk(chunk("chunk_end", index));
  noteChunk(chunk("chunk_transcript", index, { text }));
}

/** The agents that a typed comment of this text reaches: the tags in it. */
function byTags(text: string): string[] {
  const names = ["arch", "sec", "ops"];
  const tagged = names.filter((n) => text.includes(`@${n}`));
  return text.includes("@all") ? names : tagged.length > 0 ? tagged : ["arch"];
}

/** A composer whose post claims the speech post, as a dispatch path does. */
function speaker() {
  const seen: { reached: ReadonlySet<string> | null } = { reached: null };
  const composer = handle({
    targets: byTags,
    post: vi.fn(async (spoken: string) => {
      seen.reached = claimSpeechPost({ id: KEY, comments: [{ id: "k9" }] as never }, spoken);
      return "posted" as const;
    }),
  });
  return { seen, composer };
}

async function speaking(composer: ReturnType<typeof handle>) {
  resetLiveBubbleForTest();
  resetSpeechPostsForTest();
  await confirmed(composer);
}

async function endUtterance(text: string) {
  api.finishVoiceCapture.mockResolvedValueOnce(text);
  noteActivity(change("speech_end", { captureId: "c1" }));
  await settle();
}

describe("with the agent group switch off", () => {
  it("VOM-FR-RNBS: the first chunk starts a speech turn for each agent that it reaches", async () => {
    const { composer } = speaker();
    await speaking(composer);
    said(0, "@arch and @sec check this.");
    await settle();
    expect(api.dispatchSpeechTurn.mock.calls.map((c) => (c[0] as { nickname: string }).nickname)).toEqual([
      "arch",
      "sec",
    ]);
    said(1, "More.");
    await settle();
    expect(api.steerSpeechTurn).toHaveBeenCalledWith("c1", "More.");
  });
});

describe("with the agent group switch on", () => {
  it("VOM-FR-SWHT, VOM-FR-OLFT: one tagged agent starts its speech turn at the first chunk, and later chunks steer it", async () => {
    await setGroupSwitch(true);
    const { composer } = speaker();
    await speaking(composer);
    said(0, "@arch check this.");
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);
    expect(api.dispatchSpeechTurn).toHaveBeenCalledWith(
      expect.objectContaining({ nickname: "arch", speechId: "c1" }),
    );
    said(1, "And the scope.");
    await settle();
    expect(api.steerSpeechTurn).toHaveBeenCalledWith("c1", "And the scope.");
  });

  it("VOM-FR-SWHT: several tags start no speech turn and send no chunk as steering", async () => {
    await setGroupSwitch(true);
    const { composer, seen } = speaker();
    await speaking(composer);
    said(0, "@arch and @sec check this.");
    said(1, "More text.");
    said(2, "Even more.");
    await settle();
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
    expect(api.steerSpeechTurn).not.toHaveBeenCalled();
    await endUtterance("@arch and @sec check this. More text. Even more.");
    // The comment dispatches as a typed comment: the speech reached nobody.
    expect(seen.reached).toEqual(new Set());
    expect(api.endSpeechTurn).not.toHaveBeenCalled();
  });

  it("VOM-FR-SWHT: @all with several ready agents starts no speech turn", async () => {
    await setGroupSwitch(true);
    const { composer } = speaker();
    await speaking(composer);
    said(0, "@all what do you think?");
    await settle();
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
    expect(api.steerSpeechTurn).not.toHaveBeenCalled();
  });

  it("VOM-FR-SWHT, VOM-FR-MVLA: the comment of an early turn ends the speech with its id", async () => {
    await setGroupSwitch(true);
    const { composer, seen } = speaker();
    await speaking(composer);
    said(0, "@arch check this.");
    await endUtterance("@arch check this.");
    expect(seen.reached).toEqual(new Set(["arch"]));
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", "k9");
  });

  it("VOM-FR-ETCK: a later chunk that adds a tag cancels the early turn at once", async () => {
    await setGroupSwitch(true);
    const { composer, seen } = speaker();
    await speaking(composer);
    said(0, "@arch check this.");
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);
    said(1, "And ask @sec too.");
    await settle();
    expect(api.endSpeechTurn).toHaveBeenCalledTimes(1);
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", null);
    expect(api.steerSpeechTurn).not.toHaveBeenCalled();
    said(2, "Third chunk.");
    await settle();
    expect(api.steerSpeechTurn).not.toHaveBeenCalled();
    await endUtterance("@arch check this. And ask @sec too. Third chunk.");
    // The several-tag flow runs: no agent was reached, and the end is not sent again.
    expect(seen.reached).toEqual(new Set());
    expect(api.endSpeechTurn).toHaveBeenCalledTimes(1);
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-ETCK: a later chunk that adds no tag keeps steering", async () => {
    await setGroupSwitch(true);
    const { composer } = speaker();
    await speaking(composer);
    said(0, "@arch check this.");
    said(1, "And the scope.");
    await settle();
    expect(api.endSpeechTurn).not.toHaveBeenCalled();
    expect(api.steerSpeechTurn).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-SWHT: an utterance that reaches no agent starts no speech turn", async () => {
    await setGroupSwitch(true);
    const composer = handle({ targets: () => [] });
    await speaking(composer);
    said(0, "Hello there.");
    await settle();
    expect(api.dispatchSpeechTurn).not.toHaveBeenCalled();
  });
});
