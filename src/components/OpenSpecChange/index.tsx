/**
 * The change workspace tab
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md`).
 *
 * One OpenSpec change, read as one document: a header with the task progress
 * and a state chip for each artifact, an artifact list on the leading side,
 * and a reading pane. The `specs` artifact renders as requirement diffs
 * against the live specs, and every other artifact renders its files as
 * read-only Markdown. Each file also offers its Source.
 *
 * On a stream, the discussion column of every draft discussion stands beside
 * the reading pane, and the hunks of a pending change edit proposal are
 * decided in the Source of their files. A decision is the only way the tab
 * changes a file (OCW-FR-UOKM).
 *
 * The viewport mounts the active tab alone, so each mount is an open or a
 * reactivation, and each mount reads the change (OCW-FR-LABL). What the last
 * read gave stays in `changeView`, so it shows while the tab reads again.
 *
 * On a stream, the header also offers History (OCW-FR-RKCC) and Information
 * (OCW-FR-ZZCS), and names the changes a confirmed split made (OCW-FR-TZRT).
 * It offers Apply and Rework… (OCW-FR-DTJD, OCW-FR-LWFQ), and a run strip
 * under it shows the newest apply run or rework run of the change
 * (OCW-FR-JHAB).
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import {
  archivedAsOf,
  changeViewOf,
  keepChangeView,
  subscribeChangeViews,
} from "../../state/openspec/changeView";
import { noteOwnerDeleted } from "../../voice/voiceController";
import type {
  ChangeArtifact,
  ChangeDetail,
  ChangeFile,
  FragmentTarget,
  OpenableArtifact,
  OpenSpecChangeTabTarget,
  OpenSpecFailure,
} from "../../types";
import { ChangeBody } from "./ChangeBody";
import { artifactOfPath, type FileMode } from "./changeFiles";
import { archivedAsOfError, failureStatement, readFailureOf } from "./failure";
import { hasNotice } from "../../state/openspec/failureNotice";
import { OpenSpecFailureNotice } from "../OpenSpecFailureNotice";
import { ArchivedChange } from "./ArchivedChange";
import { useFocusOnOpen } from "./focusOnOpen";
import { ChangeInformationModal } from "./ChangeInformationModal";
import { StreamSplitMarker, StreamSurface, type StreamReview } from "./StreamSurface";
import { ChangeRunButtons, ChangeRunNotes, useReworkDialog } from "./ChangeRunActions";
import { ChangeRunStrip } from "./ChangeRunStrip";
import { ReworkDialog } from "./ReworkDialog";
import { useChangeRuns } from "./useChangeRuns";
import { requestChangeDiscussion } from "../../state/openspec/changeRoutes";
import { discussionTargetKey } from "../../types";

const STATUS_LABEL: Record<ChangeArtifact["status"], string> = {
  done: "done",
  ready: "ready",
  blocked: "blocked",
  skipped: "skipped",
};

/**
 * OCW-FR-IAPK: the artifact an open selects. A file a new-file hunk would
 * create selects the artifact it would belong to (OCW-FR-RRXG).
 */
function selectionFor(detail: ChangeDetail, file: string | null): string | null {
  if (file) {
    const holder = artifactOfPath(detail, file);
    if (holder) return holder;
  }
  return detail.artifacts[0]?.id ?? null;
}

/** VOM-FR-NTKT: the read codes that say the owner of a change discussion is gone. */
const OWNER_GONE = new Set(["change_not_found", "unknown_stream"]);

export interface OpenSpecChangeWorkspaceProps {
  tabId: string;
  target: OpenSpecChangeTabTarget;
  /** OCW-FR-UOKM: open a file of the active worktree in an Editor tab. */
  onOpenArtifact: (item: OpenableArtifact) => void;
}

/**
 * OCW-FR-AKVG / OCW-FR-GVZT: a tab whose change is archived states that alone.
 * The workspace of a change that is not archived mounts its runs, its
 * discussion and its proposals; the statement mounts none of them.
 */
