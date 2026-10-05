/**
 * One path list of the selected run: the change set or the paths the ignore
 * rules hide (`../../../specifications/ui/GRU-graduation-runs.md`
 * GRU-FR-RRNN, GRU-FR-WJHV, GRU-FR-TXLW, GRU-FR-HEQB, GRU-FR-NUCJ).
 *
 * The list is a folder tree in a box that scrolls inside itself, so a run that
 * changed hundreds of paths costs the region a few rows and not a wall of text.
 */

import { useMemo, useState } from "react";

import {
  buildPathTree,
  defaultOpenFolders,
  pathTotal,
  type PathNode,
} from "../../state/graduation";
import { Icon } from "../icons";

export interface PathListSectionProps {
  /** The list's name, before its count: "Changed", "Hidden by ignore rules". */
  label: string;
  paths: readonly string[];
  /** GRU-FR-WJHV: how many more paths the backend did not list. */
  omitted?: number;
  testId: string;
  /**
   * GRU-FR-AJGM: what each path is annotated with, by path — what either side
   * of a merge did to it. Read text, so a path carrying one opens nothing.
   */
  annotations?: Readonly<Record<string, string>>;
  /** GRU-FR-AJGM: words after the count in the heading, such as how many paths in all. */
  suffix?: string;
}

export function PathListSection({
  label,
  paths,
  omitted,
  testId,
  annotations,
  suffix,
}: PathListSectionProps) {
  // GRU-FR-NUCJ: a list is open when its run is first selected. The parent
  // keys this section by run, so another run starts open again.
  const [open, setOpen] = useState(true);
  const tree = useMemo(() => buildPathTree(paths), [paths]);
  // Counted from the tree, so the heading and every folder agree on a list
  // that names one path twice.
  const total = useMemo(() => pathTotal(tree), [tree]);
  const defaults = useMemo(() => defaultOpenFolders(tree, total), [tree, total]);
  // Only the folders the author toggled are held. Every other folder follows
  // GRU-FR-HEQB for the list as it is now, so a list that grows while the run
  // works keeps the rule rather than the shape it had at first.
  const [toggled, setToggled] = useState<ReadonlyMap<string, boolean>>(
    () => new Map(),
  );
  const isOpen = (key: string) => toggled.get(key) ?? defaults.has(key);
  const toggle = (key: string) =>
    setToggled((held) => new Map(held).set(key, !isOpen(key)));

  return (
    <section className="graduation__changeset" data-testid={testId}>
      <button
        type="button"
        className="graduation__paths-head t-eyebrow"
        aria-expanded={open}
        onClick={() => setOpen((held) => !held)}
      >
        <span className="graduation__paths-caret" aria-hidden="true">
          {open ? <Icon.Caret size={12} /> : <Icon.CaretRight size={12} />}
        </span>
        {label} · {pathCount(total)}
        {suffix ?? ""}
      </button>
      {open && (
        <div className="graduation__paths">
          <PathLevel
            nodes={tree}
            depth={0}
            isOpen={isOpen}
            onToggle={toggle}
            annotations={annotations}
          />
        </div>
      )}
      {omitted ? (
        <p className="t-meta">{omitted} more are not listed.</p>
      ) : null}
    </section>
  );
}

interface PathLevelProps {
  nodes: readonly PathNode[];
  depth: number;
  isOpen: (key: string) => boolean;
  onToggle: (key: string) => void;
  annotations?: Readonly<Record<string, string>>;
}

function PathLevel({
  nodes,
  depth,
  isOpen,
  onToggle,
  annotations,
}: PathLevelProps) {
  return (
    <ul className="graduation__path-level">
      {nodes.map((node) => {
        // The row's own leading inset: one step per level, after the box's.
        const inset = { paddingLeft: 4 + depth * 14 };
        if (node.kind === "file") {
          return (
            <li key={node.key}>
              {/* GRU-FR-MRPE: a path is rendered as text, never as markup. */}
              <div
                className="tree-row graduation__path-file"
                style={inset}
                data-path={node.path}
                title={node.path}
              >
                <span className="tree-row__caret" />
                <span className="tree-row__icon" aria-hidden="true">
                  <Icon.File size={13} />
                </span>
                <span className="tree-row__name">{node.name}</span>
                {annotations?.[node.path] && (
                  <span
                    className="graduation__path-fate t-meta"
                    data-testid="graduation-path-fate"
                  >
                    {annotations[node.path]}
                  </span>
                )}
              </div>
            </li>
          );
        }
        const open = isOpen(node.key);
        return (
          <li key={node.key}>
            {/* GRU-FR-HEQB: a control the keyboard reaches, with its state in
                accessible semantics. GRU-FR-NBRO: the count in words too. */}
            <button
              type="button"
              className="tree-row graduation__path-folder"
              style={inset}
              aria-expanded={open}
              data-folder={node.path}
              title={node.path}
              onClick={() => onToggle(node.key)}
            >
              <span className="tree-row__caret" aria-hidden="true">
                {open ? <Icon.Caret size={12} /> : <Icon.CaretRight size={12} />}
              </span>
              <span className="tree-row__icon" aria-hidden="true">
                <Icon.Folder size={13} />
              </span>
              <span className="tree-row__name">{node.label}</span>
              <span className="graduation__path-count" aria-hidden="true">
                {node.count}
              </span>
              <span className="sr-only">, {pathCount(node.count)}</span>
            </button>
            {open && (
              <PathLevel
                nodes={node.children}
                depth={depth + 1}
                isOpen={isOpen}
                onToggle={onToggle}
                annotations={annotations}
              />
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** GRU-FR-RRNN / GRU-FR-WJHV: how many paths a list holds, in words. */
function pathCount(paths: number): string {
  return paths === 1 ? "1 path" : `${paths} paths`;
}
