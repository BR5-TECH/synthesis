/**
 * The voice handle of an opening composer after its first post
 * (`VOM-voice-mode.md` VOM-FR-NERG, VOM-FR-BCRD). The composer and the
 * controller are real; the backend is a mock routed by command name.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("../../voice/playback", () => ({
  isPlaybackIdle: vi.fn(() => true),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));

import { resetLogBufferForTest } from "../../logging";
import { clearConversationThreads } from "../../state/conversationThreads";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerBody,
} from "../../state/discussionSession";
import { getVoiceState, resetVoiceStateForTest } from "../../state/voiceSession";
import type { Comment, Discussion } from "../../types";
import {
  enterVoiceMode,
  resetVoiceControllerForTest,
} from "../../voice/voiceController";
import { postingHandleOf } from "../../voice/voiceHandles";
import { DiscussionComposer } from "./DiscussionComposer";

const TARGET = { kind: "note", noteId: "n1" } as const;
const TARGET_KEY = "note:n1";

function comment(id: string, body: string): Comment {
  return {
    id,
    author: { kind: "human", login: "raver119" },
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function discussion(comments: Comment[]): Discussion {
  return {
    id: "created",
    target: TARGET,
    fragmentTarget: null,
    comments,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "add_comment") {
      const { body } = args as { body: string };
      return discussion([comment("c1", "first"), comment("c2", body)]);
    }
    throw new Error(`unexpected invoke ${cmd}`);
  });
  resetLogBufferForTest();
  resetVoiceControllerForTest();
  resetVoiceStateForTest();
  clearAllDiscussionSessions();
  clearConversationThreads();
});

afterEach(cleanup);

describe("an opening composer in voice mode", () => {
  it("VOM-FR-NERG, VOM-FR-BCRD: after the first post and an unmount, a transcript posts into the created discussion", async () => {
    const onOpen = vi.fn(async () => discussion([comment("c1", "first")]));
    const view = render(
      <DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />,
    );
    enterVoiceMode(TARGET_KEY, null, TARGET);

    const opening = postingHandleOf(TARGET_KEY);
    expect(opening).not.toBeNull();
    await act(async () => {
      await opening!.post("first");
    });
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(getVoiceState().activeKey).toBe("created");

    view.unmount();
    const retained = postingHandleOf("created");
    expect(retained).toBe(opening);

    let outcome: string | undefined;
    await act(async () => {
      outcome = await retained!.post("second");
    });
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith(
      "add_comment",
      expect.objectContaining({ discussionId: "created", noteId: "n1", body: "second" }),
    );
    expect(outcome).toBe("reached_nobody");
    expect(getDiscussionSession("created").error).toBeUndefined();
  });

  it("VOM-FR-QWMK: the handle names the target of the discussion, before and after the first post", async () => {
    const onOpen = vi.fn(async () => discussion([comment("c1", "first")]));
    render(<DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />);
    enterVoiceMode(TARGET_KEY, null, TARGET);
    const handle = postingHandleOf(TARGET_KEY)!;
    expect(handle.target?.()).toEqual(TARGET);
    await act(async () => {
      await handle.post("first");
    });
    expect(postingHandleOf("created")!.target?.()).toEqual(TARGET);
  });

  it("VOM-FR-NERG: the transcript follows the session body of the created discussion", async () => {
    const onOpen = vi.fn(async () => discussion([comment("c1", "first")]));
    render(<DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />);
    enterVoiceMode(TARGET_KEY, null, TARGET);
    const handle = postingHandleOf(TARGET_KEY)!;
    await act(async () => {
      await handle.post("first");
    });
    await act(async () => {
      setComposerBody("created", "typed before");
      await postingHandleOf("created")!.post("spoken");
    });
    expect(invokeMock).toHaveBeenCalledWith(
      "add_comment",
      expect.objectContaining({ discussionId: "created", body: "typed before\nspoken" }),
    );
    expect(getDiscussionSession("created").body).toBe("");
  });

  it("VOM-FR-NERG: a refused reply keeps the text and records the error on the created discussion", async () => {
    const onOpen = vi.fn(async () => discussion([comment("c1", "first")]));
    const view = render(
      <DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />,
    );
    enterVoiceMode(TARGET_KEY, null, TARGET);
    const handle = postingHandleOf(TARGET_KEY)!;
    await act(async () => {
      await handle.post("first");
    });
    view.unmount();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return undefined;
      throw "discussion_resolved";
    });
    let outcome: string | undefined;
    await act(async () => {
      outcome = await postingHandleOf("created")!.post("second");
    });
    expect(outcome).toBe("failed");
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(getDiscussionSession("created").error).toBe("discussion_resolved");
  });

  it("VOM-FR-NERG, VOM-FR-BCRD: an older opening composer of the same target posts into the discussion that a newer one created", async () => {
    const onOpen = vi.fn(async () => discussion([comment("c1", "first")]));
    const older = render(
      <DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />,
    );
    const olderHandle = postingHandleOf(TARGET_KEY)!;
    const newer = render(
      <DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />,
    );
    const newerHandle = postingHandleOf(TARGET_KEY)!;
    expect(newerHandle).not.toBe(olderHandle);
    enterVoiceMode(TARGET_KEY, null, TARGET);

    await act(async () => {
      await newerHandle.post("first");
    });
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(getVoiceState().activeKey).toBe("created");

    newer.unmount();
    expect(postingHandleOf("created")).toBe(olderHandle);
    let outcome: string | undefined;
    await act(async () => {
      outcome = await postingHandleOf("created")!.post("second");
    });
    expect(outcome).toBe("reached_nobody");
    expect(onOpen).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith(
      "add_comment",
      expect.objectContaining({ discussionId: "created", noteId: "n1", body: "second" }),
    );
    older.unmount();
  });

  it("VOM-FR-NERG: an opening composer that mounts after the first post opens a new discussion", async () => {
    const onOpen = vi.fn(async () => discussion([comment("c1", "first")]));
    const first = render(
      <DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />,
    );
    enterVoiceMode(TARGET_KEY, null, TARGET);
    await act(async () => {
      await postingHandleOf(TARGET_KEY)!.post("first");
    });
    first.unmount();

    render(<DiscussionComposer mode="opening" target={TARGET} agents={[]} onOpen={onOpen} />);
    enterVoiceMode(TARGET_KEY, null, TARGET);
    await act(async () => {
      await postingHandleOf(TARGET_KEY)!.post("again");
    });
    expect(onOpen).toHaveBeenCalledTimes(2);
  });
});
