import * as api from "../api";

/**
 * How an `EditSessionStore` reads and writes the documents it holds state for.
 *
 * The Editor is one editing surface over Markdown, and `NAW-new-artifact.md`
 * NAW-FR-11 requires a draft file to be edited in exactly that surface — the
 * same two modes, the same toolbar, the same find panels — rather than in a
 * lookalike. The only thing that actually differs between an artifact and a
 * draft file is which two backend operations move the bytes, so that is the one
 * thing this abstraction lifts out. Everything above it — the buffer, the dirty
 * flag, the undo history, the mode, the find state, the flush-before-teardown
 * path — is identical and shared.
 */
export interface DocumentTransport {
  /** Read a document's current bytes and the checksum of them. */
  load(id: string): Promise<{ body: string; checksum: string }>;
  /** Overwrite a document; returns the checksum of the bytes written. */
  save(id: string, body: string): Promise<{ checksum: string }>;
  /**
   * Whether `"artifact changed externally"` describes documents in this store.
   *
   * True for artifacts, which the project watcher reports on (EXC-FR-LKHZ). False
   * for drafts: nothing watches `.synthesis/drafts/`, and the ids in that event
   * are project-relative paths — a draft key could collide with one and raise a
   * divergence modal over an unrelated file's change.
   */
  readonly watchesExternalChanges: boolean;
}

/** EXC-FR-WCOM / EDT-FR-31: the project's own artifacts. */
export const artifactTransport: DocumentTransport = {
  load: (id) => api.loadArtifactContentsById(id),
  save: (id, body) => api.saveArtifactContents(id, body),
  watchesExternalChanges: true,
};

/**
 * A draft file's identity in a store: the draft it belongs to and its
 * draft-relative path within it.
 *
 * Joined with `/` and split at the *first* one. A draft id is a bare token of
 * `[A-Za-z0-9_-]` (`is_valid_draft_id` in `drafts.rs`) so it holds no separator
 * of its own, which is what makes the split unambiguous however deeply the file
 * is nested.
 */
export function draftDocKey(draftId: string, path: string): string {
  return `${draftId}/${path}`;
}

export function parseDraftDocKey(key: string): { draftId: string; path: string } {
  const cut = key.indexOf("/");
  // A key with no separator names no file. Returning an empty path rather than
  // throwing keeps the failure on the backend's validation, which reports it as
  // an error the surface shows, instead of taking down a render.
  if (cut === -1) return { draftId: key, path: "" };
  return { draftId: key.slice(0, cut), path: key.slice(cut + 1) };
}

/** NAW-FR-11 / NAW-FR-13: draft files, edited in the same surface artifacts are. */
export const draftTransport: DocumentTransport = {
  load: (key) => {
    const { draftId, path } = parseDraftDocKey(key);
    return api.loadDraftFileContents(draftId, path);
  },
  save: (key, body) => {
    const { draftId, path } = parseDraftDocKey(key);
    return api.saveDraftFileContents(draftId, path, body);
  },
  watchesExternalChanges: false,
};
