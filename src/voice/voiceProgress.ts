/**
 * The progress updates of a long turn (`VOM-voice-mode.md` VOM-FR-JTWB,
 * VOM-FR-NKCG, VOM-FR-ZAHM, VOM-FR-RUDX, VOM-FR-LQYS, VOM-FR-BGXE).
 *
 * The end of an utterance starts one stream. An update is due 30 s later and
 * every 30 s after that, while a turn of the voice discussion runs. The text
 * comes from one model call on the active tool calls, and one agent speaks it.
 */
import { writeVoiceProgressUpdate, type ProgressActivity } from "../api";
import { logDebug, logWarn } from "../logging";
import { driverOf } from "../state/groupSpeech";
import { getVoiceState } from "../state/voiceSession";
import type { AgentTurn, GroupRole } from "../types";
import { speakAs } from "./voiceSpeech";
import { errorCode } from "./voiceMessages";
import { hasUnsettledUtterance } from "./voiceUtterances";

/** VOM-FR-JTWB: the fixed interval of the updates. */
export const PROGRESS_INTERVAL_MS = 30_000;

/** What a stream needs of a running turn. */
interface KnownTurn {
  agentId: string;
  nickname: string;
  discussionId: string;
  language: string | null;
  role: GroupRole | null;
  /** The group of the turn, if it has one. */
  groupId: string | null;
  /** The order of registration, so the oldest running turn is known. */
  order: number;
  activity: ProgressActivity[];
}

/** The stream of one utterance. */
interface Stream {
  captureId: string;
  timer: ReturnType<typeof setTimeout> | null;
  /** VOM-FR-LKXN: the utterance settled with a comment or a spoken answer. */
  posted: boolean;
  /** The utterance has settled. */
  settled: boolean;
  /** VOM-FR-RUDX: an update is due and waits for the author or an utterance. */
  waiting: boolean;
  /** The text of an update is on its way. */
  writing: boolean;
  /** VOM-FR-ZAHM: the agent that speaks this stream, picked at the first update. */
  agentId: string | null;
}

const running = new Map<string, KnownTurn>();
/** The agent of each nickname that a turn named, so a driver finds its voice. */
const agents = new Map<string, string>();
let turnOrder = 0;
let stream: Stream | null = null;
/**
 * A `synthesis` group of the voice discussion that has not started its
 * synthesis turn yet. The turns of the group end one by one, so the stream
 * waits for the synthesis turn before it stops (VOM-FR-LQYS).
 */
let synthesisPending = false;

/** VOM-FR-LQYS: the stream of the utterance goes. */
export function cancelProgress(reason: string): void {
  if (stream === null) return;
  if (stream.timer !== null) clearTimeout(stream.timer);
  stream = null;
  synthesisPending = false;
  logDebug(["frontend", "ai"], "a voice progress stream was cancelled", { reason });
}

function schedule(s: Stream): void {
  s.timer = setTimeout(() => {
    if (stream !== s) return;
    s.timer = null;
    // The next update is due 30 s after this one, whether this one speaks or not.
    schedule(s);
    due(s);
  }, PROGRESS_INTERVAL_MS);
}

/** VOM-FR-JTWB: the end of an utterance starts its stream. */
export function armProgress(captureId: string): void {
  cancelProgress("new utterance end");
  const { responseMode, activeKey, discussionId } = getVoiceState();
  // A group of the discussion that already runs has not started its synthesis turn.
  synthesisPending = [...running.values()].some(
    (t) => t.discussionId === discussionId && t.role === "participant",
  );
  if (responseMode !== "voice" || activeKey === null) return;
  const s: Stream = {
    captureId,
    timer: null,
    posted: false,
    settled: false,
    waiting: false,
    writing: false,
    agentId: null,
  };
  stream = s;
  schedule(s);
}

/** VOM-FR-JTWB, VOM-FR-LKXN: the utterance of the stream settled. */
export function noteProgressSettled(captureId: string, posted: boolean): void {
  if (stream === null || stream.captureId !== captureId) return;
  if (!posted) {
    cancelProgress("nothing posted");
    return;
  }
  stream.posted = true;
  stream.settled = true;
}

/** The running turns of the voice discussion. */
function turnsOfVoice(discussionId: string): KnownTurn[] {
  return [...running.values()].filter((t) => t.discussionId === discussionId);
}

/** VOM-FR-ZAHM: the agent that speaks the stream. */
function pickAgent(discussionId: string): string | null {
  const turns = turnsOfVoice(discussionId);
  const byRole = (role: GroupRole) => turns.find((t) => t.role === role)?.agentId ?? null;
  // VOM-FR-ZAHM: the driver counts only while a turn of its group still runs.
  const recorded = driverOf(discussionId);
  const driver =
    recorded !== null && turns.some((t) => t.groupId !== null && t.groupId === recorded.groupId)
      ? recorded.nickname
      : null;
  return (
    byRole("synthesis") ??
    byRole("meta") ??
    (driver !== null ? (agents.get(driver.toLowerCase()) ?? null) : null) ??
    [...turns].sort((a, b) => a.order - b.order)[0]?.agentId ??
    null
  );
}

/** VOM-FR-NKCG: the active tool calls of the running turns of the voice discussion. */
function activityOf(discussionId: string): ProgressActivity[] {
  return turnsOfVoice(discussionId).flatMap((t) => t.activity);
}

