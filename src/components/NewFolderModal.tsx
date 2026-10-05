import { useEffect, useState } from "react";
import { ARTIFACT_TYPES } from "../artifactTypes";
import type { FolderOption } from "../hooks/useProjectFolders";
import { Icon } from "./icons";
import type { ArtifactType, NewFolderSeed } from "../types";

// NFW-FR-08: a creatable name is non-empty and carries no path separator. One
// invocation therefore creates exactly one folder rather than a chain of nested
// ones — the same rule the backend enforces (PST-FR-25), checked client-side so
// Create is disabled before any call is made.
function isValidName(name: string): boolean {
  const t = name.trim();
  return t !== "" && !t.includes("/") && !t.includes("\\");
}

export type NewFolderPayload = {
  location: string | null;
  name: string;
  artifactType: ArtifactType | null;
};

export type SubmitResult = { ok: true } | { ok: false; error: string };

interface NewFolderModalProps {
  // The invocation context. The modal is mounted only while creating, so this is
  // always present (a closed modal is unmounted by the parent).
  seed: NewFolderSeed;
  /**
   * Every folder in the project, for the Parent Folder select (NFW-FR-04).
   * Supplied by the shell from the tree the Library already holds — the window
   * never scans on its own (NFW contract boundary / NFR).
   */
  folders: FolderOption[];
  onClose: () => void;
  // Performs the creation; resolves ok on success (the parent then closes and
  // reveals the folder) or an error to show inline (the modal stays open).
  onSubmit: (payload: NewFolderPayload) => Promise<SubmitResult>;
}

/**
 * The New Folder modal (NFW-new-folder.md). A centered overlay (reusing the
 * shared `.scrim`/`.modal` chrome, NFW-FR-01 / NFW-FR-02) presenting Parent
 * Folder / Name / Artifact Type and nothing else — a folder has no body, so
 * there is no content or AI-prompt input here.
 *
 * The two flows differ in exactly one value: the File menu starts the parent at
 * the project root (NFW-FR-06) and the Library context menu starts it at the
 * right-clicked folder (NFW-FR-07). Both leave it editable, and both leave the
 * artifact type unset — it is never seeded, not even from the parent's own type
 * (NFW-FR-05), so a folder is typed only when the user says so.
 */
export function NewFolderModal({
  seed,
  folders,
  onClose,
  onSubmit,
}: NewFolderModalProps) {
  // NFW-FR-04 / NFW-FR-06 / NFW-FR-07: pre-filled, always editable. The empty
  // string is the project root, which is also the File-menu flow's default.
  const [parent, setParent] = useState(seed.initialParent ?? "");
  const [name, setName] = useState("");
  // NFW-FR-05: unset in every flow. Nothing seeds this.
  const [type, setType] = useState<ArtifactType | "">("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // NFW-FR-12: Escape dismisses without creating anything.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  // The seeded parent may name a folder the published list does not carry yet —
  // a folder created moments ago, in the window before the list refreshed. Offer
  // it regardless, so a context-menu invocation never opens showing a parent
  // other than the folder that was right-clicked.
  const options =
    parent !== "" && !folders.some((f) => f.path === parent)
      ? [{ path: parent, label: parent }, ...folders]
      : folders;

  const canCreate = isValidName(name) && !submitting;

  const submit = async () => {
    if (!isValidName(name)) return;
    setSubmitting(true);
    setError(null);
    let res: SubmitResult;
    try {
      res = await onSubmit({
        location: parent === "" ? null : parent,
        name: name.trim(),
        artifactType: type === "" ? null : type,
      });
    } catch (e) {
      // `onSubmit` is contracted to resolve a result rather than reject, but a
      // rejection must still leave a usable window rather than a Create button
      // stuck disabled forever with nothing said (NFW-FR-13).
      res = { ok: false, error: String(e) };
    }
    // On success the parent unmounts this modal; only the failure path needs to
    // restore the form (NFW-FR-13: stay open with inputs intact, show the error).
    if (!res.ok) {
      setError(res.error);
      setSubmitting(false);
    }
  };

  return (
    <div
      className="scrim"
      onClick={(e) => {
        // NFW-FR-12: a click on the backdrop (outside the modal frame) cancels.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-labelledby="new-folder-title">
        <div className="modal__head">
          <Icon.Folder size={14} />
          <div className="modal__title" id="new-folder-title">
            New Folder
          </div>
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body">
          <div className="picker-field">
            <label className="picker-field__label" htmlFor="nf-parent">
              Parent Folder
            </label>
            <select
              id="nf-parent"
              className="select"
              aria-label="Parent Folder"
              value={parent}
              onChange={(e) => setParent(e.target.value)}
            >
              <option value="">(project root)</option>
              {options.map((f) => (
                <option key={f.path} value={f.path}>
                  {f.label}
                </option>
              ))}
            </select>
          </div>

          <div className="picker-field">
            <label className="picker-field__label" htmlFor="nf-name">
              Name
            </label>
            <input
              id="nf-name"
              className="input input--mono"
              aria-label="Name"
              // NFW NFR: the only value the user must always supply.
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && canCreate) void submit();
              }}
            />
          </div>

          <div className="picker-field">
            <label className="picker-field__label" htmlFor="nf-type">
              Artifact Type
            </label>
            {/* NFW-FR-05: the explicit unset state first, then exactly the eight
                built-in types (ASC-FR-02). Choosing one makes the folder carry it
                as a folder-scope assignment, so the files put inside it later
                resolve to that type by inheritance (NFW-FR-10). */}
            <select
              id="nf-type"
              className="select"
              aria-label="Artifact Type"
              value={type}
              onChange={(e) => setType(e.target.value as ArtifactType | "")}
            >
              <option value="">(not set)</option>
              {ARTIFACT_TYPES.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </select>
          </div>

          {error && (
            <div style={{ fontSize: "var(--fs-ui-sm)", color: "var(--danger, #e5484d)" }} role="alert">
              {error}
            </div>
          )}
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!canCreate}
            onClick={() => void submit()}
          >
            <Icon.Plus size={12} /> Create
          </button>
        </div>
      </div>
    </div>
  );
}
