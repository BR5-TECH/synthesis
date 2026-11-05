/**
 * The speed and the style of the agent reach each spoken segment
 * (`VOM-voice-mode.md` VOM-FR-UALP), through the handle of the composer.
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

import { patchVoiceState } from "../state/voiceSession";
import { KEY, comment, discussion, handle, settle, useVadBench } from "../test/voiceVadKit";
import {
  enterVoiceMode,
  noteDiscussionChanged,
  registerVoiceComposer,
  setResponseMode,
} from "./voiceController";

useVadBench();

async function replyWith(over: Parameters<typeof handle>[0]) {
  enterVoiceMode(KEY, discussion([]));
  patchVoiceState({ providerReady: true, providerSpeaks: true });
  registerVoiceComposer(KEY, handle(over));
  setResponseMode("voice");
  noteDiscussionChanged(discussion([comment("r1")]));
  await settle();
}

describe("the voice of a reply", () => {
  it("VOM-FR-UALP: the options that the handle knows for the agent reach the segment", async () => {
    await replyWith({ voiceOptionsOf: () => ({ speedPercent: 150, style: "calm" }) });
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      "A reply.",
      "cedar",
      expect.any(Function),
      { speedPercent: 150, style: "calm" },
    );
  });

  it("VOM-FR-UALP: an agent with no options speaks with the values of the provider", async () => {
    await replyWith({ voiceOptionsOf: () => null });
    expect(api.speakVoiceSegment).toHaveBeenCalledWith(
      expect.any(String),
      "A reply.",
      "cedar",
      expect.any(Function),
    );
  });
});
