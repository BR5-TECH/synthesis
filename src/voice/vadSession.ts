/**
 * One open VAD session, on the WebView side (`VOM-voice-mode.md` VOM-FR-MMAG): the
 * microphone, the feed to the backend, and the gate. The backend finds the
 * utterances, and `voiceController.ts` acts on them.
 */
import {
  closeVoiceActivitySession,
  feedVoiceActivity,
  openVoiceActivitySession,
  setVoiceActivityGate,
} from "../api";
import { logWarn } from "../logging";
import { ChunkPump, FrameBatcher, openMicrophone, type Microphone } from "./capture";
import { errorCode } from "./voiceMessages";

/** What the gate reports (VOP-FR-QZCE). */
export interface VadGate {
  blocked: boolean;
  playing: boolean;
  /** VOP-FR-ERDS: the next VAD capture opens chunked. */
  chunked: boolean;
  keyterms: string[];
}

export interface VadSource {
  gate(): VadGate;
  /** VOM-FR-MUZB: false while muted, so silence goes to the session in place of the audio. */
  feeds?(): boolean;
  /** VOM-FR-WFHN: each block of microphone audio, for the waveform. */
  heard?(pcm: ArrayBuffer): void;
  /** VOM-FR-CSXI: the microphone went, or the backend refused the feed. */
  lost(code: string): void;
}

export class VadSession {
  private seq = 0;
  private sent = "";
  private closed = false;
  private micStopped = false;
  private loss: string | null = null;

  private constructor(
    readonly activityId: string,
    private readonly mic: Microphone,
    private readonly batcher: FrameBatcher,
    private readonly pump: ChunkPump,
    private readonly source: VadSource,
  ) {}

  /** A refused microphone rejects with `MicrophoneError`, and all else with a code. */
  static async open(keyterms: readonly string[], source: VadSource): Promise<VadSession> {
    let session: VadSession | null = null;
    let lostEarly: string | null = null;
    const lost = (code: string) => {
      if (session === null) lostEarly = code;
      else if (!session.closed) source.lost(code);
    };
    const pump = new ChunkPump((activityId, bytes) =>
      feedVoiceActivity(activityId, bytes).catch((error: unknown) => lost(errorCode(error))),
    );
    const batcher = new FrameBatcher((pcm) => pump.push(pcm));
    const mic = await openMicrophone(
      (pcm) => {
        source.heard?.(pcm);
        // VOP-FR-KUGV: a session with no feed ends, so a Mute feeds silence.
        batcher.push((source.feeds?.() ?? true) ? pcm : new ArrayBuffer(pcm.byteLength));
      },
      () => lost("microphone_unavailable"),
    );
    batcher.setRate(mic.rate);
    try {
      const { activityId } = await openVoiceActivitySession(mic.rate, keyterms);
      session = new VadSession(activityId, mic, batcher, pump, source);
      pump.attach(activityId);
    } catch (error) {
      mic.stop();
      throw error;
    }
    session.loss = lostEarly;
    return session;
  }

  /**
   * A loss before `open` returned. Nobody holds the session then, so a report at
   * that time would find nothing to close.
   */
  takeLoss(): string | null {
    const loss = this.loss;
    this.loss = null;
    return loss;
  }

  /** VOP-FR-QZCE: a push goes only when the record changed. */
  pushGate(): void {
    if (this.closed) return;
    const gate = this.source.gate();
    const record = JSON.stringify(gate);
    if (record === this.sent) return;
    this.sent = record;
    this.seq += 1;
    setVoiceActivityGate(this.activityId, { ...gate, seq: this.seq }).catch((error: unknown) => {
      logWarn(["frontend", "ai"], "a voice activity gate was refused", {
        code: errorCode(error),
      });
    });
  }

  /** The audio of the session stops here. The rest waits in the batcher. */
  stopMicrophone(): void {
    if (this.micStopped) return;
    this.micStopped = true;
    this.mic.stop();
    this.batcher.flush();
  }

  /** VOP-FR-FAAB: gives the id of a capture that has not settled. */
  async close(discard: boolean): Promise<string | null> {
    this.closed = true;
    await this.pump.flush();
    const { captureId } = await closeVoiceActivitySession(this.activityId, discard);
    return captureId;
  }
}
