/**
 * The HTML comments of a Markdown file on the WYSIWYG surface (EDT-FR-68):
 * vertical breaks (EDT-FR-QALB), alignment markers (EDT-FR-XPMC), and every
 * other comment as a chip (EDT-FR-UYPS).
 *
 * The surface parses Markdown with `html: false`, so a sequence that looks like
 * a tag stays prose (EDT-FR-67). The rules here read the comments alone: a
 * block rule for a comment on lines of its own, and an inline rule for one
 * inside a paragraph. Their tokens render to inert elements that carry the raw
 * comment in a data attribute, never as markup, and the schema reads them back
 * from those attributes (EDT-FR-KMAR).
 *
 * Each node writes back the raw spelling it was read with. A marker that the
 * toolbar sets or changes is written in canonical form (EDT-FR-OZPO). A break
 * and an alignment marker end with one newline and close no block, so the
 * block under them stays on the very next line.
 */
import { Extension, Node } from "@tiptap/react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { NodeSelection, Plugin, PluginKey } from "@tiptap/pm/state";
import type { EditorState, Transaction } from "@tiptap/pm/state";
import type { EditorView } from "@tiptap/pm/view";
import type { Node as PMNode } from "@tiptap/pm/model";
import {
  BREAK_LINE_HEIGHTS,
  breakLabel,
  canonicalAlign,
  canonicalBreak,
  classifyMarkerLine,
  commentText,
  COMMENT_CHIP_LABEL,
  findCommentClose,
  startsComment,
  type BlockAlignment,
  type BreakLevel,
  type MarkerAlignment,
} from "../markdown/markers";

/** EDT-FR-GVJM: the top-level block types an alignment marker can align. */
export const ALIGNABLE_TYPES = [
  "paragraph",
  "heading",
  "bulletList",
  "orderedList",
  "taskList",
  "blockquote",
  "codeBlock",
  "table",
  "horizontalRule",
] as const;

/** The shape of the `prosemirror-markdown` serializer state these nodes use. */
type SerializerState = {
  write: (content?: string) => void;
  flushClose: (size?: number) => void;
  inTightList?: boolean;
  /** Set by the table serializer of `tiptap-markdown` while it writes a row. */
  inTable?: boolean;
  text: (value: string, escape?: boolean) => void;
  ensureNewLine: () => void;
  closeBlock: (node: PMNode) => void;
};
type NodeSerializer = (state: SerializerState, node: PMNode, parent: PMNode, index: number) => void;

type RenderRule = (tokens: Array<{ meta: unknown }>, idx: number) => string;
type CoreToken = { type: string; map: [number, number] | null; level: number };

/** The part of a markdown-it instance the rules use; it is not a direct dependency. */
type MarkdownIt = {
  utils: { escapeHtml: (value: string) => string };
  block: { ruler: { before: (at: string, name: string, rule: unknown, options?: { alt: string[] }) => void } };
  core: { ruler: { push: (name: string, rule: (state: { tokens: CoreToken[] }) => void) => void } };
  inline: { ruler: { before: (at: string, name: string, rule: unknown) => void } };
  renderer: { rules: Record<string, RenderRule> };
};

// ---------------------------------------------------------------------------
// markdown-it rules
// ---------------------------------------------------------------------------

type BlockState = {
  src: string;
  bMarks: number[];
  eMarks: number[];
  tShift: number[];
  sCount: number[];
  blkIndent: number;
  level: number;
  line: number;
  isEmpty: (line: number) => boolean;
  push: (type: string, tag: string, nesting: number) => { meta: unknown; map: [number, number] | null; block: boolean };
};
type InlineState = {
  src: string;
  pos: number;
  posMax: number;
  push: (type: string, tag: string, nesting: number) => { meta: unknown };
};

/** The text of `line` inside its container, with the container indent removed. */
function lineText(state: BlockState, line: number): string {
  const from = state.bMarks[line] + Math.min(state.tShift[line], state.blkIndent);
  return state.src.slice(from, state.eMarks[line]);
}

