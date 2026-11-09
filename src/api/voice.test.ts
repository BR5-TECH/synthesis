/** The wire shape of the voice commands (`VOP-voice-ai-providers.md`). */
import { beforeEach, describe, expect, it, vi } from "vitest";

const core = vi.hoisted(() => {
  class Channel {
    onmessage: ((frame: ArrayBuffer) => void) | null = null;
  }
  return { invoke: vi.fn(async (): Promise<unknown> => undefined), Channel };
});

vi.mock("@tauri-apps/api/core", () => core);

import {
  appendVoiceCapture,
  closeVoiceActivitySession,
  feedVoiceActivity,
  listVoiceAiVoices,
  openVoiceActivitySession,
  setVoiceActivityGate,
  setVoiceAiDefaultVoice,
  setVoiceAiRoutes,
  setVoiceAiSpeechOptions,
  speakVoiceSegment,
  startVoiceCapture,
  verifyVoiceAiIntegration,
} from "./voice";
import { translateVoiceTranscript, writeVoiceProgressUpdate } from "./integrations";

beforeEach(() => core.invoke.mockClear());

describe("the voice activity commands", () => {
  it("VOP-FR-WYKO: open takes the rate and the keyterms", async () => {
    await openVoiceActivitySession(48_000, ["arch"]);
    expect(core.invoke).toHaveBeenCalledWith("open_voice_activity_session", {
      captureRate: 48_000,
      keyterms: ["arch"],
    });
  });

  it("VOP-FR-TADX: audio goes as a raw body, with its session in a header", async () => {
    const bytes = new Uint8Array([1, 2]);
    await feedVoiceActivity("va1", bytes);
    expect(core.invoke).toHaveBeenCalledWith("feed_voice_activity", bytes, {
      headers: { "x-voice-session": "va1" },
    });
  });

  it("VOP-FR-QZCE: the gate goes as one whole record", async () => {
    const gate = { seq: 2, blocked: true, playing: false, chunked: true, keyterms: ["arch"] };
    await setVoiceActivityGate("va1", gate);
    expect(core.invoke).toHaveBeenCalledWith("set_voice_activity_gate", {
      activityId: "va1",
      ...gate,
    });
  });

  it("VOP-FR-FAAB: close takes the session and `discard`", async () => {
    await closeVoiceActivitySession("va1", true);
    expect(core.invoke).toHaveBeenCalledWith("close_voice_activity_session", {
      activityId: "va1",
      discard: true,
    });
  });
});