/** VOM-FR-RUDX: nothing is left that stops an update from speaking. */
function canSpeak(): boolean {
  const state = getVoiceState();
  return !(state.hearing || hasUnsettledUtterance() || state.playing || state.playbackPaused);
}

/** VOM-FR-JTWB, VOM-FR-RUDX: an update is due. */
function due(s: Stream): void {
  const state = getVoiceState();
  if (state.activeKey === null || state.discussionId === null || state.responseMode !== "voice") {
    cancelProgress("voice off");
    return;
  }
  if (turnsOfVoice(state.discussionId).length === 0) {
    cancelProgress("no running turn");
    return;
  }
  if (s.writing) {
    logDebug(["frontend", "ai"], "a voice progress update was dropped", { reason: "writing" });
    return;
  }
  // VOM-FR-LKXN: the author speaks, or an utterance has not settled.
  if (state.hearing || hasUnsettledUtterance() || !s.settled) {
    if (s.waiting) {
      logDebug(["frontend", "ai"], "a voice progress update was dropped", { reason: "waiting" });
    }
    s.waiting = true;
    return;
  }
  // VOM-FR-RUDX: a reply plays or is paused, so the update is dropped.
  if (state.playing || state.playbackPaused) {
    logDebug(["frontend", "ai"], "a voice progress update was dropped", { reason: "playing" });
    return;
  }
  void speakUpdate(s);
}

/** VOM-FR-RUDX: an update that waited speaks when nothing is left to settle. */
export function tryProgress(): void {
  const s = stream;
  if (s === null || !s.waiting || !s.settled || !s.posted) return;
  const state = getVoiceState();
  if (state.activeKey === null || state.discussionId === null) return;
  if (s.writing) return;
  const { hearing } = state;
  if (hearing || hasUnsettledUtterance()) return;
  s.waiting = false;
  // VOM-FR-RUDX: a reply plays or is paused, so the update is dropped.
  if (state.playing || state.playbackPaused) {
    logDebug(["frontend", "ai"], "a voice progress update was dropped", { reason: "playing" });
    return;
  }
  void speakUpdate(s);
}

/** VOM-FR-NKCG, VOM-FR-ZAHM, VOM-FR-BGXE: write the text, and speak it. */
async function speakUpdate(s: Stream): Promise<void> {
  const { discussionId } = getVoiceState();
  if (discussionId === null) return;
  s.agentId ??= pickAgent(discussionId);
  const speaker = [...running.values()].find((t) => t.agentId === s.agentId);
  const turns = turnsOfVoice(discussionId);
  const language = speaker?.language ?? turns.find((t) => t.language)?.language ?? "English";
  const activity = activityOf(discussionId);
  s.writing = true;
  let text: string;
  try {
    text = (await writeVoiceProgressUpdate(activity, language)).text.trim();
  } catch (error) {
    // VOM-FR-NKCG: a failed call speaks nothing and shows no error.
    logWarn(["frontend", "ai"], "a voice progress update could not be written", {
      code: errorCode(error),
    });
    if (stream === s) s.writing = false;
    return;
  }
  if (stream !== s) return;
  s.writing = false;
  const state = getVoiceState();
  // VOM-FR-RUDX: the rules apply again when the text arrives.
  if (
    text === "" ||
    state.activeKey === null ||
    state.discussionId !== discussionId ||
    state.responseMode !== "voice" ||
    turnsOfVoice(discussionId).length === 0 ||
    !canSpeak()
  ) {
    logDebug(["frontend", "ai"], "a voice progress update was dropped", { reason: "not ready" });
    return;
  }
  logDebug(["frontend", "ai"], "a voice progress update speaks", {
    words: text.split(/\s+/).length,
    activity: activity.length,
  });
  void speakAs({ text, agentId: s.agentId, filler: true }, state.activeKey);
}

/** VOM-FR-LQYS: a turn changed. The last turn to end stops the stream. */
export function noteProgressTurn(turn: AgentTurn): void {
  agents.set(turn.nickname.toLowerCase(), turn.agentId);
  const discussionId = turn.origin.discussionId;
  if (turn.state !== "running") {
    running.delete(turn.id);
    // A `synthesis` group ends when its synthesis turn ends.
    if (turn.group?.role === "synthesis") synthesisPending = false;
    if (
      stream !== null &&
      discussionId === getVoiceState().discussionId &&
      turnsOfVoice(discussionId).length === 0 &&
      !synthesisPending
    ) {
      cancelProgress("turns ended");
    }
    return;
  }
  if (turn.group?.role === "participant" && turn.group.groupId) synthesisPending = true;
  if (turn.group?.role === "synthesis") synthesisPending = false;
  const known = running.get(turn.id);
  turnOrder += 1;
  running.set(turn.id, {
    agentId: turn.agentId,
    nickname: turn.nickname,
    discussionId,
    language: turn.spokenLanguage ?? null,
    role: turn.group?.role ?? null,
    groupId: turn.group?.groupId ?? null,
    order: known?.order ?? turnOrder,
    activity: turn.activeToolCalls.map((call) => ({
      tool: call.tool,
      ...(call.advisor ? { advisor: call.advisor } : {}),
    })),
  });
}

export function resetVoiceProgressForTest(): void {
  if (stream?.timer) clearTimeout(stream.timer);
  stream = null;
  running.clear();
  agents.clear();
  turnOrder = 0;
  synthesisPending = false;
}
