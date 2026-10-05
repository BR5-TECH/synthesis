import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties, RefObject } from "react";
import { logError } from "../../logging";
import {
  loadAppPreferences,
  patchAppPreferences,
  peekAppPreferences,
} from "../../state/appPreferences";
import {
  clampPathsFraction,
  DEFAULT_PATHS_FRACTION,
  MAX_PATHS_FRACTION,
  MIN_PATHS_FRACTION,
  pathsFractionForDrag,
  pathsFractionForKey,
  pathsPercent,
} from "../../state/graduation";
import type { CommitFile, DiffPayload } from "../../types";
import { Icon } from "../icons";
import { rejectionMessage } from "./errors";
import type { RejectionSubject } from "./errors";
import { groupFilesByFolder, statusLetter, statusWord } from "./format";
import { RegionNote, SelectedMark } from "./parts";
import type { Load } from "./useLoad";

/** One change's text, hunk by hunk (GIT-FR-JRYS). */
export function DiffBody({
  payload,
  testIdPrefix,
}: {
  payload: DiffPayload;
  testIdPrefix: string;
}) {
  if (payload.isBinary) {
    return (
      <RegionNote kind="empty" testId={`${testIdPrefix}-diff-binary`}>
        Binary file. There are no text changes to show.
      </RegionNote>
    );
  }
  if (payload.hunks.length === 0) {
    return (
      <RegionNote kind="empty" testId={`${testIdPrefix}-diff-empty`}>
        This file has no text changes.
      </RegionNote>
    );
  }
  return (
    <>
      {payload.hunks.map((hunk, hi) => (
        <div className="diff git-log__hunk" key={`${hunk.header}:${hi}`}>
          <div className="diff-line" data-kind="hunk">
            <span className="diff-line__gutter diff-line__gutter--old" />
            <span className="diff-line__gutter diff-line__gutter--new" />
            <span className="diff-line__sign" />
            <span className="diff-line__text">{hunk.header}</span>
          </div>
          {hunk.lines.map((line, li) => (
            <div className="diff-line" data-kind={line.kind} key={`${hi}:${li}`}>
              <span className="diff-line__gutter diff-line__gutter--old">
                {line.oldLineno ?? ""}
              </span>
              <span className="diff-line__gutter diff-line__gutter--new">
                {line.newLineno ?? ""}
              </span>
              <span className="diff-line__sign">
                {line.kind === "add" ? "+" : line.kind === "del" ? "−" : " "}
              </span>
              <span className="diff-line__text">{line.content}</span>
            </div>
          ))}
        </div>
      ))}
    </>
  );
}

/**
 * GIT-FR-FATV: the files column's width, shared by the Commits and the
 * Branches sections through one app preference (GSS-FR-MSPQ).
 *
 * It holds the bounds and the default of the graduation run region's paths
 * column (GRU-FR-KWRB), so the helpers of that column serve it as they are.
 * It is read once and written only when a gesture ends.
 */
function useFilesWidth() {
  const [fraction, setFraction] = useState(() =>
    clampPathsFraction(peekAppPreferences()?.gitFilesWidthFraction),
  );
  /** Whether the author has set the width since this mounted. */
  const touched = useRef(false);
  useEffect(() => {
    let live = true;
    void loadAppPreferences().then((prefs) => {
      // A width the author set while the record loaded is the newer one.
      if (live && !touched.current) {
        setFraction(clampPathsFraction(prefs.gitFilesWidthFraction));
      }
    });
    return () => {
      live = false;
    };
  }, []);
  const persist = useCallback((next: number) => {
    touched.current = true;
    setFraction(next);
    void patchAppPreferences({ gitFilesWidthFraction: next }).catch(() => {
      logError(["frontend"], "the git files column width could not be stored", {});
    });
  }, []);
  const preview = useCallback((next: number) => {
    touched.current = true;
    setFraction(next);
  }, []);
  return { fraction, persist, preview };
}

/** GIT-FR-FATV: the boundary the author drags, and the same width by keyboard. */
function FilesDivider({
  columnsRef,
  fraction,
  persist,
  preview,
}: {
  columnsRef: RefObject<HTMLDivElement | null>;
  fraction: number;
  persist: (next: number) => void;
  preview: (next: number) => void;
}) {
  const dragging = useRef(false);
  const dragged = useRef(DEFAULT_PATHS_FRACTION);
  const start = useRef(DEFAULT_PATHS_FRACTION);
  /** How far right of the boundary the pointer took hold of the divider. */
  const grab = useRef(0);
  const percent = pathsPercent(fraction);
  const widthAt = (clientX: number) => {
    const box = columnsRef.current?.getBoundingClientRect();
    if (!box || !(box.width > 0)) return null;
    return pathsFractionForDrag(clientX - grab.current - box.left, box.width);
  };
  return (
    <div
      className="git-split__divider"
      role="separator"
      aria-orientation="vertical"
      aria-label="Files width"
      aria-valuenow={percent}
      aria-valuemin={pathsPercent(MIN_PATHS_FRACTION)}
      aria-valuemax={pathsPercent(MAX_PATHS_FRACTION)}
      aria-valuetext={`Files width ${percent} percent`}
      title="Drag, or use the arrow keys, to set the files width"
      tabIndex={0}
      data-testid="git-files-divider"
      onPointerDown={(event) => {
        if (event.button !== undefined && event.button !== 0) return;
        const box = columnsRef.current?.getBoundingClientRect();
        grab.current = box ? event.clientX - (box.left + fraction * box.width) : 0;
        dragging.current = true;
        dragged.current = fraction;
        start.current = fraction;
        event.currentTarget.setPointerCapture?.(event.pointerId);
      }}
      onPointerMove={(event) => {
        if (!dragging.current) return;
        const next = widthAt(event.clientX);
        if (next === null) return;
        dragged.current = next;
        preview(next);
      }}
      onPointerUp={(event) => {
        if (!dragging.current) return;
        dragging.current = false;
        event.currentTarget.releasePointerCapture?.(event.pointerId);
        persist(dragged.current);
      }}
      onPointerCancel={() => {
        if (!dragging.current) return;
        // A cancelled drag stores nothing and shows the stored width again.
        dragging.current = false;
        preview(start.current);
      }}
      onKeyDown={(event) => {
        const next = pathsFractionForKey(event.key, fraction);
        if (next === null) return;
        event.preventDefault();
        persist(next);
      }}
    />
  );
}

