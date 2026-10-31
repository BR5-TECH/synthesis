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
  listVoiceAiVoices,
  setVoiceAiDefaultVoice,
  setVoiceAiRoutes,
  speakVoiceSegment,
  startVoiceCapture,
  verifyVoiceAiIntegration,
} from "./voice";

beforeEach(() => core.invoke.mockClear());

describe("the voice commands", () => {
  it("VOP-FR-DXPK: audio goes as a raw body, with its session in a header", async () => {
    const bytes = new Uint8Array([1, 2]);
    await appendVoiceCapture("s1", bytes);
    expect(core.invoke).toHaveBeenCalledWith("append_voice_capture", bytes, {
      headers: { "x-voice-session": "s1" },
    });
  });

  it("VOP-FR-MOSY, VOP-FR-XZET: a segment gets a channel that delivers each frame", async () => {
    const frames: ArrayBuffer[] = [];
    await speakVoiceSegment("id", "text", "cedar", (frame) => frames.push(frame));
    const [command, args] = core.invoke.mock.calls[0] as unknown as [
      string,
      { speechId: string; text: string; voiceName: string; onFrame: InstanceType<typeof core.Channel> },
    ];
    expect(command).toBe("speak_voice_segment");
    expect(args).toMatchObject({ speechId: "id", text: "text", voiceName: "cedar" });
    args.onFrame.onmessage?.(new ArrayBuffer(4));
    expect(frames).toHaveLength(1);
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
    });
  });

  it("VOP-FR-XJRF: a provider is optional", async () => {
    await listVoiceAiVoices();
    expect(core.invoke).toHaveBeenLastCalledWith("list_voice_ai_voices", { provider: null });
    await listVoiceAiVoices("openai");
    expect(core.invoke).toHaveBeenLastCalledWith("list_voice_ai_voices", { provider: "openai" });
  });
});
