/**
 * The shell's link opener (`specifications/ui/EDT-editor.md` EDT-FR-GEHE,
 * EDT-FR-YXUD, EDT-FR-HYVP): a link to a project file opens that file as a
 * click in the Project panel opens it, and its fragment waits for the tab.
 */
import { useEffect, useMemo, useRef } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { LinkOpener } from "../../components/linkFollow";
import { showLinkTargetMissing } from "../../state/linkToasts";
import { isMarkdownFile } from "../../state/syntaxHighlight";
import type { OpenableArtifact } from "../../types";

/** The file name of a project-relative path. */
function nameOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/**
 * `roots` are the absolute paths of the project root (EDT-FR-AWFH). `open` is
 * the Project panel's way to open a file (per `LIB-library.md` LIB-FR-03).
 */
export function useFileLinkOpener(
  roots: readonly string[],
  open: (item: OpenableArtifact) => void,
): LinkOpener {
  const openRef = useRef(open);
  openRef.current = open;
  /** EDT-FR-YXUD: the fragment that waits for the tab of one file. */
  const pending = useRef<{ path: string; fragment: string } | null>(null);
  /** The latest link: an earlier one that answers later opens nothing. */
  const generation = useRef(0);
  // Empty and repeated roots are dropped, and the opener changes only when
  // the set of roots does.
  const rootsKey = [...new Set(roots.filter((root) => root !== ""))].join("\n");

  return useMemo<LinkOpener>(
    () => ({
      roots: rootsKey === "" ? [] : rootsKey.split("\n"),
      openFile: (path, fragment) => {
        pending.current = null;
        const mine = ++generation.current;
        if (path === "") {
          logWarn(["frontend"], "link file not found", { rule: "EDT-FR-HYVP" });
          showLinkTargetMissing();
          return;
        }
        // EDT-FR-HYVP: the file is asked for first, so a missing one opens no
        // tab.
        void api.openArtifactById(path).then(
          (opened) => {
            if (mine !== generation.current) return;
            if (fragment !== null && isMarkdownFile(path)) {
              pending.current = { path, fragment };
            }
            logDebug(["frontend"], "link file opened", {
              path,
              fragment: fragment !== null,
              rule: "EDT-FR-GEHE",
            });
            openRef.current({ id: path, name: nameOf(path), artifactType: opened?.artifactType });
          },
          (e: unknown) => {
            if (mine !== generation.current) return;
            logWarn(["frontend"], "link file not found", {
              path,
              error: e instanceof Error ? e.message : String(e),
              rule: "EDT-FR-HYVP",
            });
            showLinkTargetMissing();
          },
        );
      },
      activeFileChanged: (path) => {
        // A file that opened in no Editor tab, such as a PDF or a change
        // workspace, takes no jump later.
        if (pending.current && pending.current.path !== path) pending.current = null;
      },
      takePendingFragment: (path) => {
        const waiting = pending.current;
        if (!waiting || waiting.path !== path) return null;
        pending.current = null;
        return waiting.fragment;
      },
    }),
    [rootsKey],
  );
}

/**
 * The opener of the viewport, with the hand-over of EDT-FR-YXUD: `activeFile`
 * is the file of the active tab. The Editor of that file takes its fragment
 * first, in its own effect of the same commit, so a fragment still waiting
 * here is for a file that no tab showed, and it is dropped.
 */
export function useViewportLinkOpener(
  roots: readonly string[],
  open: (item: OpenableArtifact) => void,
  activeFile: string | null,
  activeTab: string,
): LinkOpener {
  const opener = useFileLinkOpener(roots, open);
  useEffect(() => {
    opener.activeFileChanged(activeFile);
  }, [opener, activeFile, activeTab]);
  return opener;
}