/**
 * EDT-FR-UYPS / EDT-FR-QALB / EDT-FR-XPMC / EDT-FR-ANTD: an HTML comment on
 * lines of its own.
 */
function commentBlockRule(state: BlockState, startLine: number, endLine: number, silent: boolean): boolean {
  if (state.sCount[startLine] - state.blkIndent >= 4) return false;
  const pos = state.bMarks[startLine] + state.tShift[startLine];
  if (!state.src.startsWith("<!--", pos)) return false;

  // The first line keeps its indent, so a marker keeps its spelling (EDT-FR-OZPO).
  const open = pos - (state.bMarks[startLine] + Math.min(state.tShift[startLine], state.blkIndent));
  const lines: string[] = [];
  let line = startLine;
  for (; line < endLine; line++) {
    const text = lineText(state, line);
    const close = findCommentClose(text, line === startLine ? open + 4 : 0);
    if (!close) {
      lines.push(text);
      continue;
    }
    // Text after the close makes the comment inline text of a paragraph.
    if (text.slice(close.end).trim() !== "") return false;
    lines.push(text.slice(0, close.end));
    break;
  }
  if (line >= endLine) return false;
  if (silent) return true;

  const raw = lines.join("\n");
  const topLevel = state.level === 0;
  const marker = topLevel && line === startLine ? classifyMarkerLine(raw) : null;
  state.line = line + 1;

  if (marker?.kind === "break") {
    const token = state.push("md_break", "div", 0);
    token.meta = { level: marker.level, raw };
    token.map = [startLine, state.line];
    return true;
  }
  if (marker?.kind === "align") {
    // EDT-FR-ANTD: the marker aligns the block on the very next line only.
    const next = state.line;
    const aligns = next < endLine && !state.isEmpty(next) && !startsComment(lineText(state, next));
    if (aligns) {
      const token = state.push("md_align", "div", 0);
      token.meta = { value: marker.value, raw };
      token.map = [startLine, state.line];
      return true;
    }
  }
  const token = state.push("md_comment", "div", 0);
  token.meta = { raw };
  token.map = [startLine, state.line];
  return true;
}

/** EDT-FR-UYPS: an HTML comment inside a paragraph. */
function commentInlineRule(state: InlineState, silent: boolean): boolean {
  if (!state.src.startsWith("<!--", state.pos)) return false;
  const close = findCommentClose(state.src, state.pos + 4);
  if (!close || close.end > state.posMax) return false;
  if (!silent) {
    const token = state.push("md_comment_inline", "span", 0);
    token.meta = { raw: state.src.slice(state.pos, close.end) };
  }
  state.pos = close.end;
  return true;
}

const installed = new WeakSet<object>();

/**
 * The rules, installed once per markdown-it instance: `tiptap-markdown` runs
 * every `parse.setup` again on each parse.
 */
export function markupPlugin(md: MarkdownIt): void {
  if (installed.has(md)) return;
  installed.add(md);
  const escape = md.utils.escapeHtml;
  md.block.ruler.before("html_block", "md_comment_block", commentBlockRule, {
    alt: ["paragraph", "reference", "blockquote"],
  });
  md.inline.ruler.before("html_inline", "md_comment_inline", commentInlineRule);
  // EDT-FR-ANTD: a marker aligns a block only when that block starts on the
  // very next line. A line that makes no block of its own, such as a link
  // reference definition, leaves the marker an ordinary comment.
  md.core.ruler.push("md_align_target", (state) => {
    const tokens = state.tokens;
    tokens.forEach((token, index) => {
      if (token.type !== "md_align") return;
      const next = tokens[index + 1];
      if (!next?.map || !token.map || next.level !== 0 || next.map[0] !== token.map[1]) {
        token.type = "md_comment";
      }
    });
  });
  const meta = (tokens: Array<{ meta: unknown }>, idx: number) =>
    tokens[idx].meta as { raw: string; level?: number; value?: string };
  md.renderer.rules.md_break = (tokens, idx) => {
    const { raw, level } = meta(tokens, idx);
    return `<div data-md-break="" data-level="${level}" data-raw="${escape(raw)}"></div>`;
  };
  md.renderer.rules.md_align = (tokens, idx) => {
    const { raw, value } = meta(tokens, idx);
    return `<div data-md-align="${value}" data-raw="${escape(raw)}"></div>`;
  };
  md.renderer.rules.md_comment = (tokens, idx) =>
    `<div data-md-comment="" data-raw="${escape(meta(tokens, idx).raw)}"></div>`;
  md.renderer.rules.md_comment_inline = (tokens, idx) =>
    `<span data-md-comment-inline="" data-raw="${escape(meta(tokens, idx).raw)}"></span>`;
}

