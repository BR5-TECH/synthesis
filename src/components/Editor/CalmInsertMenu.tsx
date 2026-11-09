import { CALM_TEMPLATES, type CalmTemplate } from "../../calm/templates";
import { FilterableSelect, type FilterableOption } from "../FilterableSelect";
import { Icon } from "../icons";

/** CAD-FR-HMXS: the accessible name and the tooltip of the button. */
export const INSERT_CALM_LABEL = "Insert CALM architecture";

const OPTIONS: FilterableOption[] = CALM_TEMPLATES.map((template) => ({
  id: template.key,
  label: template.name,
}));
const BY_KEY = new Map(CALM_TEMPLATES.map((template) => [template.key, template]));

/** CAD-FR-JNWR: the filter matches the name of a template, with no regard to case. */
export function matchesCalmTemplate(option: FilterableOption, filter: string): boolean {
  const needle = filter.trim().toLowerCase();
  return needle === "" || option.label.toLowerCase().includes(needle);
}

/**
 * CAD-FR-HMXS / CAD-FR-JNWR / CAD-FR-JWXC: the Insert CALM architecture
 * button of the formatting toolbar and the CALM template menu it opens — the
 * filterable selector of `AII-ai-integrations.md` AII-FR-36 to AII-FR-39, on
 * the terms of the diagram type menu (`EDT-editor.md` EDT-FR-OWNT).
 */
export function CalmInsertMenu({
  disabled,
  onInsert,
}: {
  /** CAD-FR-HMXS: true while the surface accepts no input. */
  disabled: boolean;
  onInsert: (template: CalmTemplate) => void;
}) {
  return (
    <FilterableSelect
      options={OPTIONS}
      value=""
      onChange={(key) => {
        const template = BY_KEY.get(key);
        if (template) onInsert(template);
      }}
      label={INSERT_CALM_LABEL}
      disabled={disabled}
      testId="calm-template"
      matches={matchesCalmTemplate}
      emptyText="Nothing matches"
      fitWindow
      alwaysNotify
      returnFocus={false}
      menuWidth={240}
      menuClassName="diagram-menu"
      className="toolbar-dropdown"
      trigger={{ content: <Icon.Architecture size={14} />, className: "btn btn--ghost btn--sm" }}
    />
  );
}
