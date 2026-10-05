/**
 * The split action control: one primary half that performs the selected action
 * and one dropdown half that changes which action that is.
 *
 * It is the Changes panel's commit control (`CHG-changes.md` CHG-FR-33,
 * CHG-FR-35, CHG-FR-49), lifted out of that panel so the graduation publication
 * choice can be the same control rather than a second one that looks like it
 *. Everything the panel's own control
 * did, this does: the two halves read as one object, the whole object takes the
 * unavailable treatment when the selected action cannot be performed, the
 * dropdown stays operable while it does, an entry that cannot be chosen renders
 * inactive with its reason on hover rather than disappearing, and the menu
 * dismisses on an outside press, on Escape, and when the window loses focus.
 *
 * It decides nothing about the actions themselves — what they are, which is
 * selected, whether each can be performed, and what performing one does are all
 * the host's.
 */
import { useEffect, useRef, useState } from "react";
import { Icon } from "./icons";

export interface SplitActionOption<T extends string> {
  value: T;
  label: string;
  /**
   * Why this entry cannot be chosen, or null when it can. The reason is carried
   * on hover rather than as a second label: the dropdown reads as its list of
   * actions whatever the moment allows.
   */
  unavailable?: string | null;
}

export interface SplitActionProps<T extends string> {
  /** The selected action. Its option supplies the primary half's label. */
  value: T;
  options: readonly SplitActionOption<T>[];
  onChange: (next: T) => void;
  onActivate: () => void;
  /**
   * Whether the **selected action** can be performed right now. This is what the
   * unavailable treatment describes, so both halves dim together rather than the
   * primary dimming against a fully saturated dropdown.
   */
  available: boolean;
  /** The primary half's own enablement — a busy host disables it too. */
  primaryDisabled?: boolean;
  /** The dropdown half's enablement. Left enabled in all but the rarest case. */
  menuDisabled?: boolean;
  /** Replaces the selected option's label, for a host that says "Working…". */
  primaryLabel?: string;
  primaryTitle?: string | null;
  /** The accessible name of the dropdown's list. */
  menuLabel: string;
  /** The accessible name of the dropdown's trigger. */
  triggerLabel?: string;
  /** Carried on the control itself, for a host that positions it. */
  className?: string;
  menuMinWidth?: number;
}

export function SplitAction<T extends string>({
  value,
  options,
  onChange,
  onActivate,
  available,
  primaryDisabled = false,
  menuDisabled = false,
  primaryLabel,
  primaryTitle,
  menuLabel,
  triggerLabel = "Choose action",
  className,
  menuMinWidth = 150,
}: SplitActionProps<T>) {
  const [open, setOpen] = useState(false);
  // The whole control, so a press on either half or on a menu entry counts as
  // inside and everything else counts as outside.
  const root = useRef<HTMLDivElement>(null);

  /**
   * CHG-FR-33: dismiss on an outside press, on Escape, and when the window loses
   * focus — the behaviour every other dropdown in this shell has, and what keeps
   * two menus from standing open at once. `mousedown` rather than `click`, so
   * activating another surface's trigger closes this one on the way down instead
   * of leaving both open.
   */
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // The Escape that closes this menu closes **only** this menu. The control
      // is used inside a modal whose own Escape dismisses it, and
      // that handler is on `window`, one step further out than this one — so
      // without this a single Escape over an open dropdown took the surface down
      // with it and the author lost the choice they were making.
      e.stopPropagation();
      setOpen(false);
    };
    const onBlur = () => setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onBlur);
    };
  }, [open]);

  const selected = options.find((option) => option.value === value);

  const choose = (next: T) => {
    setOpen(false);
    onChange(next);
  };

  return (
    <div className="split-action" ref={root}>
      {/* CHG-FR-49: one object, one appearance. The treatment describes the
          selected ACTION rather than the control's reachability, so the dropdown
          half dims with the primary and stays operable. */}
      <div
        className={
          className ? `split-action__control ${className}` : "split-action__control"
        }
        data-unavailable={!available}
      >
        <button
          className="btn btn--primary btn--sm"
          disabled={primaryDisabled}
          title={primaryTitle ?? undefined}
          onClick={onActivate}
        >
          {primaryLabel ?? selected?.label ?? ""}
        </button>
        <button
          className="btn btn--primary btn--sm"
          aria-label={triggerLabel}
          aria-haspopup="listbox"
          aria-expanded={open}
          disabled={menuDisabled}
          onClick={() => setOpen((v) => !v)}
        >
          <Icon.Caret size={12} />
        </button>
      </div>
      {/* CHG-FR-35: the dropdown draws the line at what the author can change.
          An entry nothing in this surface makes available renders inactive with
          the reason rather than disappearing, so the list of actions is the same
          list whatever the moment allows. */}
      {open && (
        <div
          className="menu split-action__menu"
          role="listbox"
          aria-label={menuLabel}
          style={{ minWidth: menuMinWidth }}
        >
          {options.map((option) => (
            <div
              key={option.value}
              role="option"
              tabIndex={option.unavailable ? -1 : 0}
              aria-selected={option.value === value}
              aria-disabled={option.unavailable ? true : undefined}
              title={option.unavailable ?? undefined}
              className={
                option.unavailable ? "menu-item menu-item--disabled" : "menu-item"
              }
              onClick={() => {
                if (option.unavailable) return;
                choose(option.value);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  if (!option.unavailable) choose(option.value);
                }
              }}
            >
              {option.label}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
