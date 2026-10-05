import * as api from "../api";
import type { DocumentTransport } from "./documentTransport";

/**
 * The project's draft template as an editable document (PSS-FR-21 / SET-FR-16
 * … SET-FR-20).
 *
 * Lives beside the other document stores rather than inside the section that
 * renders it, for the reason `editSessions.ts` gives for owning a write
 * schedule: a pending write must survive the surface showing it. The Draft
 * template section is one section of one tab, and the tab is torn down by a
 * project or worktree change — so the shell has to be able to reach a pending
 * template write without knowing which section happens to be mounted.
 */

/**
 * The document key the template is edited under.
 *
 * It ends in `.md` deliberately: the editing surface reads the file's *shape*
 * off its name (ESH-FR-SSDV), so this is what puts the template on the Markdown
 * surface — the two modes of EDT-FR-17, the frontmatter region of EDT-FR-18 and
 * the formatting band of EFR-FR-ABVQ — rather than on the raw source surface a
 * name without a Markdown extension would get.
 */
export const TEMPLATE_DOC_KEY = "draft-template.md";

/**
 * A checksum for the template's bytes.
 *
 * The store measures a document against the checksum its last read or write
 * returned. Nothing watches `project.toml` for external change (the transport
 * below says so), so this is never compared against anything a filesystem
 * watcher produced — it only has to change when the bytes do. FNV-1a over the
 * text is enough for that and costs nothing on a document of this size.
 */
export function checksumOf(text: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `${hash.toString(16)}:${text.length}`;
}

/**
 * SET-FR-17 / SET-FR-19 / PSS-FR-21: how the template's bytes are read and
 * written.
 *
 * The write is a **read-modify-write**: `"save project config"` is a whole-store
 * write, so it re-reads the store first and persists the template beside
 * whatever every other project-public section currently holds — a line-ending
 * convention the status bar changed while this section was open is carried
 * through rather than reverted to the one this mount happened to read.
 *
 * Empty text persists the **unset** state rather than a configured empty
 * template (SET-FR-19), which is what returns the project to the behaviour it
 * had before a template was ever written.
 */
export const draftTemplateTransport: DocumentTransport = {
  load: async () => {
    const config = await api.loadProjectConfig();
    const body = config.draftTemplate ?? "";
    return { body, checksum: checksumOf(body) };
  },
  save: async (_id, body) => {
    const current = await api.loadProjectConfig();
    await api.saveProjectConfig({
      lineEndings: current.lineEndings,
      draftTemplate: body,
    });
    return { checksum: checksumOf(body) };
  },
  // Nothing watches `.synthesis/project.toml`, and the key above is not a
  // project-relative artifact path, so an `"artifact changed externally"` event
  // must never be taken to describe this document.
  watchesExternalChanges: false,
};

/**
 * The mounted Draft template section's own write, or null when no section is
 * mounted.
 *
 * One at a time: exactly one Project settings window exists (SWN-FR-05), and
 * exactly one of its sections is rendered. A second registration replaces the
 * first, which is what a remount does.
 */
let pending: (() => Promise<boolean>) | null = null;

/**
 * Publish the mounted section's write so a teardown can bring it forward.
 * Returns the deregistration the section calls when it unmounts.
 */
export function registerDraftTemplateWrite(
  write: () => Promise<boolean>,
): () => void {
  pending = write;
  return () => {
    if (pending === write) pending = null;
  };
}

/**
 * SET-FR-17: bring the template's pending write forward, and **wait for it**.
 *
 * Called from the shell's teardown path (`flushBeforeTeardown`) alongside the
 * artifact, Flow, and draft flushes, for the reason stated there: an unawaited
 * write races the teardown that follows and can land against the wrong content
 * root, or after the project has been closed under it. A worktree switch closes
 * every tab, so without this the author's last burst of typing would go with
 * the section that held it.
 *
 * Resolves whatever happened. Unlike an artifact's flush this never cancels a
 * teardown: the section may not even be on screen, and a template that could
 * not be written is reported where the author can act on it (SET-FR-18) rather
 * than by refusing to close the project. A failure that arrives here is logged
 * by the section's own write path.
 */
export async function flushDraftTemplate(): Promise<boolean> {
  return (await pending?.()) ?? true;
}
