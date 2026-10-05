import { onArtifactChangedExternally } from "../../events";
import { logDebug, logError } from "../../logging";
import { artifactTransport, type DocumentTransport } from "../documentTransport";
import type { ArtifactChangedPayload } from "../../types";
import { createHistory, hasEdits } from "../editHistory";
import { findStateIsPristine, type FindState } from "../findState";
import { detectIndentation, type Indentation } from "../indentation";
import { AUTOSAVE_DELAY_MS, WriteSchedule } from "../writeSchedule";
import { newSession } from "./session";
import type {
  EditSession,
  FlushResult,
  QuiesceFailure,
} from "./types";

// Re-exported so the Editor's collaborators can reach the rest without knowing
// where it is implemented.
export { AUTOSAVE_DELAY_MS };

export type {
  EditSession,
  FlushBlock,
  FlushResult,
  QuiesceFailure,
} from "./types";

export class EditSessionStore {
  /**
   * How this store's documents are read and written. Artifacts by default; a
   * store built over `draftTransport` holds draft files instead and behaves
   * identically in every other respect, which is what lets a draft be edited in
   * the Editor's own surface rather than a lookalike (NAW-FR-11).
   */
  readonly transport: DocumentTransport;

  /**
   * EDT-FR-70: whether this store schedules a document's write itself.
   *
   * On for artifacts and plain text files, which write themselves. Off for the
   * draft-file store a New Artifact tab mounts over: a draft's rest-after-typing
   * schedule is that tab's own (NAW-FR-13), because writing a draft file also
   * reports the write in the tab and refreshes the draft list — two things this
   * store knows nothing about. Two schedules over one buffer would write it
   * twice.
   */
  readonly writes: WriteSchedule;

  constructor(
    transport: DocumentTransport = artifactTransport,
    autoWrite = true,
  ) {
    this.transport = transport;
    this.writes = new WriteSchedule((id) => {
      // A refusal is recorded on the record itself (a divergence holds the
      // write, an error is attached to the tab), and nothing is retried on a
      // timer (EDT-FR-71) — the next edit schedules the next write.
      void this.flush(id);
    }, autoWrite);
  }

  private sessions = new Map<string, EditSession>();
  private listeners = new Set<() => void>();
  /**
   * Bumped on every change a *renderer* can observe (dirty, conflict, error,
   * mode, load). Deliberately NOT bumped by buffer/history mutation: those
   * happen on every keystroke and the Editor reads them without re-rendering.
   */
  private version = 0;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Snapshot for `useSyncExternalStore` — a number, so it is referentially stable. */
  getVersion = (): number => this.version;

  // --- external-change watch (EXC-FR-LKHZ / EXC-FR-UWYK) -----------------------
  // Ref-counted so several surfaces can depend on it while exactly one
  // `"artifact changed externally"` subscription exists per store.
  private watchCount = 0;
  private unlistenWatch: (() => void) | null = null;
  // Bumped on every stop, so a `listen()` that resolves after its watch was
  // stopped detaches itself instead of overwriting a newer one's unlisten (which
  // would leak the older listener — reachable under React StrictMode's
  // mount/cleanup/mount).
  private watchGeneration = 0;

  /**
   * Start watching `"artifact changed externally"` for every artifact this store
   * holds state for; returns the matching stop.
   *
   * The watch belongs here rather than to the Editor because only the *active*
   * tab's Editor is mounted (EDT-FR-30 keeps the others alive without rendering
   * them). A per-Editor subscription would miss a change to any artifact the
   * user is not looking at — and since re-activating a tab performs no reload,
   * that change would never be noticed at all, letting the next write overwrite
   * it with no prompt.
   */
  watchExternalChanges(): () => void {
    // Nothing watches a draft's folder, and the event's ids are project-relative
    // paths — subscribing would let an unrelated artifact's change raise a
    // blocking divergence modal over a draft file whose key happened to match.
    if (!this.transport.watchesExternalChanges) return () => {};
    this.watchCount += 1;
    if (this.watchCount === 1) {
      const generation = ++this.watchGeneration;
      void onArtifactChangedExternally((p) => this.applyExternalChange(p)).then(
        (fn) => {
          if (this.watchGeneration !== generation || this.watchCount === 0) fn();
          else this.unlistenWatch = fn;
        },
      );
    }
    return () => {
      this.watchCount -= 1;
      if (this.watchCount > 0) return;
      this.watchGeneration += 1;
      this.unlistenWatch?.();
      this.unlistenWatch = null;
    };
  }

