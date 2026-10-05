/**
 * The graduation section of the Runs panel
 * (`../../../specifications/ui/GRU-graduation-runs.md`).
 *
 * A history rail listing every run the project has made, and beside it the
 * region for the selected run: what stream it runs in, what stage it stands
 * at, what it changed, what it waits for, and what the author may do about it.
 * The two are divided by a boundary the author moves, the rail taking the
 * fraction of the section they set (GRH-FR-MCHQ).
 *
 * It starts no graduation. That begins from a draft's own **Graduate** action
 * (GRU-FR-GLSO).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../../api";
import {
  onGraduationLogRecordsAppended,
  onGraduationQueueChanged,
  onGraduationRunChanged,
} from "../../events";
import { logDebug, logError, logInfo, logWarn } from "../../logging";
import { loadAppPreferences, patchAppPreferences } from "../../state/appPreferences";
import { forgetRunsExcept } from "../../state/escalationDrafts";
import {
  aheadOf,
  coalesce,
  graduationErrorMessage,
  readableCapacity,
  waitsForSlot,
  raiseStatement,
  runTitle,
  stageDescriptorsFor,
  stateLabelOf,
} from "../../state/graduation";
import {
  admitted,
  clampRailFraction,
  DEFAULT_RAIL_FRACTION,
  DEFAULT_VIEW,
  fractionForDrag,
  fractionForKey,
  isGraduationView,
  MAX_RAIL_FRACTION,
  MIN_RAIL_FRACTION,
  railPercent,
  revealingText,
  revealingView,
  type GraduationView,
} from "../../state/graduationRail";
import {
  recallFilters,
  recallRun,
  rememberFilters,
  rememberRoute,
  rememberRun,
  routeAnswered,
  type RailFilters,
} from "../../state/graduationSelection";
import type {
  GraduationCapacity,
  GraduationQueue,
  GraduationRun,
} from "../../types";
import { GraduationLogWindow } from "../GraduationLogWindow";
import { awaitsLiveSegment, stagesWithLogAccess } from "../../state/graduation/logScopes";
import { type PressedAction } from "./actionRow";
import { RunRegion } from "./region";
import { GraduationRail } from "./rail";

export interface GraduationRunsProps {
  /** GRU-FR-RZDI: the route to a run's source draft. */
  onOpenDraft?: (draftId: string) => void;
  /**
   * A run the shell asked to be shown, from a notification or a draft row.
   *
   * `nonce` identifies the request, so one the section has already answered is
   * told from a fresh one and is not replayed when the section is mounted
   * again.
   */
  selectRun?: { runId: string; nonce?: number; override?: boolean } | null;
  /** NTF-FR-39: which run is selected, reported upwards. */
  onSelectionChange?: (runId: string | null) => void;
  /**
   * GLW-FR-ALZI / SNV-FR-56: this section is about to mount a floating overlay.
   *
   * The window's overlays are mutually exclusive, so the shell takes down
   * whatever else is open rather than letting two action surfaces coexist.
   */
  onOverlayOpening?: () => void;
  /**
   * GRU-FR-QLRQ: open Project settings on a named section. A run stopped at
   * its time limit routes the author to the section that sets the limit.
   */
  onOpenSettings?: (section: string) => void;
}

