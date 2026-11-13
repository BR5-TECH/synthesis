/**
 * The artifact list and the reading pane of a change workspace
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QIZW,
 * OCW-FR-ABUR, OCW-FR-COLR, OCW-FR-RRXG, OCW-FR-GCME).
 */
import { useMemo } from "react";

import type { EditorReview } from "../Editor/props";
import type { ChangeArtifact, ChangeDetail, ChangeEditProposal, ChangeFile } from "../../types";
import { ArtifactNav, NavLayout, useCollapsedList, type NavItem } from "./ArtifactNav";
import { ArtifactView, type FileViewing } from "./ArtifactView";
import type { ChangeSelection } from "./ChangeDiscussion";
import type { FragmentMarks } from "./fragmentMarks";
import {
  modeOf,
  pathsWithHunks,
  shownFiles,
  undecidedByArtifact,
  type FileMode,
} from "./changeFiles";
import type { HunkRefusal } from "./useChangeReview";

const STATUS_LABEL: Record<ChangeArtifact["status"], string> = {
  done: "done",
  ready: "ready",
  blocked: "blocked",
  skipped: "skipped",
};

export function ChangeBody({
  detail,
  shown,
  onChoose,
  modes,
  onMode,
  pending,
  review,
  pathOf,
  refusal,
  onComment,
  marks,
  onOpenFile,
}: {
  detail: ChangeDetail;
  /** The id of the artifact on show. */
  shown: string | null;
  onChoose: (id: string) => void;
  /** OCW-FR-ABUR: the choices the tab holds. */
  modes: Readonly<Record<string, FileMode>>;
  onMode: (path: string, mode: FileMode) => void;
  /** OCW-FR-COLR: the pending proposal of the change, on a stream alone. */
  pending?: ChangeEditProposal;
  review?: EditorReview;
  pathOf?: (hunkId: string) => string | undefined;
  refusal?: HunkRefusal | null;
  onComment?: (selection: ChangeSelection) => void;
  marks?: FragmentMarks;
  onOpenFile?: (file: ChangeFile) => void;
}) {
  const counts = useMemo(() => undecidedByArtifact(detail, pending), [detail, pending]);
  const withHunks = useMemo(() => pathsWithHunks(pending), [pending]);
  const artifact = detail.artifacts.find((a) => a.id === shown) ?? null;
  const files = useMemo(
    () => (artifact ? shownFiles(detail, artifact, pending) : []),
    [detail, artifact, pending],
  );

  const viewing: FileViewing = {
    modeOf: (file) => modeOf(file, modes, withHunks),
    onMode,
    // OCW-FR-COLR: each file takes the hunks of the proposal on its own path.
    reviewFor: (path) => {
      if (!review || !pathOf) return undefined;
      const hunks = review.hunks.filter((h) => pathOf(h.id) === path);
      if (hunks.length === 0) return undefined;
      return { ...review, hunks, onPlaced: () => {} };
    },
    refusalFor: (path) =>
      refusal && pathOf && pathOf(refusal.hunkId) === path ? refusal.message : null,
    onComment,
    marks,
    onOpenFile,
  };

  // OCW-FR-GCME: the list collapses to a menu where the pane would be narrow.
  const { collapsed, ref: bodyRef } = useCollapsedList();
  // OCW-FR-QIZW / OCW-FR-RRXG: the artifacts of the schema, in order, with
  // their states and the count of the undecided hunks each holds.
  const items: NavItem[] = detail.artifacts.map((a) => ({
    id: a.id,
    detail: STATUS_LABEL[a.status],
    status: a.status,
    undecided: counts.get(a.id) ?? 0,
  }));

  return (
    <div className="openspec-change__body" ref={bodyRef} data-collapsed={collapsed || undefined}>
      <NavLayout
        collapsed={collapsed}
        nav={<ArtifactNav items={items} shown={shown} onChoose={onChoose} collapsed={collapsed} />}
      >
        <div
          className="openspec-change__reading"
          role="region"
          aria-label="Reading pane"
          tabIndex={0}
          data-testid="openspec-reading"
        >
          {artifact && (
            <ArtifactView artifact={artifact} detail={detail} files={files} viewing={viewing} />
          )}
        </div>
      </NavLayout>
    </div>
  );
}
