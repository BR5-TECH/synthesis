/**
 * The agent-output section of the Runs bottom panel
 * (`../../specifications/ui/RUN-runs.md`).
 *
 * What the agent CLI is doing, line by line, while it does it. A graduation turn
 * runs unattended for many minutes inside a container nobody can see into, so
 * this is the only place an author can tell a turn that is working from one that
 * is wedged — and, when one fails, the only place the reason is legible without
 * reproducing the container by hand.
 *
 * Out of scope: it starts nothing (RUN-FR-09), it decides nothing about a run,
 * and it masks nothing — every record arrives already masked by the executor,
 * which is the only place that holds the credentials to mask against
 * (AGV-FR-04).
 */
import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import { onAgentActivityAppended } from "../events";
import type { AgentActivityKind, AgentActivityRecord } from "../types";
import { PanelEmptyState } from "./PanelEmptyState";

interface RunsProps {
  /**
   * RUN-FR-02: the run whose output this shows — the working graduation run
   * where there is one, and otherwise the most recent. `null` when the project
   * has no run at all, which is the empty state.
   *
   * Chosen outside this component and never by anything the user can click
   * here: no surface of the application starts a run (RUN-FR-09), so this panel
   * is a reader of activity it never initiates.
   */
  runId: string | null;
  /** RUN-FR-04: whether that run is still working, for the live indicator. */
  live: boolean;
  /** What the run is called, for the header. */
  label?: string | null;
}

/**
 * RUN-FR-03: which level treatment a kind takes.
 *
 * The kinds are the backend's (EAC-FR-33) and this maps them onto the four the
 * row already has, so a stream reads at a glance: what went wrong is red, what
 * the agent decided is accented, and its narration is plain.
 */
const LEVEL: Record<string, "info" | "ok" | "warn" | "err" | "step"> = {
  invocation: "step",
  task: "step",
  started: "step",
  reasoning: "info",
  message: "info",
  tool_call: "step",
  tool_result: "info",
  command: "step",
  file_change: "ok",
  retry: "warn",
  usage: "info",
  finished: "ok",
  error: "err",
  diagnostic: "warn",
  unrecognized: "warn",
};

/** The row's own short label for a kind. Wider than the level, and specific. */
function kindLabel(kind: AgentActivityKind | string): string {
  return kind.replace(/_/g, " ");
}