/**
 * GIT-FR-WMVK: the large view of the Commits and the Branches sections. The
 * changed files, grouped by folder (GIT-FR-DXNC), stand in the left column and
 * the diff of the selected file (GIT-FR-JRYS) in the right one, with a divider
 * between them (GIT-FR-FATV). Each column scrolls on its own.
 *
 * Each region shows its own loading, empty and failure state, and a failure
 * leaves the other region usable (GIT-FR-TFAU, GIT-FR-SION).
 */
export function ChangedFilesView({
  files,
  filePath,
  onSelectFile,
  onRetryFiles,
  diff,
  onRetryDiff,
  diffOwner,
  emptyFilesText,
  testIdPrefix,
  errorSubject = {},
}: {
  files: Load<CommitFile[]>;
  filePath: string | null;
  onSelectFile: (path: string) => void;
  onRetryFiles: () => void;
  diff: Load<DiffPayload>;
  onRetryDiff: () => void;
  /** What the diff belongs to, named above it: a short commit id or a branch. */
  diffOwner: string;
  /** The words for a change that holds no file. */
  emptyFilesText: string;
  /** The prefix of every test id, so each section's regions stay apart. */
  testIdPrefix: string;
  /** What an error sentence names: the branch compared, when there is one. */
  errorSubject?: RejectionSubject;
}) {
  const columnsRef = useRef<HTMLDivElement | null>(null);
  const { fraction, persist, preview } = useFilesWidth();
  const groups = groupFilesByFolder(files.data ?? []);
  return (
    <div
      className="git-log__view"
      ref={columnsRef}
      style={
        {
          "--git-files": `${+(clampPathsFraction(fraction) * 100).toFixed(2)}%`,
        } as CSSProperties
      }
    >
      <section
        className="git-log__files"
        aria-label="Changed files"
        data-testid={`${testIdPrefix}-files`}
      >
        {files.status === "loading" && (
          <RegionNote kind="loading" testId={`${testIdPrefix}-files-loading`}>
            Loading changed files…
          </RegionNote>
        )}
        {files.status === "error" && (
          <RegionNote
            kind="error"
            onRetry={onRetryFiles}
            testId={`${testIdPrefix}-files-error`}
          >
            {rejectionMessage(files.error, errorSubject)}
          </RegionNote>
        )}
        {files.status === "ready" && groups.length === 0 && (
          <RegionNote kind="empty" testId={`${testIdPrefix}-files-empty`}>
            {emptyFilesText}
          </RegionNote>
        )}
        {groups.map((group) => (
          <div
            key={group.folder}
            role="group"
            aria-label={group.label}
            data-testid={`${testIdPrefix}-folder`}
          >
            <div className="git__section-head git-log__folder">
              <Icon.Folder size={12} />
              <span className="git__file-name" title={group.label}>
                {group.label}
              </span>
            </div>
            {group.files.map(({ file, name }) => {
              const selected = file.path === filePath;
              const letter = statusLetter(file.status);
              return (
                <button
                  key={file.path}
                  type="button"
                  className="git__file git-log__file"
                  data-selected={selected || undefined}
                  aria-current={selected ? "true" : undefined}
                  data-testid={`${testIdPrefix}-file`}
                  onClick={() => onSelectFile(file.path)}
                >
                  <span
                    className="git__file-status"
                    data-st={letter}
                    aria-hidden="true"
                  >
                    {letter}
                  </span>
                  <span className="git__file-name" title={file.path}>
                    {name}
                  </span>
                  <span className="sr-only">{statusWord(file.status)}</span>
                  {file.previousPath && (
                    <span className="git__file-path" title={file.previousPath}>
                      from {file.previousPath}
                    </span>
                  )}
                  {file.isBinary && <span className="badge">binary</span>}
                  {selected && <SelectedMark />}
                </button>
              );
            })}
          </div>
        ))}
      </section>
      <FilesDivider
        columnsRef={columnsRef}
        fraction={fraction}
        persist={persist}
        preview={preview}
      />
      <section
        className="git-log__diff"
        aria-label="File diff"
        data-testid={`${testIdPrefix}-diff`}
      >
        {filePath === null ? (
          <RegionNote kind="empty" testId={`${testIdPrefix}-no-file`}>
            Select a file to see its diff.
          </RegionNote>
        ) : (
          <>
            <div className="git-log__diff-head">
              <span className="t-hash">{diffOwner}</span>
              <span className="git__file-path" title={filePath}>
                {filePath}
              </span>
            </div>
            {diff.status === "loading" && (
              <RegionNote kind="loading" testId={`${testIdPrefix}-diff-loading`}>
                Loading diff…
              </RegionNote>
            )}
            {diff.status === "error" && (
              <RegionNote
                kind="error"
                onRetry={onRetryDiff}
                testId={`${testIdPrefix}-diff-error`}
              >
                {rejectionMessage(diff.error, errorSubject)}
              </RegionNote>
            )}
            {diff.status === "ready" && diff.data && (
              <DiffBody payload={diff.data} testIdPrefix={testIdPrefix} />
            )}
          </>
        )}
      </section>
    </div>
  );
}
