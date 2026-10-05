import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import {
  onGraduationQueueChanged,
  onGraduationRunChanged,
  onWorkStreamUpdateProgress,
} from "../events";
import { logWarn } from "../logging";
import {
  coalesce,
  holdsStream,
  isActive,
  isWorking,
  runTitle,
} from "../state/graduation";
import type {
  BottomSurface,
  GraduationRun,
  WorkStreamUpdateProgress,
  WorktreeContext,
} from "../types";
import type { SwitchOutcome } from "./WorktreeSelector";
import { Icon } from "./icons";
import { GraduationRuns } from "./GraduationRuns";
import { Runs } from "./Runs";
import { Logs } from "./Logs";
import { Git } from "./Git";
import type { ReadyTasksBinding } from "./GitReadyTasks";

interface BottomPanelProps {
  surface: BottomSurface;
  onHide: () => void;
  /** SNV-FR-50: the drag handle on the panel's top edge. */
  resizer?: React.ReactNode;
  // The active artifact entity the History surface scopes to, if any.
  activeEntity: string | null;
  // GIT-FR-06 / OVW-FR-12: run a checkout requested from the Git panel's
  // branches section through the worktree-switch transition — the same one the
  // top-chrome selector uses.
  onSwitchWorktree: (
    operation: () => Promise<WorktreeContext>,
  ) => Promise<SwitchOutcome>;
  // GIT-FR-06 / WTC-FR-21: whether the Git panel's branches section may check
  // a branch out — only from the repository's primary worktree.
  canCheckOutBranches: boolean;
  /** GHA-FR-16: open the GitHub token picker for a blocked operation. */
  onRequestGithubToken?: () => Promise<boolean>;
  /** GRU-FR-RZDI: open a run's source draft in a New Artifact tab. */
  onOpenDraft?: (draftId: string) => void;
  /** GRU-FR-QLRQ: open Project settings on a named section. */
  onOpenSettings?: (section: string) => void;
  /**
   * SNV-FR-56: a surface of this panel is about to mount a floating overlay.
   *
   * The shell takes down whatever else is open, so two action surfaces never
   * coexist.
   */
  onOverlayOpening?: () => void;
  /**
   * GRH-FR-ODLT / GRU-FR-BLSS: the run to select when the author arrived here from
   * somewhere that named one. Selecting the graduation section is part of what
   * routing to a run means, that being where a run is.
   *
   * A nonce rather than a bare id, so routing to the same run twice routes
   * twice — and so a request that has already been taken can be told from one
   * that has not, which is what stops it wedging the section control here for
   * the rest of the session.
   */
  selectGraduationRun?: {
    runId: string;
    nonce: number;
    /**
     * GRU-FR-MYFA: whether this request may **override** a selection the author
     * already has. An explicit route — a notification, a draft's Go to run —
     * always may. A graduation start may only where the author was looking at
     * this section when they started it.
     */
    override?: boolean;
  } | null;
  /**
   * GIT-FR-FZMS: the branch the Git panel selects in its branches section. A
   * nonce, for the same reason `selectGraduationRun` carries one.
   */
  selectGitBranch?: { branch: string; nonce: number } | null;
  /** GIT-FR-FZMS: the Git panel has taken the request, so the shell clears it. */
  onGitBranchSelected?: () => void;
  /**
   * NTF-FR-39 / NTF-FR-08: the graduation run the author is **looking at**, or
   * null when they are not looking at one.
   *
   * The panel is what knows this: a run is showing only while Runs is the
   * active bottom surface *and* its graduation section is the one the section
   * control has chosen. Reported upwards because the notification facility is
   * the shell's, and "already looking at it" is a fact about the window rather
   * than about this panel.
   */
  onGraduationRunInView?: (runId: string | null) => void;
  /**
   * GRU-FR-MYFA: whether the graduation section is the active section of this
   * showing panel, right now.
   *
   * The panel is what knows it, for the same reason it knows which run is in
   * view. The shell needs it at the instant a graduation is started, to decide
   * whether the run that starts may take the author's selection: a Graduate
   * from a hidden panel, or from agent output, moves nothing.
   */
  onGraduationSectionActive?: (active: boolean) => void;
  /** GIT-FR-OGHO: the Git panel's Ready tasks section. */
  readyTasks?: ReadyTasksBinding;
}

