/**
 * The graduation log window
 * (`../../../specifications/ui/GLW-graduation-log-window.md`).
 *
 * A modal overlay opened from one stage of one run's stage row (GLW-FR-ALZI).
 * It is bound to that run and that stage and to no other: it holds no run
 * selector, changes no run state, and the only operation it invokes is the read
 * of one page of one stream (GLW-FR-BLWH, GLW-FR-OPQQ).
 *
 * It knows no stage id, no stage label, and no stage count (GLW-FR-MZUP): the
 * stage is the descriptor the row passed it.
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { runTitle } from "../../state/graduation/merge";
import { openingEntry, scopeEntries } from "../../state/graduation/logScopes";
import type { ProgressStage } from "../../state/runProgress";
import type { GraduationLogStream, GraduationRun } from "../../types";
import { Icon } from "../icons";
import {
  EmptyState,
  LoadingState,
  LogRows,
  NoMatchState,
  PersistenceFailureState,
  UnavailableState,
  streamLabel,
} from "./viewport";
import { useLogScope } from "./useLogScope";

/** How long the window waits before a typed query is sent. */
export const SEARCH_DEBOUNCE_MS = 200;
/** How near the end the viewport must be for follow to stay enabled. */
const END_SLACK_PX = 8;
/** How near the leading edge the viewport must be to ask for an older page. */
const OLDER_TRIGGER_PX = 48;

/** GLW-FR-FPUX: the two positions of the stream toggle, and no third. */
const STREAMS: GraduationLogStream[] = ["source", "structured"];

export interface GraduationLogWindowProps {
  /** GLW-FR-BLWH: the run whose stage the author activated, and no other. */
  run: GraduationRun;
  /** GLW-FR-BWOS: the stage descriptor the row passed. It is not changed here. */
  stage: ProgressStage;
  onClose: () => void;
  /** GLW-FR-OOYK: where focus returns when the window closes. */
  returnFocus?: HTMLElement | null;
}

