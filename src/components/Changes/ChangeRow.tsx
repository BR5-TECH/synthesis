/**
 * The rows of the Changes tree: the diffstat cell, the checkbox, the file count
 * of a folder or group, and the file or folder row (CHG-FR-10, CHG-FR-11,
 * CHG-FR-12, CHG-FR-28, CHG-FR-50).
 */
import { useEffect, useRef } from "react";
import { typeChip } from "../../artifactTypes";
import type { ChangeEntry } from "../../types";
import { Icon } from "../icons";
import { visibleFileCount, type ChangeNode, type CheckState } from "./tree";

/**
 * The diffstat cell (CHG-FR-10 / CHG-FR-11). Rendered at a fixed width so rows
 * do not shift horizontally as counts change during a reload.
 */
function DiffStat({ entry }: { entry: ChangeEntry }) {
  if (entry.isBinary) {
    return (
      <span className="change-row__stat" title="Binary file">
        <span className="change-row__binary">binary</span>
      </span>
    );
  }
  return (
    <span className="change-row__stat">
      <span className="change-row__added">+{entry.addedLines ?? 0}</span>
      <span className="change-row__removed">−{entry.removedLines ?? 0}</span>
    </span>
  );
}

/**
 * A checkbox whose indeterminate state is applied imperatively — the DOM
 * property has no JSX attribute, and a folder with only some of its visible
 * descendants ticked has to render as neither on nor off (CHG-FR-28).
 */
export function CheckBox({
  state,
  label,
  onChange,
}: {
  state: CheckState;
  label: string;
  onChange: (next: boolean) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = state === "indeterminate";
  }, [state]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className="change-check"
      aria-label={label}
      checked={state === "checked"}
      // CHG-FR-18: toggling a checkbox is not a click on the row — it changes
      // the check and opens nothing.
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
}

/**
 * CHG-FR-50 / CHG-FR-53: the count of changed files visible beneath a folder or
 * group node, stated immediately after its label. It answers "how much is in
 * here?" without the node being opened and keeps answering it once it is, so it
 * is not conditioned on the node's expand state.
 *
 * CHG-FR-51: stated with the noun it counts and agreeing with it — `1 file`,
 * `13 files` — so the row reads as a sentence about its contents rather than
 * leaving the reader to work out what the digit beside a folder measures.
 *
 * It is text on the row and nothing more (CHG-FR-53): no activation target of
 * its own, so a pointer on it toggles the node exactly as the label does. A node
 * with nothing visible beneath it is never rendered (CHG-FR-08 / CHG-FR-16), so
 * no zero reaches the screen.
 */
export function NodeCount({ node }: { node: ChangeNode }) {
  const count = visibleFileCount(node);
  // No tooltip: the row carries none anywhere else, and one here would be an
  // affordance the count is not supposed to have.
  return (
    <span className="tree-row__count">
      {count} {count === 1 ? "file" : "files"}
    </span>
  );
}

interface ChangeRowProps {
  node: ChangeNode;
  depth: number;
  open: boolean;
  selected: boolean;
  /** CHG-FR-26: null in Branch mode, which renders no checkbox at all. */
  checkState: CheckState | null;
  onCheck: (node: ChangeNode, next: boolean) => void;
  onToggle: (node: ChangeNode) => void;
  onOpen: (node: ChangeNode) => void;
}

export function ChangeRow({
  node,
  depth,
  open,
  selected,
  checkState,
  onCheck,
  onToggle,
  onOpen,
}: ChangeRowProps) {
  const isFolder = node.kind === "folder";
  const entry = node.entry;
  return (
    <div
      className="tree-row"
      style={{ paddingLeft: 4 + depth * 14 }}
      data-selected={selected}
      // CHG-FR-54: what a reveal scrolls to. The key rather than the path,
      // because the same path under **Revisioned** and **Unrevisioned** is two
      // distinct rows (CHG-FR-09).
      data-node-key={node.key}
      data-change-status={entry?.changeStatus}
      onClick={() => (isFolder ? onToggle(node) : onOpen(node))}
    >
      {checkState && (
        <CheckBox
          state={checkState}
          label={`Include ${node.path}`}
          onChange={(next) => onCheck(node, next)}
        />
      )}
      <span className="tree-row__caret">
        {isFolder ? (
          open ? (
            <Icon.Caret size={12} />
          ) : (
            <Icon.CaretRight size={12} />
          )
        ) : null}
      </span>
      <span className="tree-row__icon">
        {isFolder ? <Icon.Folder size={13} /> : <Icon.File size={13} />}
      </span>
      <span
        className={
          isFolder ? "tree-row__name tree-row__name--counted" : "tree-row__name"
        }
      >
        {node.name}
        {/* CHG-FR-12: a renamed entry sits at its current path and additionally
            shows where it came from. */}
        {entry?.previousPath && (
          <span className="change-row__renamed"> (was {entry.previousPath})</span>
        )}
      </span>
      {/* CHG-FR-50: how many changed files are visible beneath this folder,
          against the label rather than in the trailing column the diffstats
          occupy — a folder's file count read as a line count would be worse
          than no count at all. */}
      {isFolder && <NodeCount node={node} />}
      {entry?.artifactType && (
        <span
          className="chip-type"
          data-type={entry.artifactType}
          title={
            entry.typeSource
              ? `${entry.artifactType} (${entry.typeSource})`
              : entry.artifactType
          }
        >
          {typeChip(entry.artifactType)}
        </span>
      )}
      {entry && <DiffStat entry={entry} />}
    </div>
  );
}