export function GraduationRuns({
  onOpenDraft,
  selectRun,
  onSelectionChange,
  onOverlayOpening,
  onOpenSettings,
}: GraduationRunsProps) {
  const [queue, setQueue] = useState<GraduationQueue | null>(null);
  /** The listing as last read, for the log-append listener (GRU-FR-QKSY). */
  const queueRef = useRef<GraduationQueue | null>(null);
  /**
   * GRU-FR-QKSY: the runs a log append named while a read of the listing was
   * in flight. That read can answer from before the append, so each is checked
   * again against the listing it brings.
   */
  const readsInFlight = useRef(0);
  const heardDuringRead = useRef(new Set<string>());
  const recheckAppends = useRef<((runIds: string[]) => void) | null>(null);
  // GRU-FR-QKDB: what the project's slots hold. Null is a capacity that was not
  // read, and a queued run then says nothing about a slot.
  const [capacity, setCapacity] = useState<GraduationCapacity | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [view, setView] = useState<GraduationView>(DEFAULT_VIEW);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // GRU-FR-KQPE: the refusal of an act a run's action row issued, held with the
  // run it belongs to.
  const [actionRefusal, setActionRefusal] = useState<{
    runId: string;
    message: string;
  } | null>(null);
  // GRU-FR-ZMHB: the action row control an act in flight was issued from.
  const pressedRef = useRef<PressedAction | null>(null);
  /**
   * GRU-FR-HXNC: the stage whose log window is open, and the entry that opened
   * it. Held per run and per stage, so opening another run's stage reuses
   * nothing of the previous window (GLW-FR-BVYN).
   */
  const [openLog, setOpenLog] = useState<{
    runId: string;
    stageId: string;
    opener: HTMLElement | null;
  } | null>(null);
  const announceRef = useRef<HTMLParagraphElement>(null);
  /** What is selected right now, for effects that must not depend on it. */
  const selectedRef = useRef<string | null>(null);
  /** The last selection announced, so a reload announces nothing again. */
  const announcedRef = useRef<string | null>(null);

  /** GRU-FR-ZVTC: the section renders what the backend reports. */
  const reload = useCallback(async () => {
    // GRU-FR-QKDB: the capacity is read with the queue, on every reload. A read
    // that fails is handled here and never costs the queue its own read.
    const capacityRead = Promise.resolve()
      .then(() => api.getGraduationCapacity())
      .then((value) => readableCapacity(value))
      .catch(() => {
        logWarn(["frontend"], "the graduation capacity could not be read", {});
        return null;
      });
    readsInFlight.current += 1;
    try {
      const next = await api.listGraduationQueue();
      queueRef.current = next;
      setQueue(next);
      setCapacity(await capacityRead);
      // GEA-FR-YFMY: an unsent answer belongs to a run that is still in the
      // project's listing. A run that has left it takes its draft with it.
      forgetRunsExcept(next.runs.map((entry) => entry.id));
      setError(null);
    } catch (reason) {
      setError(graduationErrorMessage(String(reason)));
      logError(["frontend"], "the graduation queue could not be read", {});
      setCapacity(await capacityRead);
    } finally {
      readsInFlight.current -= 1;
      if (readsInFlight.current === 0 && heardDuringRead.current.size > 0) {
        const runIds = [...heardDuringRead.current];
        heardDuringRead.current.clear();
        recheckAppends.current?.(runIds);
      }
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    // GRD-FR-EFAU: one turn emits many events, and each is a real change. They
    // are gathered into one read rather than one read each.
    const coalesced = coalesce(() => void reload());
    // GRU-FR-QKSY: an append reloads only where it can open a stage. A working
    // turn appends many records and saves its run only at its end.
    const appended = (runId: string) => {
      const run = queueRef.current?.runs.find((entry) => entry.id === runId);
      if (!run || !awaitsLiveSegment(run)) return;
      logDebug(["frontend"], "a log append can open a stage of a listed run", { runId });
      coalesced.ask();
    };
    recheckAppends.current = (runIds) => runIds.forEach(appended);
    const unsubscribers: Array<Promise<() => void>> = [
      onGraduationQueueChanged(coalesced.ask),
      onGraduationRunChanged(coalesced.ask),
      onGraduationLogRecordsAppended((payload) => {
        if (readsInFlight.current > 0) heardDuringRead.current.add(payload.runId);
        appended(payload.runId);
      }),
    ];
    return () => {
      recheckAppends.current = null;
      coalesced.cancel();
      for (const pending of unsubscribers) void pending.then((off) => off());
    };
  }, [reload]);

  const runs = useMemo(() => queue?.runs ?? [], [queue]);

  /**
   * What the two controls hold, for the effects that reveal a run.
   *
   * Synchronised after every commit rather than during a render — a render
   * React discards must leave nothing behind — and written eagerly by an effect
   * that changes either, so a second effect in the same flush reads what the
   * first one set rather than what the last commit held. This effect is
   * declared before both of them, so a flush starts from the committed values.
   */
  const viewRef = useRef<GraduationView>(view);
  const filterRef = useRef(filter);
  useEffect(() => {
    viewRef.current = view;
    filterRef.current = filter;
  }, [view, filter]);

  /**
   * GRH-FR-XBWU: put a run the author is sent to in front of them, by relaxing
   * only the filters that hide it.
   *
   * `base` is what the two controls hold at the moment of the reveal, given
   * rather than read, because a caller that has just restored a project's
   * remembered filters must relax those rather than the ones they replaced.
   */
  const reveal = useCallback((target: GraduationRun, base: RailFilters) => {
    const next: RailFilters = {
      view: revealingView(target, base.view),
      text: revealingText(target, base.text, stateLabelOf),
    };
    // A relaxation moves a control the author set, without them touching it, so
    // a rail that "changed its own filter" is readable afterwards. The run's id
    // and the two positions only — a draft name and a prompt are the author's
    // own words, and the entered text is theirs too, so its length stands for
    // it.
    if (next.view !== base.view || next.text !== base.text) {
      logInfo(["frontend"], "the run history relaxed a filter to show a run", {
        runId: target.id,
        fromView: base.view,
        toView: next.view,
        clearedTextLength: next.text === base.text ? 0 : base.text.length,
      });
    }
    setView(next.view);
    viewRef.current = next.view;
    setFilter(next.text);
    filterRef.current = next.text;
  }, []);

  // GRH-FR-OFHS: the selected row is announced when it changes, so a reader
  // moving down the rail by keyboard hears which run the region beside it now
  // holds. A reload that changes nothing announces nothing.
  useEffect(() => {
    if (announcedRef.current === selected) return;
    announcedRef.current = selected;
    if (!announceRef.current) return;
    const chosen = runs.find((entry) => entry.id === selected);
    announceRef.current.textContent = chosen
      ? `Showing “${runTitle(chosen)}”.`
      : "No run is selected.";
  }, [selected, runs]);

  /** GRH-FR-WIMY / GRH-FR-BDMB: the view in force, then the text filter. */
  const listed = useMemo(
    () => admitted(runs, view, filter, stateLabelOf),
    [runs, view, filter],
  );

  // GRH-FR-WDSH: a selection naming a run the listing no longer holds falls
  // back to the first row, and to nothing where the filters admit none. The
  // fallback reveals nothing: a rail that opened in In-flight stays there and
  // renders its empty result, rather than relaxing itself to show a run the
  // author never asked for.
  useEffect(() => {
    if (!queue) return;
    if (selected && listed.some((run: GraduationRun) => run.id === selected)) return;
    const next = listed[0]?.id ?? null;
    if (next === selected) return;
    // The selection moves without the author asking, and a rail that admits
    // nothing leaves them with no run at all — the most surprising thing this
    // rail does, so it is on the record. Ids and counts alone: a draft name is
    // the author's own words.
    logDebug(["frontend"], "the run history fell back to another selection", {
      from: selected ?? "",
      to: next ?? "",
      view,
      listed: listed.length,
    });
    setSelected(next);
  }, [queue, listed, selected, view]);

  useEffect(() => {
    selectedRef.current = selected;
    onSelectionChange?.(selected);
    // GRH-FR-ODLT: remembered per project, so leaving the section for the
    // agent output and coming back leaves the reader where they were.
    if (queue?.projectKey && selected) rememberRun(queue.projectKey, selected);
  }, [selected, onSelectionChange, queue?.projectKey]);

  // GRH-FR-ODLT / GRH-FR-QVEX: restore what this project was last reading and
  // last narrowed by, once. A project this session holds nothing for keeps the
  // resting position and the empty text filter it mounted with.
  const restoredRef = useRef(false);
  useEffect(() => {
    if (restoredRef.current || !queue?.projectKey) return;
    restoredRef.current = true;
    const held = recallFilters(queue.projectKey);
    let filters: RailFilters = { view: DEFAULT_VIEW, text: "" };
    if (held !== null) {
      if (isGraduationView(held.view) && typeof held.text === "string") {
        filters = held;
      } else {
        // A held value this build does not recognise is dropped rather than
        // restored. It is handled, so it is otherwise invisible.
        logWarn(["frontend"], "a held run-history filter was not recognised", {
          view: String(held.view),
        });
      }
    }
    setView(filters.view);
    viewRef.current = filters.view;
    setFilter(filters.text);
    filterRef.current = filters.text;
    // Written back at once as well as by the effect below, which would
    // otherwise run later in this same flush and put the mounted defaults over
    // what has just been recalled.
    rememberFilters(queue.projectKey, filters);
    logDebug(["frontend"], "the run history restored what this project holds", {
      view: filters.view,
      textLength: filters.text.length,
      hasRun: recallRun(queue.projectKey) !== null,
    });

    const run = recallRun(queue.projectKey);
    if (!run) return;
    setSelected(run);
    // GRH-FR-XBWU: the run the author was left reading is one they are put in
    // front of, so a restored filter that hides it is relaxed — and one that
    // admits it is left exactly as they set it.
    const target = runs.find((entry) => entry.id === run);
    if (target) {
      reveal(target, filters);
    } else {
      // The run this project was left reading has left the listing. Nothing is
      // revealed, and GRH-FR-WDSH takes the selection instead.
      logWarn(["frontend"], "the remembered run is no longer listed", {
        runId: run,
        listed: runs.length,
      });
    }
  }, [queue, runs, reveal]);

  // GRH-FR-QVEX: the narrowing is remembered with the selection.
  useEffect(() => {
    if (!queue?.projectKey || !restoredRef.current) return;
    rememberFilters(queue.projectKey, { view, text: filter });
  }, [queue?.projectKey, view, filter]);

  /**
   * GRU-FR-PAHN: the run a request named that the listing did not hold, even
   * after the one reload the request is owed.
   */
  const [notFound, setNotFound] = useState<string | null>(null);
  /** The request whose one reload is under way, so it asks for no second. */
  const reloadingFor = useRef<object | null>(null);

  useEffect(() => {
    if (!selectRun) return;
    // Nothing is known about what this project was left reading until the
    // first listing has been read, and a route honoured before that would take
    // a selection the restore below was about to make — and be remembered in
    // its place.
    if (!queue) return;
    // A request the section has already answered is not answered again. The
    // section is unmounted every time the author looks at the agent output, so
    // without this the same request would be replayed on every return — moving
    // the selection the author has made since, and now relaxing the filters
    // they have set as well (GRH-FR-XBWU).
    if (routeAnswered(selectRun.nonce)) return;
    const target = runs.find((entry) => entry.id === selectRun.runId);
    if (!target) {
      // GRU-FR-PAHN: a run the listing does not hold makes the section reload
      // the listing once. A run still absent after it changes no selection, and
      // the section says that the run is not found.
      if (reloadingFor.current === selectRun) return;
      reloadingFor.current = selectRun;
      logInfo(["frontend"], "a named run is not listed, so the listing is read once", {
        runId: selectRun.runId,
        listed: runs.length,
      });
      const request = selectRun;
      void api
        .listGraduationQueue()
        .then((next) => {
          if (reloadingFor.current !== request) return;
          queueRef.current = next;
          setQueue(next);
          forgetRunsExcept(next.runs.map((entry) => entry.id));
          if (next.runs.some((entry) => entry.id === request.runId)) return;
          rememberRoute(request.nonce);
          setNotFound(request.runId);
          logWarn(["frontend"], "a named run is not in the run history", {
            runId: request.runId,
            listed: next.runs.length,
          });
        })
        .catch(() => {
          if (reloadingFor.current !== request) return;
          rememberRoute(request.nonce);
          setNotFound(request.runId);
          logError(["frontend"], "the listing a named run asked for could not be read", {
            runId: request.runId,
          });
        });
      return;
    }
    rememberRoute(selectRun.nonce);
    setNotFound(null);
    // GRH-FR-ODLT: `override: false` is a route that must not move a reader.
    // A newly started run asks to be shown, but the author already reading
    // another run keeps their place; a request that names no preference takes
    // the selection like any other route.
    //
    // What the project was left reading counts as a reader too: the section is
    // unmounted every time the author looks at the agent output, so a route
    // arriving on the way back would otherwise take a selection that only
    // looks absent because the restore has not run yet.
    if (selectRun.override === false) {
      const remembered = queue?.projectKey ? recallRun(queue.projectKey) : null;
      if (selectedRef.current || remembered) return;
    }
    setSelected(selectRun.runId);
    // GRH-FR-XBWU / GRU-FR-PAHN: a run another surface names is revealed, so
    // the row the author is routed to is one they can see beside the run they
    // are reading.
    reveal(target, { view: viewRef.current, text: filterRef.current });
  }, [selectRun, queue, runs, reveal]);

  // GRH-FR-MCHQ: the rail's width, as the author last set it. It is read once
  // and written only when a gesture ends, so a drag costs one write rather
  // than one per frame.
  const [railFraction, setRailFraction] = useState(DEFAULT_RAIL_FRACTION);
  const sectionRef = useRef<HTMLDivElement | null>(null);
  const draggingRef = useRef(false);
  /** Where the gesture under way has put the boundary. */
  const draggedRef = useRef(DEFAULT_RAIL_FRACTION);
  useEffect(() => {
    void loadAppPreferences().then((prefs) => {
      setRailFraction(clampRailFraction(prefs.graduationRailWidthFraction));
    });
  }, []);
  const persistRail = useCallback((fraction: number) => {
    setRailFraction(fraction);
    void patchAppPreferences({ graduationRailWidthFraction: fraction }).catch(
      () => {
        logError(["frontend"], "the history width could not be stored", {});
      },
    );
  }, []);
  const widthAt = useCallback((clientX: number) => {
    const box = sectionRef.current?.getBoundingClientRect();
    if (!box) return null;
    return fractionForDrag(clientX - box.left, box.width);
  }, []);

  const run = listed.find((r: GraduationRun) => r.id === selected) ?? null;

  /** GRT-FR-AMHS: the run whose restart confirmation the rail asked to open. */
  const [restartAsk, setRestartAsk] = useState<string | null>(null);

  /**
   * One act on a run. `pressed` names the action-row control that issued it,
   * where one did: its refusal then renders beside that row (GRU-FR-KQPE), and
   * focus returns to that row when the act settles (GRU-FR-ZMHB). An act issued
   * from anywhere else is refused against the run region as a whole
   * (GRU-FR-XQVG). `onRefused` hands the refusal to a control that renders it
   * itself. The answer says whether the act was accepted.
   */
  const act = async (
    what: string,
    action: () => Promise<unknown>,
    pressed?: PressedAction,
    onRefused?: (message: string) => void,
  ): Promise<boolean> => {
    if (busy) return false;
    pressedRef.current = pressed ?? null;
    setBusy(true);
    setError(null);
    if (pressed) setActionRefusal(null);
    try {
      await action();
      await reload();
      if (announceRef.current) announceRef.current.textContent = what;
      return true;
    } catch (reason) {
      logError(["frontend"], "a graduation act was refused", {});
      const message = graduationErrorMessage(String(reason));
      // GRU-FR-KQPE: one refusal alert at most, and a refusal renders only
      // while the run it belongs to is selected.
      if (onRefused) {
        setActionRefusal(null);
        onRefused(message);
      } else if (pressed) {
        if (selectedRef.current === pressed.runId) {
          setActionRefusal({ runId: pressed.runId, message });
        }
      } else {
        setActionRefusal(null);
        setError(message);
      }
      return false;
    } finally {
      setBusy(false);
    }
  };

  // A refusal belongs to the run it was about, so selecting another run drops it.
  // So does a press: focus never returns into a run the author left. This runs
  // after the action row's own effect, which has taken a press it owns.
  useEffect(() => {
    setActionRefusal(null);
    if (pressedRef.current && pressedRef.current.runId !== selected) {
      pressedRef.current = null;
    }
  }, [selected]);

  /**
   * GRU-FR-HXNC / GLW-FR-BWOS: the run and the stage descriptor the open log
   * window is bound to.
   *
   * A window whose run has left the listing closes with it rather than reading
   * a run the section no longer holds.
   */
  const logWindow = useMemo(() => {
    if (!openLog) return null;
    const target = runs.find((entry) => entry.id === openLog.runId);
    if (!target) return null;
    const stages = stagesWithLogAccess(stageDescriptorsFor(target), target.logs);
    const stage = stages.find((entry) => entry.id === openLog.stageId);
    // GRU-FR-VKPD: a stage that holds no segment opens nothing.
    if (!stage?.activatable) return null;
    return { run: target, stage };
  }, [openLog, runs]);

  /**
   * GRH-FR-FYTH: where a queued run stands in its own stream's queue. GRU-FR-KMNF:
   * a run that waits for a project slot says so beside its place.
   */
  const positionOf = useCallback(
    (entry: GraduationRun) => {
      if (entry.state !== "queued" || !queue) return null;
      const place = positionLabel(aheadOf(queue, entry.id));
      if (!waitsForSlot(entry, capacity)) return place;
      return place ? `${place} · project slot` : "Project slot";
    },
    [queue, capacity],
  );

  return (
    <div
      className="graduation"
      data-testid="graduation-section"
      ref={sectionRef}
      // GRH-FR-MCHQ: the fraction, applied as the rail's own width. The run
      // region takes what is left and reflows within it, so no width the clamp
      // admits gives either region a horizontal scrollbar.
      style={
        {
          "--graduation-rail": `${railPercent(railFraction)}%`,
        } as React.CSSProperties
      }
    >
      <GraduationRail
        runs={runs}
        listed={listed}
        selected={selected}
        view={view}
        onView={setView}
        filter={filter}
        onFilter={setFilter}
        busy={busy}
        onSelect={(runId) => {
          setNotFound(null);
          setSelected(runId);
        }}
        positionOf={positionOf}
        onPause={(entry) =>
          void act("Paused the run.", () => api.pauseGraduationRun(entry.id))
        }
        onContinue={(entry) =>
          void act("Continued the run.", () =>
            api.continueGraduationRun(entry.id),
          )
        }
        onAutoStart={(entry, enabled) =>
          void act(
            enabled ? "The run may start." : "The run will not start on its own.",
            () => api.setGraduationAutoStart(entry.id, enabled),
          )
        }
        onArchive={(entry, archived) =>
          void act(archived ? "Filed the run away." : "Restored the run.", () =>
            archived
              ? api.archiveGraduationRun(entry.id)
              : api.unarchiveGraduationRun(entry.id),
          )
        }
        // GRT-FR-AMHS: the rail's Restart is the region's act, so it selects
        // the discarded run and opens the one confirmation there.
        onRestart={(entry) => {
          setSelected(entry.id);
          setRestartAsk(entry.id);
        }}
      />

      {/* GRH-FR-MCHQ / GRH-FR-OFHS: the boundary the author drags, and the
          same width by keyboard alone. A separator carrying its name and its
          current width as a percentage. */}
      <div
        className="graduation__divider"
        role="separator"
        aria-orientation="vertical"
        aria-label="History width"
        aria-valuenow={railPercent(railFraction)}
        aria-valuemin={railPercent(MIN_RAIL_FRACTION)}
        aria-valuemax={railPercent(MAX_RAIL_FRACTION)}
        aria-valuetext={`History width ${railPercent(railFraction)} percent`}
        title="Drag, or use the arrow keys, to set the history width"
        tabIndex={0}
        data-testid="graduation-divider"
        onPointerDown={(event) => {
          draggingRef.current = true;
          draggedRef.current = railFraction;
          event.currentTarget.setPointerCapture?.(event.pointerId);
        }}
        onPointerMove={(event) => {
          if (!draggingRef.current) return;
          const next = widthAt(event.clientX);
          if (next === null) return;
          // Held in state and in a ref while the pointer is down: the width is
          // written once, when the gesture ends, and the value written is the
          // one the last move reached rather than whatever the render that
          // installed this handler was holding.
          draggedRef.current = next;
          setRailFraction(next);
        }}
        onPointerUp={(event) => {
          if (!draggingRef.current) return;
          draggingRef.current = false;
          event.currentTarget.releasePointerCapture?.(event.pointerId);
          persistRail(draggedRef.current);
        }}
        onPointerCancel={() => {
          draggingRef.current = false;
        }}
        onKeyDown={(event) => {
          const next = fractionForKey(event.key, railFraction);
          if (next === null) return;
          event.preventDefault();
          persistRail(next);
        }}
      />

      <section
        className="graduation__run"
        aria-label="The selected run"
        // Which run the region is rendering, for a reader and for a test that
        // asks the DOM rather than inferring it from what is inside.
        data-run={run?.id}
        // GRU-FR-FZCN: while the run waits on the author, the question is what
        // the region is for, so the layout gives it the room rather than the
        // account of how the run got here.
        data-awaiting={run?.escalation ? "true" : undefined}
      >
        {error && (
          <p
            className="graduation__error graduation__warning"
            role="alert"
            data-testid="graduation-error"
          >
            {error}
          </p>
        )}

        {/* GRU-FR-PAHN: a request for a run the listing does not hold, said
            once, with the selection left where it was. */}
        {notFound && (
          <p
            className="graduation__warning"
            role="status"
            data-testid="graduation-run-not-found"
          >
            That run was not found.
          </p>
        )}

        {run && (
          <RunRegion
            run={run}
            runs={runs}
            queue={queue}
            capacity={capacity}
            busy={busy}
            act={act}
            onOpenDraft={onOpenDraft}
            onOpenSettings={onOpenSettings}
            onActivateStage={(stageId) => {
              // GLW-FR-ALZI: the log window is one of the window's floating
              // overlays, so opening it closes any other.
              onOverlayOpening?.();
              setOpenLog({
                runId: run.id,
                stageId,
                opener: document.activeElement as HTMLElement | null,
              });
            }}
            onAnswered={() => void reload()}
            onEscalationError={(message) => {
              setActionRefusal(null);
              setError(message);
            }}
            // GRT-FR-XHLN: an accepted restart selects the run it created, also
            // over a run the author selected meanwhile, and focus goes to that
            // run's first control once its row stands. A filter that hides the
            // new run is relaxed (GRH-FR-XBWU); otherwise GRH-FR-WDSH would take
            // the selection back before the row stands.
            onRestarted={(created) => {
              pressedRef.current = { runId: created.id, action: "open-draft" };
              setSelected(created.id);
              reveal(created, { view: viewRef.current, text: filterRef.current });
            }}
            refusal={actionRefusal?.runId === run.id ? actionRefusal.message : null}
            pressedRef={pressedRef}
            openRestart={restartAsk === run.id}
            onRestartOpened={() => setRestartAsk(null)}
          />
        )}
      </section>

      <p
        aria-live="polite"
        className="sr-only"
        data-testid="graduation-row-announcement"
        ref={announceRef}
      />

      {/* GRU-FR-HXNC: the log window, bound to the run and the stage that
          opened it. Keyed by both, so opening another run's stage begins from
          what that run holds and reuses nothing of the previous window
          (GLW-FR-BVYN). */}
      {logWindow && (
        <GraduationLogWindow
          key={`${logWindow.run.id}-${logWindow.stage.id}`}
          run={logWindow.run}
          stage={logWindow.stage}
          returnFocus={openLog?.opener ?? null}
          onClose={() => setOpenLog(null)}
        />
      )}
    </div>
  );
}

/** GRU-FR-HKBD: where a queued run stands in its own stream's queue. */
function positionLabel(ahead: number | null): string | null {
  if (ahead === null) return null;
  return ahead === 0 ? "Next" : `${ahead} ahead`;
}

export { raiseStatement };
