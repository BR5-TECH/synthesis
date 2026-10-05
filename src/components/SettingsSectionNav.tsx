import { useRef } from "react";

/**
 * The section navigation both settings windows are divided by
 * (`../../specifications/ui/GLS-global-settings.md` GLS-FR-03,
 * `../../specifications/ui/SET-project-settings.md` SET-FR-03).
 *
 * One component for the two windows, because they are the same control: a
 * vertical list where exactly one section is presented at a time. Sharing it is
 * also what keeps them from drifting apart in the ways two hand-written copies
 * did — one carrying `role="tab"` and the other nothing, one sized by the UI
 * role and the other pinned to 30 pixels.
 *
 * **Every row is a real button.** A settings window has no tab strip, no
 * activity bar, and no menu bar of its own (per
 * `../../specifications/ui/SWN-settings-windows.md` SWN-FR-01), so this nav is
 * the *only* way to reach another section — and a `div` with a click handler is
 * reachable by pointer alone. A keyboard author would be able to operate the
 * section they landed on and nothing else.
 *
 * Keyboard model is the standard vertical tablist: one tab stop for the whole
 * nav (the selected row), with the arrow keys moving between rows. That is what
 * keeps a nav of ten sections from costing ten presses to tab past.
 */
export interface SettingsSectionNavProps<K extends string> {
  /** `[key, label]` in render order. */
  sections: ReadonlyArray<readonly [K, string, ...unknown[]]>;
  selected: K;
  onSelect: (key: K) => void;
  /** Names the list for a screen reader — "Global settings sections". */
  label: string;
}

export function SettingsSectionNav<K extends string>({
  sections,
  selected,
  onSelect,
  label,
}: SettingsSectionNavProps<K>) {
  const refs = useRef(new Map<K, HTMLButtonElement | null>());

  /**
   * Arrow keys move the selection, and the focus with it — the roving tab stop
   * is what keeps `Tab` from walking the whole list. Home and End go to the
   * ends, which on a ten-section nav is the difference between one press and
   * nine.
   */
  const onKeyDown = (event: React.KeyboardEvent, index: number) => {
    const step =
      event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowUp"
          ? -1
          : event.key === "Home"
            ? -index
            : event.key === "End"
              ? sections.length - 1 - index
              : null;
    if (step === null) return;
    event.preventDefault();
    const next = sections[(index + step + sections.length) % sections.length];
    if (!next) return;
    onSelect(next[0]);
    refs.current.get(next[0])?.focus();
  };

  return (
    <div
      role="tablist"
      aria-orientation="vertical"
      aria-label={label}
      style={{
        borderRight: "1px solid var(--border-1)",
        background: "var(--bg-panel)",
        padding: "12px 8px",
        // The window is fixed at 800 × 600 (SWN-FR-03) and a section list can
        // outgrow that at a large UI role, so the nav scrolls rather than
        // pushing its last rows out of a frame that cannot be resized.
        overflowY: "auto",
      }}
    >
      {sections.map(([key, text], index) => (
        <button
          key={key}
          type="button"
          ref={(el) => {
            refs.current.set(key, el);
          }}
          className="menu-item"
          role="tab"
          aria-selected={selected === key}
          data-active={selected === key}
          // One tab stop for the nav, on the row that is selected.
          tabIndex={selected === key ? 0 : -1}
          style={{
            // A floor rather than a fixed height: a label that wraps to two
            // lines needs two line-heights, and a pinned row would let the
            // second paint over the row below it once the UI role outgrows the
            // box — and this nav is the one control an author needs in order to
            // get back and undo the size they just chose (GLS-FR-19).
            minHeight: "var(--ctl-h-lg)",
            width: "100%",
            // A button carries a border, a background, a font and an alignment
            // of its own; the row is styled by `.menu-item` and those would
            // fight it.
            border: 0,
            textAlign: "left",
            font: "inherit",
            color: "inherit",
            background: selected === key ? "var(--bg-active)" : "transparent",
          }}
          onClick={() => onSelect(key)}
          onKeyDown={(event) => onKeyDown(event, index)}
        >
          {text}
        </button>
      ))}
    </div>
  );
}