export function GraduationLogWindow({
  run,
  stage,
  onClose,
  returnFocus,
}: GraduationLogWindowProps) {
  const entries = useMemo(() => scopeEntries(run, stage.id), [run, stage.id]);

  // GLW-FR-TMRQ: the window opens on Source, for every run and every stage,
  // and the position is remembered from no other window.
  const [stream, setStream] = useState<GraduationLogStream>("source");
  // GLW-FR-XZQM: the opening selection is settled once, from the index the run
  // record already carries, and no later stream change moves it.
  const [selectedKey, setSelectedKey] = useState<string | null>(
    () => openingEntry(run, stage.id, "source")?.key ?? null,
  );
  const selected =
    entries.find((entry) => entry.key === selectedKey) ?? entries[0] ?? null;

  const [queryInput, setQueryInput] = useState("");
  const [query, setQuery] = useState("");
  useEffect(() => {
    const timer = setTimeout(() => setQuery(queryInput), SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  }, [queryInput]);

  const { state, loadOlder, reload } = useLogScope({
    runId: run.id,
    phaseId: stage.id,
    entry: selected,
    stream,
    query,
  });

  // GLW-FR-KWHL: changing the run, the stage, the scope, or the stream releases
  // follow to its opening position for the new scope, which is the end.
  const [following, setFollowing] = useState(true);
  useEffect(() => {
    setFollowing(true);
  }, [run.id, stage.id, selected?.key, stream]);

  const viewport = useRef<HTMLDivElement | null>(null);
  /**
   * GLW-FR-TCQP: what the viewport measured when an older page was asked for,
   * beside the records it was holding then.
   *
   * The records are part of it because an unrelated re-render — follow being
   * released by the very scroll that asked for the page — must not consume the
   * anchor before the rows it exists to compensate for have arrived.
   */
  const anchor = useRef<{ height: number; held: unknown } | null>(null);
  const overlay = useRef<HTMLDivElement | null>(null);

  // GLW-FR-KKUM: follow keeps the viewport at the end as records arrive.
  // GLW-FR-TCQP: an older page that arrived keeps the reader where they were,
  // so what they are looking at does not move under them.
  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const pending = anchor.current;
    if (pending) {
      // Nothing has arrived yet, so the anchor still has its work to do.
      if (pending.held === state.entries) return;
      node.scrollTop += node.scrollHeight - pending.height;
      anchor.current = null;
      return;
    }
    if (following) node.scrollTop = node.scrollHeight;
  }, [state.entries, following]);

  const onScroll = useCallback(() => {
    const node = viewport.current;
    if (!node) return;
    const fromEnd = node.scrollHeight - node.scrollTop - node.clientHeight;
    // GLW-FR-KTWX: scrolling away from the end releases follow, and scrolling
    // back to it does not resume follow — only the labelled control does.
    if (fromEnd > END_SLACK_PX && following) setFollowing(false);
    if (
      node.scrollTop <= OLDER_TRIGGER_PX &&
      !state.oldestReached &&
      !state.loadingOlder
    ) {
      anchor.current = { height: node.scrollHeight, held: state.entries };
      loadOlder();
    }
  }, [following, loadOlder, state.entries, state.loadingOlder, state.oldestReached]);

  const resumeFollowing = useCallback(() => {
    const node = viewport.current;
    if (node) node.scrollTop = node.scrollHeight;
    setFollowing(true);
  }, []);

  const askForOlder = useCallback(() => {
    const node = viewport.current;
    if (node) anchor.current = { height: node.scrollHeight, held: state.entries };
    loadOlder();
  }, [loadOlder, state.entries]);

  // GLW-FR-ONEV / GLW-FR-OOYK: focus moves into the overlay and stays there,
  // Escape closes it, and closing returns focus to the stage that opened it.
  useEffect(() => {
    const opener = returnFocus ?? (document.activeElement as HTMLElement | null);
    const focusables = () =>
      Array.from(
        overlay.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      );
    focusables()[0]?.focus();
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (event.shiftKey && (active === first || !overlay.current?.contains(active))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && active === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
    // Mount-only: re-running would take focus back to the close control every
    // time an arriving page redrew the viewport.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // GLW-FR-QNHH: moving between the entries of the pass list with the keyboard
  // selects the entry, and the viewport re-reads the newly selected scope.
  const onListKey = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      // The entry the reader is standing on is the one the move starts from,
      // which is the focused one where there is one and the selected one
      // otherwise.
      const focused = (document.activeElement as HTMLElement | null)?.dataset?.entry;
      const from = focused ?? selected?.key;
      const at = entries.findIndex((entry) => entry.key === from);
      if (at < 0) return;
      const next = event.key === "ArrowDown" ? at + 1 : at - 1;
      if (next < 0 || next >= entries.length) return;
      event.preventDefault();
      setSelectedKey(entries[next].key);
      const node = overlay.current?.querySelector<HTMLElement>(
        `[data-entry="${entries[next].key}"]`,
      );
      node?.focus();
    },
    [entries, selected?.key],
  );

  const title = `${runTitle(run)} · ${stage.label}`;

  return (
    <div
      className="scrim"
      onClick={(event) => event.target === event.currentTarget && onClose()}
    >
      <div
        ref={overlay}
        className="modal glw"
        role="dialog"
        aria-modal="true"
        // GLW-FR-OMZA: the run and the stage in the visible title and in the
        // accessible name, so a reader who opened it from one of three progress
        // bars is told which one they are in.
        aria-label={`Log of ${runTitle(run)}, stage ${stage.label}`}
        data-testid="graduation-log-window"
        data-run={run.id}
        data-phase={stage.id}
      >
        <div className="modal__head">
          <div className="modal__title" data-testid="glw-title">
            {title}
          </div>
          <button className="btn btn--icon" aria-label="Close" onClick={onClose}>
            <Icon.X size={12} />
          </button>
        </div>

        <div className="glw__controls">
          {/* GLW-FR-FPUX / GLW-FR-PHBA: two positions, exactly one selected,
              carried in words and in accessible semantics. */}
          <div
            className="glw__streams"
            role="group"
            aria-label="Log stream"
            data-testid="glw-stream-toggle"
          >
            {STREAMS.map((candidate) => (
              <button
                key={candidate}
                type="button"
                className="btn btn--ghost btn--sm"
                aria-pressed={stream === candidate}
                data-selected={stream === candidate ? "true" : undefined}
                onClick={() => setStream(candidate)}
              >
                {streamLabel(candidate)}
              </button>
            ))}
          </div>
          <input
            className="input glw__search"
            type="search"
            aria-label="Search this log"
            placeholder="Search this log"
            value={queryInput}
            onChange={(event) => setQueryInput(event.target.value)}
            data-testid="glw-search"
          />
          {/* GLW-FR-KBZE: the count is the whole scope's rather than the page's. */}
          {query.trim() !== "" && (
            <span className="t-meta" role="status" data-testid="glw-match-count">
              {state.matchedTotal === 1
                ? "1 match"
                : `${state.matchedTotal} matches`}
            </span>
          )}
        </div>

        <div className="glw__body">
          {/* GLW-FR-CKLZ: the passes that entered this stage, and after them
              the run-level entry where the stage holds one. */}
          <div
            className="glw__passes"
            role="group"
            aria-label="Passes and run-level output"
            onKeyDown={onListKey}
            data-testid="glw-pass-list"
          >
            <p className="glw__passes-head t-meta">Passes</p>
            {entries.length === 0 && (
              <p className="t-meta" data-testid="glw-no-passes">
                This stage recorded no pass.
              </p>
            )}
            {entries.map((entry) => (
              <button
                key={entry.key}
                type="button"
                data-entry={entry.key}
                data-selected={entry.key === selected?.key ? "true" : undefined}
                className="glw__pass"
                aria-current={entry.key === selected?.key ? "true" : undefined}
                // GLW-FR-DDXJ: the run-level entry is labelled as run-level and
                // is never named as a pass or numbered as one.
                aria-label={
                  entry.kind === "run_level"
                    ? "Run-level output of this stage"
                    : `Pass ${entry.pass}`
                }
                onClick={() => setSelectedKey(entry.key)}
              >
                {entry.label}
              </button>
            ))}
          </div>

          <div
            className="glw__viewport"
            ref={viewport}
            onScroll={onScroll}
            tabIndex={0}
            aria-label={`${stage.label} log`}
            data-testid="glw-viewport"
            data-following={following ? "true" : "false"}
          >
            {/* GLW-FR-UZAB: loading an older page is its own indication and is
                none of the five states — what is held stays readable. */}
            {state.loadingOlder && (
              <p className="t-meta glw__older" data-testid="glw-loading-older">
                Reading older output…
              </p>
            )}
            {/* GLW-FR-QDWA: once a page reports it holds the scope's oldest
                record, the window states that the beginning is on screen
                rather than leaving the reader to wonder. */}
            {state.oldestReached &&
              state.entries.length > 0 &&
              !state.loadingOlder && (
                <p className="t-meta glw__older" data-testid="glw-oldest-reached">
                  The beginning of this log is on screen.
                </p>
              )}
            {state.olderFailed && (
              <p className="t-meta glw__older" data-testid="glw-older-failed">
                The older output could not be read.{" "}
                <button className="btn btn--ghost btn--sm" onClick={askForOlder}>
                  Try again
                </button>
              </p>
            )}
            <ViewportBody
              entryLabel={
                selected?.kind === "run_level"
                  ? "This stage's run-level output"
                  : (selected?.label ?? "This stage")
              }
              query={query}
              stream={stream}
              state={state}
              onRetry={reload}
            />
          </div>

          {/* GLW-FR-KTWX: the labelled control is the only thing that resumes
              follow, and it states which of its two positions it is in. */}
          <button
            type="button"
            className="btn btn--ghost btn--sm glw__follow"
            aria-pressed={following}
            onClick={resumeFollowing}
            data-testid="glw-follow"
          >
            {following ? "Following" : "Resume following"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** GLW-FR-NMOD: the five states, each rendered distinctly, inside the viewport. */
function ViewportBody({
  entryLabel,
  query,
  state,
  stream,
  onRetry,
}: {
  entryLabel: string;
  query: string;
  state: ReturnType<typeof useLogScope>["state"];
  stream: GraduationLogStream;
  onRetry: () => void;
}) {
  if (state.loading && state.entries.length === 0) return <LoadingState />;
  if (state.readError) {
    return (
      <>
        <UnavailableState failure={null} message={state.readError} />
        <button className="btn btn--ghost btn--sm" onClick={onRetry}>
          Try again
        </button>
      </>
    );
  }
  if (state.status === "persistence_failed") {
    return <PersistenceFailureState failure={state.failure} />;
  }
  if (state.status === "unavailable") {
    return <UnavailableState failure={state.failure} />;
  }
  if (state.search === "search_no_match") return <NoMatchState query={query} />;
  if (state.entries.length === 0) {
    return <EmptyState entryLabel={entryLabel} stream={stream} />;
  }
  return <LogRows entries={state.entries} stream={stream} />;
}
