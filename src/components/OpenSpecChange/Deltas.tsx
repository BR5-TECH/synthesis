/**
 * The `specs` artifact of a change, as requirement diffs against the live
 * specs (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-OUXA through OCW-FR-IGTS).
 *
 * Every text goes through the read-only Markdown view of the Document tab
 * (DTV-FR-SSQI), so a requirement renders no script, no raw HTML, no remote
 * resource, and no link that navigates.
 */
import type { ReactNode } from "react";

import { DocumentMarkdown } from "../DocumentViewer/DocumentMarkdown";
import type { ChangeDelta, ChangeFile, Requirement } from "../../types";

/** OCW-FR-OUXA: the deltas of one capability, in the order the read gives them. */
interface CapabilityGroup {
  capability: string;
  exists: boolean;
  deltas: ChangeDelta[];
}

/** OCW-FR-OUXA: group by capability, in the order of first appearance. */
export function groupDeltas(deltas: ChangeDelta[]): CapabilityGroup[] {
  const groups: CapabilityGroup[] = [];
  const byId = new Map<string, CapabilityGroup>();
  for (const delta of deltas) {
    let group = byId.get(delta.capability);
    if (!group) {
      group = { capability: delta.capability, exists: delta.capabilityExists, deltas: [] };
      byId.set(delta.capability, group);
      groups.push(group);
    }
    group.deltas.push(delta);
  }
  return groups;
}

/** A requirement, with its text and every scenario whole. */
function RequirementBody({ requirement }: { requirement: Requirement }) {
  return (
    <div className="openspec-requirement">
      <div className="doc openspec-requirement__text">
        <DocumentMarkdown text={requirement.text} />
      </div>
      {requirement.scenarios.map((scenario, index) => (
        <section className="openspec-scenario" key={`${scenario.name}:${index}`}>
          <h5 className="openspec-scenario__name">Scenario: {scenario.name}</h5>
          <div className="doc openspec-scenario__text">
            <DocumentMarkdown text={scenario.text} />
          </div>
        </section>
      ))}
    </div>
  );
}

/** The head of one delta: its operation and the name it is about. */
function DeltaHead({
  operation,
  children,
}: {
  operation: ChangeDelta["operation"];
  children: ReactNode;
}) {
  return (
    <h4 className="openspec-delta__head">
      <span className="openspec-delta__op" data-op={operation}>
        {operation}
      </span>{" "}
      <span className="openspec-delta__name">{children}</span>
    </h4>
  );
}

/** OCW-FR-YHIX / OCW-FR-FPFE: the live specs hold no requirement of that name. */
function NoLive({ name }: { name: string }) {
  return (
    <p className="openspec-delta__note" data-testid="openspec-no-live">
      The live specs hold no requirement named “{name}”.
    </p>
  );
}

