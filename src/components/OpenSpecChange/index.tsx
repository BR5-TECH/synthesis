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
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { formatSectionAddress } from "../../settingsWindow";
import { changeViewOf, keepChangeView } from "../../state/openspec/changeView";
import { pendingChangeProposalOf } from "../../state/openspec/changeProposals";
import { noteOwnerDeleted } from "../../voice/voiceController";
import type {
  ChangeArtifact,
  ChangeDetail,
  ChangeFile,
  FragmentTarget,
  OpenableArtifact,
  OpenSpecChangeTabTarget,
} from "../../types";
import { ChangeBody } from "./ChangeBody";
import { ChangeDiscussionSplit, useChangeDiscussion } from "./ChangeDiscussion";
import { artifactOfPath, type FileMode } from "./changeFiles";
import { failureStatement, readErrorCode, routesToSettings } from "./failure";
import { useChangeReview } from "./useChangeReview";

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

export function OpenSpecChangeWorkspace({
  tabId,
  target,
  onOpenArtifact,
}: OpenSpecChangeWorkspaceProps) {
  const { location, name } = target;
  const [detail, setDetail] = useState<ChangeDetail | null>(() => changeViewOf(tabId).detail);
  const [selected, setSelected] = useState<string | null>(() => changeViewOf(tabId).selected);
  const [error, setError] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const [modes, setModes] = useState<Readonly<Record<string, FileMode>>>(
    () => changeViewOf(tabId).modes,
  );
  const live = useRef(true);
  const readSeq = useRef(0);

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
      const code = readErrorCode(e);
      setError(code);
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
    void read();
    return () => {
      live.current = false;
    };
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

  const openSettings = () => {
    void api.openSettingsWindow(formatSectionAddress("project", "openspec")).catch((e) => {
      logWarn(["frontend"], "Settings window could not be opened", {
        reason: e instanceof Error ? e.message : String(e),
      });
    });
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
          <FirstFailure
            code={error}
            statement={failureStatement(error, name, location)}
            onRetry={() => void read()}
            onOpenSettings={openSettings}
          />
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
        onOpenFile={onOpenFile}
      />
    );

  return (
    <section className="openspec-change" data-testid="openspec-change">
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
          <span className="spacer" />
          <button
            type="button"
            className="btn btn--sm btn--ghost"
            disabled={reading}
            onClick={() => void read()}
          >
            Refresh
          </button>
        </div>
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

      {/* OCW-FR-PFUQ: a later failure keeps the content and states itself above it. */}
      {detail && error && (
        <div className="openspec-change__failure" role="alert" data-testid="openspec-change-stale">
          <span>{failureStatement(error, name, location)}</span>
          <button type="button" className="btn btn--sm btn--default" onClick={() => void read()}>
            Retry
          </button>
        </div>
      )}

      {location.kind === "stream" ? (
        <StreamSurface
          streamId={location.streamId}
          change={name}
          onDecided={readAgain}
          onFocusFragment={showFragment}
          render={(stream) => readingPane(stream)}
        />
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
    </section>
  );
}

/** OCW-FR-RFGG: the failure of a first read, in the reading pane. */
function FirstFailure({
  code,
  statement,
  onRetry,
  onOpenSettings,
}: {
  code: string;
  statement: string;
  onRetry: () => void;
  onOpenSettings: () => void;
}) {
  return (
    <div className="openspec-change__state" role="alert" data-testid="openspec-change-failure">
      <p>{statement}</p>
      {routesToSettings(code) ? (
        <button type="button" className="btn btn--sm btn--default" onClick={onOpenSettings}>
          Open OpenSpec settings
        </button>
      ) : code === "change_not_found" ? null : (
        <button type="button" className="btn btn--sm btn--default" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  );
}

/** What a stream location lends its reading pane. */
interface StreamReview {
  review: ReturnType<typeof useChangeReview>;
  onComment: ReturnType<typeof useChangeDiscussion>["commentOn"];
}

/**
 * OCW-FR-TRLG / DDS-FR-POJC: a stream location's discussion column beside the
 * reading pane, and the review of its pending proposal (OCW-FR-COLR).
 */
function StreamSurface({
  streamId,
  change,
  onDecided,
  onFocusFragment,
  render,
}: {
  streamId: string;
  change: string;
  onDecided: () => void;
  onFocusFragment: (fragment: FragmentTarget) => void;
  render: (stream: StreamReview) => ReactNode;
}) {
  const discussion = useChangeDiscussion(streamId, change);
  const { discussHunk } = discussion;
  const onDiscussHunk = useCallback(
    (hunkId: string) => discussHunk(pendingChangeProposalOf(streamId, change), hunkId),
    [discussHunk, streamId, change],
  );
  const review = useChangeReview({
    streamId,
    change,
    onDecided,
    onDiscussHunk,
    agents: discussion.discussions.agents,
  });
  return (
    <ChangeDiscussionSplit
      discussion={discussion}
      change={change}
      pending={review.pending}
      onFocusFragment={onFocusFragment}
    >
      {render({ review, onComment: discussion.commentOn })}
    </ChangeDiscussionSplit>
  );
}