/**
 * RUN-FR-10: which section of the Runs panel is showing.
 *
 * The panel hosts two things that are both agent activity — a graduation run
 * and an adapter run's output — and the author looks for agent activity in one
 * place, so the choice between them is the panel's own rather than a second
 * surface in the activity bar.
 */
type RunsSection = "graduation" | "output";

// Canned commit rows for the History surface (the real history loader is a
// separate feature). Lifted out of the JSX so the panel body stays readable.
const HISTORY_SAMPLE = [
  { h: "7e3f1a2", m: "tighten editor toolbar token usage", a: "kira", t: "2h ago" },
  { h: "a91bc04", m: "spec: flow editor rules", a: "kira", t: "5h ago" },
  { h: "14b7d92", m: "add settings sections placeholder", a: "mara", t: "yesterday" },
];

function HistoryPanel({ activeEntity }: { activeEntity: string | null }) {
  return (
    <div style={{ padding: "12px 16px" }}>
      {/* SNV-FR-57 / HVW-FR-01: "HISTORY" is a label and keeps the eyebrow
          treatment; the artifact beside it is a file name and keeps its
          on-disk case, so the same file is not spelled two ways across the
          window. The em dash and the no-scope wording are label text too. */}
      <div className="t-eyebrow" style={{ marginBottom: 8 }}>
        HISTORY —{" "}
        {activeEntity ? (
          <span className="t-eyebrow__literal">{activeEntity}</span>
        ) : (
          "no entity scope"
        )}
      </div>
      {HISTORY_SAMPLE.map((c) => (
        <div key={c.h} className="git__file">
          <span className="t-hash">{c.h}</span>
          <span
            className="git__file-name"
            style={{ color: "var(--fg-1)", fontFamily: "var(--font-sans)" }}
          >
            {c.m}
          </span>
          <span className="t-meta">
            {c.a} · {c.t}
          </span>
        </div>
      ))}
    </div>
  );
}

/**
 * How each surface titles the panel's header. The header names where the panel
 * is; it is not a switcher (SNV-FR-47).
 */
const SURFACE_TITLES: Record<
  BottomSurface,
  { label: string; icon: (typeof Icon)["Terminal"] }
> = {
  runs: { label: "Runs", icon: Icon.Terminal },
  logs: { label: "Logs", icon: Icon.Code },
  git: { label: "Git", icon: Icon.Branch },
  history: { label: "History", icon: Icon.History },
};

/**
 * The bottom dock: one of the Runs / Logs / Git / History surfaces
 * (RUN/LOG/GIT/HVW).
 *
 * SNV-FR-47: the panel presents no surface switcher of its own — the activity
 * bar's bottom-panel cluster is the only control that chooses between the four.
 * The header names the surface the panel is on, and the hide control closes the
 * panel exactly as re-activating that surface's toggle would.
 */
/**
 * RUN-FR-VKLS: an update is named as an update, and says which strategy it
 * runs by.
 *
 * An update runs the source into the stream, so a label that named only the
 * stream and the turn would tell the author nothing about which of the two
 * strategies is changing their work.
 */
export function updateLabel(
  streamName: string,
  progress: WorkStreamUpdateProgress,
): string {
  const stream = streamName || progress.streamId;
  // "from source" is the direction — the source goes into the stream — and the
  // word after it is which strategy the author chose (WSS-FR-BDMU).
  const how = progress.strategy === "rebase_source" ? "rebase" : "merge";
  return `Updating ${stream} from source by ${how} · turn ${progress.turn} of ${progress.turnsMax}`;
}

/**
 * RUN-FR-02: one piece of work the agent-output section can show, reduced to
 * what choosing between them needs.
 */
interface Shown {
  /** What the activity stream is read by (RUN-FR-ZQWA). */
  runId: string | null;
  label: string | null;
  /** Whether its turns are still running (RUN-FR-04). */
  live: boolean;
  /** When it last reported, which is what RUN-FR-02 orders it by. */
  at: number;
}

/** What both stream progress events carry that this panel reads. */
interface StreamProgress {
  streamId: string;
  attemptId: string;
  turn: number;
}

