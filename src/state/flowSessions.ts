import * as api from "../api";
import { onArtifactChangedExternally } from "../events";
import { logError, logWarn } from "../logging";
import type {
  ArtifactChangedPayload,
  FlowValidationReport,
  FlowViolation,
} from "../types";
import type { FlushResult } from "./editSessions";
import { WriteSchedule } from "./writeSchedule";
import {
  parseFlowDocument,
  serializeFlowDocument,
  type FlowDocument,
} from "./flowDocument";

/**
 * FLO-FR-05 / FLO-FR-47: a validation report as the tab states it — every
 * violation the report named, not the first one.
 */
export function describeViolations(violations: readonly FlowViolation[]): string {
  return violations
    .map((v) => {
      const at = v.elementId ?? v.edgeId;
      return at ? `${at}: ${v.message}` : v.message;
    })
    .join("\n");
}

/**
 * FLO-FR-26–FLO-FR-31: the in-memory state of every Flow open on a Flow canvas.
 *
 * The counterpart of `editSessions` for Flows, and deliberately a smaller one: a
 * Flow's editing state belongs to the Flow *tab* showing it, not to the Flow
 * (FLO-FR-28). Closing the tab writes the pending changes and drops the record,
 * so reopening deserializes the on-disk document afresh — there is no
 * counterpart to the artifact-level retained edit state or undo history.
 *
 * What the record does have to hold is the graph itself. Only the active tab's
 * surface is mounted, so a graph kept in component state would be lost the
 * moment the user switched tabs; FLO-FR-30 requires the opposite, that switching
 * away and back neither reloads the Flow nor discards its edits.
 *
 * The store also owns the read and write paths, through the two artifact-content
 * operations a Flow shares with every other artifact file (FLO contract
 * boundary): `"load artifact contents by id"` and `"save artifact contents"`.
 */
export interface FlowSession {
  flowId: string;
  /**
   * The deserialized graph, or null while the first load is in flight or after
   * it failed. A canvas renders nothing but its state row until this is set.
   */
  doc: FlowDocument | null;
  /**
   * FLO-FR-05: the body could not be read as a Flow document, and this names
   * the failure. While it is set the tab offers no editing and `flush` writes
   * nothing, so a file the editor cannot read is never replaced by an empty
   * graph.
   */
  parseError: string | null;
  /** Last load or save failure, surfaced by the tab showing the Flow. */
  error: string | null;
  /**
   * FLO-FR-05 / FLO-FR-47: the violations the last validation named, so the tab
   * lists them against the elements they sit on rather than as one blob of
   * prose. Empty whenever the last validation passed.
   */
  violations: FlowViolation[];
  /** FLO-FR-26: unsaved canvas edits exist. */
  dirty: boolean;
  /** True once the first load attempt has settled, however it settled. */
  loaded: boolean;
  /**
   * PST-FR-16: the checksum this store last served or wrote. An
   * `"artifact changed externally"` carrying it is the echo of our own write
   * rather than a real divergence (FLO-FR-31).
   */
  baseline: string | null;
  /** Whether a Flow tab is currently open on this Flow. */
  tabOpen: boolean;
  /**
   * A load in flight. A write waits on it so a reload landing mid-save cannot
   * overwrite the graph the save just serialized.
   */
  pendingLoad: Promise<void> | null;
  /**
   * The tail of this Flow's write queue. Every `flush` links itself onto it and
   * waits, so two writes of the same Flow never overlap — see
   * `EditSessionStore`'s field of the same name for what overlapping costs.
   */
  writeChain: Promise<void>;
}

function newSession(flowId: string): FlowSession {
  return {
    flowId,
    doc: null,
    parseError: null,
    error: null,
    violations: [],
    dirty: false,
    loaded: false,
    baseline: null,
    tabOpen: false,
    pendingLoad: null,
    writeChain: Promise.resolve(),
  };
}

