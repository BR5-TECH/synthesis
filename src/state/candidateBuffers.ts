/**
 * The **candidate buffer** of a proposed change
 * (`DCR-draft-change-review.md` DCR-FR-25, DCR-FR-26).
 *
 * A candidate is the text the review surface edits, and it is the *proposal's*
 * material rather than the surface's: it survives the review being left and
 * re-entered, the New Artifact tab being closed and the draft reopened, and the
 * tab being backgrounded and refocused, so no route back into a review restores
 * the agent's original text over what the author wrote. That is why the buffer
 * lives in a module-level store rather than in a component's own state, which
 * the first unmount would throw away.
 *
 * ## What a buffer is keyed by
 *
 * The key is opaque to the store, which is what lets the two flavours key by
 * different things. A draft's proposal is an ordered list of changes and each
 * one is edited on its own (DCP-FR-XDRV), so its buffers are keyed by proposal
 * **and change** through `hunkKey`. A prompt artifact's proposal is still one
 * whole candidate (PCP-FR-29), so its buffers are keyed by proposal alone. The
 * store never takes a key apart; the writer does.
 *
 * ## Why this is not the editing-session store
 *
 * A Diff tab's target *is* the artifact's editing session (DFV-FR-42): editing
 * it is editing the file, and the write lands in the working tree. A candidate
 * is the opposite promise. Editing it is **never** editing the draft — the write
 * this store performs touches only `<proposal-id>.content`, creates, modifies
 * and deletes no file under the draft's `files/`, refreshes no `updated_at`, and
 * emits no `"drafts changed"` (per `../../specifications/core/DCP-draft-change-proposals.md`
 * DCP-FR-25). Acceptance is the only operation that lands it, and that has not
 * changed. Sharing the artifact store would have made "saved" mean "written into
 * the draft", which is exactly the confusion DCR-FR-27 exists to prevent.
 */
import {
  editDraftChangeHunk,
  savePromptChangeProposalCandidate,
} from "../api";
import {
  createHistory,
  recordEdit,
  redoStep,
  undoStep,
  type EditHistory,
} from "./editHistory";
import { logDebug, logError } from "../logging";
import { PROMPT_PROPOSAL_ERRORS, PROPOSAL_ERRORS } from "../types";
import { AUTOSAVE_DELAY_MS } from "./writeSchedule";

/**
 * DCR-FR-29: neither revision is ever replaced under an unresolved edit, and
 * the two things that can move are told apart because their resolutions differ.
 */
export type CandidateConflict =
  /**
   * The draft file the candidate is read against has been rewritten. Resolved
   * by re-reading the base and re-deriving the comparison against it — keeping
   * the candidate — or by leaving the review and coming back to it.
   */
  | { kind: "base" }
  /**
   * The write refused `candidate_stale`: storage holds a text this review has
   * not seen (DCP-FR-27). Resolved by keeping what is on screen or by taking
   * what storage holds; neither is chosen for the author.
   */
  | { kind: "candidate" };

/**
 * What separates a proposal id from a change id inside a key.
 *
 * A character neither id can hold, so the two parts are always recoverable.
 */
const KEY_SEPARATOR = "\u0000";

/** The key one change of one draft proposal is buffered under. */
export function hunkKey(proposalId: string, hunkId: string): string {
  return `${proposalId}${KEY_SEPARATOR}${hunkId}`;
}

/** The proposal and change a key names. */
export function parseHunkKey(key: string): {
  proposalId: string;
  hunkId: string;
} {
  const at = key.indexOf(KEY_SEPARATOR);
  if (at < 0) return { proposalId: key, hunkId: "" };
  return { proposalId: key.slice(0, at), hunkId: key.slice(at + 1) };
}

export interface CandidateBuffer {
  key: string;
  /** The candidate as the author currently has it. */
  text: string;
  /** The checksum of what storage holds, which the next write is checked against. */
  baseline: string;
  /**
   * The text the agent composed, so DCR-FR-27's standing line can say whether
   * the candidate has been edited rather than whether it has been written.
   */
  origin: string;
  /** An edit that has not reached proposal storage yet. */
  dirty: boolean;
  /** A write is in flight, which is one of the things that disables a decision. */
  saving: boolean;
  /** DCR-FR-26: the typed error a failed write reported. Never cleared silently. */
  error: string | null;
  conflict: CandidateConflict | null;
  /**
   * DCR-FR-24: the candidate's own undo/redo history.
   *
   * A candidate is not an artifact, so it has no editing session to borrow one
   * from — but DCR-FR-24 says undo and redo behave here as they do in a Diff
   * tab, and a surface that lets an author rewrite a document without letting
   * them take it back is not the same surface at all. Its floor is the text the
   * review opened on, so undo can never reach a state the author did not author.
   */
  history: EditHistory;
}

