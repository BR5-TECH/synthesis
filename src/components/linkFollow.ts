/**
 * Who a link of a rich Markdown surface goes to
 * (`specifications/ui/EDT-editor.md` EDT-FR-GEHE, EDT-FR-YXUD, EDT-FR-QBIE).
 *
 * The shell gives each tab a **link opener**: the project root and the way a
 * file opens. Each surface makes its **link host** from it, with the base that
 * its own file gives a relative path.
 */
import { createContext, useContext, useMemo } from "react";
import { folderOf, type LinkBase } from "./markdownLinks";

/** EDT-FR-GEHE / EDT-FR-YXUD: what the shell gives the surfaces of a tab. */
export interface LinkOpener {
  /** EDT-FR-AWFH: the absolute paths of the project root. */
  roots: readonly string[];
  /** EDT-FR-GEHE / EDT-FR-HYVP: open the project file `path`, if it exists. */
  openFile: (path: string, fragment: string | null) => void;
  /** EDT-FR-YXUD: the fragment that waits for the file `path`, taken one time. */
  takePendingFragment: (path: string) => string | null;
  /**
   * EDT-FR-YXUD: the active tab now shows the file `path`, or no file. A
   * fragment that waits for another file is dropped.
   */
  activeFileChanged: (path: string | null) => void;
}

/** EDT-FR-QBIE: what one surface resolves its links against. */
export interface LinkHost {
  base: LinkBase;
  /** EDT-FR-GEHE: the project-relative path of the file on the surface. */
  selfPath: string | null;
  /** Absent where the host opens no file: each file link is refused. */
  openFile?: (path: string, fragment: string | null) => void;
  /**
   * EDT-FR-HMJI: the jump of a surface that is not one editor, such as the
   * Rich rendering of a Diff tab. `origin` is the link that was clicked.
   */
  revealFragment?: (fragment: string, origin: Element) => boolean;
}

/** The shell's opener. `null` in a window with no project tabs. */
export const LinkOpenerContext = createContext<LinkOpener | null>(null);

/**
 * The host of a rich surface that has no editor of its own to take it as a
 * prop, such as a read-only reading or a Diff tab's run.
 */
export const RichLinkHostContext = createContext<LinkHost | null>(null);

export function useLinkOpener(): LinkOpener | null {
  return useContext(LinkOpenerContext);
}

export function useRichLinkHost(): LinkHost | null {
  return useContext(RichLinkHostContext);
}

/** Where a surface's relative paths start, and which file is its own. */
export interface LinkPlace {
  fileDir: string;
  selfPath: string | null;
}

/** EDT-FR-QBIE: the place of a surface on the project file `path`. */
export function placeOfFile(path: string): LinkPlace {
  return { fileDir: folderOf(path), selfPath: path };
}

/**
 * EDT-FR-QBIE: the host of a surface at `place`. With no opener, or no place,
 * the host resolves no file, and only external links and fragments work.
 */
export function useLinkHost(
  place: LinkPlace | null,
  revealFragment?: LinkHost["revealFragment"],
): LinkHost {
  const opener = useLinkOpener();
  const fileDir = place?.fileDir ?? null;
  const selfPath = place?.selfPath ?? null;
  return useMemo(
    () => ({
      base: { fileDir: opener ? fileDir : null, roots: opener?.roots ?? [] },
      selfPath,
      openFile: opener?.openFile,
      revealFragment,
    }),
    [opener, fileDir, selfPath, revealFragment],
  );
}
