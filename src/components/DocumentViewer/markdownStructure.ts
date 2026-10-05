/**
 * The structure of a Markdown document for the rich view of a Document tab
 * (DTV-FR-SSQI).
 *
 * The shared parser returns one block for each list item and each table row.
 * This module puts consecutive items into one list and consecutive rows into
 * one table, and it splits off a leading YAML frontmatter region. The result is
 * plain data. The renderer turns it into React elements.
 */
import { parseMarkdownBlocks } from "../../diff/markdown";
import type { MarkdownBlock } from "../../diff/markdown";

export interface ListItemNode {
  text: string;
  children: ListNode[];
}

export interface ListNode {
  ordered: boolean;
  /** The number of the first item of an ordered list. */
  start: number;
  items: ListItemNode[];
}

export type DocNode =
  | { kind: "frontmatter"; text: string }
  | { kind: "block"; block: MarkdownBlock }
  | { kind: "list"; list: ListNode }
  | { kind: "table"; header: string[] | null; rows: string[][] };

/** A leading region between two `---` lines at the very start of the text. */
const FRONTMATTER = /^---[ \t]*\n([\s\S]*?)\n---[ \t]*(?:\n|$)/;

function startOf(marker: string | undefined): number {
  const parsed = Number.parseInt(marker ?? "", 10);
  return Number.isFinite(parsed) ? parsed : 1;
}

function newList(block: MarkdownBlock): ListNode {
  const ordered = block.ordered === true;
  return { ordered, start: ordered ? startOf(block.marker) : 1, items: [] };
}

/** Build the lists of a run of list-item blocks. Depth becomes nesting. */
function buildLists(blocks: MarkdownBlock[]): ListNode[] {
  const roots: ListNode[] = [];
  const stack: { depth: number; list: ListNode }[] = [];
  for (const block of blocks) {
    const depth = block.depth ?? 0;
    while (stack.length > 1 && stack[stack.length - 1].depth > depth) {
      stack.pop();
    }
    let top = stack[stack.length - 1];
    const ordered = block.ordered === true;
    if (top && top.depth === depth && top.list.ordered !== ordered) {
      stack.pop();
      top = stack[stack.length - 1];
      const list = newList(block);
      attach(roots, top, list);
      stack.push({ depth, list });
      top = stack[stack.length - 1];
    } else if (!top || top.depth < depth) {
      const list = newList(block);
      attach(roots, top, list);
      stack.push({ depth, list });
      top = stack[stack.length - 1];
    }
    top.list.items.push({ text: block.text, children: [] });
  }
  return roots;
}

/** Put a new list under the last item of its parent, or at the root. */
function attach(
  roots: ListNode[],
  parent: { depth: number; list: ListNode } | undefined,
  list: ListNode,
): void {
  const owner = parent?.list.items[parent.list.items.length - 1];
  if (owner) owner.children.push(list);
  else roots.push(list);
}

function buildTable(rows: MarkdownBlock[]): DocNode {
  const dividerAt = rows.findIndex((row) => row.isDivider === true);
  if (dividerAt === 1) {
    return {
      kind: "table",
      header: rows[0].cells ?? [],
      rows: rows.slice(2).filter((r) => !r.isDivider).map((r) => r.cells ?? []),
    };
  }
  return {
    kind: "table",
    header: null,
    rows: rows.filter((r) => !r.isDivider).map((r) => r.cells ?? []),
  };
}

/** Turn the text of a Markdown document into the nodes the view renders. */
export function buildDocument(text: string): DocNode[] {
  let body = text.replace(/\r\n?/g, "\n");
  const nodes: DocNode[] = [];
  const front = FRONTMATTER.exec(body);
  if (front) {
    nodes.push({ kind: "frontmatter", text: front[0].replace(/\n$/, "") });
    body = body.slice(front[0].length);
  }
  const blocks = parseMarkdownBlocks(body);
  let i = 0;
  while (i < blocks.length) {
    const block = blocks[i];
    if (block.kind === "list-item") {
      const run: MarkdownBlock[] = [];
      while (i < blocks.length && blocks[i].kind === "list-item") {
        run.push(blocks[i++]);
      }
      for (const list of buildLists(run)) nodes.push({ kind: "list", list });
    } else if (block.kind === "table-row") {
      const run: MarkdownBlock[] = [];
      while (i < blocks.length && blocks[i].kind === "table-row") {
        run.push(blocks[i++]);
      }
      nodes.push(buildTable(run));
    } else {
      nodes.push({ kind: "block", block });
      i++;
    }
  }
  return nodes;
}
