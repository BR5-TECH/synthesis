/**
 * The Markdown the WYSIWYG surface parses on the way in and serialises on the
 * way out (EDT-FR-66–EDT-FR-69).
 *
 * The rich surface is a round trip: Markdown becomes a ProseMirror document and
 * a document becomes Markdown again. Anything the schema cannot model is lost in
 * the middle of that trip, and anything the serialiser escapes differently from
 * how the author wrote it comes back altered — so which extensions the editor
 * carries, and how text is written back out, are the two things that decide
 * whether a file survives being opened (EDT-FR-67).
 *
 * Two departures from the defaults carry that guarantee:
 *
 * - **Text is written without HTML escaping.** `tiptap-markdown` rewrites every
 *   `<` and `>` in a text node as `&lt;`/`&gt;`, which is right for Markdown
 *   destined for an HTML renderer and wrong for a Markdown *file*: it turns the
 *   prose `5 < 6` into `5 &lt; 6` and the placeholder `<discussion_history>`
 *   into an entity soup. `LiteralText` overrides that one method and leaves the
 *   ordinary Markdown escaping — the part that keeps a literal `*` from becoming
 *   emphasis — exactly as it was.
 * - **Raw HTML is not parsed** (`html: false`). With HTML parsing on, a sequence
 *   that merely looks like a tag is read as markup the schema has no node for
 *   and is dropped outright: `<artifact>` disappears from the file. Off, it is
 *   the prose the author wrote, and it round-trips as itself — as do HTML
 *   comments, which survive as the inert literal text EDT-FR-68 requires rather
 *   than being deleted.
 *
 * The rest is coverage: every construct this project's files are written in has
 * a node in the schema, so round-tripping one is a question of spelling rather
 * than of survival (EDT-FR-68).
 */
import { Extension, Node } from "@tiptap/react";
import type { Extensions, Mark } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Image } from "@tiptap/extension-image";
import { Table, TableCell, TableHeader, TableRow } from "@tiptap/extension-table";
import { TaskItem, TaskList } from "@tiptap/extension-list";
import { Markdown } from "tiptap-markdown";
import { LiteralBacktick } from "./markdownBacktick";

/** The shape `tiptap-markdown` calls a node's serializer with. */
type SerializerState = { text: (value: string, escape?: boolean) => void };
type TextNode = { text?: string };

/**
 * The `text` node, serialised as the characters the author actually wrote
 * (EDT-FR-67).
 *
 * This replaces StarterKit's Text — which is why the kit is configured with
 * `text: false` below. The node definition is Text's own (an inline leaf); only
 * the Markdown serializer differs, and `getMarkdownSpec` prefers an extension's
 * own `storage.markdown` over the library's default, so this wins.
 *
 * `state.text(value)` keeps `escape` at its default, so Markdown syntax the
 * author meant literally is still escaped. What is dropped is the extra pass
 * that rewrote `<` and `>` as HTML entities.
 */
export const LiteralText = Node.create({
  name: "text",
  group: "inline",
  addStorage() {
    return {
      markdown: {
        serialize(state: SerializerState, node: TextNode) {
          state.text(node.text ?? "");
        },
        parse: {},
      },
    };
  },
});

/**
 * A task list is a **tight** list, like the bullet list it is written as
 * (EDT-FR-68).
 *
 * `tiptap-markdown` carries a tight-list attribute but applies it to
 * `bulletList` and `orderedList` alone, so a task list serialises loose — a
 * blank line between every item, which is a different construct from the one
 * the author wrote. This supplies the same attribute for `taskList`, on the
 * same terms: a list whose items hold no paragraph is tight.
 */
export const TightTaskList = Extension.create({
  name: "markdownTightTaskList",
  addGlobalAttributes() {
    return [
      {
        types: ["taskList"],
        attributes: {
          tight: {
            default: true,
            parseHTML: (element: HTMLElement) =>
              element.getAttribute("data-tight") === "true" ||
              !element.querySelector("p"),
            renderHTML: (attributes: { tight?: boolean }) => ({
              "data-tight": attributes.tight ? "true" : null,
            }),
          },
        },
      },
    ];
  },
});