function newBuffer(
  key: string,
  text: string,
  baseline: string,
): CandidateBuffer {
  return {
    key,
    text,
    baseline,
    origin: text,
    dirty: false,
    saving: false,
    error: null,
    conflict: null,
    history: createHistory(text),
  };
}

/**
 * How a store writes a candidate through, and what a stale refusal reads as.
 *
 * A parameter rather than a hardcoded call because the two proposal modules
 * share no command (`DCP-draft-change-proposals.md` DCP-FR-25,
 * `PCP-prompt-change-proposals.md` PCP-FR-29): one change of a draft's proposal
 * is written by `edit_draft_change_hunk` and a prompt artifact's whole
 * candidate by `save_prompt_change_proposal_candidate`, and a store that
 * guessed would write one proposal's text into the other's store.
 */
export interface CandidateWriter {
  /**
   * `key` is whatever the flavour keys by. The writer is the one thing that
   * knows its shape, and takes it apart to address the right command.
   */
  save: (
    key: string,
    content: string,
    baselineChecksum: string,
  ) => Promise<{ checksum: string }>;
  /** The typed refusal that means storage has moved on since this read it. */
  staleError: string;
}

export class CandidateBufferStore {
  private buffers = new Map<string, CandidateBuffer>();
  private listeners = new Set<() => void>();
  private timers = new Map<string, ReturnType<typeof setTimeout>>();
  /** The tail of each candidate's write queue, so two writes never overlap. */
  private chains = new Map<string, Promise<void>>();
  private version = 0;

  /** How long the candidate rests before it writes itself (DCR-FR-26). */
  readonly delay = AUTOSAVE_DELAY_MS;

  constructor(private readonly writer: CandidateWriter) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  getVersion = (): number => this.version;

  private notify(): void {
    this.version += 1;
    this.listeners.forEach((l) => l());
  }

  get(key: string): CandidateBuffer | undefined {
    return this.buffers.get(key);
  }

  /**
   * DCR-FR-08 / DCR-FR-25: adopt what `"load draft change proposal content"`
   * returned as this proposal's candidate — **unless a buffer for it is already
   * held**, in which case the buffer is what the review renders and the load is
   * only telling us what storage holds.
   *
   * The distinction is the whole of DCR-FR-25: a modal reopened after a
   * dismissal must show what the author last wrote in it rather than what the
   * agent first composed, and a load that overwrote the buffer would put the
   * agent's text back every time the review was reopened.
   */
  adopt(key: string, content: string, checksum: string): CandidateBuffer {
    const held = this.buffers.get(key);
    if (held) {
      // Storage may have moved on while this review was closed. A buffer with no
      // edits in it simply takes what storage holds; one with edits keeps them
      // and raises the conflict of DCR-FR-29 rather than silently discarding
      // either side.
      if (held.baseline !== checksum) {
        if (held.dirty || held.text !== held.origin) {
          held.conflict = { kind: "candidate" };
        } else {
          held.text = content;
          held.baseline = checksum;
          held.origin = content;
          held.history = createHistory(content);
        }
        this.notify();
      }
      return held;
    }
    const buffer = newBuffer(key, content, checksum);
    this.buffers.set(key, buffer);
    this.notify();
    return buffer;
  }

  /**
   * DCR-FR-24 / DCR-FR-26: the author edited the candidate. Mark it dirty and
   * (re)start the rest, so a burst of typing is one write.
   */
  edit(key: string, text: string): void {
    const buffer = this.buffers.get(key);
    if (!buffer || buffer.text === text) return;
    buffer.text = text;
    // "source" because that is what the candidate is edited as — the literal
    // text of a document — whichever rendering the review is showing it in.
    recordEdit(buffer.history, text, "text", "source");
    buffer.dirty = true;
    // A previous failure describes bytes the author has since changed, so it is
    // no longer the reason a decision is blocked — the write about to be
    // scheduled is. Left standing, it would disable both decisions for good
    // (DCR-FR-11) with no way to clear it.
    buffer.error = null;
    this.notify();
    this.schedule(key);
  }

