/**
 * The **Selected sources** section of the Documents panel
 * (`../../../specifications/ui/DPN-documents-panel.md` DPN-FR-FZFL,
 * DPN-FR-RTLG).
 *
 * Every source in stored order. A row names its kind with the word **File** or
 * **Folder**, shows the path as selected, and for an unavailable source shows
 * the word **Unavailable** followed by the reason. State is always in words.
 * The **Remove** button removes the reference and asks for no confirmation,
 * because no file or folder is deleted.
 */
import { Icon } from "../icons";
import type { DocumentSource, DocumentSourceReason } from "../../types";

const REASONS: Record<DocumentSourceReason, string> = {
  missing: "not found",
  unreadable: "cannot be read",
  link: "symbolic link",
};

/** How many characters of the path's end stay visible when the row is narrow. */
const TAIL_LENGTH = 18;

/**
 * A long path truncates in the middle. The head gives way with an ellipsis and
 * the tail stays, so the end of the path, which names the thing, always shows.
 * The whole path is also in the row's tooltip and in the text a reader hears.
 */
function MiddleTruncated({ path }: { path: string }) {
  const cut = Math.max(0, path.length - TAIL_LENGTH);
  return (
    <span className="documents-source__path" title={path}>
      <span className="documents-source__path-head" aria-hidden="true">
        {path.slice(0, cut)}
      </span>
      <span className="documents-source__path-tail" aria-hidden="true">
        {path.slice(cut)}
      </span>
      <span className="sr-only">{path}</span>
    </span>
  );
}

interface SourcesSectionProps {
  sources: DocumentSource[];
  /** The path whose removal is under way, or null. */
  removing: string | null;
  onRemove: (source: DocumentSource, index: number) => void;
}

export function SourcesSection({
  sources,
  removing,
  onRemove,
}: SourcesSectionProps) {
  return (
    <section className="documents-sources" aria-labelledby="documents-sources-heading">
      <h3 id="documents-sources-heading" className="documents-sources__heading">
        Selected sources
      </h3>
      <ul className="documents-sources__list">
        {sources.map((source, index) => (
          <li
            key={source.path}
            className="documents-source"
            data-unavailable={source.status === "unavailable" || undefined}
          >
            <span className="documents-source__kind">
              {source.kind === "file" ? "File" : "Folder"}
            </span>
            <MiddleTruncated path={source.path} />
            <button
              type="button"
              className="icon-btn documents-source__remove"
              aria-label={`Remove ${source.path}`}
              title="Remove"
              disabled={removing !== null}
              onClick={() => onRemove(source, index)}
            >
              <Icon.X size={12} aria-hidden="true" />
            </button>
            {source.status === "unavailable" && (
              <span className="documents-source__state">
                Unavailable
                {source.reason ? ` — ${REASONS[source.reason]}` : ""}
              </span>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