/**
 * EDT-FR-XPMC: move each alignment marker onto the block under it, and drop the
 * marker element. The rule emits a marker only with a block on the next line.
 */
function attachAlignment(element: HTMLElement): void {
  element.querySelectorAll("div[data-md-align]").forEach((marker) => {
    const block = marker.nextElementSibling;
    if (!block) {
      // Never drop the comment: with no block to align it is a chip.
      const chip = element.ownerDocument.createElement("div");
      chip.setAttribute("data-md-comment", "");
      chip.setAttribute("data-raw", marker.getAttribute("data-raw") ?? "");
      marker.replaceWith(chip);
      return;
    }
    block.setAttribute("data-align", marker.getAttribute("data-md-align") ?? "");
    block.setAttribute("data-align-raw", marker.getAttribute("data-raw") ?? "");
    marker.remove();
  });
}

// ---------------------------------------------------------------------------
// Nodes
// ---------------------------------------------------------------------------

function asLevel(value: unknown): BreakLevel {
  const n = Number(value);
  return (n >= 1 && n <= 4 ? Math.round(n) : 4) as BreakLevel;
}

/** The comment glyph and the raw text a chip holds, as DOM spec children. */
function chipContent(raw: string) {
  // The raw text is in the DOM but visually hidden, so a selection over the
  // chip copies, and maps onto, the source it stands for (CMT-FR-WJED). The
  // glyph is CSS content, which no selection holds.
  return ["span", { class: "md-chip__raw" }, raw] as const;
}

