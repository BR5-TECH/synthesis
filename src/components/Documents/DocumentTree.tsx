/**
 * The tree of the Documents panel and its keyboard model
 * (`../../../specifications/ui/DPN-documents-panel.md` DPN-FR-MICG,
 * DPN-FR-UGVG).
 *
 * The tree is one keyboard stop. The rows are not tab stops: the tree names its
 * current row with `aria-activedescendant`, and the arrow keys move that row.
 * The rows sit in one flat list and carry `aria-level`, which is how a tree is
 * exposed when it renders only the rows of expanded folders.
 *
 * - Down and Up move to the next and the previous row. Home and End move to the
 *   first and the last.
 * - Right opens a closed folder and moves into an open one. Left closes an open
 *   folder and moves out of a closed one or of a document.
 * - Enter and Space open a document, and open or close a folder.
 */
import { useId, useRef } from "react";
import { Icon } from "../icons";
import type { DocumentEntry } from "../../types";
import type { TreeRow } from "./tree";

interface DocumentTreeProps {
  rows: TreeRow[];
  /** The key of the row that holds the keyboard position, or null for none yet. */
  activeKey: string | null;
  onActiveChange: (key: string) => void;
  onToggle: (folderKey: string, expanded: boolean) => void;
  onOpenDocument: (entry: DocumentEntry) => void;
}

function FormatIcon({ format }: { format: DocumentEntry["format"] }) {
  if (format === "pdf") return <Icon.Pdf size={14} aria-hidden="true" />;
  if (format === "markdown") return <Icon.Doc size={14} aria-hidden="true" />;
  return <Icon.File size={14} aria-hidden="true" />;
}

export function DocumentTree({
  rows,
  activeKey,
  onActiveChange,
  onToggle,
  onOpenDocument,
}: DocumentTreeProps) {
  const treeId = useId();
  const treeRef = useRef<HTMLDivElement | null>(null);
  const activeIndex = rows.findIndex((row) => row.node.key === activeKey);
  const rowId = (index: number) => `${treeId}-row-${index}`;

  const activate = (row: TreeRow) => {
    if (row.node.kind === "document") onOpenDocument(row.node.entry);
    else onToggle(row.node.key, !row.expanded);
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (rows.length === 0) return;
    const index = activeIndex < 0 ? 0 : activeIndex;
    const row = rows[index];
    const go = (target: number) => {
      const next = rows[Math.max(0, Math.min(rows.length - 1, target))];
      onActiveChange(next.node.key);
    };
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        go(activeIndex < 0 ? 0 : index + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        go(activeIndex < 0 ? 0 : index - 1);
        break;
      case "Home":
        event.preventDefault();
        go(0);
        break;
      case "End":
        event.preventDefault();
        go(rows.length - 1);
        break;
      case "ArrowRight":
        event.preventDefault();
        if (row.node.kind === "folder") {
          if (!row.expanded) onToggle(row.node.key, true);
          else if (rows[index + 1]?.parentKey === row.node.key) go(index + 1);
        }
        break;
      case "ArrowLeft":
        event.preventDefault();
        if (row.node.kind === "folder" && row.expanded) {
          onToggle(row.node.key, false);
        } else if (row.parentKey !== null) {
          onActiveChange(row.parentKey);
        }
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        if (activeIndex >= 0) activate(row);
        break;
      default:
        break;
    }
  };

  return (
    <div
      ref={treeRef}
      className="documents-tree"
      role="tree"
      aria-label="Documents"
      tabIndex={0}
      aria-activedescendant={activeIndex >= 0 ? rowId(activeIndex) : undefined}
      onKeyDown={onKeyDown}
      onFocus={(event) => {
        // The first arrival at the tree puts the position on the first row.
        if (event.target === treeRef.current && activeKey === null && rows[0]) {
          onActiveChange(rows[0].node.key);
        }
      }}
    >
      {rows.map((row, index) => {
        const node = row.node;
        const isFolder = node.kind === "folder";
        const unavailable = node.kind === "document" && node.entry.status === "unavailable";
        return (
          <div
            key={node.key}
            id={rowId(index)}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-expanded={isFolder ? row.expanded : undefined}
            aria-selected={node.key === activeKey}
            className="tree-row documents-row"
            data-kind={node.kind}
            data-active={node.key === activeKey}
            data-unavailable={unavailable || undefined}
            // DPN-FR-MICG: the full path of a document is its tooltip. A folder
            // row has none, because the collection reports no path for it.
            title={node.kind === "document" ? node.entry.path : undefined}
            style={{ paddingLeft: row.depth * 14 + 4 }}
            onClick={() => {
              onActiveChange(node.key);
              treeRef.current?.focus();
              activate(row);
            }}
          >
            <span className="tree-row__caret" aria-hidden="true">
              {isFolder &&
                (row.expanded ? (
                  <Icon.Caret size={10} />
                ) : (
                  <Icon.CaretRight size={10} />
                ))}
            </span>
            <span className="tree-row__icon" aria-hidden="true">
              {node.kind === "document" ? (
                <FormatIcon format={node.entry.format} />
              ) : row.expanded ? (
                <Icon.FolderOpen size={14} />
              ) : (
                <Icon.Folder size={14} />
              )}
            </span>
            <span className="tree-row__name documents-row__name">{node.label}</span>
            {unavailable && (
              <span className="tree-row__meta documents-row__status">Unavailable</span>
            )}
          </div>
        );
      })}
    </div>
  );
}
