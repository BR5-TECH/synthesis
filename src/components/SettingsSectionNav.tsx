import { useEffect, useRef } from "react";

import type { SettingsScope } from "../settingsWindow";

/**
 * The side nav of the Settings window
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-QXTV, SWN-FR-KBNV).
 *
 * One nav holds both groups: **Global** first, then **Project**, each under its
 * own label and a visible divider between them. Exactly one section is
 * presented at a time. A section that exists in both groups (Plugins, Agents,
 * Docker, GitHub, and the adapters) appears once in each, so a row is told apart
 * by the group it stands in — visibly by the label and the divider, and for
 * assistive technology by the name each group's list carries.
 *
 * **Every row is a real button.** The window has no tab strip, no activity bar,
 * and no menu bar of its own (per `SWN-settings-windows.md` SWN-FR-01), so this
 * nav is the *only* way to reach another section — and a `div` with a click
 * handler is reachable by pointer alone.
 *
 * Keyboard model is the standard vertical tablist across the whole nav: one tab
 * stop (the selected row), with the arrow keys moving between rows and over the
 * divider into the next group. That is what keeps a nav of twenty-two sections
 * from costing twenty-two presses to tab past.
 */
/**
 * The element ids that tie the nav to the one content region it controls
 * (SWN-FR-KBNV). The window and the nav derive them from one base id, so a row
 * names the region it controls and the region is named by its group and its
 * section.
 */
export function settingsTabId(
  base: string,
  scope: SettingsScope,
  section: string,
): string {
  return `${base}-tab-${scope}-${section}`;
}

export function settingsGroupLabelId(base: string, scope: SettingsScope): string {
  return `${base}-group-${scope}`;
}

export function settingsPanelId(base: string): string {
  return `${base}-panel`;
}

export interface SettingsNavGroup {
  scope: SettingsScope;
  /** The visible label, and the accessible name of the group's list. */
  label: string;
  /** `[key, label]` in render order. */
  sections: ReadonlyArray<readonly [string, string, ...unknown[]]>;
}

export interface SettingsSelection {
  scope: SettingsScope;
  section: string;
}

export interface SettingsSectionNavProps {
  /** The base of the ids this nav shares with the region it controls. */
  idBase: string;
  groups: ReadonlyArray<SettingsNavGroup>;
  selected: SettingsSelection;
  onSelect: (selection: SettingsSelection) => void;
  /**
   * Move the keyboard focus onto the selected row when the nav first mounts, so
   * focus is inside the window as soon as it opens (SWN-FR-FCRT).
   */
  autoFocus?: boolean;
}

export function SettingsSectionNav({
  idBase,
  groups,
  selected,
  onSelect,
  autoFocus = false,
}: SettingsSectionNavProps) {
  const refs = useRef(new Map<string, HTMLButtonElement | null>());
  const keyOf = (scope: SettingsScope, section: string) => `${scope}/${section}`;

  useEffect(() => {
    if (!autoFocus) return;
    refs.current.get(keyOf(selected.scope, selected.section))?.focus();
    // Mount-only: the focus belongs to the opening of the window, and a route
    // to another section afterwards must not pull it back from where the
    // author put it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The rows of every group in the order they are read, so an arrow key crosses
  // the divider instead of stopping at the end of a group.
  const flat = groups.flatMap((group) =>
    group.sections.map(([section]) => ({ scope: group.scope, section })),
  );

  const onKeyDown = (event: React.KeyboardEvent, index: number) => {
    const step =
      event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowUp"
          ? -1
          : event.key === "Home"
            ? -index
            : event.key === "End"
              ? flat.length - 1 - index
              : null;
    if (step === null) return;
    event.preventDefault();
    const next = flat[(index + step + flat.length) % flat.length];
    if (!next) return;
    onSelect(next);
    refs.current.get(keyOf(next.scope, next.section))?.focus();
  };

  return (
    <nav
      aria-label="Settings sections"
      data-testid="settings-nav"
      style={{
        borderRight: "1px solid var(--border-1)",
        background: "var(--bg-panel)",
        padding: "12px 8px",
        // The window has a minimum size and a section list can outgrow it at a
        // large UI role, so the nav scrolls rather than pushing its last rows
        // out of the frame.
        overflowY: "auto",
      }}
    >
      {groups.map((group, groupIndex) => {
        const labelId = settingsGroupLabelId(idBase, group.scope);
        return (
          <div
            key={group.scope}
            data-testid={`settings-group-${group.scope}`}
            style={
              groupIndex === 0
                ? undefined
                : {
                    // SWN-FR-QXTV: the visible boundary between the scopes.
                    borderTop: "1px solid var(--border-1)",
                    marginTop: 12,
                    paddingTop: 12,
                  }
            }
          >
            <div
              id={labelId}
              className="t-eyebrow"
              style={{ padding: "0 10px 6px" }}
            >
              {group.label}
            </div>
            <div
              role="tablist"
              aria-orientation="vertical"
              aria-labelledby={labelId}
            >
              {group.sections.map(([section, text]) => {
                const key = keyOf(group.scope, section);
                const isSelected =
                  selected.scope === group.scope && selected.section === section;
                const index = flat.findIndex(
                  (row) => row.scope === group.scope && row.section === section,
                );
                return (
                  <button
                    key={key}
                    id={settingsTabId(idBase, group.scope, section)}
                    type="button"
                    ref={(el) => {
                      refs.current.set(key, el);
                    }}
                    className="menu-item"
                    role="tab"
                    aria-selected={isSelected}
                    aria-controls={settingsPanelId(idBase)}
                    // SWN-FR-KBNV: the presented section is the current one for
                    // assistive technology.
                    aria-current={isSelected ? "page" : undefined}
                    data-active={isSelected}
                    // One tab stop for the nav, on the row that is selected.
                    tabIndex={isSelected ? 0 : -1}
                    style={{
                      // A floor rather than a fixed height: a label that wraps
                      // to two lines needs two line-heights, and a pinned row
                      // would let the second paint over the row below it once
                      // the UI role outgrows the box — and this nav is the one
                      // control an author needs in order to get back and undo
                      // the size they just chose (GLS-FR-19).
                      minHeight: "var(--ctl-h-lg)",
                      width: "100%",
                      // A button carries a border, a background, a font and an
                      // alignment of its own; the row is styled by `.menu-item`
                      // and those would fight it.
                      border: 0,
                      textAlign: "left",
                      font: "inherit",
                      color: "inherit",
                      background: isSelected ? "var(--bg-active)" : "transparent",
                    }}
                    onClick={() => onSelect({ scope: group.scope, section })}
                    onKeyDown={(event) => onKeyDown(event, index)}
                  >
                    {text}
                  </button>
                );
              })}
            </div>
          </div>
        );
      })}
    </nav>
  );
}
