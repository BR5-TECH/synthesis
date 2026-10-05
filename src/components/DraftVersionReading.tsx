import { useEffect, useMemo } from "react";
import { EditorContent, useEditor } from "@tiptap/react";

import { FRONTMATTER_RE, markdownExtensions } from "./markdownFidelity";
import type { EditMode } from "../state/editHistory";

/**
 * A past version of a draft's prompt, read-only, on the surface the live prompt
 * is edited on (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-09).
 *
 * A version is read **the way the prompt itself is read**: the same two modes
 * behind the same toggle, the same page set on the same field, the same
 * frontmatter region at its head, and the same typographic roles on both — so
 * the only thing that changes between reading a version and editing the prompt
 * is whether the surface takes a keystroke. A version rendered as a slab of
 * monospace would be a different document from the one it is a version of, and
 * comparing it against the live prompt would mean reading past the difference in
 * presentation before reaching the difference in text.
 *
 * Nothing here writes: the surface is not editable, it holds no session, it
 * schedules no save, and it serialises nothing back. It is the Editor's
 * rendering without the Editor's machinery, which is why it is a component of
 * its own rather than the Editor mounted over a synthetic document — a draft's
 * history has no buffer, no dirty state and no undo history to give it.
 */

/**
 * EDT-FR-18: the leading frontmatter block, split off before the body is parsed.
 *
 * The rich surface has to make the same split the Editor makes, or the fences
 * render as a horizontal rule with the YAML as a paragraph between them — which
 * is not what the author sees when they open the same text as the live prompt.
 */
export function splitReadingFrontmatter(text: string): {
  frontmatter: string | null;
  body: string;
} {
  const match = FRONTMATTER_RE.exec(text);
  if (!match) return { frontmatter: null, body: text };
  return { frontmatter: match[1], body: text.slice(match[0].length) };
}

export function DraftVersionReading({
  text,
  mode,
  label,
}: {
  /** The snapshot's text, exactly as `"load draft history entry"` returned it. */
  text: string;
  /** NAW-FR-09: which of the two modes the reading is in. */
  mode: EditMode;
  /** The accessible name, naming the version and its standing. */
  label: string;
}) {
  const { frontmatter, body } = useMemo(
    () => splitReadingFrontmatter(text),
    [text],
  );
  const rich = mode === "wysiwyg";

  const editor = useEditor(
    {
      // The Editor's own extension set, so a construct that renders as itself
      // there renders as itself here (EDT-FR-68).
      extensions: markdownExtensions(),
      editorProps: {
        attributes: {
          // `doc` is what typesets an Editor tab's WYSIWYG surface, so a version
          // reads here exactly as the prompt reads there (EDT-FR-62).
          class: "doc editor__prose",
          "data-testid": "draft-version-reading",
          // NAW-FR-09: read-only in accessible semantics rather than by colour
          // or a fill alone. `aria-readonly` is ignored on a generic element, so
          // it rides on a role assistive technology will expose it from.
          role: "document",
          "aria-readonly": "true",
          "aria-label": label,
        },
      },
      content: body,
      // NAW-FR-09: nothing in the reading accepts a keystroke. The only editable
      // text in the tab is the live prompt.
      editable: false,
    },
    // Mounted per surface rather than per version: the content is adopted below,
    // which keeps the scroll position across a re-render that changed only the
    // label.
    [rich],
  );

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    editor.commands.setContent(body, { emitUpdate: false });
  }, [editor, body]);

  if (!rich) {
    // EDT-FR-17: the raw-Markdown surface — the whole file, frontmatter
    // included, on the sheet the Editor's own source surface is set on. A
    // `<pre>` rather than a disabled textarea: there is nothing to type into,
    // and a disabled field reads as a control that has been taken away.
    return (
      <div className="editor-tab draft-reading__tab" data-frontmatter="off">
        <div className="editor__source-wrap draft-reading__page">
          <pre
            className="editor__source draft-reading__text"
            data-testid="draft-version-reading"
            role="document"
            aria-readonly="true"
            aria-label={label}
            tabIndex={0}
          >
            {text}
          </pre>
        </div>
      </div>
    );
  }

  return (
    <div
      className="editor-tab draft-reading__tab"
      // EDT-FR-63: the sheet gives up its top inset and top corners to the
      // frontmatter region, so the two read as one page rather than two boxes.
      data-frontmatter={frontmatter !== null ? "on" : "off"}
    >
      {frontmatter !== null && (
        /* EDT-FR-18: the same visually distinct region above the rich body,
           rendered rather than edited — a past version has no YAML to correct. */
        <div className="editor__frontmatter">
          <div className="editor__frontmatter-head">
            <span className="editor__frontmatter-label">frontmatter</span>
          </div>
          <pre className="draft-reading__yaml">{frontmatter}</pre>
        </div>
      )}
      <div className="editor__with-rail">
        <div className="editor" style={{ flex: 1, overflow: "auto" }}>
          <EditorContent editor={editor} />
        </div>
      </div>
    </div>
  );
}
