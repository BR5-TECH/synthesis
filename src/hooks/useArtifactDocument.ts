import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import type { EditSession, EditSessionStore } from "../state/editSessions";

/**
 * The Editor's live document lifecycle for one artifact (`EXC-editor-external-change.md`,
 * EDT-FR-28–EDT-FR-32), kept independent of the editing surface so the WYSIWYG
 * view (Tiptap) owns the rendering while this hook owns load/save/divergence.
 *
 * The state itself lives in the artifact's `EditSession` (`state/editSessions`),
 * not in this hook: it outlives the Editor component, so switching tabs or
 * closing and reopening the artifact resumes the same buffer, dirty flag,
 * editing mode and undo history (EDT-FR-28–EDT-FR-30). This hook binds a mounted
 * Editor to that record — deciding on mount whether to load or to resume, owning
 * the external-change subscription, and exposing the two divergence resolutions.
 *
 * Owns:
 *  - the first load through the store's transport (EXC-FR-WCOM) — which is
 *    `load_artifact_contents_by_id` for an artifact and `load_draft_file_contents`
 *    for a draft file (NAW-FR-11) — and the revalidating load a reopen
 *    performs (EDT-FR-29);
 *  - the two resolutions for a divergence: reload-from-filesystem (EXC-FR-WDAV)
 *    and keep-mine (EXC-FR-WDEJ).
 *
 * Detecting the divergence in the first place is NOT here: `useExternalChanges`
 * watches every session at the shell level, because only the active tab's Editor
 * is mounted and a change to any other open artifact would otherwise go unseen.
 *
 * The write path itself belongs to the store (EDT-FR-31–EDT-FR-33, EDT-FR-70),
 * and is not re-exposed here: the rest elapsing, a tab or project close, and
 * File → Save all reach `EditSessionStore.flush` directly, because each of them
 * has to write artifacts no Editor is mounted on. This hook covers only what a
 * mounted Editor does — resume or load, report, and resolve a divergence.
 */
export interface UseArtifactDocument {
  /** The artifact's session record; the Editor reads buffer/history/mode off it. */
  session: EditSession;
  /** True once the first load attempt has settled (success or error). */
  loaded: boolean;
  /** Unsaved changes exist (EDT-FR-04). */
  dirty: boolean;
  /** The blocking external-change modal is showing (EXC-FR-VTUH). */
  conflict: boolean;
  /** Last load/save error, if any. */
  error: string | null;
  /**
   * EDT-FR-70: the editor reports a user edit. Marks the artifact dirty and
   * restarts the rest after which it writes itself.
   */
  markDirty: () => void;
  /** EXC-FR-WDAV: discard edits and reload the on-disk version. */
  loadFromFilesystem: () => Promise<void>;
  /** EXC-FR-WDEJ: dismiss the modal, keep the in-memory edits (overwrite on save). */
  keepMine: () => void;
}

