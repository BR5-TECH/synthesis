import { memo, useEffect, useRef } from "react";
import { typeChip } from "../../artifactTypes";
import { Icon } from "../icons";
import type { NestedRepo, TreeNode } from "../../types";

/** LIB-FR-VCJO: the accessible name of each repository marker. */
export const REPO_MARKER_LABEL: Record<NestedRepo, string> = {
  submodule: "Git submodule",
  repository: "Nested repository",
  uninitialised: "Git submodule, not initialised",
};

interface TreeRowProps {
  node: TreeNode;
  depth: number;
  open: boolean;
  selected: boolean;
  /** LIB-FR-18: this row is the target of a reveal, so scroll it into view. */
  revealed: boolean;
  onToggle: (node: TreeNode) => void;
  onOpen: (node: TreeNode) => void;
  onContextMenu: (e: React.MouseEvent, node: TreeNode) => void;
}

/**
 * One row of the Project panel's tree. Memoised: the panel renders only the
 * rows in view (LIB-FR-BXMZ), and a row whose node and state did not change
 * does no work on a re-render of the panel.
 */
export const TreeRow = memo(function TreeRow({
  node,
  depth,
  open,
  selected,
  revealed,
  onToggle,
  onOpen,
  onContextMenu,
}: TreeRowProps) {
  const isFolder = node.nodeKind === "folder";
  // LIB-FR-JZNT: a folder that holds nothing has no caret to open.
  const expandable = isFolder && node.hasChildren !== false;
  const indent = { paddingLeft: 4 + depth * 14 };
  const rowRef = useRef<HTMLDivElement>(null);

  // LIB-FR-18: a reveal ends with the node in view. Runs when this row *becomes*
  // the revealed one, which is also the first render after the tree reload
  // brought a just-created node in — the point at which there is an element to
  // scroll to at all.
  useEffect(() => {
    if (!revealed) return;
    // `scrollIntoView` is absent under jsdom, so this is an optional call rather
    // than a guarded one: the behaviour has no test-environment stand-in.
    rowRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [revealed]);

  return (
    <div
      ref={rowRef}
      className="tree-row"
      style={indent}
      data-selected={selected}
      onClick={() => (isFolder ? onToggle(node) : onOpen(node))}
      onContextMenu={(e) => {
        e.preventDefault();
        onContextMenu(e, node);
      }}
    >
      <span className="tree-row__caret">
        {expandable ? (
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
      <span className="tree-row__name">{node.name}</span>
      {/* LIB-FR-VCJO: a repository boundary is marked after its name. The
          marker changes no visibility rule (LIB-FR-VIAZ). */}
      {isFolder && node.nestedRepo && (
        <span
          className="tree-row__repo"
          data-state={node.nestedRepo}
          role="img"
          aria-label={REPO_MARKER_LABEL[node.nestedRepo]}
          title={REPO_MARKER_LABEL[node.nestedRepo]}
        >
          <Icon.Branch size={11} />
        </span>
      )}
      {/* LIB-FR-08: a file shows its resolved type, and a folder shows the
          folder-scope assignment it carries of its own (ASC-FR-18) — on the same
          terms, so the type a folder imposes on its contents is legible from the
          tree rather than only from the context menu. A folder with no assignment
          carries no `artifactType` whatever its contents resolve to, so this one
          condition covers both kinds. */}
      {node.artifactType && (
        <span
          className="chip-type"
          data-type={node.artifactType}
          title={
            node.typeSource
              ? `${node.artifactType} (${node.typeSource})`
              : node.artifactType
          }
        >
          {typeChip(node.artifactType)}
        </span>
      )}
    </div>
  );
});

/** LIB-FR-JZNT: the row under a folder whose children are loading. */
export function LoadingRow({ depth }: { depth: number }) {
  return (
    <div
      className="tree-row tree-row--loading"
      style={{ paddingLeft: 4 + depth * 14 }}
      role="status"
      aria-label="Loading folder"
    >
      <span className="tree-row__caret" />
      <span className="tree-row__name">Loading…</span>
    </div>
  );
}

/** LIB-FR-PPAT: the text of the last row of a truncated filter result. */
const TRUNCATED_TEXT = "More matches exist. Type more to narrow them.";

/** LIB-FR-PPAT: the last row when a filter has more matches than it shows. */
export function TruncatedRow() {
  return (
    <div className="tree-row tree-row--note" role="note" title={TRUNCATED_TEXT}>
      <span className="tree-row__caret" />
      <span className="tree-row__name">{TRUNCATED_TEXT}</span>
    </div>
  );
}