  private schedule(key: string): void {
    const existing = this.timers.get(key);
    if (existing) clearTimeout(existing);
    this.timers.set(
      key,
      setTimeout(() => {
        this.timers.delete(key);
        void this.flush(key);
      }, this.delay),
    );
  }

  /** Whether a candidate holds a write that has not landed. */
  hasPendingWrite(key: string): boolean {
    const buffer = this.buffers.get(key);
    return this.timers.has(key) || buffer?.dirty === true;
  }

  /**
   * DCR-FR-24: traverse the candidate's own history, on the terms a Diff tab
   * traverses the artifact's (DFV-FR-50). A traversal is an edit like any other
   * — it leaves the candidate unwritten and schedules the write that lands it.
   */
  traverse(key: string, direction: "undo" | "redo"): void {
    const buffer = this.buffers.get(key);
    if (!buffer || buffer.conflict) return;
    const move =
      direction === "undo"
        ? undoStep(buffer.history)
        : redoStep(buffer.history);
    if (!move || move.doc === buffer.text) return;
    buffer.text = move.doc;
    buffer.dirty = true;
    buffer.error = null;
    this.notify();
    this.schedule(key);
  }

  /** DCR-FR-27: the candidate differs from the text the agent composed. */
  isEdited(key: string): boolean {
    const buffer = this.buffers.get(key);
    return buffer != null && buffer.text !== buffer.origin;
  }

  /**
   * DCR-FR-26: write the candidate now rather than waiting the rest out.
   *
   * Brought forward by a dismissal (DCR-FR-23), by either decision (DCR-FR-12,
   * DCR-FR-13), and by the New Artifact tab closing, so an edit is never lost to
   * any of the three. Resolves to whether the candidate is safe to decide on.
   */
  async flush(key: string): Promise<boolean> {
    const timer = this.timers.get(key);
    if (timer) {
      clearTimeout(timer);
      this.timers.delete(key);
    }
    const buffer = this.buffers.get(key);
    if (!buffer) return true;
    // An unresolved conflict outranks everything: writing over it would decide
    // for the author which of the two candidates survives (DCR-FR-29).
    if (buffer.conflict) return false;
    if (!buffer.dirty) return buffer.error == null;

    // One write of a candidate at a time. Queue behind whatever is in flight and
    // only then read the buffer, so a write that arrives during another carries
    // what the candidate holds *after* it rather than racing it to storage with
    // an older text and an older baseline.
    const ahead = this.chains.get(key) ?? Promise.resolve();
    let done!: () => void;
    this.chains.set(
      key,
      new Promise<void>((resolve) => {
        done = resolve;
      }),
    );
    try {
      await ahead;
      return await this.write(buffer);
    } finally {
      done();
    }
  }

  private async write(buffer: CandidateBuffer): Promise<boolean> {
    if (!buffer.dirty || buffer.conflict) return !buffer.conflict;
    const text = buffer.text;
    buffer.saving = true;
    this.notify();
    try {
      const { checksum } = await this.writer.save(
        buffer.key,
        text,
        buffer.baseline,
      );
      buffer.baseline = checksum;
      buffer.saving = false;
      // Only the bytes that were written are clean. An edit made while the write
      // was in flight is still outstanding, and clearing the flag for it would
      // leave it unwritten with nothing scheduled to write it.
      if (buffer.text === text) buffer.dirty = false;
      else this.schedule(buffer.key);
      buffer.error = null;
      this.notify();
      // The count is the shape of the write; the author's rewrite of an agent's
      // text is content and appears nowhere.
      logDebug(["frontend"], "candidate written to proposal storage", {
        key: buffer.key,
        bytes: text.length,
      });
      return buffer.text === text;
    } catch (e) {
      buffer.saving = false;
      const reason = String(e);
      // DCP-FR-27: storage holds a candidate this review has not seen. Neither
      // side is chosen for the author (DCR-FR-29).
      if (reason.includes(this.writer.staleError)) {
        buffer.conflict = { kind: "candidate" };
      } else {
        // DCR-FR-26: every edit stays in the buffer, the error is rendered
        // inline, and nothing is retried on a timer — the next edit and the next
        // attempt to decide are what retry it.
        buffer.error = reason;
      }
      this.notify();
      logError(["frontend"], "a candidate could not be written", {
        key: buffer.key,
        reason,
      });
      return false;
    }
  }

