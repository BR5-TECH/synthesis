import { useEffect, useRef, useState } from "react";
import { ARTIFACT_TYPES } from "../artifactTypes";
import type { FolderOption } from "../hooks/useProjectFolders";
import { Icon } from "./icons";
import type { ArtifactType, NewTypedArtifactSeed } from "../types";

// NTA-FR-09: a creatable name is non-empty and carries no path separator. One
// invocation therefore creates exactly one file rather than a file plus the
// folders leading to it — the same rule the backend enforces (PST-FR-29,
// mirroring PST-FR-26), checked client-side so Create is disabled before any
// call is made.
//
// Deliberately says nothing about extensions (NTA-FR-05): the name is the
// file's complete basename, so `overview.md`, `login.flow`, and `Makefile` are
// all expressible, nothing is appended, and no extension is derived from the
// chosen type or rejected for not matching it.
function isValidName(name: string): boolean {
  const t = name.trim();
  return t !== "" && !t.includes("/") && !t.includes("\\");
}

export type NewTypedArtifactPayload = {
  location: string | null;
  name: string;
  artifactType: ArtifactType;
};

export type SubmitResult = { ok: true } | { ok: false; error: string };

interface NewTypedArtifactModalProps {
  // The invocation context. The modal is mounted only while creating, so this is
  // always present (a closed modal is unmounted by the parent).
  seed: NewTypedArtifactSeed;
  /**
   * Every folder in the project, for the Location select (NTA-FR-04). Supplied
   * by the shell from the tree the Project panel already holds — the window
   * never scans on its own, so opening it issues no backend call (NTA contract
   * boundary / NFR).
   */
  folders: FolderOption[];
  onClose: () => void;
  // Performs the creation; resolves ok on success (the parent then closes, opens
  // the file in its natural surface, and reveals it) or an error to show inline
  // (the modal stays open with every input as the user left it).
  onSubmit: (payload: NewTypedArtifactPayload) => Promise<SubmitResult>;
}

/**
 * The New Artifact modal (NTA-new-typed-artifact.md). A centered overlay
 * (reusing the shared `.scrim`/`.modal` chrome, NTA-FR-01 / NTA-FR-02)
 * presenting Location, Name, and Artifact Type and nothing else: no Markdown
 * editor, no initial-content editor, no AI-prompt block, no draft control, and
 * no draft-template control (NTA-FR-03). An artifact is named, placed, and
 * typed here and authored in the surface it opens in.
 *
 * It is the typed sibling of the New File window (`NFI-new-file.md`) and is
 * built from that window's primitives, so the two read as one system. It is
 * **not** the New Artifact *tab*, which is a draft's workspace (NAW-FR-01):
 * this window creates a project file, never a draft, and never reads the
 * project's draft template (NTA-FR-14).
 *
 * The two flows differ in exactly one value: the File menu starts the location
 * at the project root (NTA-FR-07) and the Project context menu starts it at the
 * right-clicked folder (NTA-FR-08). Both leave it editable, and both leave the
 * artifact type unchosen — it is never seeded, not even from a location that
 * carries a folder-scope assignment of its own (NTA-FR-06).
 */