/**
 * StarterKit with the Code mark's own exit turned off (EDT-FR-CCBX).
 *
 * Tiptap's exit moves out of inline code with the right arrow only at the end
 * of a paragraph, and inserts a space to do it. `LiteralBacktick` closes the
 * open span in place, wherever it ends, so the two must not both act on the
 * same key press.
 */
const MarkdownStarterKit = StarterKit.extend({
  addExtensions() {
    return (this.parent?.() ?? []).map((extension) =>
      extension.name === "code"
        ? (extension as Mark).extend({ exitable: false })
        : extension,
    );
  },
});

/**
 * EDT-FR-18: a leading YAML frontmatter block (`---` fences), matched verbatim
 * so that an untouched one round-trips byte-for-byte.
 *
 * Shared rather than kept private to the Editor, because the frontmatter region
 * is the one part of a Markdown file the WYSIWYG surface deliberately does not
 * hold: it is split off before the body is parsed, and anything else that mounts
 * that surface over a file's lines has to make the same split or the round trip
 * writes the fences back as a horizontal rule with a paragraph between them
 * (`DFV-diff-viewer.md` DFV-FR-47).
 */
export const FRONTMATTER_RE = /^---[ \t]*\r?\n([\s\S]*?)\r?\n---[ \t]*(?:\r?\n|$)/;

/** How many lines the leading frontmatter block occupies, zero if there is none. */
export function frontmatterLineCount(text: string): number {
  const match = FRONTMATTER_RE.exec(text);
  if (!match) return 0;
  const raw = match[0];
  const lines = raw.split("\n").length;
  return raw.endsWith("\n") ? lines - 1 : lines;
}

/**
 * The Markdown a Tiptap surface currently holds, as the fidelity extension
 * serialises it (EDT-FR-67, EDT-FR-69).
 *
 * Shared rather than reached for through the Editor, because the editable rich
 * target of a Diff tab and of a draft-change review write back through exactly
 * this serialisation (`DFV-diff-viewer.md` DFV-FR-47) — a second reading of the
 * storage would be a second place for the round-trip guarantee to be lost.
 */
export function getMarkdown(editor: {
  storage: unknown;
}): string {
  const storage = editor.storage as {
    markdown?: { getMarkdown?: () => string };
  };
  return storage.markdown?.getMarkdown?.() ?? "";
}

/**
 * The extension set the WYSIWYG surface runs on, given the extensions the
 * Editor supplies for its own concerns (find highlighting, comment
 * highlighting, and whatever else the tab carries).
 *
 * `undoRedo` is off because undo and redo are served from the tab-wide history
 * (EDT-FR-22); two competing stacks would let ⌘Z reverse a body edit without the
 * tab knowing.
 */
export function markdownExtensions(extra: Extensions = []): Extensions {
  return [
    MarkdownStarterKit.configure({ undoRedo: false, text: false }),
    LiteralText,
    // EDT-FR-FDGH, EDT-FR-VCOH, EDT-FR-LLBU: the backtick key opens, closes and
    // wraps inline code, and the platform adds no backtick of its own.
    LiteralBacktick,
    // EDT-FR-68: the constructs this project's Markdown is written in, each
    // modelled so it renders as itself and round-trips as itself.
    // An image sits in the line of prose that carries it, so it must be an
    // inline node — a block image would be lifted into a paragraph of its own
    // and the sentence around it broken in two. `allowBase64` because an image
    // written as a `data:` URI is an image like any other: the extension's
    // default parse rule excludes `src^="data:"`, which would drop it from the
    // document and so from the bytes a save writes (EDT-FR-68).
    Image.configure({ inline: true, allowBase64: true }),
    Table,
    TableRow,
    TableHeader,
    TableCell,
    TaskList,
    TaskItem,
    TightTaskList,
    Markdown.configure({ html: false }),
    ...extra,
  ];
}
