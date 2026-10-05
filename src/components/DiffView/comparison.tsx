import type { ReactElement } from "react";
import {
  RENDERING_MODES,
  VISUALIZATION_MODES,
  setRenderingMode,
  setVisualizationMode,
} from "../../state/diffModes";
import { Icon } from "../icons";
import { EditableLine, type TargetEditing } from "../DiffTarget";
import type {
  DiffPayload,
  DiffRenderingMode,
  DiffVisualizationMode,
  FileRevisions,
} from "../../types";
import {
  SourceFinal,
  SourceSideBySide,
  SourceUnified,
  useRevisionTokens,
} from "./source";
import { RichFinal, RichSideBySide, RichUnified } from "./rich";
import { FlowFinal, FlowSideBySide, FlowUnified } from "./flow";

/**
 * DFV-FR-07: one toggle. The glyph is the affordance and the mode's name is its
 * accessible name — the label still reaches a screen reader and the tooltip
 * still says what the mode does, so nothing is lost by dropping the caption.
 */
const MODE_ICONS: Record<string, (props: { size?: number }) => ReactElement> = {
  unified: Icon.Unified,
  side_by_side: Icon.SideBySide,
  final: Icon.Final,
  source: Icon.Code,
  rich: Icon.Rich,
};

function ModeToggle({
  option,
  active,
  disabled,
  onActivate,
}: {
  option: { value: string; label: string; hint: string };
  active: boolean;
  disabled?: boolean;
  onActivate: () => void;
}) {
  const Glyph = MODE_ICONS[option.value];
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      aria-label={option.label}
      className="btn btn--ghost btn--sm btn--icon"
      data-active={active}
      disabled={disabled}
      title={`${option.label} — ${option.hint}`}
      onClick={onActivate}
    >
      <Glyph size={14} />
    </button>
  );
}

/**
 * The comparison itself, in whichever of the six mode combinations is active.
 *
 * Extracted from the Diff tab rather than duplicated for it, because
 * `DCR-draft-change-review.md` reads a proposed change "in the modes of
 * `DFV-diff-viewer.md` and in no others" (DCR-FR-07). Two renderers would drift
 * on the first change to either; one means a diff read in the review modal and a
 * diff read in a Diff tab really are one thing read in two places.
 *
 * What the two callers do differ in is where the comparison came from — the
 * backend for a tab, the surface's own derivation for the modal — which is why
 * this takes a payload and revisions rather than fetching either.
 */
export function DiffComparisonBody({
  payload,
  revisions,
  visualization,
  rendering,
  flow,
  isBinary,
  error,
  awaiting,
  emptyLabel = "No changes in this file for this comparison.",
  editing,
  deletedState,
  emptyTargetLabel,
  fileName,
}: {
  payload: DiffPayload | null;
  revisions: FileRevisions | null;
  visualization: DiffVisualizationMode;
  rendering: DiffRenderingMode;
  flow: boolean;
  isBinary: boolean;
  error: string | null;
  awaiting: boolean;
  emptyLabel?: string;
  /**
   * DFV-FR-41: the target's editing surface. Absent — a Flow's graph reading
   * (DFV-FR-48), a decided proposal (DCR-FR-17), a comparison with no target at
   * all — and every mode renders read-only, which is what the tab did before the
   * target became editable.
   */
  editing?: TargetEditing;
  /**
   * DFV-FR-15 / DFV-FR-54: what **Final** renders in place of an outcome when
   * the comparison deletes the file. The restore affordance is the caller's,
   * because creating the file is an operation this renderer does not reach.
   */
  deletedState?: ReactElement;
  /**
   * DFV-FR-54 / DCR-FR-31: what an empty-but-existing target says about itself,
   * so an empty editable surface is a place to write rather than a blank region
   * with nothing to act on.
   */
  emptyTargetLabel?: string;
  /**
   * DFV-FR-57: the file's name, from which its language is resolved. A caller
   * that cannot name the file passes nothing and its rows render plain — a
   * language cannot honestly be guessed from a comparison alone.
   */
  fileName?: string;
}) {
  // DFV-FR-57 / DFV-FR-27: Source is the only rendering with rows to colour, and
  // a binary comparison has no text rendered anywhere to colour.
  const tokens = useRevisionTokens(
    fileName,
    revisions,
    rendering !== "rich" && !isBinary && !flow,
  );
  if (error) {
    return (
      <div className="changes-state" data-state="error">
        {error}
      </div>
    );
  }
  // DFV-FR-27: binary content has no reading in any visualization mode.
  if (isBinary) {
    return (
      <div className="changes-state" data-state="binary">
        Binary file — no textual diff.
      </div>
    );
  }
  // DFV-FR-28: the comparison yields no change for this file. Scoped to
  // Unified, because that is the mode with nothing to put in its place —
  // DFV-FR-09 renders nothing outside a hunk, so an unchanged file would leave
  // a blank pane. The whole-file modes render the file in full whether or not
  // it changed, which is what DFV-FR-11 and DFV-FR-14 say they do. Distinct
  // from the binary state above and the deleted state below.
  if (
    visualization === "unified" &&
    payload != null &&
    !payload.isBinary &&
    payload.hunks.length === 0
  ) {
    return (
      <div className="changes-state" data-state="empty">
        {emptyLabel}
      </div>
    );
  }
  if (payload == null || awaiting) {
    return (
      <div className="changes-state" data-state="loading">
        Loading…
      </div>
    );
  }

  // DFV-FR-15: the comparison deletes the file outright.
  const deleted = revisions != null && revisions.new == null;

  // DFV-FR-54: a target that exists but holds nothing is still a place to
  // write, and says so rather than rendering as a blank region. Distinct from
  // the deleted state below, which has no target at all.
  const emptyTarget =
    editing != null && revisions?.new === "" && emptyTargetLabel != null ? (
      <div className="diff diff-empty-target" data-testid="diff-empty-target">
        <p className="diff-empty-target__note">{emptyTargetLabel}</p>
        {/* The row the author types the first line into. Every mode renders the
            target as rows, and an empty target has none — so without this the
            statement above would be a message with nothing to act on, which is
            exactly what DFV-FR-54 says it must not be. */}
        <div className="diff-line" data-kind="add">
          <span className="diff-line__gutter diff-line__gutter--new">1</span>
          <span className="diff-line__sign">+</span>
          <EditableLine content="" line={0} editing={editing} />
        </div>
      </div>
    ) : null;

  if (visualization === "final") {
    if (deleted) {
      return (
        deletedState ?? (
          <div className="changes-state" data-state="deleted">
            This file does not exist in the new revision.
          </div>
        )
      );
    }
    if (rendering !== "rich")
      return (
        <>
          {emptyTarget}
          <SourceFinal revisions={revisions} editing={editing} tokens={tokens} />
        </>
      );
    // A Flow first, wherever a file is both: its resolved type is what it is,
    // while `.md` says only that the bytes are Markdown (DFV-FR-17).
    // DFV-FR-48: a Flow's rich reading is a reading of two graphs rather than a
    // document, so no editing contract is exposed over it at all.
    return flow ? (
      <FlowFinal revisions={revisions} />
    ) : (
      <>
        {emptyTarget}
        <RichFinal revisions={revisions} editing={editing} />
      </>
    );
  }

  if (visualization === "side_by_side") {
    if (rendering !== "rich")
      return (
        <>
          {emptyTarget}
          <SourceSideBySide revisions={revisions} editing={editing} tokens={tokens} />
        </>
      );
    return flow ? (
      <FlowSideBySide revisions={revisions} />
    ) : (
      <>
        {emptyTarget}
        <RichSideBySide revisions={revisions} editing={editing} />
      </>
    );
  }

  if (rendering !== "rich")
    return (
      <>
        {emptyTarget}
        <SourceUnified payload={payload} editing={editing} tokens={tokens} />
      </>
    );
  return flow ? (
    <FlowUnified revisions={revisions} />
  ) : (
    <>
      {emptyTarget}
      <RichUnified revisions={revisions} editing={editing} />
    </>
  );
}