  /**
   * EXC-FR-QCNO: an event for an artifact we hold no state for is ignored (nothing
   * is open on it and there is no baseline to diverge from), and one carrying
   * the session's own baseline is the echo of our own write. Anything else is a
   * real divergence and raises (or re-raises) the blocking modal (EXC-FR-UWYK).
   */
  private applyExternalChange(p: ArtifactChangedPayload): void {
    const s = this.sessions.get(p.artifactId);
    if (!s || p.checksum === s.baseline) return;
    // EDT-FR-84 / PCR-FR-24: an artifact with a prompt-change review showing
    // over it takes no external-change modal for as long as the review stands.
    // The divergence is still recorded — it is what the review's standing line
    // is said from, and what is raised once the review's reason for passing it
    // over turns out not to have come true (PCR-FR-11) — but no modal is put up
    // and no tab is made inert, because the author is about to replace the whole
    // file on purpose and a dialog answering something else must not hold that
    // decision behind it.
    if (this.externalHold.has(p.artifactId)) {
      this.update(p.artifactId, { pending: p.checksum });
      return;
    }
    this.update(p.artifactId, { pending: p.checksum, conflict: true });
  }

  /**
   * PCR-FR-24: artifacts whose external-change modal is held back while a
   * prompt-change review is showing over them.
   *
   * A set rather than a flag on the session, because the hold belongs to the
   * review rather than to the file: it is put on when the modal opens and taken
   * off when it goes, and a session created after the hold was placed inherits
   * it without anything having to remember to copy it.
   */
  private externalHold = new Map<string, number>();

  /**
   * PCR-FR-11 / PCR-FR-24: hold back the external-change modal for `artifactId`.
   *
   * Called by the review as it opens. Returns the release, which raises the
   * modal for a divergence that arrived while the hold stood — because the
   * reason for passing it over was that the file was about to be replaced whole,
   * and a review dismissed or an acceptance refused means that did not happen.
   */
  holdExternalChanges(artifactId: string): () => void {
    this.externalHold.set(artifactId, (this.externalHold.get(artifactId) ?? 0) + 1);
    let released = false;
    return () => {
      // Ref-counted, and each release usable once: React mounts an effect,
      // cleans it up and mounts it again under StrictMode, and two views of one
      // artifact may each hold. A release that took the hold off while another
      // holder was still relying on it would raise the modal the review exists
      // to keep down.
      if (released) return;
      released = true;
      const left = (this.externalHold.get(artifactId) ?? 1) - 1;
      if (left > 0) {
        this.externalHold.set(artifactId, left);
        return;
      }
      this.externalHold.delete(artifactId);
      this.raiseHeldExternalChange(artifactId);
    };
  }

  /**
   * PCR-FR-11 / EDT-FR-84: raise the external-change modal for a divergence
   * that arrived **while the hold stood**, because the replacement it was
   * passed over for did not happen.
   *
   * Called on a failed acceptance, which is the one case where the reason for
   * passing the event over — that the whole file was about to be replaced on
   * the author's instruction — turns out not to have come true. The hold itself
   * stays in place, so an event arriving *after* this is still passed over for
   * as long as the review is showing (PCR-FR-24).
   */
  raiseHeldExternalChange(artifactId: string): void {
    const s = this.sessions.get(artifactId);
    if (!s || s.conflict) return;
    if (s.pending === null || s.pending === s.baseline) return;
    this.update(artifactId, { conflict: true });
  }

  /**
   * EXC-FR-UPGM / EDT-FR-84: dismiss an external-change modal standing over
   * `artifactId` **with neither of its resolutions taken**.
   *
   * Invoked when the author activates **Accept** on a proposed change to this
   * file: "Load from filesystem" and "Keep my version" are both questions about
   * a buffer that is about to stop existing, and a decision the author has
   * already made must not be held behind a dialog answering something else.
   *
   * The buffer, the dirty indicator, and the undo history are left exactly as
   * they stand — the acceptance's own reset is what replaces them, and only on
   * success (EXC-FR-UVJY).
   */
  dismissConflict(artifactId: string): void {
    const s = this.sessions.get(artifactId);
    if (!s?.conflict) return;
    // The **divergence** is kept while the modal goes: an acceptance that then
    // fails did not replace the file after all, so the question the modal was
    // asking is live again and `raiseHeldExternalChange` puts it back
    // (EDT-FR-84). A success clears both, `adoptLoad` resetting the session to
    // what the filesystem now holds (EXC-FR-JAWT).
    this.update(artifactId, { conflict: false });
  }

