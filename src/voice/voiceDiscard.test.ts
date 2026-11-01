/**
 * A recording that is discarded while it finishes (`VOM-voice-mode.md`
 * VOM-FR-UXZT, VOM-FR-QCZQ). Nothing waits for its transcript, so nothing may
 * reject unhandled.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const { api, audio, mic } = await vi.hoisted(() => import("../test/voiceVadMocks"));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => audio);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));

import { getVoiceState, patchVoiceState } from "../state/voiceSession";
import { KEY, deferred, discussion, handle, settle, useVadBench } from "../test/voiceVadKit";
import {
  SHORT_PRESS_MS,
  TAIL_MS,
  enterVoiceMode,
  pressRecord,
  registerVoiceComposer,
  releaseRecord,
  stopVoiceMode,
} from "./voiceController";
import { beginUtterance, finishUtterance, settleUtterance } from "./voiceUtterances";

useVadBench();

/** Taken before the bench installs fake timers. */
const realImmediate = globalThis.setImmediate;
const unhandled = vi.fn();
beforeEach(() => {
  unhandled.mockReset();
  process.on("unhandledRejection", unhandled);
});
afterEach(() => {
  process.off("unhandledRejection", unhandled);
});

/** Lets a rejection with no handler reach the process event. */
async function drain() {
  await settle();
  await new Promise((resolve) => realImmediate(resolve));
}

describe("a discard while a recording finishes", () => {
  it("VOM-FR-UXZT: a Stop while the last audio is sent asks for no transcript", async () => {
    const composer = handle();
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ providerReady: true, providerSpeaks: true });
    registerVoiceComposer(KEY, composer);
    pressRecord();
    await settle();
    expect(getVoiceState().phase).toBe("recording");
    // The last audio is on its way when the author lets go.
    const sent = deferred<undefined>();
    api.appendVoiceCapture.mockReturnValueOnce(sent.promise);
    mic.onChunk?.(new ArrayBuffer(8));
    await vi.advanceTimersByTimeAsync(SHORT_PRESS_MS);
    releaseRecord();
    await vi.advanceTimersByTimeAsync(TAIL_MS);
    stopVoiceMode();
    api.finishVoiceCapture.mockRejectedValueOnce("voice_session_not_found");
    sent.resolve(undefined);
    await drain();
    expect(api.cancelVoiceCapture).toHaveBeenCalledWith("s1");
    expect(api.finishVoiceCapture).not.toHaveBeenCalled();
    expect(composer.post).not.toHaveBeenCalled();
    expect(unhandled).not.toHaveBeenCalled();
    api.finishVoiceCapture.mockReset();
  });

  it("VOM-FR-QCZQ: a transcript of a settled utterance that rejects is handled", async () => {
    const u = beginUtterance(KEY, "c9", false, null);
    settleUtterance(u, null);
    await finishUtterance(u, Promise.reject("voice_session_not_found"));
    await drain();
    expect(unhandled).not.toHaveBeenCalled();
  });
});