export class FlowSessionStore {
  private sessions = new Map<string, FlowSession>();
  private listeners = new Set<() => void>();
  private version = 0;
  /**
   * FLO-FR-27: the rest-after-editing schedule of every Flow holding a write
   * that has not yet been performed.
   *
   * Held by the store rather than the canvas for the same reason the external-
   * change watch is: only the active tab's canvas is mounted (FLO-FR-30), so a
   * timer the component owned would be torn down the moment the author switched
   * tabs and the edit they had just made would sit unwritten.
   */
  private writes = new WriteSchedule((id) => {
    // FLO-FR-47: a refusal lists its violations on the record and is not retried
    // on a timer — the next edit schedules the next write, so a graph the author
    // is midway through repairing is not re-refused every few seconds.
    void this.flush(id);
  });

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Snapshot for `useSyncExternalStore` — a number, so it is referentially stable. */
  getVersion = (): number => this.version;

  private notify(): void {
    this.version += 1;
    this.listeners.forEach((l) => l());
  }

  get(flowId: string): FlowSession | undefined {
    return this.sessions.get(flowId);
  }

  /** The Flow's record, created on first use. Does NOT start a load. */
  ensure(flowId: string): FlowSession {
    let s = this.sessions.get(flowId);
    if (!s) {
      s = newSession(flowId);
      this.sessions.set(flowId, s);
    }
    return s;
  }

  // --- external-change watch (FLO-FR-31) ------------------------------------
  // Ref-counted so several surfaces can depend on it while exactly one
  // `"artifact changed externally"` subscription exists per store.
  private watchCount = 0;
  private unlistenWatch: (() => void) | null = null;
  // Bumped on every stop, so a `listen()` resolving after its watch was stopped
  // detaches itself instead of overwriting a newer one's unlisten.
  private watchGeneration = 0;