/** EDT-FR-QALB / EDT-FR-CSKH / EDT-FR-XKTS: a vertical break as a spacer. */
export const VerticalBreak = Node.create({
  name: "verticalBreak",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() {
    return {
      level: {
        default: 2,
        parseHTML: (element: HTMLElement) => asLevel(element.getAttribute("data-level")),
        renderHTML: () => ({}),
      },
      raw: {
        default: null,
        parseHTML: (element: HTMLElement) => element.getAttribute("data-raw"),
        renderHTML: () => ({}),
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-md-break]" }];
  },
  renderHTML({ node }) {
    const level = asLevel(node.attrs.level);
    return [
      "div",
      {
        class: "md-break",
        "data-md-break": "",
        "data-level": String(level),
        role: "separator",
        "aria-label": breakLabel(level),
        contenteditable: "false",
        style: `--md-break-lines: ${BREAK_LINE_HEIGHTS[level]}`,
      },
    ];
  },
  addStorage() {
    return {
      markdown: {
        serialize(state: SerializerState, node: PMNode) {
          state.write(node.attrs.raw ?? canonicalBreak(asLevel(node.attrs.level)));
          state.ensureNewLine();
        },
        parse: {},
      },
    };
  },
});

/** EDT-FR-UYPS / EDT-FR-FXAG: an HTML comment on lines of its own, as a chip. */
export const HtmlComment = Node.create({
  name: "htmlComment",
  group: "block",
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() {
    return {
      raw: {
        default: "<!-- -->",
        parseHTML: (element: HTMLElement) => element.getAttribute("data-raw") ?? "<!-- -->",
        renderHTML: () => ({}),
      },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-md-comment]" }];
  },
  renderHTML({ node }) {
    return [
      "div",
      { class: "md-chip-line", "data-md-comment": "", contenteditable: "false" },
      [
        "span",
        { class: "md-chip", "data-md-chip": "", role: "img", "aria-label": COMMENT_CHIP_LABEL },
        chipContent(node.attrs.raw),
      ],
    ];
  },
  addStorage() {
    return {
      markdown: {
        serialize(state: SerializerState, node: PMNode) {
          // In a tight list a blank line would make the list loose, so the
          // comment stands on the lines of the item with no gap.
          if (state.inTightList) {
            state.flushClose(1);
            state.text(node.attrs.raw, false);
            state.ensureNewLine();
            return;
          }
          state.text(node.attrs.raw, false);
          state.closeBlock(node);
        },
        parse: { setup: markupPlugin },
      },
    };
  },
});

/** EDT-FR-UYPS: an HTML comment inside a paragraph, as an inline chip. */
export const HtmlCommentInline = Node.create({
  name: "htmlCommentInline",
  group: "inline",
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,
  addAttributes() {
    return {
      raw: {
        default: "<!-- -->",
        parseHTML: (element: HTMLElement) => element.getAttribute("data-raw") ?? "<!-- -->",
        renderHTML: () => ({}),
      },
    };
  },
  parseHTML() {
    return [{ tag: "span[data-md-comment-inline]" }];
  },
  renderHTML({ node }) {
    return [
      "span",
      {
        class: "md-chip",
        "data-md-chip": "",
        "data-md-comment-inline": "",
        role: "img",
        "aria-label": COMMENT_CHIP_LABEL,
        contenteditable: "false",
      },
      chipContent(node.attrs.raw),
    ];
  },
  addStorage() {
    return {
      markdown: {
        serialize(state: SerializerState, node: PMNode) {
          // In a table cell a bare `|` ends the cell, so it is escaped again
          // as the table rule read it (EDT-FR-68).
          const raw = node.attrs.raw as string;
          state.text(state.inTable ? raw.replace(/\\?\|/g, "\\|") : raw, false);
        },
        parse: {},
      },
    };
  },
});

function asAlignment(value: unknown): BlockAlignment {
  return value === "center" || value === "right" ? value : null;
}

/**
 * EDT-FR-XPMC / EDT-FR-GVJM / EDT-FR-OZPO: the alignment of a top-level block,
 * read from the marker above it and written back as that marker.
 */
export const BlockAlignmentMarkers = Extension.create({
  name: "markdownBlockAlignment",
  // After `tiptap-markdown` (priority 50), whose serializer it wraps below.
  priority: 40,
  addGlobalAttributes() {
    return [
      {
        types: [...ALIGNABLE_TYPES],
        attributes: {
          align: {
            default: null,
            // A marker aligns one block, so Enter starts a left-aligned one.
            keepOnSplit: false,
            parseHTML: (element: HTMLElement) => asAlignment(element.getAttribute("data-align")),
            renderHTML: (attributes: { align?: unknown }) => {
              const align = asAlignment(attributes.align);
              return align ? { "data-align": align } : {};
            },
          },
          alignRaw: {
            default: null,
            keepOnSplit: false,
            parseHTML: (element: HTMLElement) => element.getAttribute("data-align-raw"),
            renderHTML: () => ({}),
          },
        },
      },
    ];
  },
  addStorage() {
    return {
      markdown: {
        parse: { setup: markupPlugin, updateDOM: attachAlignment },
      },
    };
  },
  onBeforeCreate() {
    // The node serializers of `tiptap-markdown` are built on each serialise
    // from a getter. Each alignable type is wrapped so that it writes its
    // marker on the line directly above itself.
    const serializer = (this.editor.storage as { markdown?: { serializer?: object } }).markdown
      ?.serializer;
    if (!serializer) return;
    const getter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(serializer), "nodes")?.get;
    if (!getter) return;
    Object.defineProperty(serializer, "nodes", {
      configurable: true,
      get() {
        const nodes = getter.call(this) as Record<string, NodeSerializer>;
        for (const type of ALIGNABLE_TYPES) {
          const inner = nodes[type];
          if (!inner) continue;
          nodes[type] = (state, node, parent, index) => {
            const align = asAlignment(node.attrs.align);
            if (align) {
              state.write((node.attrs.alignRaw as string | null) ?? canonicalAlign(align));
              state.ensureNewLine();
            }
            inner(state, node, parent, index);
          };
        }
        return nodes;
      },
    });
  },
});

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/** The positions of the top-level blocks that the selection touches. */
function selectedTopLevelBlocks(state: EditorState): Array<{ pos: number; node: PMNode }> {
  const { selection, doc } = state;
  const out: Array<{ pos: number; node: PMNode }> = [];
  if (selection instanceof NodeSelection && selection.$from.depth === 0) {
    out.push({ pos: selection.from, node: selection.node });
    return out;
  }
  const { $from, $to } = selection;
  const first = $from.index(0);
  const last = $to.depth === 0 ? Math.max(first, $to.index(0) - 1) : $to.index(0);
  let pos = 0;
  for (let i = 0; i < doc.childCount; i++) {
    const node = doc.child(i);
    if (i >= first && i <= last) out.push({ pos, node });
    pos += node.nodeSize;
  }
  return out;
}

