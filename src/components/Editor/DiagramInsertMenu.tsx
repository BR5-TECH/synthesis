import { FilterableSelect, type FilterableOption } from "../FilterableSelect";
import { Icon } from "../icons";
import { DIAGRAM_TEMPLATES, type DiagramTemplate } from "./diagramTemplates";

const OPTIONS: FilterableOption[] = DIAGRAM_TEMPLATES.map((template) => ({
  id: template.key,
  label: template.name,
}));
const BY_KEY = new Map(DIAGRAM_TEMPLATES.map((template) => [template.key, template]));

/**
 * EDT-FR-OWNT: the filter matches the name of the chart type and the Mermaid
 * keyword that declares it, with no regard to case.
 */
export function matchesDiagramType(option: FilterableOption, filter: string): boolean {
  const needle = filter.trim().toLowerCase();
  if (!needle) return true;
  const keyword = BY_KEY.get(option.id)?.keyword ?? "";
  return (
    option.label.toLowerCase().includes(needle) ||
    keyword.toLowerCase().includes(needle)
  );
}

/**
 * EDT-FR-VUGQ / EDT-FR-SCSP / EDT-FR-OWNT: the Insert diagram button of the
 * formatting toolbar and the diagram type menu it opens — the filterable
 * selector of `AII-ai-integrations.md` AII-FR-36 to AII-FR-39.
 *
 * The filter box takes focus while the menu is open. The editor keeps its
 * selection while it does not have focus, so the fence goes where the caret
 * was, and the insert gives focus back to the editor.
 */
export function DiagramInsertMenu({
  disabled,
  onInsert,
}: {
  /** EDT-FR-VUGQ: true while the surface accepts no input. */
  disabled: boolean;
  onInsert: (template: DiagramTemplate) => void;
}) {
  return (
    <FilterableSelect
      options={OPTIONS}
      value=""
      onChange={(key) => {
        const template = BY_KEY.get(key);
        if (template) onInsert(template);
      }}
      label="Insert diagram"
      disabled={disabled}
      testId="diagram-type"
      matches={matchesDiagramType}
      emptyText="Nothing matches"
      fitWindow
      alwaysNotify
      returnFocus={false}
      menuWidth={240}
      menuClassName="diagram-menu"
      className="toolbar-dropdown"
      trigger={{ content: <Icon.Diagram size={14} />, className: "btn btn--ghost btn--sm" }}
    />
  );
}