  /** Notify subscribers that observable state changed. */
  private notify(): void {
    this.version += 1;
    this.listeners.forEach((l) => l());
  }

  get(artifactId: string): EditSession | undefined {
    return this.sessions.get(artifactId);
  }

  /**
   * Whether a record exists for `artifactId` — an artifact that has been edited,
   * opened in a Diff tab, or had a find panel opened over it (EDT-FR-28).
   *
   * Distinct from `get`, for a caller that only wants to know whether there is
   * a session to act on rather than to read one. A rollback asks this before
   * resetting: a path nobody has open needs no reset at all.
   */
  has(artifactId: string): boolean {
    return this.sessions.has(artifactId);
  }

  /**
   * The artifact's record, created on first use. A record created for a tab that
   * is never edited is dropped again when the tab closes (see `closeTab`), so an
   * artifact that was only read carries nothing forward (EDT-FR-28).
   */
  ensure(artifactId: string): EditSession {
    let s = this.sessions.get(artifactId);
    if (!s) {
      s = newSession(artifactId);
      this.sessions.set(artifactId, s);
    }
    return s;
  }

  /** Apply a mutation and notify — the single write path for observable fields. */
  update(artifactId: string, patch: Partial<EditSession>): void {
    const s = this.ensure(artifactId);
    Object.assign(s, patch);
    this.notify();
  }

  // --- the rest-after-typing write schedule (EDT-FR-70) ---------------------

  /**
   * EDT-FR-70: the author edited the document. Mark it dirty and (re)start the
   * rest, so a burst of typing is one write at the end of it rather than one per
   * character.
   *
   * Called on every keystroke, so it must stay cheap: only the clean→dirty
   * transition notifies, and restarting the timer is not observable at all.
   */
  noteEdit(artifactId: string): void {
    const s = this.ensure(artifactId);
    if (!s.dirty) {
      s.dirty = true;
      this.notify();
    }
    this.scheduleWrite(artifactId);
  }

  /**
   * EDT-FR-70: schedule this document's write for a short rest from now,
   * restarting any rest already running so the write carries the settled text
   * rather than an intermediate one. A no-op on a store whose documents are
   * written on someone else's schedule (see `writes`).
   */
  scheduleWrite(artifactId: string): void {
    // EDT-FR-81: a quiesced session schedules nothing. The buffer a write would
    // carry is one the author has asked to discard, so arming a rest here would
    // only give it a chance to land on the file the rollback is about to write.
    if (this.sessions.get(artifactId)?.quiesced) return;
    this.writes.schedule(artifactId);
  }

  /** Drop a scheduled write without performing it. */
  private cancelWrite(artifactId: string): void {
    this.writes.cancel(artifactId);
  }

  /**
   * EDT-FR-70: whether this document holds a write that has not yet landed — a
   * rest still running, or one held behind an unresolved divergence.
   *
   * Deliberately **not** what Save's enablement is computed from, despite
   * SNV-FR-28's wording: a write that failed leaves the artifact dirty with no
   * rest armed, and Save has to stay enabled there because it is how EDT-FR-71
   * says the author retries. `dirty` is the enablement; this is the narrower
   * question of whether a write is actually queued, which only a caller asking
   * "was that write dropped, or merely made harmless" needs to distinguish.
   */
  hasPendingWrite(artifactId: string): boolean {
    return (
      this.writes.has(artifactId) ||
      this.sessions.get(artifactId)?.heldWrite === true
    );
  }

  /**
   * EXC-FR-NMXQ: the author chose "Keep my version", so a write held behind the
   * modal (EDT-FR-70) is performed at once. Returns whether there was one.
   */
  takeHeldWrite(artifactId: string): boolean {
    const s = this.sessions.get(artifactId);
    if (!s?.heldWrite) return false;
    s.heldWrite = false;
    return true;
  }