/** EDT-FR-GELS: the level of the selected spacer, or null. */
export function selectedBreakLevel(state: EditorState): BreakLevel | null {
  const { selection } = state;
  if (selection instanceof NodeSelection && selection.node.type.name === "verticalBreak") {
    return asLevel(selection.node.attrs.level);
  }
  return null;
}

/** EDT-FR-GELS: the alignment of the top-level block that holds the caret. */
export function caretAlignment(state: EditorState): BlockAlignment {
  const [block] = selectedTopLevelBlocks(state);
  return block ? asAlignment(block.node.attrs.align) : null;
}

function dispatch(editor: TiptapEditor, tr: Transaction): void {
  editor.view.dispatch(tr.scrollIntoView());
  editor.view.focus();
}

/**
 * EDT-FR-UXCK: a Vx choice. It resizes a selected spacer, or inserts a break
 * above the first top-level block of the selection. A break goes above the
 * alignment marker of its block, because the marker is part of the block.
 * Returns false when the surface accepts no input or there is no block.
 */
export function setVerticalBreak(editor: TiptapEditor, level: BreakLevel): boolean {
  if (!editor.isEditable) return false;
  const { state } = editor;
  const type = state.schema.nodes.verticalBreak;
  if (!type) return false;
  const { selection } = state;
  if (selection instanceof NodeSelection && selection.node.type === type) {
    const tr = state.tr.setNodeMarkup(selection.from, undefined, { level, raw: null });
    tr.setSelection(NodeSelection.create(tr.doc, selection.from));
    dispatch(editor, tr);
    return true;
  }
  const [first] = selectedTopLevelBlocks(state);
  if (!first) return false;
  dispatch(editor, state.tr.insert(first.pos, type.create({ level, raw: null })));
  return true;
}

/**
 * EDT-FR-MAVJ: an Ax choice. It sets the alignment of each top-level block the
 * selection touches; `null` (Left) removes the marker. Returns false when the
 * surface accepts no input or nothing changed.
 */
export function setBlockAlignment(editor: TiptapEditor, align: MarkerAlignment | null): boolean {
  if (!editor.isEditable) return false;
  const { state } = editor;
  const tr = state.tr;
  for (const { pos, node } of selectedTopLevelBlocks(state)) {
    if (!(ALIGNABLE_TYPES as readonly string[]).includes(node.type.name)) continue;
    if (asAlignment(node.attrs.align) === align && node.attrs.alignRaw == null) continue;
    tr.setNodeMarkup(pos, undefined, { ...node.attrs, align, alignRaw: null });
  }
  if (!tr.docChanged) return false;
  dispatch(editor, tr);
  return true;
}

// ---------------------------------------------------------------------------
// Chip tooltip
// ---------------------------------------------------------------------------

const tooltipKey = new PluginKey("markdownCommentTooltip");

/** The chip element under `target`, when it is a chip of this surface. */
function chipAt(target: EventTarget | null): HTMLElement | null {
  return target instanceof Element ? (target.closest("[data-md-chip]") as HTMLElement | null) : null;
}

