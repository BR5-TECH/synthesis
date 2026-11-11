/**
 * One version of a change, read-only, and its comparison with the current
 * change (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-DRQD, OCW-FR-ABAR, OCW-FR-GCME).
 *
 * The version has its own artifact list and its own Rich and Source. Nothing
 * here decides a hunk, opens a fragment discussion, or edits a file: a
 * version is a commit of the stream branch, and it is only read.
 */
import { useMemo, useState } from "react";

import type { ChangeDetail, ChangeVersionFile } from "../../types";
import { formatRelative } from "../ProjectPicker";
import { ArtifactNav, NavLayout, useCollapsedList, type NavItem } from "./ArtifactNav";
import { ChangeMarkdown } from "./ChangeMarkdown";
import type { FileMode } from "./changeFiles";
import type { ShownVersion } from "./useChangeHistory";
import {
  compareWithCurrent,
  shortCommit,
  versionArtifacts,
  type CompareMark,
  type ComparedFile,
} from "./versionFiles";

const MARK_LABEL: Record<CompareMark, string> = {
  added: "added since this version",
  removed: "removed since this version",
  changed: "changed since this version",
  unchanged: "unchanged",
  notText: "not text",
};

function filesLabel(count: number): string {
  return count === 1 ? "1 file" : `${count} files`;
}

/** OCW-FR-DRQD: one file of the version, with its own Rich and Source. */
function VersionFile({
  file,
  mode,
  onMode,
}: {
  file: ChangeVersionFile;
  mode: FileMode;
  onMode: (mode: FileMode) => void;
}) {
  const text = file.binary || file.text == null ? null : file.text;
  return (
    <article className="openspec-file" data-testid="openspec-version-file" data-path={file.path}>
      <header className="openspec-file__head">
        <span className="openspec-file__path t-meta">{file.path}</span>
        <span className="spacer" />
        {text !== null && (
          <span className="openspec-mode" role="group" aria-label={`View of ${file.path}`}>
            {(["rich", "source"] as const).map((m) => (
              <button
                key={m}
                type="button"
                className="btn btn--sm btn--ghost openspec-mode__option"
                aria-pressed={mode === m}
                onClick={() => onMode(m)}
              >
                {m === "rich" ? "Rich" : "Source"}
              </button>
            ))}
          </span>
        )}
      </header>
      {text === null ? (
        <p className="openspec-file__unreadable" data-testid="openspec-unreadable">
          This file is not text, so it is not shown.
        </p>
      ) : mode === "source" ? (
        <div className="openspec-source" data-testid="openspec-source" data-path={file.path}>
          <pre className="openspec-source__text" data-testid="openspec-source-text">
            {text}
          </pre>
        </div>
      ) : (
        <div className="doc openspec-file__text">
          <ChangeMarkdown text={text} path={file.path} />
        </div>
      )}
    </article>
  );
}

/** OCW-FR-ABAR: one file of the comparison, as a line diff. */
function ComparedFileView({ file }: { file: ComparedFile }) {
  return (
    <article
      className="openspec-compare"
      data-testid="openspec-compare-file"
      data-path={file.path}
      data-mark={file.mark}
    >
      <header className="openspec-file__head">
        <span className="openspec-file__path t-meta">{file.path}</span>
        <span className="openspec-compare__mark" data-mark={file.mark}>
          {MARK_LABEL[file.mark]}
        </span>
      </header>
      {file.notText ? (
        <p className="openspec-file__unreadable">This file is not text in one of the two, so no diff is shown.</p>
      ) : file.mark === "unchanged" ? null : (
        <table className="openspec-compare__table" aria-label={`Changes in ${file.path}`}>
          <thead>
            <tr>
              <th scope="col">This version</th>
              <th scope="col">Current change</th>
            </tr>
          </thead>
          <tbody>
            {file.rows.map((row, at) => (
              <tr key={at} data-changed={row.changed || undefined}>
                <td data-side="old" data-empty={row.old === null || undefined}>
                  {row.old?.content ?? ""}
                </td>
                <td data-side="new" data-empty={row.new === null || undefined}>
                  {row.new?.content ?? ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </article>
  );
}

export function VersionView({
  shown,
  detail,
  onBack,
}: {
  shown: ShownVersion;
  /** The current change, for the artifacts of the schema and the comparison. */
  detail: ChangeDetail;
  onBack: () => void;
}) {
  const { version, files, error } = shown;
  const [comparing, setComparing] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [modes, setModes] = useState<Record<string, FileMode>>({});
  const { collapsed, ref: bodyRef } = useCollapsedList();

  const artifacts = useMemo(
    () => (files ? versionArtifacts(detail, files) : []),
    [detail, files],
  );
  const compared = useMemo(
    () => (files && comparing ? compareWithCurrent(files, detail) : []),
    [files, comparing, detail],
  );
  const shownId =
    selected !== null && artifacts.some((a) => a.id === selected)
      ? selected
      : (artifacts.find((a) => a.files.length > 0)?.id ?? artifacts[0]?.id ?? null);
  const artifact = artifacts.find((a) => a.id === shownId) ?? null;
  const items: NavItem[] = artifacts.map((a) => ({ id: a.id, detail: filesLabel(a.files.length) }));

  return (
    <div className="openspec-version" data-testid="openspec-version-view">
      {/* OCW-FR-DRQD: the banner names the version and leads back. */}
      <div className="draft-reading openspec-version__banner" role="status" data-testid="openspec-version-banner">
        <button type="button" className="btn btn--sm" onClick={onBack}>
          ← Back to current
        </button>
        <span className="t-ui-xs openspec-version__name">
          Version {shortCommit(version.commitId)} · {version.summary} · {version.authorName} ·{" "}
          {formatRelative(version.committedAt)} · read-only
        </span>
        <span className="spacer" />
        <button
          type="button"
          className="btn btn--sm btn--ghost"
          aria-pressed={comparing}
          disabled={files === null}
          onClick={() => setComparing((on) => !on)}
        >
          Compare with current
        </button>
      </div>
      {error !== null ? (
        <p className="openspec-change__state" role="alert">
          {error}
        </p>
      ) : files === null ? (
        <p className="openspec-change__state" role="status">
          Reading the version…
        </p>
      ) : comparing ? (
        <div className="openspec-change__reading" role="region" aria-label="Comparison" tabIndex={0}>
          {compared.map((file) => (
            <ComparedFileView key={file.path} file={file} />
          ))}
        </div>
      ) : (
        <div className="openspec-change__body" ref={bodyRef} data-collapsed={collapsed || undefined}>
          <NavLayout
            collapsed={collapsed}
            nav={
              <ArtifactNav
                items={items}
                shown={shownId}
                onChoose={setSelected}
                collapsed={collapsed}
                label="Artifacts of this version"
              />
            }
          >
            <div
              className="openspec-change__reading"
              role="region"
              aria-label="Reading pane"
              tabIndex={0}
              data-testid="openspec-reading"
            >
              {artifact && artifact.files.length === 0 && (
                <p className="openspec-reading__empty" data-testid="openspec-no-file">
                  This version holds no file for {artifact.id}.
                </p>
              )}
              {artifact?.files.map((file) => (
                <VersionFile
                  key={file.path}
                  file={file}
                  mode={modes[file.path] ?? "rich"}
                  onMode={(mode) => setModes((m) => ({ ...m, [file.path]: mode }))}
                />
              ))}
            </div>
          </NavLayout>
        </div>
      )}
    </div>
  );
}
