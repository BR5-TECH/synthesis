/**
 * CHG-FR-61: the pseudo-kind a save failure carried into the rollback report
 * takes, so it renders beside the backend's own typed causes without pretending
 * to be one of them.
 */
export const SAVE_FAILED_KIND = "unsaved_edit_lost";

/**
 * CHG-FR-65: render a typed rollback failure as a sentence.
 *
 * The kinds are the ones `../specifications/core/GTC-git.md` GTC-FR-25 returns.
 * An unrecognised kind is shown as it arrived rather than swallowed — a cause
 * the panel cannot name is still a cause the author needs to see.
 */
export function rollbackFailureText(kind: string): string {
  switch (kind) {
    case SAVE_FAILED_KIND:
      return "an unsaved edit could not be written before it was discarded";
    case "permission_denied":
      return "permission denied";
    case "not_found":
      return "no longer exists";
    case "is_directory":
      return "is a folder, not a file";
    case "path_outside_content_root":
      return "lies outside the project";
    case "write_failed":
      return "could not be written";
    default:
      return kind;
  }
}
