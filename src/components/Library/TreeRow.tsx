import { useEffect, useRef } from "react";
import { typeChip } from "../../artifactTypes";
import { Icon } from "../icons";
import type { TreeNode } from "../../types";

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

export function TreeRow({
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
      <span className="tree-row__name">{node.name}</span>
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
}