  /**
   * EDT-FR-24: adopt a load result as the artifact's new starting point — the
   * buffer, the history floor, and the divergence baseline all move to it. Used
   * for a first open, for "Load from filesystem" (EXC-FR-WDAV), and for a document
   * rewritten wholesale by an act of the author's own — an accepted draft-change
   * proposal (`reload`, DCR-FR-12). Never for the revalidating load a reopen
   * performs (EDT-FR-29), which keeps the retained buffer.
   */
  adoptLoad(artifactId: string, body: string, checksum: string): void {
    const s = this.ensure(artifactId);
    // EXC-FR-WDAV / EDT-FR-70: the buffer a scheduled write would have carried is
    // gone, so the write goes with it — held or merely resting, it is dropped
    // rather than left to overwrite what was just loaded.
    this.cancelWrite(artifactId);
    s.heldWrite = false;
    s.buffer = body;
    s.baseline = checksum;
    s.pending = null;
    s.dirty = false;
    s.conflict = false;
    s.error = null;
    s.loaded = true;
    s.history = createHistory(body);
    // EDT-FR-37: detection runs once over the body as the artifact loads, not
    // per keystroke. An override the user chose earlier in the session outranks
    // it — re-detecting would silently undo their choice on a
    // Load-from-filesystem (EXC-FR-WDAV).
    if (!s.indentationOverridden) s.indentation = detectIndentation(body);
    // EFR-FR-HVVO: `s.find` is deliberately untouched. "Load from filesystem"
    // (EXC-FR-WDAV) replaces the content, not the search over it — the panel stays
    // open with its query, replacement text and mode, and simply recomputes its
    // matches against what was loaded.
    s.seedToken += 1;
    this.notify();
  }

  /**
   * `EFR-editor-find-replace.md` EFR-FR-AYNZ–EFR-FR-DBOW/EFR-FR-GBJT: apply a change to the artifact's find panel.
   *
   * Observable — the panel occupies the band the formatting toolbar otherwise
   * holds (EFR-FR-ABHF) and the query drives what is highlighted — so unlike a
   * buffer mutation this notifies.
   */
  setFind(artifactId: string, patch: Partial<FindState>): void {
    const s = this.ensure(artifactId);
    s.find = { ...s.find, ...patch };
    this.notify();
  }

  /**
   * STB-FR-23 / EDT-FR-38: adopt a convention the user chose. It changes only
   * what subsequent Tab keypresses insert — no buffer is touched, the artifact
   * is not marked dirty, and nothing joins the undo history (EDT-FR-23).
   */
  setIndentation(artifactId: string, indentation: Indentation): void {
    const s = this.ensure(artifactId);
    s.indentation = indentation;
    s.indentationOverridden = true;
    this.notify();
  }

  /**
   * EDT-FR-41 / STB-FR-18: the project's line-ending convention changed, so
   * every artifact a tab is open on will be rewritten by its next write. Mark
   * each dirty and schedule that write on the ordinary terms of EDT-FR-70, so
   * the conversion is visible in the indicator while it is outstanding and lands
   * shortly after rather than riding invisibly on some later unrelated edit.
   *
   * No buffer is altered and no history step is added (EDT-FR-23), and an
   * artifact with no open tab is deliberately skipped — it gains no dirty
   * marker and is converted whenever it is next written.
   */
  markOpenTabsDirty(): void {
    let changed = false;
    this.sessions.forEach((s, id) => {
      if (!s.tabOpen) return;
      if (!s.dirty) {
        s.dirty = true;
        changed = true;
      }
      this.scheduleWrite(id);
    });
    if (changed) this.notify();
  }

  /** A tab opened on this artifact (or focus jumped to its existing one). */
  openTab(artifactId: string): void {
    const s = this.ensure(artifactId);
    if (s.tabOpen) return;
    s.tabOpen = true;
    this.notify();
  }

  /**
   * EDT-FR-29: the artifact's tab closed. A record holding edits is retained for
   * the rest of the session and revalidated on the next open; a record for a tab
   * that was only read is dropped, so reopening it is a plain first load.
   *
   * EDT-FR-28: a find panel the user opened keeps the record alive too, even
   * with no edit behind it — the panel and its query are state they authored,
   * and reopening the artifact must bring them back (EFR-FR-GIPZ, EFR-FR-GBJT, EFR-FR-GNBZ). A comment rail
   * the author opened or closed counts for the same reason (CMT-FR-30 /
   * CMT-FR-30, EDT-FR-28); the *default* rail state does not, which is why only an explicit
   * toggle writes `railOpen` at all. An expanded resolved disclosure counts
   * too, on the same footing.
   */
  closeTab(artifactId: string): void {
    const s = this.sessions.get(artifactId);
    if (!s) return;
    s.tabOpen = false;
    if (
      !hasEdits(s.history) &&
      !s.dirty &&
      !s.indentationOverridden &&
      s.railOpen === null &&
      !s.resolvedOpen &&
      findStateIsPristine(s.find)
    ) {
      this.cancelWrite(artifactId);
      this.sessions.delete(artifactId);
    } else {
      s.revalidate = true;
    }
    this.notify();
  }