  /**
   * DCR-FR-29: the author chose which candidate survives.
   *
   * `keep` writes what is on screen over whatever storage holds, taking the
   * stored checksum as the new baseline so the write is not refused again;
   * `take` adopts storage's candidate, discarding the author's edits with the
   * conflict.
   */
  resolveCandidate(
    key: string,
    resolution: "keep" | "take",
    stored: { content: string; checksum: string },
  ): void {
    const buffer = this.buffers.get(key);
    if (!buffer) return;
    buffer.conflict = null;
    buffer.baseline = stored.checksum;
    if (resolution === "take") {
      buffer.text = stored.content;
      buffer.origin = stored.content;
      buffer.dirty = false;
      buffer.error = null;
      // The floor the author's history was measured against is gone with their
      // text. A history left standing over it would let one undo quietly
      // reinstate the candidate they chose to discard — as an unwritten edit,
      // which the next rest would then write back over the one they took.
      buffer.history = createHistory(stored.content);
      this.notify();
      return;
    }
    buffer.dirty = true;
    this.notify();
    void this.flush(key);
  }

  /** DCR-FR-29: the base moved, and the author has said what to do about it. */
  resolveBase(key: string): void {
    const buffer = this.buffers.get(key);
    if (!buffer?.conflict) return;
    buffer.conflict = null;
    this.notify();
  }

  /** DCR-FR-29: the draft file this candidate is read against has been rewritten. */
  raiseBaseConflict(key: string): void {
    const buffer = this.buffers.get(key);
    // Only an edited candidate has anything to protect: an untouched one is
    // re-read with the base when the modal next opens (DCR-FR-08).
    if (!buffer || buffer.text === buffer.origin) return;
    if (buffer.conflict) return;
    buffer.conflict = { kind: "base" };
    this.notify();
  }

  /**
   * DCR-FR-25: the buffer is dropped when the proposal is decided, when the
   * draft is deleted or graduated, when the project closes, and when the active
   * worktree changes — never merely because a surface went away.
   */
  drop(key: string): void {
    const timer = this.timers.get(key);
    if (timer) clearTimeout(timer);
    this.timers.delete(key);
    this.chains.delete(key);
    if (this.buffers.delete(key)) this.notify();
  }

  /** Every candidate this store holds, for a teardown that must write them. */
  pendingKeys(): string[] {
    const keys: string[] = [];
    this.buffers.forEach((buffer, key) => {
      if (buffer.dirty) keys.push(key);
    });
    return keys;
  }

  /** Every key this store holds whose id part is `proposalId`. */
  keysOfProposal(proposalId: string): string[] {
    const prefix = `${proposalId}${KEY_SEPARATOR}`;
    const keys: string[] = [];
    this.buffers.forEach((_buffer, key) => {
      if (key === proposalId || key.startsWith(prefix)) keys.push(key);
    });
    return keys;
  }

  /** Drop every candidate of one proposal, however its keys are shaped. */
  dropProposal(proposalId: string): void {
    for (const key of this.keysOfProposal(proposalId)) this.drop(key);
  }

  clear(): void {
    this.timers.forEach((timer) => clearTimeout(timer));
    this.timers.clear();
    this.chains.clear();
    this.buffers.clear();
    this.notify();
  }
}

/**
 * The application's one candidate store for **one change at a time** of a
 * draft's proposal (DCP-FR-XDRV, DCR-FR-26).
 *
 * The same store class as above, keyed by proposal and change together and
 * writing through `edit_draft_change_hunk`. Module-level for the reason
 * DCR-FR-25 gives: the promise is about outliving surfaces, and the review, its
 * tab, and the draft's place in the strip all come and go around it.
 */
export const hunkCandidateBuffers = new CandidateBufferStore({
  save: (key, content, baseline) => {
    const { proposalId, hunkId } = parseHunkKey(key);
    return editDraftChangeHunk(proposalId, hunkId, content, baseline);
  },
  staleError: PROPOSAL_ERRORS.candidateStale,
});

/**
 * The application's one candidate store for changes proposed to **prompt
 * artifacts** (`PCR-prompt-change-review.md` PCR-FR-21).
 *
 * Separate from the draft store above rather than keyed alongside it: the two
 * modules share no command, no folder, and no event (PCP-FR-29), and a proposal
 * id from one store is not a proposal id in the other. Module-level for the same
 * reason the draft store is — the promise PCR-FR-21 makes is about outliving
 * surfaces: the modal, its Editor tab, and the tab's place in the strip all come
 * and go around it.
 */
export const promptCandidateBuffers = new CandidateBufferStore({
  save: savePromptChangeProposalCandidate,
  staleError: PROMPT_PROPOSAL_ERRORS.candidateStale,
});