  /**
   * FLO-FR-31: watch for external changes to every Flow this store holds state
   * for; returns the matching stop.
   *
   * The watch belongs here rather than to the canvas because only the active
   * tab's surface is mounted (FLO-FR-30 keeps the others alive without
   * rendering them). A per-canvas subscription would miss a change to any Flow
   * the user is not looking at — and since re-activating a tab performs no
   * reload, that change would never be noticed at all.
   */
  watchExternalChanges(): () => void {
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
   * FLO-FR-31: a Flow tab holding no unsaved changes reloads its graph; one
   * holding unsaved changes keeps them and stays on its in-memory graph, and
   * its next write replaces the file's contents.
   *
   * Deliberately no modal and no prompt: the Editor's blocking external-change
   * flow (EXC-FR-VTUH) is Editor-scoped, and FLO-FR-31 asks for exactly these two
   * behaviours. An event for a Flow this store holds nothing for is ignored,
   * and one carrying our own baseline is the echo of our own write.
   */
  private applyExternalChange(p: ArtifactChangedPayload): void {
    const s = this.sessions.get(p.artifactId);
    if (!s || p.checksum === s.baseline) return;
    if (s.dirty) return;
    void this.load(p.artifactId);
  }

  /**
   * FLO-FR-03: read the file's body through `"load artifact contents by id"`
   * and deserialize it into the graph.
   *
   * A body that is empty or whitespace-only is an empty graph rather than a
   * failure (FLO-FR-04); one that will not parse leaves the record with a
   * `parseError` and no document, which is the tab's error state (FLO-FR-05).
   * A load that cannot reach the file at all is an `error` instead — the
   * difference matters, because only the first says something about the file's
   * contents.
   */
  load(flowId: string): Promise<void> {
    const s = this.ensure(flowId);
    const run = (async () => {
      const { body, checksum } = await api.loadArtifactContentsById(flowId);
      // FLO-FR-46: the backend is the authority on whether this is a Flow at
      // all, and it judges the body before the canvas renders anything.
      const report = await api.validateFlowDocument(body);
      s.loaded = true;
      // FLO-FR-31: the record went dirty while this read was in flight — the
      // user edited between the external change arriving and the file coming
      // back. Their graph wins: adopting the file now would discard an edit
      // they made after the decision to reload, and their next write replaces
      // the file's contents anyway. The baseline is deliberately left alone
      // too, so it keeps naming the content this store last served or wrote.
      if (s.dirty) return;
      s.baseline = checksum;
      s.error = null;
      if (!report.valid) {
        // FLO-FR-05: the tab's error state, listing what the report named. No
        // graph is rendered and no write happens while it stands, so a file the
        // editor cannot read is never replaced by an empty graph.
        s.doc = null;
        s.violations = report.violations;
        s.parseError = describeViolations(report.violations);
        logWarn(["frontend"], "flow document refused by validation", {
          artifact: flowId,
          violations: report.violations.length,
          codes: report.violations.map((v) => v.code).join(","),
        });
        return;
      }
      // The document the backend called a Flow deserializes into the graph. A
      // parse that disagrees with a passing validation is a defect in one of the
      // two, so it is reported rather than swallowed.
      const parsed = parseFlowDocument(body);
      s.violations = [];
      if (parsed.ok) {
        s.doc = parsed.doc;
        s.parseError = null;
      } else {
        s.doc = null;
        s.parseError = parsed.error;
        logError(["frontend"], "validated flow document failed to deserialize", {
          artifact: flowId,
          reason: parsed.error,
        });
      }
    })()
      .catch((e) => {
        s.loaded = true;
        s.error = String(e);
        logError(["frontend"], "flow load failed", {
          artifact: flowId,
          reason: String(e),
        });
      })
      .finally(() => {
        if (s.pendingLoad === run) s.pendingLoad = null;
        this.notify();
      });
    s.pendingLoad = run;
    return run;
  }

  /**
   * A Flow tab opened on this Flow. The first open loads it; a focus jump to an
   * existing tab (TAB-FR-05) does not, so switching away and back neither
   * reloads the Flow nor discards its edits (FLO-FR-30).
   */
  openTab(flowId: string): void {
    const s = this.ensure(flowId);
    if (s.tabOpen) return;
    s.tabOpen = true;
    this.notify();
    if (!s.loaded && !s.pendingLoad) void this.load(flowId);
  }

  /**
   * FLO-FR-28: the Flow's tab closed, and with it its editing session. Nothing
   * is retained — callers flush first (`flush`), so nothing dropped here is
   * unsaved, and reopening the Flow deserializes the on-disk document afresh.
   */
  closeTab(flowId: string): void {
    this.cancelWrite(flowId);
    if (!this.sessions.delete(flowId)) return;
    this.notify();
  }

  /** Discard everything (project close / switch / worktree switch, FLO-FR-29). */
  clear(): void {
    // Callers flush first (FLO-FR-29), so nothing dropped here is unsaved; a
    // write left scheduled would land in a project that is no longer open.
    this.writes.cancelAll();
    this.sessions.clear();
    this.notify();
  }

  // --- the rest-after-editing write schedule (FLO-FR-27) -------------------

  /**
   * FLO-FR-27: schedule this Flow's write for a short rest from now, restarting
   * any rest already running — so a drag across the canvas is one write on
   * release rather than one per frame.
   */
  scheduleWrite(flowId: string): void {
    this.writes.schedule(flowId);
  }

  /** Drop a scheduled write without performing it. */
  private cancelWrite(flowId: string): void {
    this.writes.cancel(flowId);
  }

  /**
   * FLO-FR-27: whether this Flow holds a write that has not yet landed. Not what
   * Save's enablement reads — see `EditSessionStore.hasPendingWrite` for why.
   */
  hasPendingWrite(flowId: string): boolean {
    return this.writes.has(flowId);
  }

  /**
   * FLO-FR-26: replace the graph with the result of a user edit and mark the
   * Flow unsaved. Every canvas mutation goes through here, so there is one place
   * the dirty flag is raised.
   *
   * An edit against a record with no document — still loading, or in the
   * FLO-FR-05 error state — is dropped rather than creating a graph out of
   * nothing, which for the error state would be the very "replaced by an empty
   * graph" the requirement rules out.
   */
  applyEdit(
    flowId: string,
    edit: (doc: FlowDocument) => FlowDocument,
  ): void {
    const s = this.ensure(flowId);
    if (!s.doc) return;
    const next = edit(s.doc);
    // An edit that produced the very same document changed nothing, and every
    // mutator returns a new object when it changes anything — so this is a
    // no-op, not an edit, and must not raise the unsaved state (FLO-FR-26).
    if (next === s.doc) return;
    s.doc = next;
    s.dirty = true;
    // FLO-FR-27: the canvas writes itself a short rest after the last edit.
    this.scheduleWrite(flowId);
    this.notify();
  }

  /** Flows holding unsaved changes, in the order they were first opened. */
  dirtyIds(): string[] {
    const ids: string[] = [];
    this.sessions.forEach((s, id) => {
      if (s.dirty) ids.push(id);
    });
    return ids;
  }

  /**
   * FLO-FR-27: serialize the graph and write it through
   * `"save artifact contents"`, or report why it could not be written. A failed
   * write leaves the record dirty with its error set, so the Flow tab shows it
   * and a close is refused (TAB-FR-13).
   *
   * `force` writes whatever the graph holds, dirty or not — that is an explicit
   * Save (SNV-FR-29); without it a clean Flow is skipped, which is what makes a
   * tab close cheap (FLO-FR-28).
   */
  async flush(
    flowId: string,
    opts: { force?: boolean } = {},
  ): Promise<FlushResult> {
    const s = this.sessions.get(flowId);
    if (!s) return { ok: true };
    // SNV-FR-29: whatever brought this write forward spends the schedule, so the
    // rest does not fire again behind it and write the same graph twice.
    this.cancelWrite(flowId);

    // One write of a Flow at a time: queue behind whatever is in flight, and
    // only then serialize the graph and re-check the guards.
    const ahead = s.writeChain;
    let done!: () => void;
    s.writeChain = new Promise<void>((resolve) => {
      done = resolve;
    });
    try {
      await ahead;
      return await this.write(s, flowId, opts);
    } finally {
      done();
    }
  }

  /** The guarded write itself, run with this Flow's queue held. */
  private async write(
    s: FlowSession,
    flowId: string,
    opts: { force?: boolean },
  ): Promise<FlushResult> {
    // A load in flight may be about to replace the graph; let it land first so
    // this writes what the user is actually looking at.
    if (s.pendingLoad) await s.pendingLoad.catch(() => {});
    // FLO-FR-05: no write happens while the document could not be read.
    if (!s.doc) return { ok: true };
    if (!s.dirty && !opts.force) return { ok: true };

    const body = serializeFlowDocument(s.doc);
    try {
      // FLO-FR-47: the same validation that gates the write inside
      // `save_artifact_contents` (PST-FR-28), run here so the tab can name every
      // violation against the element it sits on rather than surfacing the
      // refusal as one line of prose. The write is refused either way — the
      // backend does not take this call's word for it.
      const report: FlowValidationReport = await api.validateFlowDocument(body);
      if (!report.valid) {
        s.violations = report.violations;
        s.error = describeViolations(report.violations);
        logWarn(["frontend"], "flow write refused by validation", {
          artifact: flowId,
          violations: report.violations.length,
          codes: report.violations.map((v) => v.code).join(","),
        });
        this.notify();
        return { ok: false, blocked: "error", artifactId: flowId };
      }
      const res = await api.saveArtifactContents(flowId, body);
      // PST-FR-16: the written checksum is the new baseline, so the watcher's
      // echo of our own write is not read as an external change (FLO-FR-31).
      s.baseline = res.checksum;
      s.dirty = false;
      s.error = null;
      s.violations = [];
      this.notify();
      return { ok: true };
    } catch (e) {
      s.error = String(e);
      this.notify();
      logError(["frontend"], "flow write failed", {
        artifact: flowId,
        reason: String(e),
      });
      return { ok: false, blocked: "error", artifactId: flowId };
    }
  }

  /**
   * FLO-FR-29: write every Flow holding unsaved changes before a project close,
   * project switch, worktree change, or application exit. Stops at the first
   * blocked write and reports it, so the caller can focus that tab and cancel
   * the transition rather than tearing down over a failed write.
   */
  async flushAll(): Promise<FlushResult> {
    for (const id of this.dirtyIds()) {
      const res = await this.flush(id);
      if (!res.ok) return res;
    }
    return { ok: true };
  }
}