  /** Every id this store currently holds a record for. */
  allIds(): string[] {
    return [...this.sessions.keys()];
  }

  /**
   * Discard one record outright, whatever it holds.
   *
   * Distinct from `closeTab`, which retains a record carrying edits so a reopen
   * resumes it (EDT-FR-29). This is for a document that no longer exists — a
   * deleted draft file (NAW-FR-09), or a draft that was graduated or deleted
   * (NAW-FR-20 / DRP-FR-12) — where there is nothing left to resume and a
   * retained dirty buffer would be swept into the next teardown's flush and
   * written back to a path the author removed.
   */
  forget(artifactId: string): void {
    // TAB-FR-20 / NAW-FR-09: the document is gone, so a write it had scheduled
    // must not fire and recreate the path its author just removed.
    this.cancelWrite(artifactId);
    if (this.sessions.delete(artifactId)) this.notify();
  }

  /**
   * Read the document again and adopt what comes back, whatever the record
   * currently holds.
   *
   * For a document whose bytes were replaced wholesale by an act of the author's
   * — an accepted draft-change proposal (DCR-FR-12) — rather than one that is
   * gone (`forget`). The distinction is what the surface does next: a discarded
   * record leaves a mounted Editor with nothing to show, which is right for a
   * file that no longer exists and wrong for one that was just rewritten.
   *
   * The load runs here rather than in the Editor because it must not depend on
   * one being mounted: the same acceptance can arrive for a file no tab is
   * showing, and the record it leaves behind has to be the accepted text either
   * way. `adoptLoad` replaces the buffer, the baseline and the undo history —
   * correctly, since a history whose floor no longer exists would let one undo
   * quietly reinstate the pre-acceptance text as an edit to be written back —
   * and bumps the seed a mounted surface re-reads its content from.
   */
  async reload(artifactId: string): Promise<void> {
    const s = this.ensure(artifactId);
    // Nothing is dropped up front. `adoptLoad` cancels the scheduled write and
    // clears the held one when the read SUCCEEDS, which is the only point at
    // which the buffer they were carrying has been superseded. Dropping them
    // here would drop them on the failure path too — an author's unsaved edit
    // left dirty with no write coming, and a write held behind a divergence
    // discarded without the divergence being resolved.
    let settled: Promise<void> | null = null;
    settled = (async () => {
      try {
        const res = await this.transport.load(artifactId);
        // Commit only if this is still the newest read of this document, the
        // way the Editor's own load does: a slower earlier read landing last
        // would put superseded bytes over the newer answer.
        if (s.pendingLoad !== settled) return;
        this.adoptLoad(artifactId, res.body, res.checksum);
      } catch (e) {
        if (s.pendingLoad !== settled) return;
        this.update(artifactId, { error: String(e), loaded: true });
        // Nobody asked for this read by name — it is the tail of an acceptance
        // the author has already watched succeed — so a failure here is one
        // they would report as "the tab is showing the old text" with nothing
        // recorded anywhere. The reason is the transport's own message; the
        // body it could not read is the author's content and stays out of it.
        logError(["frontend"], "a document could not be read again", {
          artifact: artifactId,
          reason: String(e),
        });
      }
    })();
    // Published like any other load, so a write racing it waits for the answer
    // instead of landing on top of what was just read (EDT-FR-29).
    s.pendingLoad = settled;
    void settled.finally(() => {
      if (s.pendingLoad === settled) s.pendingLoad = null;
    });
    await settled;
  }

  /**
   * Move a record to a new id, keeping its buffer, dirty flag, mode and history.
   *
   * A draft file renamed within its draft is the same document under a new name
   * (NAW-FR-09), so losing the unsaved buffer to the rename would be losing the
   * author's work to a filing decision. The baseline moves with it: the bytes on
   * disk did not change, only where they are.
   */
  rekey(from: string, to: string): void {
    const s = this.sessions.get(from);
    if (!s || from === to) return;
    this.sessions.delete(from);
    s.artifactId = to;
    this.sessions.set(to, s);
    // A pending write follows the document to its new path, or it would land on
    // the old one after the rename.
    this.writes.move(from, to);
    this.notify();
  }