export function useArtifactDocument(
  artifactId: string,
  sessions: EditSessionStore,
): UseArtifactDocument {
  // Re-render this Editor whenever an observable field of ANY session changes —
  // notably one changed from outside the component, such as the flush a tab
  // close performs (EDT-FR-31). Buffer and history mutation deliberately does
  // not notify, so typing costs no re-render.
  useSyncExternalStore(sessions.subscribe, sessions.getVersion);
  const session = sessions.ensure(artifactId);

  // First load, or the revalidating load of a reopen (EDT-FR-29). An artifact
  // whose session is already live (a tab switch) is resumed without a reload at
  // all (EDT-FR-30).
  useEffect(() => {
    const s = sessions.ensure(artifactId);
    if (s.loaded && !s.revalidate) return;
    const reopening = s.revalidate;
    // Declared before the IIFE so the body can compare against its own identity
    // (assigned below, before any `await` inside can resume).
    let settled: Promise<void> | null = null;
    settled = (async () => {
      try {
        const res = await sessions.transport.load(artifactId);
        // Commit unless a NEWER load has taken over this artifact. Deliberately
        // not "unless this component unmounted": the session outlives the
        // component (EDT-FR-28), and a reopen's answer is exactly what a write
        // racing it is waiting for — discarding it because the tab closed would
        // let that write overwrite the divergence this load just found.
        if (s.pendingLoad !== settled) return;
        if (reopening) {
          // EDT-FR-29: the retained buffer, history and mode stay; the load only
          // answers whether the file moved while no tab was showing it. A
          // divergence raises the same modal an open tab would get (EXC-FR-VTUH).
          const diverged = res.checksum !== s.baseline;
          sessions.update(artifactId, {
            revalidate: false,
            loaded: true,
            error: null,
            // Authoritative for this artifact's divergence state: matching
            // checksums clear a divergence the watch raised while the tab was
            // closed but which the file has since returned from.
            pending: diverged ? res.checksum : null,
            conflict: diverged,
          });
        } else {
          sessions.adoptLoad(artifactId, res.body, res.checksum);
        }
      } catch (e) {
        if (s.pendingLoad !== settled) return;
        sessions.update(artifactId, {
          error: String(e),
          loaded: true,
          revalidate: false,
        });
      }
    })();
    // Publish it so a write that races this load waits for the answer instead
    // of overwriting a divergence the load is about to find (EDT-FR-29).
    s.pendingLoad = settled;
    void settled.finally(() => {
      if (s.pendingLoad === settled) s.pendingLoad = null;
    });
  }, [artifactId, sessions]);

  const markDirty = useCallback(() => {
    // EDT-FR-70: marks dirty (only the transition notifies — this runs on every
    // keystroke) and restarts the rest, so a burst of typing is one write.
    sessions.noteEdit(artifactId);
  }, [artifactId, sessions]);

  const loadFromFilesystem = useCallback(async () => {
    // EXC-FR-WDAV/EDT-FR-24: discard the buffer AND the history above it, adopting
    // the on-disk content as the new floor.
    const s = sessions.ensure(artifactId);
    const settled = (async () => {
      try {
        const res = await sessions.transport.load(artifactId);
        sessions.adoptLoad(artifactId, res.body, res.checksum);
      } catch (e) {
        sessions.update(artifactId, { error: String(e), loaded: true });
      }
    })();
    s.pendingLoad = settled;
    void settled.finally(() => {
      if (s.pendingLoad === settled) s.pendingLoad = null;
    });
    await settled;
  }, [artifactId, sessions]);

  const keepMine = useCallback(() => {
    // EXC-FR-WDEJ: keep the in-memory edits and dirty state. Adopt the
    // acknowledged on-disk checksum as the baseline so the SAME change does not
    // re-prompt — but a FURTHER external change (a new checksum) will
    // (EXC-FR-UWYK).
    const s = sessions.ensure(artifactId);
    sessions.update(artifactId, {
      baseline: s.pending ?? s.baseline,
      pending: null,
      conflict: false,
    });
    // EDT-FR-70: a write held behind the modal is performed at once, overwriting
    // the on-disk file with the buffer the author chose to keep. One that was
    // still merely resting when they answered keeps its rest and lands on it.
    if (sessions.takeHeldWrite(artifactId)) void sessions.flush(artifactId);
  }, [artifactId, sessions]);

  // Stable while nothing it reports has changed: consumers put this object in
  // effect and useCallback dependencies (the Editor's undo/redo listeners hang
  // off it), and a fresh object per render would re-run them on every keystroke.
  return useMemo(
    () => ({
      session,
      loaded: session.loaded,
      dirty: session.dirty,
      conflict: session.conflict,
      error: session.error,
      markDirty,
      loadFromFilesystem,
      keepMine,
    }),
    [
      session,
      session.loaded,
      session.dirty,
      session.conflict,
      session.error,
      markDirty,
      loadFromFilesystem,
      keepMine,
    ],
  );
}
