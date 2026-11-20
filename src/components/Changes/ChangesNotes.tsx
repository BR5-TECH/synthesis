/**
 * What the Changes panel states inline beneath its tree: how a push or a
 * commit ended (CHG-FR-43, CHG-FR-46), the repositories a push skipped
 * (CHG-FR-NUZB), and the paths a rollback could not reach (CHG-FR-65).
 */
import { Icon } from "../icons";
import type { PushSkipLine } from "./groups";
import { rollbackFailureText } from "./rollbackText";

export interface ChangesNote {
  tone: "info" | "error";
  text: string;
  /** CHG-FR-NUZB: the repositories the push of this note skipped, with the reason. */
  skipped?: PushSkipLine[];
}

interface ChangesNotesProps {
  note: ChangesNote | null;
  /** CHG-FR-45: no token is stored, so the note routes to where one is added. */
  routesToSettings: boolean;
  onOpenGlobalSettings?: () => void;
  /** CHG-FR-65: absent outside a repository. */
  rollbackFailures: { path: string; kind: string }[];
}

export function ChangesNotes({
  note,
  routesToSettings,
  onOpenGlobalSettings,
  rollbackFailures,
}: ChangesNotesProps) {
  return (
    <>
      {/* CHG-FR-43 / CHG-FR-46: what the panel says about a push. The output
          itself is wide and line-shaped and renders in the Git panel's push/pull
          output area, not here. */}
      {note && (
        <div className="changes-note" data-tone={note.tone} role="status">
          {note.text}
          {/* CHG-FR-45: no token is stored at all, so there is nothing to choose
              between and the note routes to where one is added (GHA-FR-19). */}
          {routesToSettings && onOpenGlobalSettings && (
            <>
              {" "}
              <button
                className="btn btn--ghost btn--sm"
                onClick={() => onOpenGlobalSettings()}
              >
                Open Global settings → GitHub
              </button>
            </>
          )}
          {/* CHG-FR-NUZB: each repository the push skipped, and why. A push
              that skipped none states nothing more. */}
          {note.skipped && note.skipped.length > 0 && (
            <ul className="changes-note__skipped" aria-label="Skipped repositories">
              {note.skipped.map((s) => (
                <li key={s.repository}>
                  Skipped{" "}
                  <span className="changes-note__skipped-repository">{s.label}</span>
                  : {s.reason}.
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {/* CHG-FR-65: what a rollback could not reach, above the count row. No
          success state is shown alongside it — a rollback in which any path
          failed is not a rollback that succeeded. */}
      {rollbackFailures.length > 0 && (
        <div className="changes-rollback-failures" role="status">
          <div className="changes-rollback-failures__summary">
            <Icon.Warning size={12} />
            {rollbackFailures.length}{" "}
            {rollbackFailures.length === 1 ? "file" : "files"} could not be
            rolled back:
          </div>
          <ul className="changes-rollback-failures__list">
            {rollbackFailures.map((f) => (
              <li key={`${f.path}:${f.kind}`}>
                <span className="changes-rollback-failures__path">{f.path}</span>
                <span className="changes-rollback-failures__cause">
                  {rollbackFailureText(f.kind)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}