  /** EDT-FR-28: discard everything (project close / application exit). */
  clear(): void {
    // Callers flush first (EDT-FR-33), so nothing dropped here is unsaved; a
    // write left scheduled would land in a project that is no longer open.
    this.writes.cancelAll();
    this.sessions.clear();
    this.notify();
  }

  /**
   * Artifact ids a teardown must deal with, in the order they were first
   * opened: those holding unsaved changes, plus those showing an unresolved
   * external-change modal in an open tab — a divergence the user is looking at
   * blocks a teardown whether or not they had also edited the artifact
   * (EXC-FR-UWYK / EDT-FR-32).
   */
  pendingIds(): string[] {
    const ids: string[] = [];
    this.sessions.forEach((s, id) => {
      if (s.dirty || (s.conflict && s.tabOpen)) ids.push(id);
    });
    return ids;
  }

  /**
   * EDT-FR-35: artifacts holding unsaved changes, in the order they were first
   * opened — the scope of a Save All, and what decides whether it is offered at
   * all (SNV-FR-30).
   *
   * Narrower than `pendingIds`: an artifact showing an unresolved divergence it
   * has no edits behind has nothing to write, so Save All neither offers itself
   * for it nor reports it as blocked. A teardown still has to deal with it,
   * which is why the two lists are separate.
   */
  dirtyIds(): string[] {
    const ids: string[] = [];
    this.sessions.forEach((s, id) => {
      if (s.dirty) ids.push(id);
    });
    return ids;
  }

  /**
   * EDT-FR-31/EDT-FR-32/EDT-FR-70: write the artifact's pending changes, or
   * refuse.
   *
   * This is where a write happens on every route: the rest of EDT-FR-70
   * elapsing, and every path that brings that write forward — a tab, project, or
   * application close (EDT-FR-31, EDT-FR-33) and the File menu's Save and Save
   * All (EDT-FR-34, EDT-FR-35). Whichever route arrives first spends the
   * schedule, so a Save moments before the rest elapses writes once rather than
   * twice (SNV-FR-29).
   *
   * Refuses — without writing — when the tab has an unresolved external-change
   * modal (writing would auto-resolve the divergence, EXC-FR-VNLZ), or when the
   * save itself fails. In each case the blocker is left set on the record so
   * whichever tab shows the artifact surfaces it.
   */
  async flush(
    artifactId: string,
    opts: { force?: boolean } = {},
  ): Promise<FlushResult> {
    const s = this.sessions.get(artifactId);
    if (!s) return { ok: true };

    // SNV-FR-29: whatever brought this write forward spends the schedule, so the
    // rest does not fire again behind it and write the same buffer twice.
    this.cancelWrite(artifactId);

    // One write of a document at a time (`writeChain`). Queue behind whatever is
    // in flight, and only then read the buffer and re-check the guards — so a
    // write that arrives during another writes what the document holds *after*
    // it, rather than racing it to disk with an older buffer.
    const ahead = s.writeChain;
    let done!: () => void;
    s.writeChain = new Promise<void>((resolve) => {
      done = resolve;
    });
    try {
      await ahead;
      return await this.write(s, artifactId, opts);
    } finally {
      done();
    }
  }

