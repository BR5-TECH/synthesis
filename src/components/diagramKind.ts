/**
 * `EDT-editor.md` EDT-FR-MOYS, EDT-FR-TQWB: the two kinds of diagram fence. A
 * Mermaid fence has the language `mermaid`, and a CALM fence has the language
 * `calm` (`CAD-calm-architecture-diagrams.md` CAD-FR-PFNM). Both languages
 * are compared in the same way.
 */
import type { Node as PMNode } from "@tiptap/pm/model";

export type DiagramKind = "mermaid" | "calm";

/** EDT-FR-MOYS: the language of a Mermaid fence. */
export const MERMAID_LANGUAGE = "mermaid";
/** CAD-FR-PFNM: the language of a CALM fence. */
export const CALM_LANGUAGE = "calm";

/** The kind of diagram that a fence language names, or null. */
export function diagramKindOfLanguage(language: unknown): DiagramKind | null {
  if (typeof language !== "string") return null;
  const trimmed = language.trim();
  if (trimmed === MERMAID_LANGUAGE) return "mermaid";
  if (trimmed === CALM_LANGUAGE) return "calm";
  return null;
}

/** The kind of diagram fence that `node` is, or null when it is not one. */
export function diagramKindOf(node: PMNode): DiagramKind | null {
  if (node.type.name !== "codeBlock") return null;
  return diagramKindOfLanguage(node.attrs.language);
}

/** The kind of the diagram fence that starts at `pos` of `doc`, or null. */
export function diagramKindAt(doc: PMNode, pos: number): DiagramKind | null {
  const node = pos >= 0 && pos <= doc.content.size ? doc.nodeAt(pos) : null;
  return node ? diagramKindOf(node) : null;
}
