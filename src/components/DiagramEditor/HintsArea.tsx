/**
 * EDT-FR-FKUH – EDT-FR-NVGA: the hints area of the diagram editor overlay — a
 * read-only reference of the syntax of the chart type of the source.
 *
 * It holds text only: no control, no insert, and no copy action. The author
 * can select its text, and the keyboard can focus it to scroll it.
 */
import { useMemo } from "react";

import type { DiagramKind } from "../diagramKind";
import { hintsFor } from "./hints";
import { CALM_HINT_GROUPS, CALM_HINTS_TITLE } from "./hints/calmHints";

/** EDT-FR-HVGG: the accessible name of the hints area. */
export const HINTS_LABEL = "Mermaid syntax hints";
/** CAD-FR-KWIY: the accessible name of the hints area on a CALM fence. */
export const CALM_HINTS_LABEL = "CALM syntax hints";

export function HintsArea({
  source,
  diagram = "mermaid",
}: {
  source: string;
  diagram?: DiagramKind;
}) {
  // EDT-FR-NWZX: the hints follow the source while the author types.
  // CAD-FR-KWIY: a CALM fence has one hint set.
  const shown = useMemo(
    () =>
      diagram === "calm"
        ? { title: CALM_HINTS_TITLE, key: "calm", groups: CALM_HINT_GROUPS }
        : hintsFor(source),
    [source, diagram],
  );
  return (
    <section
      className="diagram-hints"
      role="region"
      aria-label={diagram === "calm" ? CALM_HINTS_LABEL : HINTS_LABEL}
      tabIndex={0}
      data-chart-type={shown.key ?? "general"}
    >
      {/* EDT-FR-YNQF: the name of the chart type above the list. */}
      <h2 className="diagram-hints__title">{shown.title}</h2>
      <div className="diagram-hints__groups">
        {shown.groups.map((group) => (
          <section key={group.title} className="diagram-hints__group" aria-label={group.title}>
            <h3 className="diagram-hints__group-title">{group.title}</h3>
            <dl className="diagram-hints__list">
              {group.hints.map((hint) => (
                <div key={`${hint.code}\u0000${hint.text}`} className="diagram-hints__item">
                  {/* EDT-FR-NVGA: a snippet is code text in the Source code role. */}
                  <dt className="diagram-hints__code">
                    <code>{hint.code}</code>
                  </dt>
                  <dd className="diagram-hints__text">{hint.text}</dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </section>
  );
}