/**
 * RUN-FR-02 / RUN-FR-VKLS: follow a work stream update wherever the panel is.
 *
 * An update is agent work the author waits on: its semantic turns are read here
 * like any other turn's, keyed by the attempt rather than by a run, the work
 * belonging to a stream and to no run (RUN-FR-ZQWA). A merge is no stream work
 * of this kind: a merge run is a run, and the run selection below carries it
 * (RUN-FR-LQDT).
 */
function useStreamWork<P extends StreamProgress>(
  subscribe: (handler: (payload: P) => void) => Promise<() => void>,
  label: (streamName: string, payload: P) => string,
): Shown | null {
  const [work, setWork] = useState<{
    progress: P;
    streamName: string;
    live: boolean;
    at: number;
  } | null>(null);
  /** The stream whose name has already been asked for (RUN-FR-VKLS). */
  const namedFor = useRef<string | null>(null);
  // The work is followed wherever the panel is, so switching to the section
  // shows work that started while the author was elsewhere (RUN-FR-10).
  useEffect(() => {
    let cancelled = false;
    const pending = subscribe((progress) => {
      if (cancelled) return;
      // RUN-FR-ZQWA / RUN-FR-06: settled work is marked no longer live and is
      // KEPT. Its records are the whole account of why it ended, and they stay
      // readable until newer work replaces them or the author dismisses them.
      if (progress.turn === 0) {
        setWork((held) =>
          held?.progress.streamId === progress.streamId
            ? { ...held, live: false }
            : held,
        );
        return;
      }
      setWork((held) =>
        held?.progress.streamId === progress.streamId
          ? { ...held, progress, live: true, at: Date.now() }
          // A different stream's work is a different name, so the old one is
          // dropped rather than carried over until the read below answers.
          : { progress, streamName: "", live: true, at: Date.now() },
      );
      // RUN-FR-VKLS: the section names the stream, which the report identifies
      // rather than names. Read once, when the work is first seen — every turn
      // of one operation reports the same stream.
      //
      // Held in a ref rather than read back off the state: a state updater has
      // not run by the time this line does, so reading the name from it would
      // ask for the stream again on every turn.
      if (namedFor.current === progress.streamId) return;
      namedFor.current = progress.streamId;
      void api
        .getWorkStream(progress.streamId)
        .then((stream) => {
          if (cancelled) return;
          setWork((held) =>
            held?.progress.streamId === progress.streamId
              ? { ...held, streamName: stream.name }
              : held,
          );
        })
        .catch((error) => {
          // A stream this panel cannot name is still work worth reading, so
          // the label falls back to the attempt rather than the section going
          // empty. The failure is reported because a handled one is otherwise
          // invisible, and a label that quietly reads as an identifier is the
          // only symptom the author sees.
          //
          // The stream id and the refusal, never the stream name: the name is
          // the author's own words and nothing downstream redacts a log.
          logWarn(["frontend"], "the runs panel could not name a work stream", {
            streamId: progress.streamId,
            error: String(error),
          });
        });
    });
    return () => {
      cancelled = true;
      void pending.then((un) => un());
    };
  }, [subscribe]);
  if (!work) return null;
  return {
    runId: work.progress.attemptId,
    label: label(work.streamName, work.progress),
    live: work.live,
    at: work.at,
  };
}