export function NewTypedArtifactModal({
  seed,
  folders,
  onClose,
  onSubmit,
}: NewTypedArtifactModalProps) {
  // NTA-FR-04 / NTA-FR-07 / NTA-FR-08: pre-filled, always editable. The empty
  // string is the project root, which is also the File-menu flow's default.
  const [location, setLocation] = useState(seed.initialLocation ?? "");
  const [name, setName] = useState("");
  // NTA-FR-06: no unset entry and no default. The control holds no type until
  // the user picks one, so this flow never creates an untyped file — that is
  // the New File window's job (NFI-FR-11).
  const [type, setType] = useState<ArtifactType | "">("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const frameRef = useRef<HTMLDivElement | null>(null);
  /**
   * NTA-FR-15 / NTA NFR: the surface the window was invoked from, so dismissal
   * returns focus to it rather than to the document body — which would leave a
   * keyboard author's next keystroke going nowhere.
   *
   * Captured on mount, before `autoFocus` moves focus into the Name input.
   */
  const invokedFrom = useRef<HTMLElement | null>(
    typeof document === "undefined"
      ? null
      : (document.activeElement as HTMLElement | null),
  );

  useEffect(() => {
    const restore = invokedFrom.current;
    return () => {
      // Only if it is still in the document: a context menu that unmounted
      // behind the window has nothing left to focus, and focusing a detached
      // node silently moves focus to the body instead.
      if (restore?.isConnected) restore.focus();
    };
  }, []);

  // NTA-FR-15: Escape dismisses without creating anything. NTA NFR: Tab is
  // trapped inside the frame, so the whole window is operable from the keyboard
  // without leaving it (NTA-FR-15).
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
        return;
      }
      if (e.key !== "Tab") return;
      const frame = frameRef.current;
      if (!frame) return;
      const focusable = [
        ...frame.querySelectorAll<HTMLElement>(
          "button:not([disabled]), select, input, [tabindex]:not([tabindex='-1'])",
        ),
      ];
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const active = document.activeElement as HTMLElement | null;
      // Wrapping is claimed here rather than left to the browser, which would
      // walk on to the surfaces behind the scrim.
      if (e.shiftKey && (active === first || !frame.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (active === last || !frame.contains(active))) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  // The seeded location may name a folder the published list does not carry yet
  // — a folder created moments ago, in the window before the list refreshed.
  // Offer it regardless, so a context-menu invocation never opens showing a
  // location other than the folder that was right-clicked.
  const options =
    location !== "" && !folders.some((f) => f.path === location)
      ? [{ path: location, label: location }, ...folders]
      : folders;

  // NTA-FR-09 / NTA-FR-10: Create is disabled while the name is empty or
  // invalid, while no type has been chosen, and while a call is in flight — so
  // one confirmation is one call.
  const canCreate = isValidName(name) && type !== "" && !submitting;

  const submit = async () => {
    // `submitting` gates the button too, but the disabled attribute only lands
    // on the next render — two activations inside one tick would otherwise
    // issue two creations, the second failing with a spurious collision on the
    // file the first just created.
    if (!isValidName(name) || type === "" || submitting) return;
    setSubmitting(true);
    setError(null);
    let res: SubmitResult;
    try {
      res = await onSubmit({
        location: location === "" ? null : location,
        name: name.trim(),
        artifactType: type,
      });
    } catch (e) {
      // `onSubmit` is contracted to resolve a result rather than reject, but a
      // rejection must still leave a usable window rather than a Create button
      // stuck disabled forever with nothing said (NTA-FR-16).
      res = { ok: false, error: String(e) };
    }
    // On success the parent unmounts this modal; only the failure path needs to
    // restore the form (NTA-FR-16: stay open with all three inputs exactly as
    // the user left them, show the error inline).
    if (!res.ok) {
      setError(res.error);
      setSubmitting(false);
    }
  };

  return (
    <div
      className="scrim"
      onClick={(e) => {
        // NTA-FR-15: a click on the backdrop (outside the modal frame) cancels.
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="nta-title"
        aria-describedby={error ? "nta-error" : undefined}
        ref={frameRef}
      >
        <div className="modal__head">
          <Icon.Plus size={14} />
          <div className="modal__title" id="nta-title">
            New Artifact
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
            <label className="picker-field__label" htmlFor="nta-location">
              Location
            </label>
            {/* NTA-FR-04: the project root and every folder under it, including
                the ones the Project panel's active lens hides, so an artifact
                may be created inside an empty or unclassified one. */}
            <select
              id="nta-location"
              className="select"
              aria-label="Location (required)"
              required
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
            <label className="picker-field__label" htmlFor="nta-name">
              Name
            </label>
            {/* NTA-FR-05: the complete basename, extension included. No separate
                extension control, nothing appended to what is typed, and no
                extension derived from the chosen type. */}
            <input
              id="nta-name"
              className="input input--mono"
              aria-label="Name (required)"
              required
              // NTA NFR: the first value the user must supply, so the window
              // opens with focus here.
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && canCreate) void submit();
              }}
            />
          </div>

          <div className="picker-field">
            <label className="picker-field__label" htmlFor="nta-type">
              Artifact Type
            </label>
            {/* NTA-FR-06: exactly the eight built-in types (ASC-FR-02) and
                nothing else. Unlike the New Folder window's own type control
                (NFW-FR-05) there is no unset entry — this window's whole purpose
                is to settle the type, so the placeholder below is a prompt to
                choose rather than a selectable value. */}
            <select
              id="nta-type"
              className="select"
              aria-label="Artifact Type (required)"
              required
              value={type}
              onChange={(e) => setType(e.target.value as ArtifactType | "")}
            >
              <option value="" disabled>
                Select a type
              </option>
              {ARTIFACT_TYPES.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </select>
          </div>

          {error && (
            <div
              id="nta-error"
              style={{
                fontSize: "var(--fs-ui-sm)",
                color: "var(--danger, #e5484d)",
              }}
              role="alert"
            >
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