function Delta({ delta }: { delta: ChangeDelta }) {
  const { operation, requirement, live, rename } = delta;
  switch (operation) {
    // OCW-FR-NEQF: the name, the text and the scenarios, marked as added.
    case "ADDED":
      return (
        <article className="openspec-delta" data-op="ADDED" data-testid="openspec-delta">
          <DeltaHead operation={operation}>{requirement?.name ?? ""}</DeltaHead>
          {requirement && (
            <div className="openspec-delta__added" aria-label="Added requirement">
              <RequirementBody requirement={requirement} />
            </div>
          )}
        </article>
      );
    // OCW-FR-WIKH / OCW-FR-YHIX: the live one leading, the changed one beside it.
    case "MODIFIED":
      return (
        <article className="openspec-delta" data-op="MODIFIED" data-testid="openspec-delta">
          <DeltaHead operation={operation}>{requirement?.name ?? live?.name ?? ""}</DeltaHead>
          {live ? (
            <div className="openspec-delta__sides">
              <section className="openspec-delta__side" aria-label="Live requirement">
                <h5 className="openspec-delta__side-label">Live</h5>
                <RequirementBody requirement={live} />
              </section>
              <section className="openspec-delta__side" aria-label="Changed requirement">
                <h5 className="openspec-delta__side-label">Changed</h5>
                {requirement && <RequirementBody requirement={requirement} />}
              </section>
            </div>
          ) : (
            <>
              {requirement && (
                <section className="openspec-delta__side" aria-label="Changed requirement">
                  <h5 className="openspec-delta__side-label">Changed</h5>
                  <RequirementBody requirement={requirement} />
                </section>
              )}
              <NoLive name={requirement?.name ?? ""} />
            </>
          )}
        </article>
      );
    // OCW-FR-FPFE: the live requirement struck through, or its name alone.
    case "REMOVED": {
      const name = requirement?.name ?? live?.name ?? "";
      return (
        <article className="openspec-delta" data-op="REMOVED" data-testid="openspec-delta">
          <DeltaHead operation={operation}>
            <s>{name}</s>
          </DeltaHead>
          {live ? (
            <del className="openspec-delta__removed" aria-label="Removed requirement">
              <RequirementBody requirement={live} />
            </del>
          ) : (
            <NoLive name={name} />
          )}
        </article>
      );
    }
    // OCW-FR-IGTS: the old name and the new name, and the live one under its old name.
    case "RENAMED":
      return (
        <article className="openspec-delta" data-op="RENAMED" data-testid="openspec-delta">
          <DeltaHead operation={operation}>
            <span className="openspec-delta__from">{rename?.from ?? ""}</span>
            {" → "}
            <span className="openspec-delta__to">{rename?.to ?? ""}</span>
          </DeltaHead>
          {live && (
            <section className="openspec-delta__side" aria-label="Live requirement">
              <h5 className="openspec-delta__side-label">Live, as “{rename?.from ?? live.name}”</h5>
              <RequirementBody requirement={live} />
            </section>
          )}
        </article>
      );
  }
}

export interface DeltaListProps {
  deltas: ChangeDelta[];
  /** The files of the `specs` artifact, for OCW-FR-WXFK and OCW-FR-UOKM. */
  files: ChangeFile[];
  /** OCW-FR-UOKM: present for the active worktree alone. */
  onOpenFile?: (file: ChangeFile) => void;
  /** OCW-FR-ABUR: the Rich and Source switch of the file of a capability. */
  fileControls?: (file: ChangeFile) => ReactNode;
  /**
   * OCW-FR-ABUR: the Source of the file of a capability, where the file is
   * shown in Source. Null shows the deltas.
   */
  sourceOf?: (file: ChangeFile) => ReactNode | null;
}

/** OCW-FR-OUXA: the change-relative path of the delta file of a capability. */
export function capabilityFile(capability: string): string {
  return `specs/${capability}/spec.md`;
}

/** OCW-FR-OUXA: the deltas, grouped by capability. */
export function DeltaList({ deltas, files, onOpenFile, fileControls, sourceOf }: DeltaListProps) {
  const groups = groupDeltas(deltas);
  const unreadable = files.filter((f) => f.unreadable || f.text === null);
  return (
    <div className="openspec-deltas" data-testid="openspec-deltas">
      {groups.map((group) => {
        const file = files.find((f) => f.path === capabilityFile(group.capability));
        const source = file && sourceOf ? sourceOf(file) : null;
        return (
          <section
            key={group.capability}
            className="openspec-capability"
            aria-label={`Capability ${group.capability}`}
            data-testid="openspec-capability"
          >
            <header className="openspec-capability__head">
              <h3 className="openspec-capability__id">{group.capability}</h3>
              <span className="openspec-capability__exists t-ui-xs">
                {group.exists
                  ? "The live specs hold this capability"
                  : "The live specs do not hold this capability"}
              </span>
              <span className="spacer" />
              {file && fileControls?.(file)}
              {onOpenFile && file && (
                <button
                  type="button"
                  className="btn btn--sm btn--ghost"
                  onClick={() => onOpenFile(file)}
                >
                  Open in editor
                </button>
              )}
            </header>
            {source ??
              group.deltas.map((delta, index) => (
                <Delta key={`${delta.operation}:${index}`} delta={delta} />
              ))}
          </section>
        );
      })}
      {/* OCW-FR-WXFK: a delta file that is not text says so. */}
      {unreadable.map((file) => (
        <p key={file.path} className="openspec-file__unreadable" data-testid="openspec-unreadable">
          {file.path}: this file is not text, so it is not shown.
        </p>
      ))}
      {groups.length === 0 && unreadable.length === 0 && (
        <p className="openspec-reading__empty">This change holds no deltas.</p>
      )}
    </div>
  );
}
