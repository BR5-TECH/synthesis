/**
 * What is spoken of an agent group (`VOM-voice-mode.md` VOM-FR-QMBN,
 * VOM-FR-RKOG).
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

import { noteGroupDispatched } from "../state/groupSpeech";
import type { AgentGroup, AgentTurn, Comment } from "../types";
import {
  KEY,
  comment,
  discussion,
  handle,
  settle,
  useVadBench,
} from "../test/voiceVadKit";
import { enterVoiceMode, noteDiscussionChanged, registerVoiceComposer, setResponseMode } from "./voiceController";
import { patchVoiceState } from "../state/voiceSession";

useVadBench();

function reply(id: string, agentId: string, text: string, synthesis = false): Comment {
  return {
    ...comment(id, "agent", text),
    author: { kind: "agent", agentId, handle: agentId },
    attachments: synthesis ? [{ kind: "synthesis" }] : [],
  } as Comment;
}

function turn(agentId: string, role: "participant" | "meta" | "synthesis"): AgentTurn {
  return {
    id: `t-${agentId}`,
    agentId,
    nickname: agentId,
    origin: { discussionId: KEY, target: { kind: "artifact", artifactId: "a.md" } },
    state: "running",
    group: { groupId: "g1", role },
    activeToolCalls: [],
  } as unknown as AgentTurn;
}

function group(mode: AgentGroup["mode"], agents: string[], driver: string | null): AgentGroup {
  return {
    mode,
    groupId: "g1",
    driver,
    turns: agents.map((a) => turn(a, "participant")),
    refusals: [],
  };
}

function spoken(): string[] {
  return (api.speakVoiceSegment.mock.calls as unknown as unknown[][]).map((c) => c[1] as string);
}

function voiceOn() {
  enterVoiceMode(KEY, discussion([]));
  patchVoiceState({ providerReady: true, providerSpeaks: true });
  registerVoiceComposer(KEY, handle());
  setResponseMode("voice");
}

describe("a group with a Synthesis", () => {
  it("VOM-FR-QMBN: the replies of the tagged agents are not spoken, and the Synthesis is", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("synthesis", ["a1", "a2"], "a2"));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Reply one.")]));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Reply one."), reply("r2", "a2", "Reply two.")]));
    await settle();
    expect(spoken()).toEqual([]);
    noteDiscussionChanged(
      discussion([
        reply("r1", "a1", "Reply one."),
        reply("r2", "a2", "Reply two."),
        reply("r3", "a2", "The Synthesis.", true),
      ]),
    );
    await settle();
    expect(spoken()).toEqual(["The Synthesis."]);
  });

  it("VOM-FR-QMBN: the driver's own reply as a tagged agent is not spoken either", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("synthesis", ["a1", "a2"], "a1"));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Driver reply.")]));
    await settle();
    expect(spoken()).toEqual([]);
  });

  it("VOM-FR-QMBN: the voice of the Synthesis is the voice of the driver", async () => {
    const composer = handle({ voiceOf: (agentId: string) => (agentId === "a2" ? "marin" : "cedar") });
    enterVoiceMode(KEY, discussion([]));
    patchVoiceState({ providerReady: true, providerSpeaks: true });
    registerVoiceComposer(KEY, composer);
    setResponseMode("voice");
    noteGroupDispatched(KEY, "u1", group("synthesis", ["a1", "a2"], "a2"));
    noteDiscussionChanged(discussion([reply("r3", "a2", "The Synthesis.", true)]));
    await settle();
    const call = api.speakVoiceSegment.mock.calls[0] as unknown as unknown[];
    expect(call[2]).toBe("marin");
  });

  it("VOM-FR-QMBN: a later comment of the author ends the mute", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("synthesis", ["a1", "a2"], "a2"));
    noteDiscussionChanged(discussion([comment("u2", "human", "Thanks, one more thing.")]));
    noteDiscussionChanged(
      discussion([comment("u2", "human", "Thanks, one more thing."), reply("r1", "a1", "Alone now.")]),
    );
    await settle();
    expect(spoken()).toEqual(["Alone now."]);
  });

  it("VOM-FR-QMBN: the comment that the group answers does not end the mute", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("synthesis", ["a1", "a2"], "a2"));
    noteDiscussionChanged(discussion([comment("u1", "human", "Both of you?")]));
    noteDiscussionChanged(
      discussion([comment("u1", "human", "Both of you?"), reply("r1", "a1", "Reply one.")]),
    );
    await settle();
    expect(spoken()).toEqual([]);
  });

  it("VOM-FR-QMBN: another group dispatch ends the mute of the earlier group", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("synthesis", ["a1", "a2"], "a2"));
    noteGroupDispatched(KEY, "u2", group("regular", ["a1"], null));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Plain reply.")]));
    await settle();
    expect(spoken()).toEqual(["Plain reply."]);
  });
});

describe("a group with no Synthesis", () => {
  it("VOM-FR-QMBN, VOM-FR-RKOG: when the classifier fails the group is regular, each reply is spoken, and the newest wins", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("regular", ["a1", "a2"], null));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Reply one.")]));
    await settle();
    noteDiscussionChanged(discussion([reply("r1", "a1", "Reply one."), reply("r2", "a2", "Reply two.")]));
    await settle();
    expect(spoken()).toEqual(["Reply one.", "Reply two."]);
    expect(api.cancelVoiceSpeech).toHaveBeenCalled();
  });

  it("VOM-FR-QMBN: one tagged agent in advisor mode speaks the reply of Meta", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("advisor", ["a1"], "a1"));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Meta reply.")]));
    await settle();
    expect(spoken()).toEqual(["Meta reply."]);
  });

  it("VOM-FR-QMBN: a single agent with no other ready agent speaks as it did", async () => {
    voiceOn();
    noteGroupDispatched(KEY, "u1", group("regular", ["a1"], null));
    noteDiscussionChanged(discussion([reply("r1", "a1", "Alone.")]));
    await settle();
    expect(spoken()).toEqual(["Alone."]);
  });
});
