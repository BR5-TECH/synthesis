import { useEffect, useState } from "react";
import type { FolderOption } from "../hooks/useProjectFolders";
import { Icon } from "./icons";
import type { NewFileSeed } from "../types";

// NFI-FR-08: a creatable name is non-empty and carries no path separator. One
// invocation therefore creates exactly one file rather than a file plus the
// folders leading to it — the same rule the backend enforces (PST-FR-26), checked
// client-side so Create is disabled before any call is made.
//
// Deliberately says nothing about extensions (NFI-FR-05): the name is the file's
// complete basename, so `Makefile`, `.gitignore`, and `notes.tar.gz` are all
// valid, and nothing here appends or requires a suffix.
function isValidName(name: string): boolean {
  const t = name.trim();
  return t !== "" && !t.includes("/") && !t.includes("\\");
}

export type NewFilePayload = {
  location: string | null;
  name: string;
};

export type SubmitResult = { ok: true } | { ok: false; error: string };

interface NewFileModalProps {
  // The invocation context. The modal is mounted only while creating, so this is
  // always present (a closed modal is unmounted by the parent).
  seed: NewFileSeed;
  /**
   * Every folder in the project, for the Location select (NFI-FR-04). Supplied by
   * the shell from the tree the Library already holds — the window never scans on
   * its own (NFI contract boundary / NFR).
   */
  folders: FolderOption[];
  onClose: () => void;
  // Performs the creation; resolves ok on success (the parent then closes, opens
  // the file, and reveals it) or an error to show inline (the modal stays open).
  onSubmit: (payload: NewFilePayload) => Promise<SubmitResult>;
}

/**
 * The New File modal (NFI-new-file.md). A centered overlay (reusing the shared
 * `.scrim`/`.modal` chrome, NFI-FR-01 / NFI-FR-02) presenting Location and Name
 * and nothing else: no artifact-type control, no content editor, and no AI-prompt
 * block (NFI-FR-03). A regular file is named and placed here, not typed or
 * authored — whatever type it ends up resolving to comes from the project's own
 * classification (NFI-FR-11).
 *
 * The two flows differ in exactly one value: the File menu starts the location at
 * the project root (NFI-FR-06) and the Library context menu starts it at the
 * right-clicked folder (NFI-FR-07). Both leave it editable.
 */
export function NewFileModal({
  seed,
  folders,
  onClose,
  onSubmit,
}: NewFileModalProps) {
  // NFI-FR-04 / NFI-FR-06 / NFI-FR-07: pre-filled, always editable. The empty
  // string is the project root, which is also the File-menu flow's default.
  const [location, setLocation] = useState(seed.initialLocation ?? "");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  // NFI-FR-13: Escape dismisses without creating anything.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  // The seeded location may name a folder the published list does not carry yet —
  // a folder created moments ago, in the window before the list refreshed. Offer
  // it regardless, so a context-menu invocation never opens showing a location
  // other than the folder that was right-clicked.
  const options =
    location !== "" && !folders.some((f) => f.path === location)
      ? [{ path: location, label: location }, ...folders]
      : folders;

  const canCreate = isValidName(name) && !submitting;

  const submit = async () => {
    // `submitting` gates the button too, but the disabled attribute only lands on
    // the next render — two activations inside one tick would otherwise issue two
    // creations, the second failing with a spurious collision on the file the
    // first just created.
    if (!isValidName(name) || submitting) return;
    setSubmitting(true);
    setError(null);
    let res: SubmitResult;
    try {
      res = await onSubmit({
        location: location === "" ? null : location,
        name: name.trim(),
      });
    } catch (e) {
      // `onSubmit` is contracted to resolve a result rather than reject, but a
      // rejection must still leave a usable window rather than a Create button
      // stuck disabled forever with nothing said (NFI-FR-14).
      res = { ok: false, error: String(e) };
    }
    // On success the parent unmounts this modal; only the failure path needs to
    // restore the form (NFI-FR-14: stay open with inputs intact, show the error).
    if (!res.ok) {
      setError(res.error);
      setSubmitting(false);
    }
  };

  return (
    <div
      className="scrim"
      onClick={(e) => {
        // NFI-FR-13: a click on the backdrop (outside the modal frame) cancels.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-labelledby="new-file-title">
        <div className="modal__head">
          <Icon.File size={14} />
          <div className="modal__title" id="new-file-title">
            New File
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
            <label className="picker-field__label" htmlFor="nfi-location">
              Location
            </label>
            <select
              id="nfi-location"
              className="select"
              aria-label="Location"
              value={location}
              onChange={(e) => setLocation(e.target.value)}
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
            <label className="picker-field__label" htmlFor="nfi-name">
              Name
            </label>
            {/* NFI-FR-05: the complete basename, extension included. No separate
                extension control, and nothing is appended to what is typed. */}
            <input
              id="nfi-name"
              className="input input--mono"
              aria-label="Name"
              // NFI NFR: the only value the user must always supply.
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && canCreate) void submit();
              }}
            />
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
