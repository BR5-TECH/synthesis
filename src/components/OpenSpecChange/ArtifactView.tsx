/**
 * The selected artifact of a change, in the reading pane
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QIZW,
 * OCW-FR-NOTT, OCW-FR-OUXA, OCW-FR-WXFK, OCW-FR-UOKM, OCW-FR-ABUR,
 * OCW-FR-COLR, OCW-FR-RRXG, OCW-FR-FMDZ).
 *
 * Each file offers Rich and Source. Rich is the Markdown rendering, or the
 * deltas for `specs`. Source is the text as stored, with the hunks of the
 * pending proposal where the file holds any.
 */
import type { ReactNode } from "react";

import type { EditorReview } from "../Editor/props";
import type { ChangeArtifact, ChangeDetail, ChangeFile } from "../../types";
import type { ChangeSelection } from "./ChangeDiscussion";
import type { FileMode, ShownFile } from "./changeFiles";
import { ChangeImages, ChangeMarkdown, type ChangeImageSource } from "./ChangeMarkdown";
import { DeltaList, capabilityFile } from "./Deltas";
import type { FragmentMarks } from "./fragmentMarks";
import { SourceView } from "./SourceView";

/** OCW-FR-NOTT / OCW-FR-OUXA: the artifact that renders as deltas. */
export const SPECS_ARTIFACT = "specs";

export interface FileViewing {
  modeOf: (file: ShownFile) => FileMode;
  onMode: (path: string, mode: FileMode) => void;
  /** OCW-FR-COLR: the review of the hunks of one file, if it holds any. */
  reviewFor: (path: string) => EditorReview | undefined;
  /** OCW-FR-DTRX: a refusal at a hunk of one file. */
  refusalFor: (path: string) => string | null;
  /** OCW-FR-JBFZ: present on a stream location alone. */
  onComment?: (selection: ChangeSelection) => void;
  /** CMT-FR-28: the fragment discussions to mark, on a stream alone. */
  marks?: FragmentMarks;
  /** OCW-FR-UOKM: present for the active worktree alone. */
  onOpenFile?: (file: ChangeFile) => void;
}

/** OCW-FR-ABUR: the Rich and Source switch of one file. */
function ModeSwitch({
  file,
  mode,
  onMode,
}: {
  file: ShownFile;
  mode: FileMode;
  onMode: (path: string, mode: FileMode) => void;
}) {
  return (
    <span className="openspec-mode" role="group" aria-label={`View of ${file.path}`}>
      {(["rich", "source"] as const).map((m) => (
        <button
          key={m}
          type="button"
          className="btn btn--sm btn--ghost openspec-mode__option"
          aria-pressed={mode === m}
          // A new file is not on disk yet, so it has no Rich rendering.
          disabled={m === "rich" && file.isNew}
          onClick={() => onMode(file.path, m)}
        >
          {m === "rich" ? "Rich" : "Source"}
        </button>
      ))}
    </span>
  );
}

/** The Source of one file, with its hunks and its refusal. */
function sourceOf(file: ShownFile, viewing: FileViewing): ReactNode {
  return (
    <SourceView
      path={file.path}
      text={file.text ?? ""}
      review={viewing.reviewFor(file.path)}
      refusal={viewing.refusalFor(file.path)}
      onComment={viewing.onComment}
      marks={viewing.marks}
    />
  );
}

function FileArticle({
  file,
  viewing,
  source,
}: {
  file: ShownFile;
  viewing: FileViewing;
  source?: ChangeImageSource;
}) {
  const unreadable = file.unreadable || file.text === null;
  const mode = viewing.modeOf(file);
  return (
    <article className="openspec-file" data-testid="openspec-file" data-path={file.path}>
      <header className="openspec-file__head">
        <span className="openspec-file__path t-meta">{file.path}</span>
        {/* OCW-FR-RRXG: a file a hunk would create is marked as new. */}
        {file.isNew && (
          <span className="openspec-file__new" data-testid="openspec-file-new">
            new
          </span>
        )}
        <span className="spacer" />
        {!unreadable && <ModeSwitch file={file} mode={mode} onMode={viewing.onMode} />}
        {viewing.onOpenFile && !file.isNew && (
          <button
            type="button"
            className="btn btn--sm btn--ghost"
            onClick={() => viewing.onOpenFile?.(file)}
          >
            Open in editor
          </button>
        )}
      </header>
      {unreadable ? (
        // OCW-FR-WXFK: a file that is not text says so, in place of its content.
        <p className="openspec-file__unreadable" data-testid="openspec-unreadable">
          This file is not text, so it is not shown.
        </p>
      ) : mode === "source" ? (
        sourceOf(file, viewing)
      ) : (
        <div className="doc openspec-file__text">
          <ChangeMarkdown text={file.text ?? ""} path={file.path} source={source} />
        </div>
      )}
    </article>
  );
}

export function ArtifactView({
  artifact,
  detail,
  files,
  viewing,
}: {
  artifact: ChangeArtifact;
  detail: ChangeDetail;
  /** OCW-FR-RRXG: the files of the artifact, new files included. */
  files: ShownFile[];
  viewing: FileViewing;
}) {
  // OCW-FR-FMDZ: images read from the working copy of a stream location.
  const source: ChangeImageSource | undefined =
    detail.location.kind === "stream"
      ? { streamId: detail.location.streamId, change: detail.name }
      : undefined;
  if (artifact.id === SPECS_ARTIFACT && (detail.deltas.length > 0 || files.length > 0)) {
    const byPath = new Map(files.map((f) => [f.path, f]));
    const covered = new Set(detail.deltas.map((d) => capabilityFile(d.capability)));
    // A spec file that no delta group shows, a new file among them, stands
    // after the groups as a file of its own.
    const rest = files.filter((f) => !covered.has(f.path) && !f.unreadable && f.text !== null);
    return (
      <ChangeImages dir={SPECS_ARTIFACT} source={source}>
        <DeltaList
          deltas={detail.deltas}
          files={artifact.files}
          onOpenFile={viewing.onOpenFile}
          fileControls={(file) => {
            const shown = byPath.get(file.path);
            if (!shown || shown.unreadable || shown.text === null) return null;
            return <ModeSwitch file={shown} mode={viewing.modeOf(shown)} onMode={viewing.onMode} />;
          }}
          sourceOf={(file) => {
            const shown = byPath.get(file.path);
            if (!shown || viewing.modeOf(shown) !== "source") return null;
            return sourceOf(shown, viewing);
          }}
        />
        {rest.map((file) => (
          <FileArticle key={file.path} file={file} viewing={viewing} source={source} />
        ))}
      </ChangeImages>
    );
  }
  if (files.length === 0) {
    return (
      <p className="openspec-reading__empty" data-testid="openspec-no-file">
        The change holds no file for {artifact.id}.
      </p>
    );
  }
  return (
    <>
      {files.map((file) => (
        <FileArticle key={file.path} file={file} viewing={viewing} source={source} />
      ))}
    </>
  );
}