  /** The guarded write itself, run with this document's queue held. */
  private async write(
    s: EditSession,
    artifactId: string,
    opts: { force?: boolean },
  ): Promise<FlushResult> {
    // EDT-FR-29: a revalidating load may be a moment away from discovering that
    // the file changed on disk. Let it land first, so the divergence guard below
    // sees it rather than this write silently overwriting the other version.
    if (s.pendingLoad) {
      // The load reports its own failure onto the record; a rejection here must
      // not escape and strand a teardown mid-way (a held quit would then wait
      // out its grace period instead of being answered).
      await s.pendingLoad.catch(() => {});
    }

    // EDT-FR-81: re-checked here rather than only in `flush`, because this is
    // the point *after* the write queue: a flush that queued behind an in-flight
    // write before the quiesce began must not start now that it has reached the
    // front. The in-flight write itself is already past this line and completes
    // normally, which is exactly the distinction the requirement draws between
    // a write that is running and one that is merely next.
    if (s.quiesced) return { ok: true };

    // EXC-FR-VNLZ / EDT-FR-32: an unresolved divergence outranks everything, and
    // is checked BEFORE the not-dirty shortcut — an artifact the user only read
    // can still be sitting on the modal, and closing over it would dismiss the
    // divergence and silently adopt one of the two versions.
    if (s.conflict) {
      // EDT-FR-70: the write is *held* rather than dropped — the author has yet
      // to answer the modal, and their buffer is still what it would carry. It
      // is performed the moment they choose "Keep my version" (EXC-FR-WDEJ) and
      // discarded with the buffer if they choose "Load from filesystem"
      // (EXC-FR-WDAV, handled by `adoptLoad`).
      if (s.dirty && !s.heldWrite) {
        s.heldWrite = true;
        // A write the author asked for by typing did not happen, and will not
        // until they answer the modal. Nothing else says so anywhere they can
        // look afterwards.
        logDebug(["frontend"], "artifact write held by external change", {
          artifact: artifactId,
        });
        this.notify();
      }
      return { ok: false, blocked: "conflict", artifactId };
    }
    // A teardown writes only what is unsaved (EDT-FR-31); an explicit Save
    // writes whatever the buffer holds, dirty or not.
    if (!s.dirty && !opts.force) return { ok: true };

    try {
      // The seed is the document's identity across a wholesale replacement: it
      // moves only when a load is adopted (`adoptLoad`). Captured before the
      // save so the commit below can tell whether the bytes it wrote are still
      // the bytes this record holds.
      const seed = s.seedToken;
      const res = await this.transport.save(artifactId, s.buffer);
      // A load committed while the save was outstanding — a proposal accepted
      // against this file (DCR-FR-12), or "Load from filesystem" — so the write
      // is the older story. Its checksum describes bytes the record no longer
      // holds, and adopting it as the baseline would leave the document
      // measured against a file that no longer says that. The load already set
      // baseline, dirty and history from what it read.
      if (s.seedToken !== seed) return { ok: true };
      // EXC-FR-HKKG/EXC-FR-NMXQ: the written checksum is the new baseline, so the
      // watcher's echo of our own write raises no modal.
      s.baseline = res.checksum;
      s.pending = null;
      s.dirty = false;
      s.conflict = false;
      s.error = null;
      s.heldWrite = false;
      this.notify();
      return { ok: true };
    } catch (e) {
      // EDT-FR-71: the buffer, the history, and the dirty indicator are left
      // exactly as they were, and nothing is retried on a timer — the next edit
      // schedules the next write and Save retries on demand.
      s.error = String(e);
      s.heldWrite = false;
      this.notify();
      // The write is the one operation here the user did not ask for by name,
      // so a failure they later report is a failure nobody watched happen. The
      // reason is the transport's own message; the buffer it could not write is
      // the user's content and stays out of the record.
      logError(["frontend"], "artifact write failed", {
        artifact: artifactId,
        reason: String(e),
      });
      // EDT-FR-81: a write that was in flight when the quiesce began and failed
      // as it settled. Collected rather than merely left on the record, because
      // the surface that asked for the quiesce reports it (CHG-FR-61) and a
      // pre-existing error from some earlier write is not the same thing.
      if (s.quiesced) this.quiesceFailures.set(artifactId, String(e));
      return { ok: false, blocked: "error", artifactId };
    }
  }

  // --- rollback: quiesce and the two resets (EDT-FR-81 / `EXC-editor-external-change.md` EXC-FR-JAWT – EXC-FR-ZXWI) ---------

  /**
   * Saves that failed while their session was quiesced, keyed by artifact.
   *
   * Populated by `write`'s failure path and drained by `quiesce`, so what the
   * caller is handed is exactly the writes that settled *during its own* wait —
   * not an error some earlier write left lying on the record.
   */
  private quiesceFailures = new Map<string, string>();

