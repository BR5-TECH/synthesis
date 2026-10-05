/**
 * The three creation windows of the main window — New Folder (NFW), New File
 * (NFI) and New Artifact (NTA) — as one group: what opens each of them, and
 * what each one does when it is submitted.
 *
 * Held together because they share one rule. Each is a floating overlay of the
 * main window, so opening any one of them closes every other (NAW-FR-01), and
 * that exclusion is enforced in the opening logic rather than by listeners
 * coordinating after the fact.
 *
 * Plain closures over the shell's state rather than a hook: the seeds and the
 * overlay flags stay owned by `useShellSession`, so nothing here changes what
 * React sees.
 */
import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import type { SubmitResult } from "../../components/NewFileModal";
import type { NewFilePayload } from "../../components/NewFileModal";
import type { NewFolderPayload } from "../../components/NewFolderModal";
import type { NewTypedArtifactPayload } from "../../components/NewTypedArtifactModal";
import type {
  NewFileSeed,
  NewFolderSeed,
  NewTypedArtifactSeed,
  OpenableArtifact,
} from "../../types";

export interface CreationActionDeps {
  openArtifact: (item: OpenableArtifact) => void;
  setRevealArtifactId: (id: string | null) => void;
  setSearchOpen: (open: boolean) => void;
  setProgressOverlayOpen: (open: boolean) => void;
  setNewFileSeed: (seed: NewFileSeed | null) => void;
  setNewFolderSeed: (seed: NewFolderSeed | null) => void;
  setNewTypedArtifactSeed: (seed: NewTypedArtifactSeed | null) => void;
}

export interface CreationActions {
  openNewFolder: (seed: NewFolderSeed) => void;
  openNewFile: (seed: NewFileSeed) => void;
  openNewTypedArtifact: (seed: NewTypedArtifactSeed) => void;
  submitNewFile: (payload: NewFilePayload) => Promise<SubmitResult>;
  submitNewFolder: (payload: NewFolderPayload) => Promise<SubmitResult>;
  submitNewTypedArtifact: (
    payload: NewTypedArtifactPayload,
  ) => Promise<SubmitResult>;
}