function rawOfChip(chip: HTMLElement): string {
  return chip.querySelector(".md-chip__raw")?.textContent ?? "";
}

/**
 * EDT-FR-IGZL: one tooltip per surface. It shows the comment text as plain
 * text, under the chip under the pointer or the selected chip. It takes no
 * focus and no pointer event.
 */
class ChipTooltipView {
  private readonly bubble: HTMLDivElement;
  private hovered: HTMLElement | null = null;
  private dismissed: HTMLElement | null = null;

  constructor(private readonly view: EditorView) {
    this.bubble = document.createElement("div");
    this.bubble.className = "md-chip-tooltip";
    this.bubble.setAttribute("role", "tooltip");
    this.bubble.hidden = true;
    document.body.appendChild(this.bubble);
    this.update();
  }

  /** The chip of the selected comment node, if one is selected. */
  private selectedChip(): HTMLElement | null {
    const { selection } = this.view.state;
    if (!(selection instanceof NodeSelection)) return null;
    const name = selection.node.type.name;
    if (name !== "htmlComment" && name !== "htmlCommentInline") return null;
    const dom = this.view.nodeDOM(selection.from);
    if (!(dom instanceof HTMLElement)) return null;
    return dom.matches("[data-md-chip]") ? dom : dom.querySelector<HTMLElement>("[data-md-chip]");
  }

  /** A change of focus changes whether a selected chip counts as focused. */
  focusChanged(): void {
    this.update();
  }

  hover(chip: HTMLElement | null): void {
    this.hovered = chip;
    this.update();
  }

  dismiss(): boolean {
    if (this.bubble.hidden) return false;
    this.dismissed = this.hovered ?? this.selectedChip();
    this.update();
    return true;
  }

  update(): void {
    // A selected chip has the keyboard focus only while its surface has it. An
    // editable run of a Diff tab that holds only a chip starts with it selected.
    const selected = this.view.hasFocus() ? this.selectedChip() : null;
    if (this.dismissed && this.dismissed !== (this.hovered ?? selected)) this.dismissed = null;
    const chip = this.hovered ?? selected;
    if (!chip || chip === this.dismissed || !this.view.dom.contains(chip)) {
      this.bubble.hidden = true;
      return;
    }
    this.bubble.textContent = commentText(rawOfChip(chip));
    const rect = chip.getBoundingClientRect();
    this.bubble.style.left = `${Math.round(rect.left)}px`;
    this.bubble.style.top = `${Math.round(rect.bottom + 6)}px`;
    this.bubble.hidden = false;
  }

  destroy(): void {
    this.bubble.remove();
  }
}

/** EDT-FR-IGZL: the chip tooltip of the WYSIWYG surface. */
export const CommentChipTooltip = Extension.create({
  name: "markdownCommentTooltip",
  addProseMirrorPlugins() {
    let tooltip: ChipTooltipView | null = null;
    return [
      new Plugin({
        key: tooltipKey,
        view(view) {
          tooltip = new ChipTooltipView(view);
          return {
            update: () => tooltip?.update(),
            destroy: () => {
              tooltip?.destroy();
              tooltip = null;
            },
          };
        },
        props: {
          handleDOMEvents: {
            mouseover: (_view, event) => {
              tooltip?.hover(chipAt(event.target));
              return false;
            },
            mouseleave: () => {
              tooltip?.hover(null);
              return false;
            },
            focus: () => {
              queueMicrotask(() => tooltip?.focusChanged());
              return false;
            },
            blur: () => {
              queueMicrotask(() => tooltip?.focusChanged());
              return false;
            },
          },
          // Escape that closes the tooltip does nothing else.
          handleKeyDown: (_view, event) => event.key === "Escape" && (tooltip?.dismiss() ?? false),
        },
      }),
    ];
  },
});

/** The extensions that model HTML comments, in the order the surface needs. */
export const markupExtensions = [VerticalBreak, HtmlComment, HtmlCommentInline, BlockAlignmentMarkers, CommentChipTooltip];
