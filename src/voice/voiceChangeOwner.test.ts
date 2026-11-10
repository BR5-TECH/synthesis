/**
 * VOM-FR-NTKT: voice mode on a change discussion ends when the stream leaves
 * the project's streams, or the change folder leaves its working copy.
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

import { getVoiceState } from "../state/voiceSession";
import { KEY, discussion, useVadBench } from "../test/voiceVadKit";
import { endVoiceForGoneStream } from "../hooks/shell/openspecChangeActions";
import { enterVoiceMode, noteOwnerDeleted, voiceOwner } from "./voiceController";

useVadBench();

const CHANGE = { kind: "change", streamId: "s-1", change: "add-logout" } as const;

describe("voice mode on a change discussion", () => {
  it("VOM-FR-NTKT: the deletion of its change ends voice mode, and the deletion of another change does not", () => {
    enterVoiceMode(KEY, discussion([]), CHANGE);
    expect(voiceOwner()).toEqual(CHANGE);
    noteOwnerDeleted({ kind: "change", streamId: "s-1", change: "other" });
    noteOwnerDeleted({ kind: "change", streamId: "s-2", change: "add-logout" });
    expect(getVoiceState().activeKey).toBe(KEY);
    noteOwnerDeleted(CHANGE);
    expect(getVoiceState().activeKey).toBeNull();
    expect(voiceOwner()).toBeNull();
  });

  it("VOM-FR-NTKT: the stream leaving the project's streams ends voice mode, and a live stream does not", () => {
    enterVoiceMode(KEY, discussion([]), CHANGE);
    endVoiceForGoneStream(new Set(["s-1", "s-2"]));
    expect(getVoiceState().activeKey).toBe(KEY);
    endVoiceForGoneStream(new Set(["s-2"]));
    expect(getVoiceState().activeKey).toBeNull();
  });

  it("VOM-FR-NTKT: a stream leaving does not end voice mode on a discussion of another owner", () => {
    enterVoiceMode(KEY, discussion([]), { kind: "draft", draftId: "d1" });
    endVoiceForGoneStream(new Set());
    expect(getVoiceState().activeKey).toBe(KEY);
  });
});