  /**
   * EDT-FR-81: put `artifactIds` into the quiesced state and wait for every
   * write already in flight for them to settle.
   *
   * Called by a surface that is about to discard these artifacts' edits on the
   * author's instruction (CHG-FR-60). On return, no write is running or pending
   * for any of them and none can be started, so the caller may invoke a
   * filesystem operation over their paths without a save landing behind it.
   *
   * One artifact is awaited **once** however many Editor and Diff tabs are open
   * on it: the several surfaces share one session and one `writeChain`
   * (EDT-FR-72), so the wait is per artifact rather than per view.
   *
   * Returns the saves that failed while settling. A failure does not hold the
   * quiesce open and is not retried (EDT-FR-71) — the author asked for the
   * buffer to be discarded, so a write that failed to persist it cost them
   * nothing they were keeping — but the caller reports it (CHG-FR-61).
   */
  async quiesce(artifactIds: string[]): Promise<QuiesceFailure[]> {
    const held = artifactIds.filter((id) => this.sessions.has(id));
    // Marked synchronously, before a single `await`, so no edit or save
    // starting in the meantime can slip past the guards.
    for (const id of held) {
      const s = this.sessions.get(id);
      if (!s) continue;
      s.quiesced = true;
      this.cancelWrite(id);
      s.heldWrite = false;
    }
    if (held.length > 0) this.notify();

    // Awaited in parallel: these are independent files, and a rollback of
    // twenty should not cost twenty round-trips end to end.
    await Promise.all(
      held.map((id) => this.sessions.get(id)?.writeChain.catch(() => {})),
    );

    const failures: QuiesceFailure[] = held
      .filter((id) => this.quiesceFailures.has(id))
      .map((id) => ({
        artifactId: id,
        reason: this.quiesceFailures.get(id) as string,
      }));
    held.forEach((id) => this.quiesceFailures.delete(id));
    return failures;
  }

  /**
   * EXC-FR-JAWT: the artifact's file was **restored on disk**, confirmed by the
   * operation that did it — so reset the session to what the filesystem now
   * holds.
   *
   * `adoptLoad` does the reset itself: it replaces the buffer, clears the dirty
   * indicator, moves the undo floor to the reloaded content so no undo can
   * reinstate the discarded text, adopts the new checksum as the baseline, and
   * clears `conflict` — which is what dismisses an external-change modal
   * standing over this artifact **with neither of its resolutions taken**.
   * Offering "Keep my version" here would offer the author back a buffer they
   * have just had deleted from disk on purpose (EXC-FR-NPFL).
   *
   * The mode, the indentation convention, and the find panel survive, none of
   * them describing the discarded text (`adoptLoad` leaves all three alone).
   */
  async resetRestored(artifactId: string): Promise<void> {
    const s = this.sessions.get(artifactId);
    if (!s) return;
    try {
      await this.reload(artifactId);
    } finally {
      // Lifted whether or not the read succeeded: a session left quiesced would
      // never write again, so a failed reload would silently cost the author
      // every edit they made from then on.
      s.quiesced = false;
      this.quiesceFailures.delete(artifactId);
      this.notify();
    }
  }

  /**
   * EXC-FR-ZXWI: the artifact's file was **removed**, so there is no session left
   * to reset.
   *
   * The record is discarded outright and **without being written** — `forget`
   * cancels the scheduled write rather than performing it — because writing the
   * buffer of a file the author has just deleted would recreate exactly what
   * they deleted. Its tabs are closed by the caller (TAB-FR-41), which must not
   * take the write-before-close path for the same reason.
   */
  discardRemoved(artifactId: string): void {
    this.quiesceFailures.delete(artifactId);
    this.forget(artifactId);
  }

  /**
   * EXC-FR-UOJF: the operation confirmed this artifact as neither restored nor
   * removed, so nothing on disk has been shown to have moved beneath it.
   *
   * The quiesce is lifted and everything else is left exactly as it stood — the
   * buffer, the dirty indicator, the undo history, the baseline, and any
   * external-change modal over it — so the next edit schedules a write again and
   * the author's edit is still there to be written.
   */
  unquiesce(artifactId: string): void {
    const s = this.sessions.get(artifactId);
    if (!s?.quiesced) return;
    s.quiesced = false;
    this.quiesceFailures.delete(artifactId);
    // An edit made while the session was quiesced armed no rest (EDT-FR-81), so
    // one is armed now — otherwise that edit would sit unwritten until the
    // author happened to type again.
    if (s.dirty) this.scheduleWrite(artifactId);
    this.notify();
  }

  /**
   * EDT-FR-33: write every artifact holding unsaved changes before a project
   * close, project switch, or application exit. Stops at the first blocked
   * flush and reports it, so the caller can focus that tab and cancel the
   * teardown rather than tearing down over an unresolved blocker.
   */
  async flushAll(): Promise<FlushResult> {
    for (const id of this.pendingIds()) {
      const res = await this.flush(id);
      if (!res.ok) return res;
    }
    return { ok: true };
  }
}
