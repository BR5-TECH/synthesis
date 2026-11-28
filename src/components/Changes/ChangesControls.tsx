/**
 * The control stack of the Changes panel, beneath its header: the text filter,
 * the mode selector with the target-branch picker, and the artifact-type lens
 * (CHG-FR-02, CHG-FR-05, CHG-FR-14, SNV-FR-58, SNV-FR-60).
 */
import type { TypeLens } from "../../artifactTypes";
import type { BranchOption, ChangesMode } from "../../types";
import { Icon } from "../icons";
import { SelectorRow, type SelectorPosition } from "../SelectorRow";

/**
 * CHG-FR-02 / SNV-FR-62: the mode selector's two positions. Module-level so its
 * identity is stable across renders — `SelectorRow` re-measures its fit when the
 * position list changes, and a fresh array every render would make that a
 * per-render cost for a list that never varies.
 */
const MODE_POSITIONS: SelectorPosition<ChangesMode>[] = [
  { value: "uncommitted", tag: "Uncommitted", title: "Uncommitted" },
  { value: "branch", tag: "Branch", title: "Branch" },
];

interface ChangesControlsProps {
  /** SNV-FR-60: an empty change set leaves nothing for the filter and the lens to narrow. */
  emptyChangeSet: boolean;
  text: string;
  onText: (next: string) => void;
  mode: ChangesMode;
  onMode: (next: ChangesMode) => void;
  targetBranch: string | null;
  onTargetBranch: (next: string) => void;
  branches: BranchOption[];
  lensPositions: SelectorPosition<TypeLens>[];
  typeFilter: TypeLens;
  onTypeFilter: (next: TypeLens) => void;
}

export function ChangesControls({
  emptyChangeSet,
  text,
  onText,
  mode,
  onMode,
  targetBranch,
  onTargetBranch,
  branches,
  lensPositions,
  typeFilter,
  onTypeFilter,
}: ChangesControlsProps) {
  return (
    <div className="panel-controls">
      {!emptyChangeSet && (
        <div className="search-input" style={{ height: 24 }}>
          <Icon.Search size={12} />
          <input
            placeholder="Filter files…"
            aria-label="Filter files"
            value={text}
            onChange={(e) => onText(e.target.value)}
          />
        </div>
      )}

      {/* CHG-FR-02: the two mutually-exclusive modes, in the one selector
          form SNV-FR-62 gives every panel. The branch picker rides beside the
          row rather than in it: it names *what* Branch mode compares against
          rather than being a position of the mode selector itself. */}
      {/* Centred as one group rather than leaving the toggle to centre
          itself: inside this row the selector is content-sized, so it is the
          wrapper that has width to distribute — and in Branch mode the
          picker beside it is part of what the author reads as one control. */}
      <div
        style={{
          display: "flex",
          gap: 4,
          alignItems: "center",
          justifyContent: "center",
          minWidth: 0,
        }}
      >
        <SelectorRow
          label="Comparison mode"
          positions={MODE_POSITIONS}
          value={mode}
          onChange={onMode}
        />
        {/* CHG-FR-05: shown in Branch mode only. */}
        {mode === "branch" && (
          <>
            <span style={{ fontSize: "var(--fs-ui-xs)", color: "var(--fg-3)" }}>
              against
            </span>
            <select
              className="select select--sm"
              aria-label="Target branch"
              value={targetBranch ?? ""}
              onChange={(e) => onTargetBranch(e.target.value)}
            >
              {/* A persisted branch that no longer exists is still shown as
                  the current value, so the picker never silently retargets
                  the comparison (CHG-FR-27). */}
              {targetBranch != null &&
                !branches.some((b) => b.name === targetBranch) && (
                  <option value={targetBranch}>{targetBranch}</option>
                )}
              {branches.map((b) => (
                <option key={b.name} value={b.name}>
                  {b.name}
                  {b.isDefault ? " (default)" : ""}
                </option>
              ))}
            </select>
          </>
        )}
      </div>

      {/* CHG-FR-14: the lens, on the terms LIB-FR-19 states — it offers the
          types this comparison actually touched. Absent while the change set
          is empty: SNV-FR-60 leaves nothing beside the block for it to
          narrow. */}
      {!emptyChangeSet && (
        <SelectorRow
          label="Filter by type"
          positions={lensPositions}
          value={typeFilter}
          onChange={onTypeFilter}
        />
      )}
    </div>
  );
}
