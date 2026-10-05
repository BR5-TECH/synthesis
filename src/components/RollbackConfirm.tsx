/**
 * CHG-FR-59: the blocking confirmation a rollback opens before anything else
 * happens.
 *
 * A rollback destroys uncommitted work and cannot be undone from the panel, so
 * this surface exists to be read rather than clicked through: it names how many
 * files are affected, lists every one of them by project-relative path — a
 * rename showing both of its identities, a deletion marked as one — and says in
 * words what will be discarded. Dismissing it in any way performs no operation
 * at all (CHG-FR-59), which is why it resolves with a boolean rather than
 * starting anything itself.
 *
 * It is a floating overlay of the main window and is mutually exclusive with the
 * others (per `SNV-shell-navigation.md` SNV-FR-56), so the shell mounts it — the
 * Changes panel asks for it rather than rendering it.
 */
import { useEffect, useRef } from "react";

/** One entry of the rollback set, as the Changes panel hands it over. */
export interface RollbackFile {
  /** Project-relative path — what `rollback_paths` is called with. */
  path: string;
  /**
   * GTC-FR-26: the pre-rename location, for an entry Git reports as renamed.
   * Shown beside the current path because a rollback acts on both identities.
   */
  previousPath?: string | null;
  /** CHG-FR-59: an untracked file is deleted rather than restored — said so. */
  untracked: boolean;
  /** A tracked path the working tree no longer holds; the rollback puts it back. */
  deleted?: boolean;
}

/** How one row of the confirmation describes what will happen to it. */
export function rollbackFateOf(file: RollbackFile): string | null {
  if (file.untracked) return "deleted";
  if (file.previousPath) return `was ${file.previousPath}`;
  if (file.deleted) return "restored";
  return null;
}

interface RollbackConfirmProps {
  files: RollbackFile[];
  /** `true` to proceed, `false` for every route that dismisses (CHG-FR-59). */
  onSettle: (confirmed: boolean) => void;
}

export function RollbackConfirm({ files, onSettle }: RollbackConfirmProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  // CHG-FR-59: focus lands on Cancel rather than on the destructive action, so
  // a reflexive Enter on a modal the author did not expect discards nothing.
  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  // Escape dismisses, performing no operation. Captured on the document so it
  // answers wherever focus sits inside the overlay.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onSettle(false);
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onSettle]);

  const count = files.length;
  const noun = count === 1 ? "file" : "files";

  return (
    <div
      className="scrim"
      // A pointer-down outside the surface dismisses, on the same terms as
      // Escape: nothing is performed and every check and buffer is preserved.
      onMouseDown={(e) => {
        if (!surfaceRef.current?.contains(e.target as Node)) onSettle(false);
      }}
    >
      <div
        className="modal rollback-confirm"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rollback-confirm-title"
        aria-describedby="rollback-confirm-warning"
        ref={surfaceRef}
      >
        <div className="modal__head">
          <h2 id="rollback-confirm-title" className="modal__title">
            Discard {count} {noun}?
          </h2>
        </div>
        <div className="modal__body">
          {/* CHG-FR-59: the warning names all four things that go, because three
              of them are invisible from the tree the author was just looking
              at. */}
          <p id="rollback-confirm-warning" className="rollback-confirm__warning">
            {count === 1 ? "This file returns" : "These files return"} to{" "}
            <code>HEAD</code>. Their tracked and staged changes are discarded,
            untracked files are deleted, and unsaved Editor and Diff edits are
            thrown away. This cannot be undone from the panel.
          </p>
          <ul className="rollback-confirm__paths">
            {files.map((f) => {
              const fate = rollbackFateOf(f);
              return (
                <li key={f.path} className="rollback-confirm__path">
                  {/* The full path in the tooltip: this dialog exists to name
                      what is about to be destroyed, and a path clipped by the
                      dialog's own width would otherwise be unrecoverable. */}
                  <span className="rollback-confirm__path-name" title={f.path}>
                    {f.path}
                  </span>
                  {fate && (
                    <span
                      className="rollback-confirm__path-fate"
                      title={f.previousPath ?? undefined}
                    >
                      ({fate})
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
        <div className="modal__actions">
          <button
            className="btn btn--sm"
            ref={cancelRef}
            onClick={() => onSettle(false)}
          >
            Cancel
          </button>
          <button
            className="btn btn--sm btn--danger"
            onClick={() => onSettle(true)}
          >
            Discard {count} {noun}
          </button>
        </div>
      </div>
    </div>
  );
}