export function createCreationActions(
  deps: CreationActionDeps,
): CreationActions {
  const {
    openArtifact,
    setRevealArtifactId,
    setSearchOpen,
    setProgressOverlayOpen,
    setNewFileSeed,
    setNewFolderSeed,
    setNewTypedArtifactSeed,
  } = deps;

  // NFW-FR-01: open the New Folder modal. Mutually exclusive with every other
  // overlay of the main window on the same terms as the New Artifact modal, so
  // opening it closes them rather than coexisting with them.
  const openNewFolder = (seed: NewFolderSeed) => {
    setSearchOpen(false);
    setProgressOverlayOpen(false);
    setNewFileSeed(null);
    setNewTypedArtifactSeed(null);
    setNewFolderSeed(seed);
  };

  // NFI-FR-01: open the New File modal. A floating overlay on the same terms as
  // the other two creation windows, so opening it closes every other rather than
  // coexisting with one.
  const openNewFile = (seed: NewFileSeed) => {
    setSearchOpen(false);
    setProgressOverlayOpen(false);
    setNewFolderSeed(null);
    setNewTypedArtifactSeed(null);
    setNewFileSeed(seed);
  };

  /**
   * NTA-FR-01: open the New Artifact modal. A floating overlay on the same terms
   * as the other two creation windows, so opening it closes every other rather
   * than coexisting with one — and NTA-FR-14 is why it does not create a draft:
   * a draft is created in the Drafts panel alone (DRP-FR-06, DRP-FR-26).
   */
  const openNewTypedArtifact = (seed: NewTypedArtifactSeed) => {
    setSearchOpen(false);
    setProgressOverlayOpen(false);
    setNewFileSeed(null);
    setNewFolderSeed(null);
    setNewTypedArtifactSeed(seed);
  };

  /**
   * NFI-FR-09 / NFI-FR-12 / NFI-FR-14: perform the plain-file creation. On success
   * the modal closes, the new file opens in its natural surface, and its id is
   * handed to the Library to reveal and select. On failure the error is returned
   * for the modal to show inline and the modal stays open.
   *
   * A file — unlike a folder — has a natural editing surface, so this does open a
   * tab: `openArtifact` routes by the node's resolved type, which for the common
   * case of an untyped file is the Editor's plain-text mode (ESH-FR-ATDS) and for
   * one that resolves to Flow is a Flow tab (LIB-FR-03). The type is whatever
   * the backend's classification returned on the node; this window never assigns
   * one (NFI-FR-11).
   */
  const submitNewFile = async (
    payload: NewFilePayload,
  ): Promise<SubmitResult> => {
    try {
      const node = await api.createFile({
        location: payload.location,
        name: payload.name,
      });
      openArtifact({
        id: node.id,
        name: node.name,
        artifactType: node.artifactType,
      });
      setRevealArtifactId(node.id);
      setNewFileSeed(null);
      return { ok: true };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  };

  /**
   * NFW-FR-09 / NFW-FR-11 / NFW-FR-13: perform the folder creation. On success
   * the modal closes and the new folder's id is handed to the Library to reveal
   * and select. Deliberately no `openArtifact` call: a folder has no natural
   * editing surface, so no tab opens (NFW-FR-11). On failure the error is
   * returned for the modal to show inline and the modal stays open.
   */
  const submitNewFolder = async (
    payload: NewFolderPayload,
  ): Promise<SubmitResult> => {
    try {
      const node = await api.createFolder({
        location: payload.location,
        name: payload.name,
        artifactType: payload.artifactType,
      });
      setRevealArtifactId(node.id);
      setNewFolderSeed(null);
      return { ok: true };
    } catch (e) {
      return { ok: false, error: String(e) };
    }
  };

  /**
   * NTA-FR-10 / NTA-FR-13 / NTA-FR-16: perform the typed-artifact creation. On
   * success the modal closes, the new file opens in its **natural surface** —
   * a Flow tab for the type Flow and an Editor tab for the other seven, routed
   * by the node's resolved type (LIB-FR-03) — and its id is handed to the
   * Project panel to reveal and select. On failure the error is returned for the
   * modal to show inline and the modal stays open with its three inputs intact.
   *
   * One backend call and nothing else: the file and its file-scope type
   * assignment are one transaction on the backend (PST-FR-29), so there is no
   * second call here to assign the type and nothing for this to clean up after a
   * half-completed creation. No draft operation is invoked (NTA-FR-14).
   */
  const submitNewTypedArtifact = async (
    payload: NewTypedArtifactPayload,
  ): Promise<SubmitResult> => {
    try {
      const node = await api.createTypedFile({
        location: payload.location,
        name: payload.name,
        artifactType: payload.artifactType,
      });
      logInfo(["frontend"], "typed artifact created", {
        // The path and the type — what routed the tab and what the panel is
        // about to reveal. The file is empty, so no content is described here.
        path: node.path,
        artifactType: payload.artifactType,
      });
      openArtifact({
        id: node.id,
        name: node.name,
        artifactType: node.artifactType,
      });
      setRevealArtifactId(node.id);
      setNewTypedArtifactSeed(null);
      return { ok: true };
    } catch (e) {
      // NTA-FR-16: the window shows this inline and stays open. Logged too,
      // because a refusal the author corrected leaves no other trace.
      logWarn(["frontend"], "typed artifact creation failed", {
        location: payload.location ?? "",
        name: payload.name,
        artifactType: payload.artifactType,
        reason: String(e),
      });
      return { ok: false, error: String(e) };
    }
  };

  return {
    openNewFolder,
    openNewFile,
    openNewTypedArtifact,
    submitNewFile,
    submitNewFolder,
    submitNewTypedArtifact,
  };
}
