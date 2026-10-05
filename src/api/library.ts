/**
 * The library tree, the files in it, and what creates one.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  ArtifactContents,
  ArtifactType,
  AssignScope,
  OpenedArtifact,
  FlowValidationReport,
  SaveResult,
  TreeNode,
} from "../types";

// --- Library tree (library.rs) --------------------------------------------

export const loadProjectTree = () => invoke<TreeNode>("load_project_tree");

export const rescanProjectTree = () => invoke<TreeNode>("rescan_project_tree");

export const assignArtifactType = (
  path: string,
  artifactType: ArtifactType,
  scope: AssignScope,
) => invoke<TreeNode>("assign_artifact_type", { path, artifactType, scope });

export const clearArtifactType = (path: string) =>
  invoke<TreeNode>("clear_artifact_type", { path });

// --- Library file operations (library.rs) ---------------------------------

/**
 * PST-FR-18 (`"delete path (path, recursive)"`).
 *
 * `recursive = false` is both the narrow delete and the emptiness probe: a
 * folder holding anything rejects with `ERR_NOT_EMPTY` and removes nothing,
 * which is how the Library learns it must ask before taking a subtree
 * (LCM-FR-11). Emptiness is decided by the backend against the filesystem, so it
 * counts entries the tree never showed.
 */
export const deletePath = (path: string, recursive: boolean) =>
  invoke<void>("delete_path", { path, recursive });

/**
 * The typed backend error a non-recursive delete returns for a folder that
 * still holds entries (PST-FR-18). Matched rather than parsed for prose, so the
 * confirmation is raised by the contract and not by an error message's wording.
 */
export const ERR_NOT_EMPTY = "directory not empty";

/**
 * PST-FR-19: the typed error for a rename to the name the entry already has.
 * The Library rejects this client-side (LCM-FR-12) so it normally never reaches
 * the backend; this is the backstop for any other caller.
 */
export const ERR_UNCHANGED_NAME = "unchanged name";

export const renamePath = (path: string, newName: string) =>
  invoke<void>("rename_path", { path, newName });

export const copyPathIntoFolder = (sourcePath: string, destFolder: string) =>
  invoke<void>("copy_path_into_folder", { sourcePath, destFolder });

// --- Plain-file creation (library.rs) -------------------------------------

// PST-FR-26 / NFI-new-file.md: create an empty plain file and return its node so
// the UI can open and reveal it. `location` unset (null) creates at the project
// root; `name` is the file's complete basename, extension included, and no
// extension is required or appended (NFI-FR-05).
//
// Deliberately not `createArtifact` with defaults: this operation records no type
// assignment at any scope, writes no body, and carries no AI prompt — the new
// file's type is left entirely to inference and folder-scope inheritance
// (NFI-FR-11).
export const createFile = (args: { location: string | null; name: string }) =>
  invoke<TreeNode>("create_file", args);

// --- Folder creation (library.rs) -----------------------------------------

// PST-FR-25 / NFW-new-folder.md: create a new folder and return its node so the
// UI can reveal it. `location` unset (null) creates at the project root;
// `artifactType` set records a FOLDER-scope assignment the folder's future
// contents inherit, and unset records nothing.
export const createFolder = (args: {
  location: string | null;
  name: string;
  artifactType: ArtifactType | null;
}) => invoke<TreeNode>("create_folder", args);

// --- Typed-artifact creation (library.rs) ---------------------------------

/**
 * PST-FR-29 / NTA-new-typed-artifact.md: create one empty file **and** record
 * the chosen type as that file's file-scope assignment, as one creation
 * transaction, and return the new node so the UI can open, reveal, and select
 * it (NTA-FR-13).
 *
 * `location` unset (null) creates at the project root; `name` is the file's
 * complete basename, extension included, with nothing appended and no extension
 * derived from the type (NTA-FR-05); `artifactType` is required (NTA-FR-06).
 *
 * Either both the file and its assignment survive the call or neither does, so
 * a failure is retried against the project exactly as the first attempt found
 * it (NTA-FR-16). Distinct from `createFile`, whose files carry no assignment
 * at all (NFI-FR-11) — and from `createDraft`, which this never reaches:
 * nothing is written under `.synthesis/drafts/` and no draft template is read
 * (NTA-FR-14).
 */
export const createTypedFile = (args: {
  location: string | null;
  name: string;
  artifactType: ArtifactType;
}) => invoke<TreeNode>("create_typed_file", args);

// --- Artifact contents (artifacts.rs) -------------------------------------

export const loadArtifactContentsById = (id: string) =>
  invoke<ArtifactContents>("load_artifact_contents_by_id", { id });

/**
 * PST-FR-08 / PST-FR-24: the stable key plus the routing hint for an id the UI
 * is about to open. `kind: "text"` names a file the scan surfaces with no
 * artifact type, which the Editor opens as a plain text file (ESH-FR-ATDS).
 */
export const openArtifactById = (id: string) =>
  invoke<OpenedArtifact>("open_artifact_by_id", { id });

export const saveArtifactContents = (id: string, body: string) =>
  invoke<SaveResult>("save_artifact_contents", { id, body });

/**
 * FGV-FR-02: judge a body against the Flow schema and the graph's structural
 * rules, returning every violation it carries.
 *
 * Pure on the backend — it reads no file and opens no project — so it is safe to
 * call on a body that has never been on disk and on one about to replace one.
 * The Flow tab calls it on load (FLO-FR-46) and before a write (FLO-FR-47); the
 * same validation runs inside `save_artifact_contents` for a `flow`-typed
 * artifact (PST-FR-28), which is what makes it authoritative rather than
 * advisory.
 */
export const validateFlowDocument = (body: string) =>
  invoke<FlowValidationReport>("validate_flow_document", { body });