/**
 * DFV-FR-07: the two toggle groups, as one centred cluster.
 *
 * Shared with the review modal for the reason the body is (DCR-FR-07, DCR-FR-09):
 * the modes are one user-global choice, so the control that sets them has to be
 * the same control wherever a comparison is read — a second copy would be a
 * second place for the persistence to be got wrong.
 */
export function DiffModeToolbar({
  visualization,
  rendering,
  richApplies,
  writeState,
  onVisualization = setVisualizationMode,
  onRendering = setRenderingMode,
}: {
  visualization: DiffVisualizationMode;
  rendering: DiffRenderingMode;
  richApplies: boolean;
  /**
   * DFV-FR-51 / DCR-FR-26: the target's write state, at the row's trailing end
   * and nowhere else — the two toggle groups keep the centre.
   */
  writeState?: ReactElement;
  /**
   * Where an activation goes. Defaults to the user-global store, which is what
   * DFV-FR-24 means by one choice governing every Diff tab and both change
   * review modals at once.
   *
   * The graduation review passes its own pair instead: it opens on
   * the outcome read as a document whatever the author last read a Diff tab in,
   * and keeps its toggles to itself — a change set an agent wrote is read to be
   * accepted or sent back rather than to check work the author did, so the
   * author's habit is not the right default there and a mode chosen inside it is
   * no statement about how they read their own diffs. Overriding the sink rather
   * than copying the control keeps one toolbar for every surface.
   */
  onVisualization?: (next: DiffVisualizationMode) => void;
  onRendering?: (next: DiffRenderingMode) => void;
}) {
  return (
    <div className="diff-toolbar">
      {/* DFV-FR-07: the two groups and their divider sit together as ONE
          cluster in the row's middle column, so the write state at the trailing
          end cannot pull them off-centre. */}
      <div className="diff-toolbar__cluster">
        <div
          className="diff-toolbar__group"
          role="radiogroup"
          aria-label="Diff visualization"
        >
          {VISUALIZATION_MODES.map((option) => (
            <ModeToggle
              key={option.value}
              option={option}
              active={visualization === option.value}
              onActivate={() => onVisualization(option.value)}
            />
          ))}
        </div>
        <span className="diff-toolbar__divider" aria-hidden />
        <div
          className="diff-toolbar__group"
          role="radiogroup"
          aria-label="Diff rendering"
        >
          {RENDERING_MODES.map((option) => (
            <ModeToggle
              key={option.value}
              option={option}
              active={rendering === option.value}
              disabled={!richApplies}
              onActivate={() => onRendering(option.value)}
            />
          ))}
        </div>
      </div>
      {/* DFV-FR-18: the explanation sits beside the group rather than inside
          it — a radiogroup holds radios. */}
      {!richApplies && (
        <span className="diff-toolbar__note">
          Rich rendering doesn’t apply to this file.
        </span>
      )}
      {writeState}
    </div>
  );
}
