/**
 * A transcript in the spoken language of the author posts in English, and the
 * comment keeps what the author said (`VOM-voice-mode.md` VOM-FR-QWMK,
 * VOM-FR-DLPA, VOM-FR-GZCV).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { api, audio, mic } = await vi.hoisted(() => import("../test/voiceVadMocks"));
const prefs = vi.hoisted(() => ({ language: "" }));

vi.mock("../api", () => api);
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn() }));
vi.mock("./playback", () => audio);
vi.mock("./capture", async (original) => ({
  ...(await original<typeof import("./capture")>()),
  openMicrophone: mic.open,
}));
vi.mock("../state/appPreferences", async (original) => ({
  ...(await original<typeof import("../state/appPreferences")>()),
  peekAppPreferences: () => ({ voiceSpokenLanguage: prefs.language }),
}));

import { getVoiceState } from "../state/voiceSession";
import { spokenOriginalFor } from "../state/speechPosts";
import { KEY, change, confirmed, handle, settle, useVadBench } from "../test/voiceVadKit";
import { leaveVoiceMode, noteActivity, noteChunk } from "./voiceController";
import { getDiscussionSession, setComposerBody } from "../state/discussionSession";

useVadBench();

/** The composer of the test, which records what the post carried. */
function recordingComposer() {
  const seen: { body: string; original: string | undefined }[] = [];
  const composer = handle({
    tagged: (text: string) => text,
    post: vi.fn(async (body: string) => {
      seen.push({ body, original: spokenOriginalFor(body) });
      return "posted" as const;
    }),
  });
  return { composer, seen };
}

async function speak(transcript: string, composer: ReturnType<typeof handle>) {
  api.finishVoiceCapture.mockResolvedValueOnce(transcript);
  await confirmed(composer);
  noteActivity(change("speech_end", { captureId: "c1" }));
  await settle();
  await settle();
}

beforeEach(() => {
  prefs.language = "";
  api.translateVoiceTranscript.mockReset();
  api.translateVoiceTranscript.mockImplementation(async (text: string) => ({ text }));
});

describe("translation before the post", () => {
  it("VOM-FR-QWMK: the translation is the body, and the comment keeps the transcript", async () => {
    prefs.language = "ru";
    api.translateVoiceTranscript.mockResolvedValueOnce({ text: "Look at the second part." });
    const { composer, seen } = recordingComposer();
    await speak("Посмотри вторую часть.", composer);
    expect(api.translateVoiceTranscript).toHaveBeenCalledWith("Посмотри вторую часть.", ["arch"]);
    expect(seen).toEqual([
      { body: "Look at the second part.", original: "Посмотри вторую часть." },
    ]);
    expect(getVoiceState().note).toBeNull();
  });

  it("VOM-FR-QWMK: with no spoken language, nothing is translated and nothing is kept", async () => {
    const { composer, seen } = recordingComposer();
    await speak("Look here.", composer);
    expect(api.translateVoiceTranscript).not.toHaveBeenCalled();
    expect(seen).toEqual([{ body: "Look here.", original: undefined }]);
  });

  it("VOM-FR-DLPA: a failed translation posts the transcript with a note", async () => {
    prefs.language = "ru";
    api.translateVoiceTranscript.mockRejectedValueOnce("translation_failed");
    const { composer, seen } = recordingComposer();
    await speak("Посмотри.", composer);
    expect(seen).toEqual([{ body: "Посмотри.", original: undefined }]);
    expect(getVoiceState().note).toBe("not_translated");
  });

  it("VOM-FR-DLPA: a translation equal to the transcript keeps no original", async () => {
    prefs.language = "de";
    const { composer, seen } = recordingComposer();
    await speak("OK.", composer);
    expect(seen).toEqual([{ body: "OK.", original: undefined }]);
  });

  it("VOM-FR-QWMK, VOM-FR-RFWC: with a held session body, the post still carries the transcript of the speech alone", async () => {
    prefs.language = "ru";
    api.translateVoiceTranscript.mockResolvedValueOnce({ text: "Look." });
    setComposerBody(KEY, "typed first");
    const seen: { body: string; original: string | undefined }[] = [];
    // As the real composer does: it posts the session body, then the speech.
    const composer = handle({
      tagged: (text: string) => text,
      post: vi.fn(async (spoken: string) => {
        const body = `${getDiscussionSession(KEY).body.trimEnd()}\n${spoken}`;
        seen.push({ body, original: spokenOriginalFor(body) });
        return "posted" as const;
      }),
    });
    await speak("Посмотри.", composer);
    expect(seen).toEqual([{ body: "typed first\nLook.", original: "Посмотри." }]);
  });

  it("VOP-FR-BYZC: a stored value that is no spoken language translates nothing", async () => {
    prefs.language = "Klingon 2";
    const { composer, seen } = recordingComposer();
    await speak("Look.", composer);
    expect(api.translateVoiceTranscript).not.toHaveBeenCalled();
    expect(seen).toEqual([{ body: "Look.", original: undefined }]);
    expect(getVoiceState().note).toBeNull();
  });

  it("VOM-FR-QWMK, VOM-FR-ABPU: a leave of voice mode during the translation posts nothing and keeps the words", async () => {
    prefs.language = "ru";
    let finish: (value: { text: string }) => void = () => undefined;
    api.translateVoiceTranscript.mockImplementationOnce(
      () => new Promise<{ text: string }>((resolve) => (finish = resolve)),
    );
    const { composer, seen } = recordingComposer();
    await speak("Посмотри.", composer);
    expect(getVoiceState().note).toBe("translating");
    leaveVoiceMode();
    finish({ text: "Look." });
    await settle();
    await settle();
    expect(seen).toEqual([]);
    expect(composer.post).not.toHaveBeenCalled();
    expect(getDiscussionSession(KEY).body).toBe("Посмотри.");
  });

  it("VOM-FR-GZCV: a chunk message goes to the speech turn as spoken, with no translation", async () => {
    prefs.language = "ru";
    await confirmed(handle({ targets: () => ["arch"], tagged: (t: string) => `@arch ${t}` }));
    noteChunk({ captureId: "c1", index: 0, state: "chunk_end", text: null, code: null });
    noteChunk({
      captureId: "c1",
      index: 0,
      state: "chunk_transcript",
      text: "Посмотри сюда.",
      code: null,
    });
    await settle();
    expect(api.dispatchSpeechTurn).toHaveBeenCalledWith(
      expect.objectContaining({ text: "@arch Посмотри сюда." }),
    );
    expect(api.translateVoiceTranscript).not.toHaveBeenCalled();
  });
});
