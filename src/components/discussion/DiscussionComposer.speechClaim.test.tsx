/**
 * The claim of a speech comment, through a real composer that holds typed text
 * (`CTA-comment-agent-turns.md` CTA-FR-SZOC, `VOM-voice-mode.md` VOM-FR-RFWC,
 * VOM-FR-MVLA). The controller computes the body that the post will send, and
 * the composer sends it. Only that body claims the speech.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

const { api, audio, mic } = await vi.hoisted(() => import("../../test/voiceVadMocks"));

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
vi.mock("../../voice/playback", () => audio);
vi.mock("../../voice/capture", async (original) => ({
  ...(await original<typeof import("../../voice/capture")>()),
  openMicrophone: mic.open,
}));

import { resetLogBufferForTest } from "../../logging";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerBody,
} from "../../state/discussionSession";
import { resetLiveBubbleForTest } from "../../state/liveBubble";
import { claimSpeechPost, resetSpeechPostsForTest } from "../../state/speechPosts";
import { patchVoiceState, resetVoiceStateForTest } from "../../state/voiceSession";
import type { ProjectAgent, VoiceChunkChange } from "../../types";
import { KEY, change, comment, discussion } from "../../test/voiceVadKit";
import {
  TAIL_MS,
  enterVoiceMode,
  noteActivity,
  noteChunk,
  pressRecord,
  resetVoiceControllerForTest,
  setInputMode,
} from "../../voice/voiceController";
import { DiscussionComposer } from "./DiscussionComposer";

function agent(nickname: string): ProjectAgent {
  return {
    scope: "project",
    agent: { id: "a1", nickname, title: "", modelId: "m", instructions: "", voice: "cedar" },
    availability: "ready",
    shadowed: false,
  } as unknown as ProjectAgent;
}

function chunk(
  state: VoiceChunkChange["state"],
  index: number,
  over: Partial<VoiceChunkChange> = {},
): VoiceChunkChange {
  return { captureId: "c1", index, state, text: null, code: null, ...over };
}

async function settle() {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(0);
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  resetLogBufferForTest();
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  resetLiveBubbleForTest();
  resetSpeechPostsForTest();
  clearAllDiscussionSessions();
  mic.open.mockImplementation(async () => ({ rate: 48_000, stop: mic.stop }));
  audio.isPlaybackIdle.mockReturnValue(true);
});

afterEach(async () => {
  cleanup();
  await vi.advanceTimersByTimeAsync(TAIL_MS * 2);
  resetVoiceControllerForTest();
  vi.useRealTimers();
});

describe("a speech comment posted by a composer that holds typed text", () => {
  it("CTA-FR-SZOC, VOM-FR-RFWC, VOM-FR-MVLA: only the body that the composer sends claims the speech, and a typed comment during the post claims nothing", async () => {
    const thread = discussion([comment("h1", "human", "@arch look at this."), comment("r1")]);
    const claims: Record<string, ReadonlySet<string>> = {};
    const sent: string[] = [];
    // The dispatch path of a reply: each comment that lands asks for the claim.
    const onReply = vi.fn(async (id: string, body: string) => {
      claims.typed = claimSpeechPost(
        { id, comments: [...thread.comments, { id: "t1" }] as never },
        "A typed comment from the other window.",
      );
      claims.bare = claimSpeechPost(
        { id, comments: [...thread.comments, { id: "t2" }] as never },
        "Check this.",
      );
      claims.sent = claimSpeechPost(
        { id, comments: [...thread.comments, { id: "k9" }] as never },
        body,
      );
      sent.push(body);
    });
    render(
      <DiscussionComposer
        mode="reply"
        discussion={thread}
        onReply={onReply}
        agents={[agent("arch")]}
      />,
    );
    act(() => {
      enterVoiceMode(KEY, thread);
      patchVoiceState({ providerReady: true, providerSpeaks: true });
      setInputMode("vad");
      setComposerBody(KEY, "Typed before.");
    });
    act(() => pressRecord());
    await settle();
    act(() => {
      noteActivity(change("speech_start"));
      noteActivity(change("speech_confirmed", { captureId: "c1", live: true }));
      noteChunk(chunk("chunk_end", 0));
      noteChunk(chunk("chunk_transcript", 0, { text: "Check this." }));
    });
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledTimes(1);

    api.finishVoiceCapture.mockResolvedValueOnce("Check this.");
    act(() => noteActivity(change("speech_end", { captureId: "c1" })));
    await settle();

    expect(sent).toEqual(["Typed before.\nCheck this."]);
    expect(claims.typed).toEqual(new Set());
    expect(claims.bare).toEqual(new Set());
    expect(claims.sent).toEqual(new Set(["arch"]));
    expect(api.endSpeechTurn).toHaveBeenCalledWith("c1", "k9");
    expect(getDiscussionSession(KEY).body).toBe("");
  });
});
