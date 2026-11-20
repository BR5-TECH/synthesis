/**
 * The body of the Changes tree: the repository groups (CHG-FR-BYWJ,
 * CHG-FR-UMCU), the two groups of CHG-FR-09 of each repository, and the folder
 * and file rows beneath them (CHG-FR-08, CHG-FR-28, CHG-FR-50). With one
 * repository no repository group renders (CHG-FR-IGSD).
 */
import { Fragment, type ReactNode } from "react";
import { Icon } from "../icons";
import { CheckBox, ChangeRow, NodeCount } from "./ChangeRow";
import {
  nodeCheckOf,
  repositoryBranchLabel,
  type RepositoryGroupModel,
  type SectionModel,
} from "./groups";
import type { ChangeNode } from "./tree";

/** What every row of the tree reads, from the panel. */
export interface TreeRowContext {
  collapsed: ReadonlySet<string>;
  selectedKey: string | null;
  /** CHG-FR-26: false in Branch mode, which renders no checkbox. */
  showChecks: boolean;
  /** CHG-FR-30: the paths the author checked. */
  checked: ReadonlySet<string>;
  /** CHG-FR-AYTR: the pointer paths whose check is implied. */
  impliedPointers: ReadonlySet<string>;
  onCheck: (node: ChangeNode, next: boolean) => void;
  onToggle: (node: ChangeNode) => void;
  onOpen: (node: ChangeNode) => void;
}

/** CHG-FR-AYTR: why a node whose rows are all implied pointers takes no input. */
const IMPLIED_TITLE =
  "Included: the commits of the submodules record these pointers.";

/** The left padding of a row at `depth`, the same step the folder rows use. */
const indent = (depth: number) => 4 + depth * 14;

/**
 * A group row: a repository group, or one of the two groups of CHG-FR-09. Each
 * is a node like a folder for the cascade (CHG-FR-28) and carries its count
 * (CHG-FR-50).
 */
function GroupRow({
  node,
  depth,
  icon,
  className,
  checkLabel,
  meta,
  title,
  ctx,
}: {
  node: ChangeNode;
  depth: number;
  icon: ReactNode;
  className: string;
  checkLabel: string;
  meta?: ReactNode;
  /** CHG-FR-UMCU: the full label, for a header that the panel width truncates. */
  title?: string;
  ctx: TreeRowContext;
}) {
  const open = !ctx.collapsed.has(node.key);
  const check = nodeCheckOf(node, ctx.checked, ctx.impliedPointers);
  return (
    <div
      className={`tree-row ${className}`}
      style={{ paddingLeft: indent(depth) }}
      data-selected={ctx.selectedKey === node.key}
      data-node-key={node.key}
      title={title}
      onClick={() => ctx.onToggle(node)}
    >
      {ctx.showChecks && (
        <CheckBox
          state={check.state}
          label={checkLabel}
          onChange={(next) => ctx.onCheck(node, next)}
          disabled={check.inert}
          title={check.inert ? IMPLIED_TITLE : undefined}
        />
      )}
      <span className="tree-row__caret">
        {open ? <Icon.Caret size={12} /> : <Icon.CaretRight size={12} />}
      </span>
      <span className="tree-row__icon">{icon}</span>
      <span className="tree-row__name tree-row__name--counted">{node.name}</span>
      {meta}
      <NodeCount node={node} />
    </div>
  );
}

/** The folder and file rows beneath a group, in tree order. */
function renderNodes(
  nodes: ChangeNode[],
  depth: number,
  ctx: TreeRowContext,
): ReactNode[] {
  return nodes.flatMap((node) => {
    const isFolder = node.kind === "folder";
    const open = isFolder && !ctx.collapsed.has(node.key);
    // CHG-FR-AYTR / CHG-FR-28: an implied pointer row, and a folder whose
    // visible rows are all implied, are checked and inert.
    const check = nodeCheckOf(node, ctx.checked, ctx.impliedPointers);
    const rows: ReactNode[] = [
      <ChangeRow
        key={node.key}
        node={node}
        depth={depth}
        open={open}
        selected={ctx.selectedKey === node.key}
        checkState={ctx.showChecks ? check.state : null}
        checkImplied={check.inert}
        onCheck={ctx.onCheck}
        onToggle={ctx.onToggle}
        onOpen={ctx.onOpen}
      />,
    ];
    if (isFolder && open) rows.push(...renderNodes(node.children, depth + 1, ctx));
    return rows;
  });
}

/** CHG-FR-09: **Revisioned** carries the commit glyph, **Unrevisioned** the new-file one. */
function sectionIcon(section: SectionModel): ReactNode {
  return section.kind === "revisioned" ? (
    <Icon.Commit size={13} />
  ) : (
    <Icon.Diamond size={13} />
  );
}

/**
 * CHG-FR-09: the two groups of one repository, **Revisioned** first. A group
 * with no visible file does not render (CHG-FR-16).
 */
function Sections({
  model,
  depth,
  repositoryLabel,
  ctx,
}: {
  model: RepositoryGroupModel;
  depth: number;
  /** Set inside a repository group, so the two checkboxes of each repository have their own names. */
  repositoryLabel?: string;
  ctx: TreeRowContext;
}) {
  return (
    <>
      {model.sections.map((section) => {
        const { node } = section;
        if (node.children.length === 0) return null;
        const open = !ctx.collapsed.has(node.key);
        return (
          <Fragment key={node.key}>
            <GroupRow
              node={node}
              depth={depth}
              icon={sectionIcon(section)}
              className="tree-row--group"
              checkLabel={
                repositoryLabel
                  ? `Include ${node.name} in ${repositoryLabel}`
                  : `Include ${node.name}`
              }
              ctx={ctx}
            />
            {open && renderNodes(node.children, depth + 1, ctx)}
          </Fragment>
        );
      })}
    </>
  );
}

/**
 * CHG-FR-BYWJ / CHG-FR-IGSD: the whole tree body. With one repository the
 * groups of CHG-FR-09 are the top level; with more, one repository group for
 * each repository with a visible changed file holds them.
 */
export function ChangeTreeBody({
  models,
  multi,
  ctx,
}: {
  models: RepositoryGroupModel[];
  multi: boolean;
  ctx: TreeRowContext;
}) {
  if (!multi) {
    const only = models[0];
    return only ? <Sections model={only} depth={0} ctx={ctx} /> : null;
  }
  return (
    <>
      {models.map((model) => {
        const open = !ctx.collapsed.has(model.node.key);
        return (
          <Fragment key={model.node.key}>
            {/* CHG-FR-UMCU: the name or the path, then the branch or
                `detached` and the short id, then the count. */}
            <GroupRow
              node={model.node}
              depth={0}
              icon={<Icon.Branch size={13} />}
              className="tree-row--repository"
              checkLabel={`Include repository ${model.node.name}`}
              title={`${model.node.name} · ${repositoryBranchLabel(model.info)}`}
              meta={
                <span className="tree-row__branch" data-testid="repository-branch">
                  {repositoryBranchLabel(model.info)}
                </span>
              }
              ctx={ctx}
            />
            {open && (
              <Sections
                model={model}
                depth={1}
                repositoryLabel={model.node.name}
                ctx={ctx}
              />
            )}
          </Fragment>
        );
      })}
    </>
  );
}