/** RUN-FR-11: the local wall-clock time a record was written. */
function clockOf(at: string): string {
  const when = new Date(at);
  if (Number.isNaN(when.getTime())) return "";
  return when.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function Runs({ runId, live, label }: RunsProps) {
  const [records, setRecords] = useState<AgentActivityRecord[]>([]);
  const [dropped, setDropped] = useState(0);
  const [expanded, setExpanded] = useState<number[]>([]);
  /**
   * RUN-FR-08: whether the reader dismissed what was on screen.
   *
   * Held apart from "there is nothing to show", because the two are different
   * statements and only one of them is the panel's to make: a finished run whose
   * output was cleared has not recorded no output, and saying so would be the
   * panel reporting its own state as the run's.
   */
  const [cleared, setCleared] = useState(false);
  /** RUN-FR-12: whether the list is pinned to the newest record. */
  const [following, setFollowing] = useState(true);

  const log = useRef<HTMLDivElement>(null);
  const alive = useRef(true);
  /**
   * The newest sequence this panel holds, read by the event handler.
   *
   * A ref rather than state: the handler is registered once per run and would
   * otherwise close over the cursor as it was at registration, asking for the
   * same delta forever.
   */
  const cursor = useRef(0);
  /**
   * The run every ref above belongs to.
   *
   * A read is asynchronous and the run it was for can be replaced while it is in
   * flight — a graduation queue moves on its own, so this is the ordinary case
   * rather than a race a user has to provoke. Without this check the answer to
   * the *previous* run's read is merged into the current run's list, and its
   * cursor is adopted as the current run's, after which the current run's own
   * sequences all read as older than what the panel thinks it holds and its
   * events are ignored for as long as it lasts.
   */
  const showing = useRef<string | null>(null);
  /**
   * Whether a read is in flight, and whether one was asked for while it was.
   *
   * A working agent emits events in bursts, and one read per event would put a
   * round trip on the wire for each of them while the previous was still
   * running. One read at a time with a single trailing read collapses a burst
   * into two calls without ever ending on a stale view: the trailing read is
   * what guarantees the last event of a burst is not the one nobody asked for.
   */
  const reading = useRef(false);
  const pending = useRef(false);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  /**
   * RUN-FR-13: read what has happened since the cursor, or the newest page when
   * there is no cursor yet.
   *
   * Everything the panel shows comes from this call. Nothing is rendered from an
   * event payload, so what is on screen is what the store holds rather than what
   * an emitter believed (AGV-FR-11).
   */
  const pull = useCallback(async () => {
    const id = showing.current;
    if (!id) return;
    if (reading.current) {
      pending.current = true;
      return;
    }
    // Taken, so the trailing read below fires only for a request that arrived
    // *during* this one. Leaving it set would make every read produce a second,
    // identical read that can only ever return nothing.
    pending.current = false;
    reading.current = true;
    try {
      const page = await api.readAgentActivity(
        id,
        cursor.current > 0 ? cursor.current : undefined,
      );
      // The run this read was for is no longer the run being shown, so its
      // answer belongs to nothing on screen. Discarded rather than merged: the
      // records are another run's, and so is the cursor they would set.
      if (!alive.current || showing.current !== id) return;
      setDropped(page.dropped);
      if (page.records.length === 0) return;
      cursor.current = page.records[page.records.length - 1].seq;
      setRecords((held) => {
        // A backfill with no cursor replaces; a delta appends. Deduplicated by
        // sequence either way, because a re-read that overlaps what is held is
        // the normal outcome of an event arriving while one is in flight.
        const seen = new Set(held.map((r) => r.seq));
        const fresh = page.records.filter((r) => !seen.has(r.seq));
        return fresh.length === 0 ? held : [...held, ...fresh];
      });
    } catch {
      // A run the backend does not know about reads as nothing rather than as a
      // failure: the panel opens on a run before its first line arrives.
    } finally {
      reading.current = false;
      // Whatever run is being shown *now*, not the one this read was for. A
      // trailing read that went back to the previous run would be the same
      // defect one step later, and would leave the current run never read at
      // all.
      if (pending.current && alive.current && showing.current) {
        pending.current = false;
        void pull();
      }
    }
  }, []);

  // RUN-FR-13: the run changed, so everything held belongs to another one.
  useEffect(() => {
    showing.current = runId;
    cursor.current = 0;
    setRecords([]);
    setExpanded([]);
    setDropped(0);
    setCleared(false);
    setFollowing(true);
    if (!runId) {
      pending.current = false;
      return;
    }
    // Asked for rather than issued directly: a read for the previous run may
    // still be in flight, and reading this one is what its completion must go on
    // to do.
    pending.current = true;
    void pull();
  }, [runId, pull]);

  // RUN-FR-03: new records arrive by event, and the panel re-reads rather than
  // rendering the event.
  useEffect(() => {
    if (!runId) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onAgentActivityAppended((state) => {
      if (state.runId !== runId) return;
      // Any event for this run is a reason to read, without comparing what it
      // carries against the cursor. Two lines recorded at once are announced by
      // two threads, so the sequence one event carries can be *lower* than one
      // already announced — and a panel that skipped on that comparison would
      // skip the read that was going to catch up. Reading when there is nothing
      // new costs one call that returns nothing, and the in-flight guard above
      // collapses a burst of them into two.
      void pull();
    }).then((off) => {
      if (cancelled) off();
      else unlisten = off;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [runId, pull]);

  // RUN-FR-12: while the pin holds, the newest record stays in view.
  useEffect(() => {
    if (!following) return;
    const node = log.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [records, following]);

  /**
   * RUN-FR-12: scrolling up releases the pin, and scrolling back to the foot
   * resumes it — so reading something older is never fought by arriving output.
   */
  const onScroll = () => {
    const node = log.current;
    if (!node) return;
    const atFoot =
      node.scrollHeight - node.scrollTop - node.clientHeight < FOOT_SLACK;
    setFollowing(atFoot);
  };

  if (!runId) {
    return (
      <div className="runs-empty">
        <PanelEmptyState line="No run">
          Runs are started outside this window; when one begins, its output
          streams here.
        </PanelEmptyState>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="runs-head">
        {/* RUN-FR-04: gated on the run's actual state rather than on this
            surface being open. */}
        {live ? (
          <>
            <span className="dot dot--live" />
            <span className="badge badge--live" role="status">
              running
            </span>
          </>
        ) : (
          <span className="badge badge--ok" role="status">
            finished
          </span>
        )}
        <span className="t-ui-sm" style={{ fontWeight: 600 }}>
          {label ?? runId}
        </span>
        <span className="t-meta">
          {records.length} {records.length === 1 ? "event" : "events"}
        </span>
        <span className="spacer" />
        {/* RUN-FR-12: visible only while the pin is released, which is the only
            time resuming it means anything. */}
        {!following && (
          <button
            className="btn btn--ghost btn--sm"
            onClick={() => setFollowing(true)}
          >
            Follow
          </button>
        )}
        {/* RUN-FR-08: clears what this panel is showing. The run's own record is
            the backend's and is untouched — a reader who clears the view has
            hidden it, not deleted it, and re-selecting the run brings it
            back. */}
        <button
          className="btn btn--ghost btn--sm"
          onClick={() => {
            setRecords([]);
            setExpanded([]);
            setCleared(true);
          }}
        >
          Clear
        </button>
      </div>
      {/* AGV-FR-05: a stream that outgrew what the backend keeps in memory says
          so at its head, so a reader knows this is not where the run began. */}
      {dropped > 0 && (
        <div className="runs-notice t-meta" role="note">
          {dropped} earlier {dropped === 1 ? "event is" : "events are"} no longer
          held in memory. The run's own file has all of them.
        </div>
      )}
      <div
        className="runs-log"
        ref={log}
        onScroll={onScroll}
        style={{ flex: 1, overflow: "auto" }}
        aria-label="Agent output"
      >
        {records.length === 0 && (
          <div className="runs-line">
            <span className="runs-line__msg t-meta">
              {cleared
                ? "Output cleared. The run's own record is unchanged."
                : live
                  ? "Waiting for the agent's first line…"
                  : "This run recorded no output."}
            </span>
          </div>
        )}
        {records.map((record) => {
          const open = expanded.includes(record.seq);
          return (
            <div key={record.seq}>
              <div className="runs-line">
                <span className="runs-line__ts">{clockOf(record.at)}</span>
                <span
                  className="runs-line__lvl"
                  data-level={LEVEL[record.kind] ?? "info"}
                  title={record.channel}
                >
                  {kindLabel(record.kind)}
                </span>
                {/* RUN-FR-14: the whole event is one activation away. The
                    summary is this build's reading of it; the payload is what
                    the vendor actually wrote, which is what a reader needs when
                    the reading is the thing that is wrong. */}
                <button
                  type="button"
                  className="runs-line__msg runs-line__expand"
                  aria-expanded={open}
                  onClick={() =>
                    setExpanded((held) =>
                      held.includes(record.seq)
                        ? held.filter((s) => s !== record.seq)
                        : [...held, record.seq],
                    )
                  }
                >
                  {record.summary}
                </button>
              </div>
              {open && (
                <pre className="runs-payload">
                  {record.payload}
                  {record.payloadTruncated ? "\n…(event truncated)" : ""}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/**
 * How near the foot counts as "at the foot" (RUN-FR-12).
 *
 * A few pixels of slack rather than an equality: sub-pixel layout and a
 * fractional scroll position mean the arithmetic rarely lands exactly on zero,
 * and an equality would release the pin the moment the first record arrived.
 */
const FOOT_SLACK = 8;
