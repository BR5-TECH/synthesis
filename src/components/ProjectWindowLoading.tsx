/**
 * The screen of a created project window while it mounts its project
 * (`../specifications/core/PWN-project-windows.md` PWN-FR-KQZB).
 *
 * The window is a project window from its start, so it never shows the Project
 * picker. It names the project it opens. When the mount fails, it shows the
 * failure and a control that closes the window.
 */
interface ProjectWindowLoadingProps {
  name: string;
  /** The failure of the mount; empty while the mount runs. */
  error: string;
  onClose: () => void;
}

export function ProjectWindowLoading({ name, error, onClose }: ProjectWindowLoadingProps) {
  const label = name || "the project";
  return (
    <div className="project-window-loading" aria-busy={error ? undefined : true}>
      <p className="project-window-loading__title" role="status">
        {error ? `Could not open ${label}` : `Opening ${label}…`}
      </p>
      {error && (
        <>
          <p className="project-window-loading__error" role="alert">
            {error}
          </p>
          <button type="button" className="btn btn--sm" onClick={onClose}>
            Close this tab
          </button>
        </>
      )}
    </div>
  );
}