describe("the voice commands", () => {
  it("VOP-FR-ERDS: a capture starts chunked or not", async () => {
    await startVoiceCapture(48_000, ["arch"], true);
    expect(core.invoke).toHaveBeenCalledWith("start_voice_capture", {
      captureRate: 48_000,
      keyterms: ["arch"],
      chunked: true,
    });
  });

  it("VOP-FR-DXPK: audio goes as a raw body, with its session in a header", async () => {
    const bytes = new Uint8Array([1, 2]);
    await appendVoiceCapture("s1", bytes);
    expect(core.invoke).toHaveBeenCalledWith("append_voice_capture", bytes, {
      headers: { "x-voice-session": "s1" },
    });
  });

  type SpeakArgs = {
    speechId: string;
    text: string;
    voiceName: string;
    onFrame: InstanceType<typeof core.Channel>;
  };
  const speakCall = () => core.invoke.mock.calls[0] as unknown as [string, SpeakArgs];
  const END = new Uint8Array([2, 0, 0, 0, 0]).buffer;

  async function ticks() {
    await new Promise((done) => setTimeout(done, 0));
  }

  it("VOP-FR-MOSY, VOP-FR-XZET: a segment gets a channel that delivers each frame", async () => {
    const frames: (ArrayBuffer | number[])[] = [];
    const segment = speakVoiceSegment("id", "text", "cedar", (frame) => frames.push(frame));
    const [command, args] = speakCall();
    expect(command).toBe("speak_voice_segment");
    expect(args).toMatchObject({ speechId: "id", text: "text", voiceName: "cedar" });
    args.onFrame.onmessage?.(new ArrayBuffer(4));
    expect(frames).toHaveLength(1);
    args.onFrame.onmessage?.(END);
    await segment;
    expect(frames).toHaveLength(1);
  });

  it("VOP-FR-MOSY, VOM-FR-SJYH: a frame that comes after the command is not lost", async () => {
    const frames: (ArrayBuffer | number[])[] = [];
    let ended = false;
    const segment = speakVoiceSegment("id", "text", "cedar", (frame) => frames.push(frame));
    void segment.then(() => (ended = true));
    await ticks();
    expect(core.invoke).toHaveBeenCalledTimes(1);
    expect(ended).toBe(false);
    const [, args] = speakCall();
    args.onFrame.onmessage?.(new ArrayBuffer(4096));
    args.onFrame.onmessage?.(END);
    await segment;
    expect(frames).toHaveLength(1);
  });

  it("VOM-FR-DYTO: a frame as a byte array reaches playback, and an end frame as an array ends the segment", async () => {
    const frames: unknown[] = [];
    const segment = speakVoiceSegment("id", "text", null, (frame) => frames.push(frame));
    const [, args] = speakCall();
    const onmessage = args.onFrame.onmessage as unknown as (frame: unknown) => void;
    onmessage([0, 0x80, 0xbb, 0, 0, 1, 2]);
    onmessage([2, 0, 0, 0, 0]);
    await segment;
    expect(frames).toEqual([[0, 0x80, 0xbb, 0, 0, 1, 2]]);
  });

  it("VOM-FR-VGQF: a frame that playback cannot take rejects the segment, and later frames are ignored", async () => {
    const seen: unknown[] = [];
    const segment = speakVoiceSegment("id", "text", null, (frame) => {
      seen.push(frame);
      throw new TypeError("not a buffer");
    });
    const [, args] = speakCall();
    args.onFrame.onmessage?.(new ArrayBuffer(8));
    args.onFrame.onmessage?.(new ArrayBuffer(8));
    args.onFrame.onmessage?.(END);
    await expect(segment).rejects.toBe("playback_failed");
    expect(seen).toHaveLength(1);
  });

  it("VOP-FR-MOSY: a failed segment rejects with no end frame", async () => {
    core.invoke.mockRejectedValueOnce("speech_failed");
    await expect(speakVoiceSegment("id", "text", null, () => undefined)).rejects.toBe(
      "speech_failed",
    );
  });

  it("VOP-FR-GVTD: argument names match the commands", async () => {
    await verifyVoiceAiIntegration("custom", "http://h", null, "s", "t");
    expect(core.invoke).toHaveBeenLastCalledWith("verify_voice_ai_integration", {
      provider: "custom",
      baseUrl: "http://h",
      apiKey: null,
      sttModel: "s",
      ttsModel: "t",
    });
    await setVoiceAiRoutes("custom", "http", null);
    expect(core.invoke).toHaveBeenLastCalledWith("set_voice_ai_routes", {
      provider: "custom",
      transcriptionRoute: "http",
      speechRoute: null,
    });
    await setVoiceAiDefaultVoice("custom", "v");
    expect(core.invoke).toHaveBeenLastCalledWith("set_voice_ai_default_voice", {
      provider: "custom",
      voiceName: "v",
    });
    await startVoiceCapture(48_000, ["arch"]);
    expect(core.invoke).toHaveBeenLastCalledWith("start_voice_capture", {
      captureRate: 48_000,
      keyterms: ["arch"],
      chunked: false,
    });
  });

  it("VOP-FR-HQTQ, VOP-FR-MOSY: the speed and the style reach the commands by their names", async () => {
    await setVoiceAiSpeechOptions("custom", 130, "warm");
    expect(core.invoke).toHaveBeenLastCalledWith("set_voice_ai_speech_options", {
      provider: "custom",
      speedPercent: 130,
      style: "warm",
    });
    void speakVoiceSegment("id", "text", "v", () => undefined, { speedPercent: 150, style: "calm" });
    expect(core.invoke).toHaveBeenLastCalledWith(
      "speak_voice_segment",
      expect.objectContaining({ voiceName: "v", voiceSpeed: 150, voiceStyle: "calm" }),
    );
    void speakVoiceSegment("id", "text", null, () => undefined, { speedPercent: null, style: "" });
    expect(core.invoke).toHaveBeenLastCalledWith(
      "speak_voice_segment",
      expect.objectContaining({ voiceSpeed: null, voiceStyle: null }),
    );
    void speakVoiceSegment("id", "text", null, () => undefined);
    expect(core.invoke).toHaveBeenLastCalledWith(
      "speak_voice_segment",
      expect.objectContaining({ voiceSpeed: null, voiceStyle: null }),
    );
  });

  it("VOP-FR-TPXA: a provider reaches the capture and the speech only when it is given", async () => {
    await startVoiceCapture(48_000, [], false, "custom");
    expect(core.invoke).toHaveBeenLastCalledWith("start_voice_capture", {
      captureRate: 48_000,
      keyterms: [],
      chunked: false,
      provider: "custom",
    });
    void speakVoiceSegment("id", "text", null, () => undefined, null, "openai");
    expect(core.invoke).toHaveBeenLastCalledWith(
      "speak_voice_segment",
      expect.objectContaining({ provider: "openai" }),
    );
    void speakVoiceSegment("id", "text", null, () => undefined);
    const args = (core.invoke.mock.lastCall as unknown[] | undefined)?.[1] as Record<string, unknown>;
    expect("provider" in args).toBe(false);
  });

  it("AGC-FR-QEOF: the translation takes the text, the nicknames, and the target", async () => {
    core.invoke.mockResolvedValueOnce({ text: "Hello." });
    const target = { kind: "draft", draftId: "d1" } as const;
    await expect(translateVoiceTranscript("Hola.", ["arch"], target)).resolves.toEqual({
      text: "Hello.",
    });
    expect(core.invoke).toHaveBeenLastCalledWith("translate_voice_transcript", {
      text: "Hola.",
      nicknames: ["arch"],
      target,
    });
  });

  it("AGC-FR-YMFC, AGC-FR-PGSJ: the progress update takes the activity and the spoken language", async () => {
    core.invoke.mockResolvedValueOnce({ text: "Checking existing specifications." });
    const activity = [{ tool: "search_specifications" }, { tool: "search_drafts", advisor: "sec" }];
    await expect(writeVoiceProgressUpdate(activity, "English")).resolves.toEqual({
      text: "Checking existing specifications.",
    });
    expect(core.invoke).toHaveBeenLastCalledWith("write_voice_progress_update", {
      activity,
      spokenLanguage: "English",
    });
  });

  it("VOP-FR-XJRF: a provider is optional", async () => {
    await listVoiceAiVoices();
    expect(core.invoke).toHaveBeenLastCalledWith("list_voice_ai_voices", { provider: null });
    await listVoiceAiVoices("openai");
    expect(core.invoke).toHaveBeenLastCalledWith("list_voice_ai_voices", { provider: "openai" });
  });
});
