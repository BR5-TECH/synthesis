// What a new artifact is seeded with (NAW-new-artifact.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

export interface NewDraftSeed {
  /** Drafts-root-relative folder to file the draft in; absent is the root. */
  folder?: string;
}

// Invocation context for the New File modal (NFI-new-file.md). Both entry points
// produce one of these: the File menu passes an empty seed, so Location starts at
// the project root (NFI-FR-06); the Library context menu passes the right-clicked
// folder as the *starting* location, which stays editable (NFI-FR-07). A seed of
// `null` means the modal is closed.
//
// Deliberately carries no artifact type and no content: the window records no
// assignment (NFI-FR-11) and the file is born empty (NFI-FR-10), so there is
// nothing else for a caller to pass.
export interface NewFileSeed {
  // Project-relative folder the Location select starts on. Absent — and the empty
  // string — mean the project root.
  initialLocation?: string;
}

// Invocation context for the New Folder modal (NFW-new-folder.md). Both entry
// points produce one of these: the File menu passes an empty seed, so Parent
// Folder starts at the project root (NFW-FR-06); the Library context menu passes
// the right-clicked folder as the *starting* parent, which stays editable
// (NFW-FR-07). A seed of `null` means the modal is closed.
//
// Deliberately carries no artifact type: NFW-FR-05 makes the type unset in every
// flow, never seeded from the parent or from anything else, so there is nothing
// for a caller to pass.
export interface NewFolderSeed {
  // Project-relative folder the Parent Folder select starts on. Absent — and the
  // empty string — mean the project root.
  initialParent?: string;
}

// Invocation context for the New Artifact modal (NTA-new-typed-artifact.md).
// Both entry points produce one of these: the File menu passes an empty seed, so
// Location starts at the project root (NTA-FR-07); the Project context menu
// passes the right-clicked folder as the *starting* location, which stays
// editable (NTA-FR-08). A seed of `null` means the modal is closed.
//
// Deliberately carries no artifact type: NTA-FR-06 and NTA-FR-08 make the type
// unchosen in every flow, never seeded from the location — not even when the
// right-clicked folder carries a folder-scope assignment of its own — so there
// is nothing for a caller to pass. And no draft anything: this window creates a
// project file and never a draft (NTA-FR-14).
export interface NewTypedArtifactSeed {
  // Project-relative folder the Location select starts on. Absent — and the
  // empty string — mean the project root.
  initialLocation?: string;
}
