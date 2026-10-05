import { useEffect, useMemo, useState } from "react";
import {
  FONT_ROLE_DEFAULTS,
  FONT_ROLE_LABELS,
  FONT_ROLE_ORDER,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  LINE_HEIGHT_MAX,
  LINE_HEIGHT_MIN,
  fontFamilyValue,
  isFontSizeInRange,
  isLineHeightInRange,
  withFontRole,
} from "../state/fontRoles";
import type {
  FontFamily,
  FontRole,
  FontRoleKey,
  FontSettings,
} from "../types";

// The Appearance section's typography controls (GLS-global-settings.md
// GLS-FR-17..GLS-FR-23). The roles themselves are defined by
// `OVW-overview.md` OVW-FR-13 and the family list comes from
// `../core/FNT-font-enumeration.md`.

/** The value the built-in-default option carries. Empty means "unset". */
const BUILT_IN = "";

/**
 * GLS-FR-21: what each role's specimen is set in. Chosen to show what the role
 * actually governs — chrome text for UI, a sentence of prose for Rich Markdown,
 * and a line of code for Source, including the glyphs a reader judges a coding
 * face by.
 */
const SPECIMENS: Record<FontRoleKey, string> = {
  ui: "Project · Notes · Comments — 24 artifacts, 3 unresolved",
  rich: "The specification describes what the surface is once built.",
  source: "const total = items.filter(Boolean).length; // 0O 1lI",
};

/** What each role governs, in the one line the control group is labelled with. */
const ROLE_DESCRIPTIONS: Record<FontRoleKey, string> = {
  ui: "Panels, tabs, chrome, modals, the status bar, and comment and note bodies.",
  rich: "The prose of a rendered Markdown document — the editor's rich mode and a diff's rich rendering.",
  source:
    "Source text and code — the editor's text mode, code blocks and frontmatter, and a diff's source rendering.",
};

interface FontRoleControlsProps {
  /** The persisted settings, or `undefined` before they have loaded. */
  fonts: FontSettings | undefined;
  /**
   * The installed families (FNT-FR-02), or `null` while the query is in flight.
   * An **empty** list is a real answer, not a failure: a platform whose font set
   * cannot be enumerated yields one (FNT-FR-07), and the controls still offer
   * the built-in faces.
   */
  families: FontFamily[] | null;
  /**
   * GLS-FR-20: apply app-wide and persist. Called with the whole record so the
   * caller performs one whole-record write (GSS-FR-20) rather than reassembling
   * the other two roles.
   */
  onChange: (next: FontSettings) => void;
}

export function FontRoleControls({
  fonts,
  families,
  onChange,
}: FontRoleControlsProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
      {FONT_ROLE_ORDER.map((role) => (
        <RoleGroup
          key={role}
          role={role}
          setting={fonts?.[role] ?? {}}
          families={families}
          onChange={(next) => onChange(withFontRole(fonts, role, next))}
        />
      ))}
      {/* GLS-FR-23: the section's own reset, returning all three roles at once
          — and applying on GLS-FR-20's terms like any other change. */}
      <div>
        <button
          className="btn btn--default btn--sm"
          onClick={() => onChange({})}
        >
          Reset all fonts
        </button>
      </div>
    </div>
  );
}

interface RoleGroupProps {
  role: FontRoleKey;
  setting: FontRole;
  families: FontFamily[] | null;
  onChange: (next: FontRole) => void;
}