export function OpenSpecChangeWorkspace(props: OpenSpecChangeWorkspaceProps) {
  const archivedAs = useSyncExternalStore(subscribeChangeViews, () => archivedAsOf(props.tabId));
  // OCW-FR-LABL: one read for each mount. The part that shows first makes it.
  // A part that a read of this mount put in its place shows that answer and
  // does not read again.
  const [firstPart] = useState(() =>
    archivedAsOf(props.tabId) !== null ? "archived" : "workspace",
  );
  if (archivedAs !== null) {
    return (
      <ArchivedChange
        tabId={props.tabId}
        target={props.target}
        archivedAs={archivedAs}
        readOnMount={firstPart === "archived"}
      />
    );
  }
  return <ChangeWorkspace {...props} readOnMount={firstPart === "workspace"} />;
}

function ChangeWorkspace({
  tabId,
  target,
  onOpenArtifact,
  readOnMount,
}: OpenSpecChangeWorkspaceProps & { readOnMount: boolean }) {
  const { location, name } = target;
  const [detail, setDetail] = useState<ChangeDetail | null>(() => changeViewOf(tabId).detail);
  const [selected, setSelected] = useState<string | null>(() => changeViewOf(tabId).selected);
  /** OCW-FR-RFGG / OCW-FR-PFUQ: the failure of the last read, or null. */
  const [error, setError] = useState<OpenSpecFailure | null>(null);
  const [reading, setReading] = useState(false);
  const [modes, setModes] = useState<Readonly<Record<string, FileMode>>>(
    () => changeViewOf(tabId).modes,
  );
  const live = useRef(true);
  const readSeq = useRef(0);
  /** OCW-FR-RKCC: the History rail of a stream location. */
  const [historyOpen, setHistoryOpen] = useState(false);
  /** OCW-FR-ZZCS: the Information modal of a stream location. */
  const [informationOpen, setInformationOpen] = useState(false);
  const informationOpener = useRef<HTMLButtonElement | null>(null);
  // OCW-FR-GVZT: the parts that read other stores (the runs, the discussion
  // column, the proposals, the requests, the versions) wait for the first
  // answer of the change read. A change that answers archived then reads none
  // of them. A kept read is an answer, so a reactivation shows them at once.
  const answered = detail !== null || error !== null;
  const streamId = location.kind === "stream" && answered ? location.streamId : null;
  /** OCW-FR-DTJD through OCW-FR-SFYL: the runs of the change on this stream. */
  const runs = useChangeRuns({ tabId, streamId, change: name });
  const rework = useReworkDialog();
  const root = useFocusOnOpen<HTMLElement>(target.openSeq);
  /** OCW-FR-FEOA / OCW-FR-SCIA: select a thread in the discussion column. */
  const selectThread = (threadId: string) => {
    if (streamId === null) return;
    requestChangeDiscussion(
      discussionTargetKey({ kind: "change", streamId, change: name }),
      threadId,
    );
  };

  /** OCW-FR-LABL / OCW-FR-HPHM: one read, and one record of how it went. */
  const read = useCallback(async () => {
    const seq = ++readSeq.current;
    setReading(true);
    try {
      const next = await api.readOpenspecChange(location, name);
      if (!live.current || seq !== readSeq.current) return;
      setDetail(next);
      setError(null);
      keepChangeView(tabId, { detail: next });
      logInfo(["frontend"], "openspec change read", {
        locationKind: location.kind,
        artifactCount: next.artifacts.length,
        deltaCount: next.deltas.length,
      });
    } catch (e) {
      if (!live.current || seq !== readSeq.current) return;
      const failure = readFailureOf(e);
      const code = failure.code;
      // OCW-FR-AKVG: an archived change, first or later, replaces the content
      // of the tab with its statement. The wrapper takes it from here.
      const archivedAs = archivedAsOfError(e);
      if (archivedAs !== null) {
        logInfo(["frontend"], "openspec change is archived", { locationKind: location.kind });
        keepChangeView(tabId, { archivedAs, detail: null });
        return;
      }
      setError(failure);
      // VOM-FR-NTKT: the stream or the change folder is gone, so voice mode on
      // the change discussion ends.
      if (location.kind === "stream" && OWNER_GONE.has(code)) {
        noteOwnerDeleted({ kind: "change", streamId: location.streamId, change: name });
      }
      logWarn(["frontend"], "openspec change could not be read", {
        locationKind: location.kind,
        code,
      });
    } finally {
      if (live.current && seq === readSeq.current) setReading(false);
    }
    // The location and the name identify the tab, so they do not change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tabId]);

  // OCW-FR-LABL: read on each mount, which is an open or a reactivation.
  useEffect(() => {
    live.current = true;
    if (readOnMount) void read();
    return () => {
      live.current = false;
    };
    // The first render decides; a later one does not read again.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [read]);

  // OCW-FR-IAPK / TAB-FR-KOTW: apply the selection of each open once, as soon
  // as there is a read to apply it to.
  useEffect(() => {
    if (!detail || changeViewOf(tabId).appliedSeq === target.openSeq) return;
    const next = selectionFor(detail, target.file);
    setSelected(next);
    keepChangeView(tabId, { selected: next, appliedSeq: target.openSeq });
  }, [detail, target.openSeq, target.file, tabId]);

  const choose = (id: string) => {
    setSelected(id);
    keepChangeView(tabId, { selected: id });
  };

  /** OCW-FR-ABUR: the Rich or Source choice of one file, kept by the tab. */
  const chooseMode = (path: string, mode: FileMode) => {
    const next = { ...modes, [path]: mode };
    setModes(next);
    keepChangeView(tabId, { modes: next });
  };

  /** CMT-FR-28: a fragment quote shows its file in Source. */
  const showFragment = (fragment: FragmentTarget) => {
    if (!detail) return;
    const holder = artifactOfPath(detail, fragment.path);
    if (holder) choose(holder);
    chooseMode(fragment.path, "source");
  };

  const readAgain = useCallback(() => void read(), [read]);

  // OCW-FR-UOKM: only the active worktree offers Open in editor.
  const onOpenFile =
    location.kind === "active_worktree"
      ? (file: ChangeFile) => {
          const path = `openspec/changes/${name}/${file.path}`;
          onOpenArtifact({ id: path, name: path.split("/").pop() || path });
        }
      : undefined;

  const where =
    location.kind === "stream"
      ? `stream ${target.streamName ?? location.streamId}`
      : "active worktree";
  // The artifact on show. An open that is not applied yet shows its own
  // selection at once, and a refresh that dropped the selected artifact shows
  // the first one.
  const shown = !detail
    ? null
    : changeViewOf(tabId).appliedSeq !== target.openSeq
      ? selectionFor(detail, target.file)
      : selected !== null && detail.artifacts.some((a) => a.id === selected)
        ? selected
        : (detail.artifacts[0]?.id ?? null);

  /** The reading pane: the loading state, the first failure, or the change. */
  const readingPane = (stream: StreamReview | null): ReactNode =>
    !detail ? (
      <div className="openspec-change__reading openspec-change__reading--alone">
        {error ? (
          <FirstFailure failure={error} name={name} location={location} onRetry={read} />
        ) : (
          // OCW-FR-LABL: the loading state until the first answer arrives.
          <p className="openspec-change__state" role="status" data-testid="openspec-change-loading">
            Reading the change…
          </p>
        )}
      </div>
    ) : (
      <ChangeBody
        detail={detail}
        shown={shown}
        onChoose={choose}
        modes={modes}
        onMode={chooseMode}
        pending={stream?.review.pending}
        review={stream?.review.review}
        pathOf={stream?.review.pathOf}
        refusal={stream?.review.refusal}
        onComment={stream?.onComment}
        marks={stream?.marks}
        onOpenFile={onOpenFile}
      />
    );

  return (
    <section ref={root} className="openspec-change" data-testid="openspec-change" tabIndex={-1}>
      <header className="openspec-change__head">
        <div className="openspec-change__facts">
          <h2 className="openspec-change__name">{name}</h2>
          {detail && <span className="openspec-change__fact">{detail.schema}</span>}
          <span className="openspec-change__fact" data-testid="openspec-change-location">
            {where}
          </span>
          {detail && (
            <span className="openspec-change__fact" data-testid="openspec-change-tasks">
              {detail.completedTasks} of {detail.totalTasks} tasks
            </span>
          )}
          {streamId !== null && <StreamSplitMarker streamId={streamId} change={name} />}
          <span className="spacer" />
          {streamId !== null && (
            <>
              <button
                type="button"
                className="btn btn--sm btn--ghost"
                aria-pressed={historyOpen}
                onClick={() => setHistoryOpen((open) => !open)}
              >
                History
              </button>
              <button
                ref={informationOpener}
                type="button"
                className="btn btn--sm btn--ghost"
                onClick={() => setInformationOpen(true)}
              >
                Information
              </button>
              <ChangeRunButtons
                runs={runs}
                notes={rework.notes}
                onRework={() => rework.setOpen(true)}
                reworkRef={rework.opener}
              />
            </>
          )}
          <button
            type="button"
            className="btn btn--sm btn--ghost"
            disabled={reading}
            onClick={() => {
              void read();
              if (streamId !== null) runs.rereadProgress();
            }}
          >
            Refresh
          </button>
        </div>
        {streamId !== null && <ChangeRunNotes runs={runs} notes={rework.notes} change={name} />}
        {/* OCW-FR-LSFO: one chip per artifact, in the build order of the schema. */}
        {detail && (
          <ul className="openspec-change__chips" aria-label="Artifact states">
            {detail.artifacts.map((a) => (
              <li
                key={a.id}
                className="openspec-chip"
                data-status={a.status}
                data-testid="openspec-chip"
              >
                {a.id} <span className="openspec-chip__state">{STATUS_LABEL[a.status]}</span>
              </li>
            ))}
          </ul>
        )}
      </header>

      {/* OCW-FR-PFUQ: a later failure keeps the content and states itself
          above it: an OpenSpec failure in its notice, every other code with
          Retry. The tab is the tab of the change, so no notice offers Open
          change (OFN-FR-RLSO). */}
      {detail && error && hasNotice(error) && (
        <div className="openspec-change__failure" data-testid="openspec-change-stale">
          <OpenSpecFailureNotice
            failure={error}
            surface="change_workspace_read"
            change={name}
            onRetry={read}
          />
        </div>
      )}
      {detail && error && !hasNotice(error) && (
        <div className="openspec-change__failure" role="alert" data-testid="openspec-change-stale">
          <span>{failureStatement(error.code, name, location)}</span>
          <button type="button" className="btn btn--sm btn--default" onClick={() => void read()}>
            Retry
          </button>
        </div>
      )}

      {streamId !== null ? (
        <StreamSurface
          streamId={streamId}
          change={name}
          detail={detail}
          historyOpen={historyOpen}
          strip={
            runs.strip ? (
              <ChangeRunStrip
                run={runs.strip}
                progress={runs.progress}
                progressFailure={runs.progressFailure}
                onClose={runs.closeStrip}
              />
            ) : null
          }
          onDecided={readAgain}
          onFocusFragment={showFragment}
          render={(stream) => readingPane(stream)}
        />
      ) : location.kind === "stream" ? (
        // OCW-FR-LABL: a stream location shows the loading state alone until
        // the first answer.
        readingPane(null)
      ) : (
        <>
          {/* OCW-FR-ZZHS: a worktree that is not a stream's working copy
              carries no discussion column. */}
          <p className="openspec-change__no-discussion t-ui-xs" role="note" data-testid="openspec-change-no-discussion">
            A change is discussed on its work stream. Open it from its run or its draft to discuss it.
          </p>
          {readingPane(null)}
        </>
      )}
      {location.kind === "stream" && informationOpen && (
        <ChangeInformationModal
          streamId={location.streamId}
          change={name}
          returnFocus={informationOpener.current}
          onClose={() => setInformationOpen(false)}
        />
      )}
      {location.kind === "stream" && rework.open && (
        <ReworkDialog
          streamId={location.streamId}
          streamName={target.streamName ?? location.streamId}
          change={name}
          returnFocus={rework.opener.current}
          onClose={() => rework.setOpen(false)}
          onThread={selectThread}
        />
      )}
    </section>
  );
}

/**
 * OCW-FR-RFGG: the failure of a first read, in the reading pane. An OpenSpec
 * failure renders its notice. `change_not_found` offers nothing, and every
 * other code offers Retry.
 */
function FirstFailure({
  failure,
  name,
  location,
  onRetry,
}: {
  failure: OpenSpecFailure;
  name: string;
  location: OpenSpecChangeTabTarget["location"];
  onRetry: () => Promise<void>;
}) {
  if (hasNotice(failure)) {
    return (
      <div className="openspec-change__state" data-testid="openspec-change-failure">
        <OpenSpecFailureNotice
          failure={failure}
          surface="change_workspace_read"
          change={name}
          onRetry={onRetry}
        />
      </div>
    );
  }
  return (
    <div className="openspec-change__state" role="alert" data-testid="openspec-change-failure">
      <p>{failureStatement(failure.code, name, location)}</p>
      {failure.code === "change_not_found" ? null : (
        <button type="button" className="btn btn--sm btn--default" onClick={() => void onRetry()}>
          Retry
        </button>
      )}
    </div>
  );
}