export function BottomPanel({
  surface,
  onHide,
  resizer,
  activeEntity,
  onSwitchWorktree,
  canCheckOutBranches,
  onRequestGithubToken,
  onOpenDraft,
  onOpenSettings,
  onOverlayOpening,
  selectGraduationRun,
  selectGitBranch,
  onGitBranchSelected,
  onGraduationRunInView,
  onGraduationSectionActive,
  readyTasks,
}: BottomPanelProps) {
  const title = SURFACE_TITLES[surface];
  const TitleIcon = title.icon;
  // RUN-FR-10: graduation first, because it is the section carrying decisions
  // the author is being waited on for — and the one a notification's `run`
  // address opens the panel onto.
  const [section, setSection] = useState<RunsSection>("graduation");
  /**
   * RUN-FR-10: how many runs the project's two queues hold between them, so the
   * work in flight is legible from the section control without switching to it.
   */
  const [queued, setQueued] = useState(0);
  // Routing to a run selects the section *once*. Deriving what is showing from
  // the request instead would mean the author could never choose the other
  // section again for as long as the request stood.
  const routed = selectGraduationRun?.nonce;
  useEffect(() => {
    if (routed !== undefined) setSection("graduation");
  }, [routed]);

  /**
   * RUN-FR-02: which run the agent-output section is showing.
   *
   * Held by the panel rather than by that section, because the choice is made
   * from the queue and the queue is what both sections are about — and because
   * the section must be current the moment the author switches to it, which
   * means following the queue while it is not showing (RUN-FR-10).
   */
  const [run, setRun] = useState<GraduationRun | null>(null);
  /**
   * RUN-FR-02 / RUN-FR-VKLS: the work stream update that is running, if one is.
   */
  const update = useStreamWork(onWorkStreamUpdateProgress, updateLabel);
  /**
   * NTF-FR-39: which run the graduation section has selected, mirrored here so
   * the shell can be told whether the author is looking at it.
   *
   * Reported as null the moment Runs stops being the active surface or the
   * section control moves off graduation, because a run behind another surface
   * is one the author has been shown nothing about.
   */
  const [inView, setInView] = useState<string | null>(null);
  const showingGraduation = surface === "runs" && section === "graduation";
  const looking = showingGraduation ? inView : null;
  useEffect(() => {
    onGraduationRunInView?.(looking);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [looking]);
  /**
   * GRU-FR-MYFA: and whether the section is showing at all, which is a different
   * fact from which run it has selected — a section showing an empty project
   * is still the section the author is looking at.
   */
  useEffect(() => {
    onGraduationSectionActive?.(showingGraduation);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showingGraduation]);
  // A panel that is unmounted is showing nothing, so the shell is told so
  // rather than left holding the last run it was told about.
  useEffect(
    () => () => {
      onGraduationRunInView?.(null);
      onGraduationSectionActive?.(false);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );
  /**
   * RUN-FR-02: which piece of agent work the section shows.
   *
   * Work that is running comes first; between two that are running, and
   * between two that are not, the more recently updated wins. An update that
   * has settled therefore stays on screen until newer work replaces
   * it, rather than vanishing at the moment its account of why it ended became
   * interesting.
   */
  const candidates: Shown[] = [];
  if (update) candidates.push(update);
  if (run)
    candidates.push({
      runId: run.id,
      // RUN-FR-LQDT: a merge run's title is its `Merge <stream>` name, and a
      // review turn is agent work of the run as a work turn is (RUN-FR-02).
      label: runTitle(run) || null,
      live: isWorking(run.state),
      at: Date.parse(run.updatedAt) || 0,
    });
  // Stream work is offered ahead of the run, so work that reported at the same
  // instant as a run read keeps the screen rather than flickering between the
  // two.
  const shown =
    candidates.sort(
      (a, b) => Number(b.live) - Number(a.live) || b.at - a.at,
    )[0] ?? null;

  const reload = useCallback(async () => {
    try {
      const queue = await api.listGraduationQueue();
      // RUN-FR-02: the run the agent-output stream follows is the one working
      // now, and the most recently updated of them where two streams work at
      // once.
      const byRecency = (a: GraduationRun, b: GraduationRun) =>
        a.updatedAt < b.updatedAt ? 1 : -1;
      const working = queue.runs.filter((entry) => holdsStream(entry.state));
      // RUN-FR-02 / RUN-FR-LQDT: where no run is working, the most recently
      // updated run that ran a turn stays, so a turn that has just ended — a
      // merge run's included — is still readable.
      const newest =
        working.sort(byRecency)[0] ??
        queue.runs
          .filter((entry) => (entry.workTurns ?? 0) + (entry.reviewTurns ?? 0) > 0)
          .sort(byRecency)[0];
      setRun(newest ?? null);
      // RUN-FR-10: the count of runs the project's streams hold between them,
      // one run counted once whichever stream it is in.
      setQueued(queue.runs.filter((entry) => isActive(entry)).length);
    } catch {
      // A project with no queue has no run to show, which is the empty state
      // rather than a failure worth reporting twice — the graduation section
      // reports it already.
      setRun(null);
      setQueued(0);
    }
  }, []);
  useEffect(() => {
    if (surface !== "runs") return;
    void reload();
    let offQueue: (() => void) | undefined;
    let offRun: (() => void) | undefined;
    let cancelled = false;
    // GRD-FR-EFAU: a working run emits many events; they are gathered into one
    // read rather than one read each.
    const coalesced = coalesce(() => void reload());
    void onGraduationQueueChanged(coalesced.ask).then((off) => {
      if (cancelled) off();
      else offQueue = off;
    });
    void onGraduationRunChanged(coalesced.ask).then((off) => {
      if (cancelled) off();
      else offRun = off;
    });
    return () => {
      cancelled = true;
      coalesced.cancel();
      offQueue?.();
      offRun?.();
    };
  }, [surface, reload]);
  return (
    <div className="bottom-panel">
      {resizer}
      <div className="bottom-panel__nav">
        {/* SNV-FR-47: the header names the surface; it is not a run-state
            display. A live indicator here was gated on which surface is showing
            rather than on a run being live, so it animated — infinitely, and on
            an idle window — for as long as the Runs panel stayed open. The
            indicator RUN-FR-04 asks for lives inside the surface (Runs.tsx),
            where it is gated on the run's actual state. */}
        <div className="bottom-panel__title" data-testid="bottom-panel-title">
          <TitleIcon size={12} /> {title.label}
        </div>
        <div className="bottom-panel__nav-right">
          <button
            className="btn btn--ghost btn--icon btn--sm"
            onClick={onHide}
            title="Hide"
          >
            <Icon.X size={12} />
          </button>
        </div>
      </div>
      <div className="bottom-panel__body">
        {/* RUN-FR-04, RUN-FR-06: opening the panel shows the previous run's output with the
            live indicator absent. Mounting it live restarted the canned stream's
            600ms interval — and left the live indicator animating — every time
            the user opened Runs; the stream is driven from the surface's own
            Re-run control (RUN-FR-03) instead. */}
        {surface === "runs" && (
          <div className="runs-sections">
            {/* RUN-FR-10: the panel's own section control. It chooses what the
                content region renders and nothing else — neither section starts
                anything (RUN-FR-09). */}
            <div
              className="runs-sections__control"
              role="tablist"
              aria-label="Runs sections"
            >
              <button
                type="button"
                role="tab"
                aria-selected={section === "graduation"}
                className={
                  section === "graduation" ? "btn btn--sm btn--default" : "btn btn--sm btn--ghost"
                }
                onClick={() => setSection("graduation")}
              >
                Graduation{queued > 0 ? ` · ${queued}` : ""}
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={section === "output"}
                className={
                  section === "output" ? "btn btn--sm btn--default" : "btn btn--sm btn--ghost"
                }
                onClick={() => setSection("output")}
              >
                Agent output
              </button>
            </div>
            <div className="runs-sections__body">
              {section === "graduation" ? (
                <GraduationRuns
                  onOpenDraft={(draftId) => onOpenDraft?.(draftId)}
                  onOpenSettings={onOpenSettings}
                  selectRun={selectGraduationRun ?? null}
                  onSelectionChange={setInView}
                  onOverlayOpening={onOverlayOpening}
                />
              ) : (
                <Runs
                  runId={shown?.runId ?? null}
                  live={shown?.live ?? false}
                  label={shown?.label ?? null}
                />
              )}
            </div>
          </div>
        )}
        {/* LOG-FR-11 / LOG-FR-21: mounted only while Logs is the active
            surface, which is what makes "no query and no redraw while nobody is
            looking" structural rather than a runtime check. It renders no run
            output — that is Runs' stream, above, and the two stay separate. */}
        {surface === "logs" && <Logs />}
        {surface === "git" && (
          <Git
            onSwitchWorktree={onSwitchWorktree}
            canCheckOutBranches={canCheckOutBranches}
            onRequestGithubToken={onRequestGithubToken}
            readyTasks={readyTasks}
            onOpenDraft={onOpenDraft}
            selectBranch={selectGitBranch ?? null}
            onBranchSelected={onGitBranchSelected}
          />
        )}
        {surface === "history" && <HistoryPanel activeEntity={activeEntity} />}
      </div>
    </div>
  );
}