function RoleGroup({ role, setting, families, onChange }: RoleGroupProps) {
  const defaults = FONT_ROLE_DEFAULTS[role];

  /**
   * Whether the stored family is missing from the offered list — for any
   * reason, including that there is no list yet or that enumeration returned
   * nothing (FNT-FR-07).
   *
   * This is what decides whether the control has to carry an option of its own,
   * and it is deliberately *not* the same question as `unavailable` below. A
   * `<select>` whose value matches no option silently falls back to the first
   * one, so without this the control would show the built-in face while the
   * record stores something else — the control lying about the stored
   * selection, which GLS-FR-22 exists to prevent.
   */
  const chosenFamily = setting.family?.trim() || undefined;
  const chosenIsListed =
    !!chosenFamily && !!families && families.some((f) => f.family === chosenFamily);
  const needsOwnOption = !!chosenFamily && !chosenIsListed;

  /**
   * GLS-FR-22: a stored family the machine no longer reports — *marked* as
   * unavailable. Only decidable against a **non-empty** list: an empty one
   * means the platform could not be enumerated at all (FNT-FR-07), and telling
   * the user their font is uninstalled on the strength of that would be
   * reporting our own blindness as their missing font. So the option is still
   * rendered above; it just is not accused of anything.
   */
  const unavailable = needsOwnOption && !!families && families.length > 0;

  // GLS-FR-18: fixed-width families first and marked as such, the rest below
  // rather than withheld — so a family the platform describes imprecisely is
  // still choosable for code. Only the Source code role groups them; for the
  // other two the distinction says nothing about the choice.
  const { fixed, proportional } = useMemo(() => {
    const list = families ?? [];
    if (role !== "source") return { fixed: [], proportional: list };
    return {
      fixed: list.filter((f) => f.monospace),
      proportional: list.filter((f) => !f.monospace),
    };
  }, [families, role]);

  const resolvedSize = setting.sizePx ?? defaults.sizePx;
  const resolvedLineHeight = setting.lineHeight ?? defaults.lineHeight;
  const isDefault =
    setting.family === undefined &&
    setting.sizePx === undefined &&
    setting.lineHeight === undefined;

  return (
    <div
      className="card"
      data-testid={`font-role-${role}`}
      style={{ padding: "14px 16px", display: "flex", flexDirection: "column", gap: 10 }}
    >
      <div style={{ display: "flex", alignItems: "baseline", gap: 10 }}>
        <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
          {FONT_ROLE_LABELS[role]}
        </div>
        {/* GLS-FR-23: the per-role reset. Marked unavailable while the role
            already carries the built-in defaults, so the control says whether
            there is anything to undo rather than only what it would do.
            `aria-disabled` rather than `disabled`: activating it is what makes
            it inert, and a control that removes itself from the tab order at
            the moment it is used drops a keyboard user back to the top of the
            document. This keeps the focus where the user put it. */}
        <button
          className="btn btn--ghost btn--sm"
          style={{
            marginLeft: "auto",
            opacity: isDefault ? 0.45 : undefined,
            cursor: isDefault ? "default" : undefined,
          }}
          aria-disabled={isDefault || undefined}
          aria-label={`Reset ${FONT_ROLE_LABELS[role]} font`}
          onClick={() => {
            if (isDefault) return;
            onChange({});
          }}
        >
          Reset
        </button>
      </div>
      <p className="t-ui-sm t-muted" style={{ margin: 0 }}>
        {ROLE_DESCRIPTIONS[role]}
      </p>

      <div style={{ display: "flex", gap: 10, alignItems: "flex-end", flexWrap: "wrap" }}>
        <label style={{ flex: "1 1 240px", minWidth: 0, display: "block" }}>
          <span className="t-label" style={{ display: "block", marginBottom: 4 }}>
            Family
          </span>
          <select
            className="select select--sm"
            style={{ width: "100%" }}
            aria-label={`${FONT_ROLE_LABELS[role]} font family`}
            data-unavailable={unavailable || undefined}
            value={setting.family ?? BUILT_IN}
            onChange={(e) => {
              const value = e.target.value;
              onChange({
                ...setting,
                family: value === BUILT_IN ? undefined : value,
              });
            }}
          >
            {/* FNT-FR-08: the built-in faces are always available and are
                offered independently of whatever the enumeration returns. */}
            <option value={BUILT_IN}>{defaults.builtInLabel}</option>
            {/* GLS-FR-22: the stored family stays the selection whenever the
                offered list does not carry it — the absence alone rewrites
                nothing, so reinstalling the font restores the role with no
                reselection. It is only *labelled* missing when we actually
                enumerated and did not find it; when we could not enumerate at
                all, it is offered plainly. */}
            {needsOwnOption && chosenFamily && (
              <option value={chosenFamily}>
                {unavailable ? `${chosenFamily} — not installed` : chosenFamily}
              </option>
            )}
            {fixed.length > 0 && (
              <optgroup label="Fixed width">
                {fixed.map((f) => (
                  <option key={f.family} value={f.family}>
                    {f.family}
                  </option>
                ))}
              </optgroup>
            )}
            {proportional.length > 0 &&
              (role === "source" ? (
                <optgroup label="Proportional">
                  {proportional.map((f) => (
                    <option key={f.family} value={f.family}>
                      {f.family}
                    </option>
                  ))}
                </optgroup>
              ) : (
                proportional.map((f) => (
                  <option key={f.family} value={f.family}>
                    {f.family}
                  </option>
                ))
              ))}
          </select>
        </label>

        <NumberField
          label="Size"
          unit="px"
          ariaLabel={`${FONT_ROLE_LABELS[role]} font size`}
          value={resolvedSize}
          min={FONT_SIZE_MIN}
          max={FONT_SIZE_MAX}
          step={1}
          isInRange={isFontSizeInRange}
          onCommit={(v) => onChange({ ...setting, sizePx: v })}
        />
        <NumberField
          label="Line height"
          ariaLabel={`${FONT_ROLE_LABELS[role]} line height`}
          value={resolvedLineHeight}
          min={LINE_HEIGHT_MIN}
          max={LINE_HEIGHT_MAX}
          step={0.05}
          isInRange={isLineHeightInRange}
          onCommit={(v) => onChange({ ...setting, lineHeight: v })}
        />
      </div>

      {unavailable && (
        <span className="t-ui-xs" style={{ color: "var(--warn, #d9a13b)" }}>
          {chosenFamily} is not installed on this machine. This role reads in
          the built-in face until the font is back.
        </span>
      )}

      {/* GLS-FR-21: a specimen in this role's current family, size, and line
          height, so the effect of a choice is legible without leaving the tab.
          Set inline rather than through the root's custom properties: the
          specimen has to show the role being edited, including a value the
          section is still deciding whether to commit. */}
      <div
        data-testid={`font-specimen-${role}`}
        style={{
          fontFamily: fontFamilyValue(role, setting.family),
          fontSize: `${resolvedSize}px`,
          lineHeight: resolvedLineHeight,
          background: "var(--bg-sunken)",
          border: "1px solid var(--border-1)",
          borderRadius: "var(--r-sm)",
          padding: "8px 10px",
          color: "var(--fg-1)",
        }}
      >
        {SPECIMENS[role]}
      </div>
    </div>
  );
}

