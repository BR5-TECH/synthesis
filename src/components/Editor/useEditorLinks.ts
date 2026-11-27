/**
 * The links of an Editor tab (`specifications/ui/EDT-editor.md` EDT-FR-QBIE,
 * EDT-FR-YXUD): where its file links go, and the jump to the heading a link
 * from another surface named.
 */
import { useEffect, useRef, type RefObject } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { logDebug, logWarn } from "../../logging";
import { showLinkTargetMissing } from "../../state/linkToasts";
import type { EditMode } from "../../state/editHistory";
import {
  placeOfFile,
  useLinkHost,
  useLinkOpener,
  type LinkHost,
  type LinkPlace,
} from "../linkFollow";
import { anchorIndex } from "../markdownLinks";
import type { DocumentOutline } from "./useDocumentOutline";

/**
 * EDT-FR-QBIE: the host of the tab's rich surface, in a ref that the surface
 * reads at each click. An Editor tab on a project file resolves against that
 * file. A host that embeds the Editor can give another `place`.
 */
export function useEditorLinkHost(
  artifactId: string,
  place: LinkPlace | null | undefined,
): RefObject<LinkHost | null> {
  const host = useLinkHost(place === undefined ? placeOfFile(artifactId) : place);
  const ref = useRef<LinkHost | null>(host);
  ref.current = host;
  return ref;
}

/**
 * EDT-FR-YXUD: when a link opened this file with a fragment, jump to its
 * heading one time, after the file loads, in the mode the tab shows.
 */
export function usePendingFragment({
  artifactId,
  loaded,
  editor,
  mode,
  outline,
  sourceText,
  buffer,
}: {
  artifactId: string;
  loaded: boolean;
  editor: TiptapEditor | null;
  mode: EditMode;
  outline: DocumentOutline;
  /** The text the raw-text surface shows. */
  sourceText: string;
  /** The text of the file, which the raw-text surface shows once it is seeded. */
  buffer: string;
}): void {
  const opener = useLinkOpener();
  useEffect(() => {
    if (!opener || !loaded) return;
    // The surface holds the file before the jump can land on it: the rich
    // surface exists, and the raw-text surface shows the text of the file.
    if (mode === "wysiwyg" ? !editor : sourceText !== buffer) return;
    const fragment = opener.takePendingFragment(artifactId);
    if (fragment === null) return;
    const entries = outline.getSnapshot();
    const entry = entries[anchorIndex(entries.map((e) => e.text), fragment)];
    if (!entry) {
      logWarn(["frontend"], "link heading not found", { rule: "EDT-FR-HYVP" });
      showLinkTargetMissing();
      return;
    }
    logDebug(["frontend"], "link heading revealed", { surface: mode, rule: "EDT-FR-YXUD" });
    outline.reveal(entry);
  }, [opener, artifactId, loaded, editor, mode, outline, sourceText, buffer]);
}
