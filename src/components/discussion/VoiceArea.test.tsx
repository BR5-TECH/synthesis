/**
 * Voice mode in the shared composer (`VOM-voice-mode.md`,
 * `CVP-conversation-presentation.md` CVP-FR-55).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  appendVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceCapture: vi.fn(async () => undefined),
  cancelVoiceSpeech: vi.fn(async () => undefined),
  finishVoiceCapture: vi.fn(async (): Promise<string> => "hello there"),
  retryVoiceCapture: vi.fn(async (): Promise<string> => "again"),
  speakVoiceSegment: vi.fn(async () => undefined),
  startVoiceCapture: vi.fn(async () => ({ sessionId: "s1", route: "http", live: false })),
}));

vi.mock("@tauri-apps/api/core", () => ({
  // A command that this test does not expect is a defect of the test.
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
  openMicrophone: vi.fn(async () => ({ rate: 48_000, stop: vi.fn() })),
}));
vi.mock("../../voice/playback", () => ({
  isPlaybackIdle: vi.fn(() => true),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  setDrainedListener: vi.fn(),
  stopPlayback: vi.fn(),
}));

import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerAttachments,
  setComposerBody,
  setComposerQuotes,
} from "../../state/discussionSession";
import {
  getVoiceState,
  patchVoiceState,
  resetVoiceStateForTest,
} from "../../state/voiceSession";
import type { Discussion, ProjectAgent } from "../../types";
import { resetLogBufferForTest } from "../../logging";
import { voiceMessage } from "../../voice/voiceMessages";
import {
  noteProvider,
  resetVoiceControllerForTest,
  setVoiceBehaviour,
} from "../../voice/voiceController";
import { DiscussionComposer } from "./DiscussionComposer";

const archAgent = {
  scope: "project" as const,
  agent: {
    id: "a1",
    nickname: "arch",
    title: "",
    modelId: "m",
    instructions: "",
    createdAt: "",
    updatedAt: "",
    reasoning: null,
  },
  availability: "ready",
  shadowed: false,
} as unknown as ProjectAgent;

function discussion(): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [
      {
        id: "c1",
        author: { kind: "human", login: "me" },
        body: "first",
        quotes: [],
        attachments: [],
        createdAt: "2026-02-01T00:00:00Z",
      },
    ],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

const onReply = vi.fn(async () => undefined);

function renderReply(over: Partial<{ disabled: boolean; agents: ProjectAgent[] }> = {}) {
  return render(
    <DiscussionComposer
      mode="reply"
      discussion={discussion()}
      onReply={onReply}
      agents={over.agents ?? [archAgent]}
      disabled={over.disabled}
    />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  resetVoiceStateForTest();
  resetVoiceControllerForTest();
  clearAllDiscussionSessions();
  resetLogBufferForTest();
  noteProvider(true, true);
});

afterEach(cleanup);

function enterVoice() {
  fireEvent.click(screen.getByTestId("voice-button"));
}

describe("the Voice button", () => {
  it("VOM-FR-LAHW: every composer shows it", () => {
    renderReply();
    expect(screen.getByRole("button", { name: "Voice mode" })).toBeInTheDocument();
  });

  it("VOM-FR-LAHW: an opening composer shows it too", () => {
    render(
      <DiscussionComposer
        mode="opening"
        target={{ kind: "artifact", artifactId: "a.md" }}
        onOpen={vi.fn()}
        agents={[]}
      />,
    );
    expect(screen.getByTestId("voice-button")).toBeInTheDocument();
  });

  it("VOM-FR-UXKL: with no provider it is disabled and says why", () => {
    noteProvider(false, false);
    renderReply();
    const button = screen.getByTestId("voice-button");
    expect(button).toBeDisabled();
    expect(button.getAttribute("aria-description")).toMatch(/Voice AI provider/);
  });
});

describe("the voice area", () => {
  it("VOM-FR-NPLU, VOM-FR-TISN: the text field gives way to the voice area", () => {
    renderReply();
    enterVoice();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getByTestId("voice-area")).toBeInTheDocument();
    expect(screen.getByTestId("voice-expand")).toBeInTheDocument();
    expect(screen.getByTestId("voice-transcript")).toBeInTheDocument();
    expect(screen.getByTestId("voice-record")).toBeInTheDocument();
  });

  it("VOM-FR-HCMT: Expand shows the field for a time, and the text stays", () => {
    renderReply();
    enterVoice();
    fireEvent.click(screen.getByTestId("voice-expand"));
    const field = screen.getByRole("textbox");
    fireEvent.change(field, { target: { value: "typed" } });
    fireEvent.click(screen.getByTestId("voice-expand"));
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(getDiscussionSession("d1").body).toBe("typed");
  });

  it("VOM-FR-JBJI: the Voice button leaves voice mode", () => {
    renderReply();
    enterVoice();
    fireEvent.click(screen.getByRole("button", { name: "Leave voice mode" }));
    expect(getVoiceState().activeKey).toBeNull();
    expect(screen.getByRole("textbox")).toBeInTheDocument();
  });

  it("DDS-FR-JKPC: the voice area stands in the row of the field", () => {
    renderReply();
    enterVoice();
    const area = screen.getByTestId("voice-area");
    expect(area.parentElement).toHaveClass("comment-composer__row");
    expect(area.querySelector(".voice-area__toggles")).toBe(area.lastElementChild);
  });

  it("VOM-FR-LAQO: focus goes to the record control", () => {
    renderReply();
    enterVoice();
    expect(screen.getByTestId("voice-record")).toHaveFocus();
  });

  it("VOM-FR-IKIK: VAD is visible and disabled, and says that it comes later", () => {
    renderReply();
    enterVoice();
    const vad = screen.getByRole("button", { name: "VAD" });
    expect(vad).toBeDisabled();
    expect(vad.getAttribute("title")).toMatch(/later/);
    expect(screen.getByRole("button", { name: "Push-to-Talk" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("VOM-FR-IAVZ: the default is Text, and Voice can be chosen", () => {
    renderReply();
    enterVoice();
    expect(screen.getByRole("button", { name: "Text" })).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(screen.getByRole("button", { name: "Voice" }));
    expect(getVoiceState().responseMode).toBe("voice");
  });

  it("VOM-FR-IAVZ: with no speech route, Voice is disabled and says why", () => {
    noteProvider(true, false);
    renderReply();
    enterVoice();
    const voice = screen.getByRole("button", { name: "Voice" });
    expect(voice).toBeDisabled();
    expect(voice.getAttribute("aria-description")).toMatch(/speech route/);
  });

  it("VOM-FR-HUXY: shows the transcript", () => {
    renderReply();
    enterVoice();
    act(() => patchVoiceState({ phase: "recording", live: true, transcript: "live words" }));
    expect(screen.getByTestId("voice-transcript")).toHaveTextContent("live words");
  });

  it("VOM-FR-SRGN: shows Recording with a timer, then Transcribing", () => {
    renderReply();
    enterVoice();
    act(() =>
      patchVoiceState({ phase: "recording", live: false, recordingSince: Date.now() - 65_000 }),
    );
    expect(screen.getByTestId("voice-transcript")).toHaveTextContent(/Recording 1:0\d/);
    act(() => patchVoiceState({ phase: "transcribing", recordingSince: null }));
    expect(screen.getByTestId("voice-transcript")).toHaveTextContent("Transcribing");
  });

  it("VOM-FR-HSKZ: the record control states its phase, and the text is no live region", () => {
    renderReply();
    enterVoice();
    expect(screen.getByRole("button", { name: "Record" })).toBeInTheDocument();
    act(() => patchVoiceState({ phase: "recording", live: true }));
    expect(screen.getByRole("button", { name: /^Recording/ })).toBeInTheDocument();
    expect(screen.getByTestId("voice-transcript")).not.toHaveAttribute("aria-live");
  });

  it("VOM-FR-ZUWW: a disabled composer does not record", () => {
    renderReply({ disabled: true });
    enterVoice();
    const record = screen.getByTestId("voice-record");
    expect(record).toBeDisabled();
    fireEvent.keyDown(record, { key: " " });
    expect(getVoiceState().phase).toBe("idle");
  });

  it("VOM-FR-HUXY: the transcript follows the newest words", () => {
    renderReply();
    enterVoice();
    const box = screen.getByTestId("voice-transcript");
    Object.defineProperty(box, "scrollHeight", { value: 400, configurable: true });
    act(() => patchVoiceState({ phase: "recording", live: true, transcript: "more words" }));
    expect(box.scrollTop).toBe(400);
  });

  it("VOM-FR-QDWW: Stop shows only while audio plays", () => {
    renderReply();
    enterVoice();
    expect(screen.queryByTestId("voice-stop")).not.toBeInTheDocument();
    act(() => patchVoiceState({ playing: true }));
    fireEvent.click(screen.getByTestId("voice-stop"));
    expect(getVoiceState().playing).toBe(false);
  });

  it("VOM-FR-ZFSA, VOM-FR-JJMY: an error shows inline, and names the fix", () => {
    renderReply();
    enterVoice();
    act(() => patchVoiceState({ error: "microphone_denied", canRetry: false }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Privacy settings/);
    expect(screen.queryByTestId("voice-retry")).not.toBeInTheDocument();
    expect(voiceMessage("anything")).toMatch(/failed/);
  });

  it("VOM-FR-WICV: a provider failure offers Retry", () => {
    renderReply();
    enterVoice();
    act(() => patchVoiceState({ error: "provider_unreachable", canRetry: true }));
    expect(screen.getByTestId("voice-retry")).toBeInTheDocument();
  });

  it("VOM-FR-RENV, VOM-FR-BVNX: a note says what happened", () => {
    renderReply();
    enterVoice();
    act(() => patchVoiceState({ note: "no_speech" }));
    expect(screen.getByTestId("voice-note")).toHaveTextContent(/No speech/);
    act(() => patchVoiceState({ note: "no_agent" }));
    expect(screen.getByTestId("voice-note")).toHaveTextContent(/No agent/);
  });
});

describe("posting by voice", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  async function advance(ms: number) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(ms);
    });
  }

  async function holdAndRelease(record: HTMLElement) {
    fireEvent.pointerDown(record, { pointerId: 1 });
    await waitFor(() => expect(getVoiceState().phase).toBe("recording"));
    await advance(310);
    fireEvent.pointerUp(record, { pointerId: 1 });
    await advance(250);
  }

  it("VOM-FR-WYIE, VOM-FR-RFWC: a recording posts through the composer, after the unsent text", async () => {
    setComposerBody("d1", "typed first");
    renderReply();
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(onReply).toHaveBeenCalled());
    expect(onReply).toHaveBeenCalledWith("d1", "typed first\nhello there", [], []);
    await waitFor(() => expect(getDiscussionSession("d1").body).toBe(""));
  });

  it("VOM-FR-BVNX: reaches nobody with no agent in the discussion", async () => {
    renderReply({ agents: [] });
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(getVoiceState().note).toBe("no_agent"));
  });

  it("VOM-FR-TAQJ: a spoken name reaches the agent", async () => {
    api.finishVoiceCapture.mockImplementationOnce(async () => "at arch, look here");
    renderReply();
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(onReply).toHaveBeenCalled());
    expect(onReply).toHaveBeenCalledWith("d1", "@arch look here", [], []);
    await waitFor(() => expect(getVoiceState().phase).toBe("idle"));
    expect(getVoiceState().note).toBeNull();
  });

  it("VOM-FR-PECJ: Space acts as a press, and its release as a release", async () => {
    renderReply();
    enterVoice();
    const record = screen.getByTestId("voice-record");
    fireEvent.keyDown(record, { key: " " });
    await waitFor(() => expect(getVoiceState().phase).toBe("recording"));
    fireEvent.keyDown(record, { key: " ", repeat: true });
    await advance(310);
    fireEvent.keyUp(record, { key: " " });
    await advance(250);
    await waitFor(() => expect(onReply).toHaveBeenCalledTimes(1));
  });

  it("VOM-FR-LIFL: a press on the control starts and the next stops", async () => {
    setVoiceBehaviour("toggle");
    renderReply();
    enterVoice();
    const record = screen.getByTestId("voice-record");
    fireEvent.pointerDown(record, { pointerId: 1 });
    await waitFor(() => expect(getVoiceState().phase).toBe("recording"));
    fireEvent.pointerUp(record, { pointerId: 1 });
    expect(getVoiceState().phase).toBe("recording");
    fireEvent.pointerDown(record, { pointerId: 1 });
    await advance(250);
    await waitFor(() => expect(onReply).toHaveBeenCalledTimes(1));
  });

  it("VOM-FR-WYIE: a post carries the quotes and the attachments of the session", async () => {
    const input = { kind: "link", url: "u" };
    const attachment = { input, name: "n" } as never;
    const quote = { commentId: "c1", excerpt: "first" };
    setComposerQuotes("d1", [quote]);
    setComposerAttachments("d1", [attachment]);
    renderReply();
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(onReply).toHaveBeenCalled());
    expect(onReply).toHaveBeenCalledWith("d1", "hello there", [quote], [input]);
    await waitFor(() => expect(getDiscussionSession("d1").attachments).toEqual([]));
    expect(getDiscussionSession("d1").quotes).toEqual([]);
  });

  it("VOM-FR-WYIE, VOM-FR-ILJW: an opening composer posts by voice and stays in voice mode", async () => {
    const created = discussion();
    const onOpen = vi.fn(async () => created);
    const onOpened = vi.fn();
    const extra = { kind: "link", url: "x" } as never;
    render(
      <DiscussionComposer
        mode="opening"
        target={{ kind: "artifact", artifactId: "a.md" }}
        onOpen={onOpen}
        onOpened={onOpened}
        extraAttachments={[extra]}
        agents={[archAgent]}
      />,
    );
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(onOpen).toHaveBeenCalled());
    expect(onOpen).toHaveBeenCalledWith(
      expect.objectContaining({ body: "hello there", attachments: [extra] }),
    );
    await waitFor(() => expect(onOpened).toHaveBeenCalledWith(created));
    expect(getVoiceState()).toMatchObject({ activeKey: "d1", discussionId: "d1" });
  });

  it("VOM-FR-ABPU: a refused post keeps the text and the error", async () => {
    onReply.mockImplementationOnce(() => Promise.reject("rejected_by_store"));
    setComposerBody("d1", "typed");
    renderReply();
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(getVoiceState().note).toBe("kept_in_body"));
    expect(getDiscussionSession("d1").body).toBe("typed\nhello there");
    expect(getDiscussionSession("d1").error).toBe("rejected_by_store");
  });

  it("VOM-FR-ABPU: a transcript during a typed post stays after the post lands", async () => {
    let finish: () => void = () => undefined;
    onReply.mockImplementationOnce(() => new Promise<undefined>((r) => (finish = () => r(undefined))));
    setComposerBody("d1", "typed");
    renderReply();
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(onReply).toHaveBeenCalledTimes(1));
    enterVoice();
    await holdAndRelease(screen.getByTestId("voice-record"));
    await waitFor(() => expect(getVoiceState().note).toBe("kept_in_body"));
    finish();
    await waitFor(() => expect(getDiscussionSession("d1").body).toBe("hello there"));
    expect(onReply).toHaveBeenCalledTimes(1);
  });

  it("VOM-FR-LAQO: focus stays on the record control after a recording", async () => {
    renderReply();
    enterVoice();
    const record = screen.getByTestId("voice-record");
    await holdAndRelease(record);
    await waitFor(() => expect(getVoiceState().phase).toBe("idle"));
    expect(record).not.toBeDisabled();
    expect(record).toHaveFocus();
  });

  it("CVP-FR-55: a typed post still works after the refactor", async () => {
    setComposerBody("d1", "typed");
    renderReply();
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(onReply).toHaveBeenCalledWith("d1", "typed", [], []));
    await waitFor(() => expect(getDiscussionSession("d1").body).toBe(""));
  });

  it("VOM-FR-WYIE: text typed while a post is in flight stays", async () => {
    let finish: () => void = () => undefined;
    onReply.mockImplementationOnce(() => new Promise<undefined>((r) => (finish = () => r(undefined))));
    setComposerBody("d1", "sent");
    renderReply();
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(onReply).toHaveBeenCalled());
    setComposerBody("d1", "sent and more");
    finish();
    await waitFor(() => expect(getDiscussionSession("d1").body).toBe("and more"));
  });
});