interface NumberFieldProps {
  label: string;
  ariaLabel: string;
  unit?: string;
  value: number;
  min: number;
  max: number;
  step: number;
  isInRange: (v: number) => boolean;
  onCommit: (v: number) => void;
}

/**
 * A bounded numeric control (GLS-FR-19).
 *
 * The typed text is held locally so a value passing *through* the invalid range
 * on its way somewhere valid — `1` while typing `18` — is not committed and
 * does not re-typeset the app. Only a value inside the range reaches
 * `onCommit`, so `save_app_preferences` is never invoked with one outside it
 * and the application's text is left alone (GLS-FR-19).
 */
function NumberField({
  label,
  ariaLabel,
  unit,
  value,
  min,
  max,
  step,
  isInRange,
  onCommit,
}: NumberFieldProps) {
  const [text, setText] = useState(String(value));

  // Adopt a value changed from outside this field — a reset (GLS-FR-23), or the
  // persisted record arriving after first paint.
  useEffect(() => {
    setText(String(value));
  }, [value]);

  const parsed = Number(text.trim());
  const invalid = text.trim() === "" || !isInRange(parsed);

  return (
    <label style={{ flex: "0 0 auto", display: "block" }}>
      <span className="t-label" style={{ display: "block", marginBottom: 4 }}>
        {label}
        {unit ? ` (${unit})` : ""}
      </span>
      <input
        className="input input--sm"
        type="number"
        aria-label={ariaLabel}
        aria-invalid={invalid || undefined}
        data-invalid={invalid || undefined}
        min={min}
        max={max}
        step={step}
        style={{
          width: 84,
          borderColor: invalid ? "var(--danger, #d9534f)" : undefined,
        }}
        value={text}
        onChange={(e) => {
          const next = e.target.value;
          setText(next);
          const n = Number(next.trim());
          // GLS-FR-19: out of range is simply not committed. Nothing is
          // persisted and nothing is applied — the field alone carries the
          // rejected text, so the app never renders a value it would refuse.
          if (next.trim() !== "" && isInRange(n)) onCommit(n);
        }}
        // A field left holding a rejected value would keep saying the role is
        // something it is not, so leaving it restores the committed value.
        onBlur={() => {
          if (invalid) setText(String(value));
        }}
      />
      {invalid && (
        <span
          className="t-ui-xs"
          style={{ display: "block", marginTop: 2, color: "var(--danger, #d9534f)" }}
        >
          {min}–{max}
        </span>
      )}
    </label>
  );
}
