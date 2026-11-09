/**
 * The filler of a long turn (`VOM-voice-mode.md` VOM-FR-YGAR, VOM-FR-VTNW,
 * VOM-FR-LKXN, VOM-FR-POBZ, VOM-FR-VQGI, VOM-FR-UFBT, VOM-FR-UCQN).
 *
 * The end of an utterance arms one filler. It is due at the first tool call that
 * starts after that end, or 10 s after it, while a turn of the voice discussion
 * runs. It speaks when the utterance posted and nothing else sounds. A reply, a
 * new utterance, and Stop cancel it.
 */
import { logDebug } from "../logging";
import { getVoiceState } from "../state/voiceSession";
import type { AgentTurn } from "../types";
import { fillerPhrase } from "./voiceFillerPhrases";
import { speakAs } from "./voiceSpeech";
import { hasUnsettledUtterance } from "./voiceUtterances";

/** VOM-FR-VTNW. */
export const FILLER_DELAY_MS = 10_000;

/** What the filler needs of a turn. */
interface KnownTurn {
  agentId: string;
  discussionId: string;
  language: string | null;
  /** The highest activation sequence of its tool calls, or 0. */
  seq: number;
  /** The order of registration, so the newest running turn is known. */
  order: number;
}

/** The filler of one utterance. */
interface Armed {
  captureId: string;
  /** VOM-FR-VTNW: the tool calls of each turn at the end of the utterance. */
  baseline: Map<string, number>;
  timer: ReturnType<typeof setTimeout> | null;
  /** The turn whose tool call made the filler due, or the timer. */
  due: string | "timer" | null;
  /** VOM-FR-LKXN: the utterance settled with a comment or a spoken answer. */
  posted: boolean;
}

const running = new Map<string, KnownTurn>();
let turnOrder = 0;
let armed: Armed | null = null;

function highestSeq(turn: AgentTurn): number {
  const calls = [...(turn.toolCalls ?? []), ...turn.activeToolCalls];
  return calls.reduce((max, call) => Math.max(max, call.activationSeq), 0);
}

/** VOM-FR-POBZ: the filler that did not speak goes. */
export function cancelFiller(reason: string): void {
  if (armed === null) return;
  if (armed.timer !== null) clearTimeout(armed.timer);
  armed = null;
  logDebug(["frontend", "ai"], "a voice filler was cancelled", { reason });
}

/** VOM-FR-YGAR: the end of an utterance arms one filler. */
export function armFiller(captureId: string): void {
  cancelFiller("new utterance end");
  const { responseMode, activeKey } = getVoiceState();
  if (responseMode !== "voice" || activeKey === null) return;
  const baseline = new Map([...running].map(([id, turn]) => [id, turn.seq]));
  const filler: Armed = { captureId, baseline, timer: null, due: null, posted: false };
  filler.timer = setTimeout(() => {
    if (armed !== filler) return;
    filler.timer = null;
    filler.due ??= "timer";
    tryFiller();
  }, FILLER_DELAY_MS);
  armed = filler;
}

/** VOM-FR-LKXN: the utterance of the filler settled. */
export function noteFillerSettled(captureId: string, posted: boolean): void {
  if (armed === null || armed.captureId !== captureId) return;
  if (!posted) {
    cancelFiller("nothing posted");
    return;
  }
  armed.posted = true;
}

/** VOM-FR-VTNW: a turn changed. A tool call after the end of the utterance makes the filler due. */
export function noteFillerTurn(turn: AgentTurn): void {
  if (turn.state !== "running") {
    running.delete(turn.id);
    return;
  }
  const seq = highestSeq(turn);
  const known = running.get(turn.id);
  turnOrder += 1;
  running.set(turn.id, {
    agentId: turn.agentId,
    discussionId: turn.origin.discussionId,
    language: turn.spokenLanguage ?? null,
    seq,
    order: known?.order ?? turnOrder,
  });
  if (armed === null || armed.due !== null) return;
  if (turn.origin.discussionId !== getVoiceState().discussionId) return;
  if (seq > (armed.baseline.get(turn.id) ?? 0)) {
    armed.due = turn.id;
    tryFiller();
  }
}

/** The turn that the filler speaks for: the due turn, or the newest that runs. */
function fillerTurn(filler: Armed, discussionId: string): KnownTurn | null {
  const due = filler.due !== "timer" && filler.due !== null ? running.get(filler.due) : undefined;
  if (due && due.discussionId === discussionId) return due;
  const turns = [...running.values()].filter((t) => t.discussionId === discussionId);
  return turns.sort((a, b) => b.order - a.order)[0] ?? null;
}

/**
 * VOM-FR-LKXN, VOM-FR-VQGI, VOM-FR-UFBT: a due filler speaks when its utterance
 * posted, no author speech or utterance is open, nothing plays, and a turn of
 * the voice discussion runs.
 */
export function tryFiller(): void {
  const filler = armed;
  if (filler === null || filler.due === null || !filler.posted) return;
  const state = getVoiceState();
  if (state.activeKey === null || state.discussionId === null || state.responseMode !== "voice") {
    cancelFiller("voice off");
    return;
  }
  if (state.hearing || hasUnsettledUtterance() || state.playing || state.playbackPaused) return;
  const turn = fillerTurn(filler, state.discussionId);
  if (turn === null) {
    cancelFiller("no running turn");
    return;
  }
  if (filler.timer !== null) clearTimeout(filler.timer);
  armed = null;
  logDebug(["frontend", "ai"], "a voice filler speaks", {
    cause: filler.due === "timer" ? "timer" : "tool call",
  });
  void speakAs({ text: fillerPhrase(turn.language), agentId: turn.agentId, filler: true }, state.activeKey);
}

export function resetVoiceFillerForTest(): void {
  if (armed?.timer) clearTimeout(armed.timer);
  armed = null;
  running.clear();
  turnOrder = 0;
}
