import { useEffect, useMemo, useRef, useState } from "react";

/**
 * The model selector of both AI integration levels —
 * `specifications/ui/AII-ai-integrations.md` AII-FR-36 .. AII-FR-40.
 *
 * A selector the section renders itself rather than a native one, because a
 * provider may offer hundreds of models and a native `<select>` offers no way
 * to reach one but scrolling. It opens into a panel carrying a filter box above
 * its options, and it is deliberately the *same* control in both levels: a list
 * of five and a list of hundreds are the same problem once one of them grows,
 * and nothing about choosing a model should have to be learned twice.
 *
 * Three properties shape it:
 *
 * - **Filtering is local** (AII-FR-36, non-functional). Typing narrows options
 *   the record already carries. No operation and no request is issued, so a
 *   catalogue of hundreds stays responsive to every keystroke offline.
 * - **The filter is not state about anything** (AII-FR-36). It is discarded when
 *   the panel closes, so the selector always reopens showing everything.
 * - **The keyboard is sufficient on its own** (AII-FR-40). Arrows move the
 *   highlight, Enter commits it, Escape closes without changing the selection,
 *   and focus returns to the selector either way.
 */

export interface FilterableOption {
  id: string;
  label: string;
  /**
   * An option the list shows but will not commit. Offered rather than omitted,
   * so a caller whose rules make some choices illegal can say *which* and *why*
   * instead of leaving the author to wonder where an option went — the Drafts
   * panel's Move to Folder… lists every folder this way, marking the ones a
   * move would be refused on (`DRP-drafts-panel.md` DRP-FR-28).
   */
  disabled?: boolean;
  /** Why the option is unavailable, rendered beside its label. */
  hint?: string;
}

export interface FilterableSelectProps {
  /**
   * The options to offer, already including the default entry that means
   * "whatever the provider or backend itself would pick". It is one of the
   * options rather than a special case, so AII-FR-37's matching applies to it
   * uniformly — an empty box offers everything, including it.
   */
  options: FilterableOption[];
  /** The selected option's id. The default entry is the empty string. */
  value: string;
  onChange: (id: string) => void;
  /** Accessible name, e.g. "Model". */
  label: string;
  disabled?: boolean;
  /** Distinguishes the two levels' selectors in tests. */
  testId?: string;
}

/**
 * AII-FR-37: case-insensitive substring over the option's label *and* its
 * identifier, anchored at neither.
 *
 * Matching the id as well as the label is what makes a provider-qualified model
 * reachable by the half of its name the author remembers — typing `opus` finds
 * `anthropic/claude-opus-5` whether the label carries the vendor or not.
 *
 * Pure and exported so the rule is testable without rendering.
 */
export function matchesOption(option: FilterableOption, filter: string): boolean {
  const needle = filter.trim().toLowerCase();
  if (!needle) return true;
  return (
    option.label.toLowerCase().includes(needle) ||
    option.id.toLowerCase().includes(needle)
  );
}

export function FilterableSelect({
  options,
  value,
  onChange,
  label,
  disabled,
  testId,
}: FilterableSelectProps) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  /** Index into the *filtered* list, which is what the arrows walk. */
  const [highlight, setHighlight] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const filterRef = useRef<HTMLInputElement | null>(null);
  const buttonRef = useRef<HTMLButtonElement | null>(null);

  const shown = useMemo(
    () => options.filter((o) => matchesOption(o, filter)),
    [options, filter],
  );

  const selected = options.find((o) => o.id === value) ?? null;

  // AII-FR-36: the box takes focus on open, so a model is reached by typing
  // rather than by first clicking into the field.
  useEffect(() => {
    if (open) filterRef.current?.focus();
  }, [open]);

  // A narrowing filter can strand the highlight past the end of the list, or on
  // an option the caller marked unavailable.
  useEffect(() => {
    setHighlight((h) => {
      if (h < shown.length && !shown[h]?.disabled) return h;
      const first = shown.findIndex((o) => !o.disabled);
      return first === -1 ? 0 : first;
    });
  }, [shown]);

  const close = (restoreFocus: boolean) => {
    setOpen(false);
    // AII-FR-36: what was typed is discarded, so the selector reopens showing
    // everything the record offers rather than a stale narrowing.
    setFilter("");
    setHighlight(0);
    if (restoreFocus) buttonRef.current?.focus();
  };

  // Dismiss on outside click, like every other dropdown in the application.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        close(false);
      }
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const commit = (id: string) => {
    if (options.find((o) => o.id === id)?.disabled) return;
    close(true);
    // A re-pick of what is already selected is not a change, and invoking for
    // it would spend a round trip to store what is already stored.
    if (id !== value) onChange(id);
  };

  /**
   * The next selectable index in `direction`, stepping over anything the caller
   * marked unavailable. An arrow that parked on an option Enter then refused
   * would be a dead end the author has no way to read.
   */
  const step = (from: number, direction: 1 | -1): number => {
    if (shown.length === 0) return 0;
    for (let i = 1; i <= shown.length; i += 1) {
      const at = (from + direction * i + shown.length * i) % shown.length;
      if (!shown[at].disabled) return at;
    }
    return from;
  };

  /**
   * AII-FR-40. The handler lives on the filter box because that is what holds
   * focus while the panel is open, so the arrows never fight the caret for the
   * key press — left and right still move within the text.
   */
  const onKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setHighlight((h) => step(h, 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlight((h) => step(h, -1));
        break;
      case "Enter": {
        e.preventDefault();
        // AII-FR-39: nothing is selectable while the filter matches nothing.
        const option = shown[highlight];
        if (option) commit(option.id);
        break;
      }
      case "Escape":
        e.preventDefault();
        // The open list owns the key. Without this the press also reaches
        // whatever surface the selector sits in — a dialog's own Escape
        // handler, say — and one press closes both, losing the author's place.
        // It has to be `stopPropagation` on the synthetic event rather than a
        // check the outer handler makes: React flushes this update
        // synchronously, so by the time a `window` listener runs the list is
        // already gone and there is nothing left for it to detect.
        e.stopPropagation();
        // AII-FR-40: the selection is left exactly as it was.
        close(true);
        break;
    }
  };

  const listboxId = testId ? `${testId}-listbox` : undefined;
  /**
   * The highlighted option's element id, which is what lets a screen reader
   * announce what the arrow keys are moving over. Without it the panel is
   * operable but silent, and AII-FR-40's keyboard path would be usable only by
   * someone who can see the highlight.
   */
  const optionDomId = (index: number) =>
    listboxId ? `${listboxId}-opt-${index}` : undefined;
  const activeDescendant =
    shown.length > 0 ? optionDomId(highlight) : undefined;

  return (
    <div ref={rootRef} className="fsel">
      <button
        ref={buttonRef}
        type="button"
        className="input fsel__trigger"
        role="combobox"
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-controls={open ? listboxId : undefined}
        aria-label={label}
        disabled={disabled}
        data-testid={testId}
        onClick={() => (open ? close(true) : setOpen(true))}
      >
        <span
          className={
            selected ? "fsel__value" : "fsel__value fsel__value--placeholder"
          }
        >
          {selected?.label ?? "Nothing selected"}
        </span>
        <span aria-hidden="true" className="fsel__caret">
          ▾
        </span>
      </button>

      {open && (
        // `.menu` is what makes this an opaque elevated surface rather than a
        // transparent one over the rows beneath.
        <div className="fsel__menu menu">
          <div className="fsel__filter">
            <input
              ref={filterRef}
              className="input"
              type="text"
              value={filter}
              role="combobox"
              aria-expanded="true"
              aria-controls={listboxId}
              aria-activedescendant={activeDescendant}
              aria-label={`Filter ${label.toLowerCase()}`}
              data-testid={testId ? `${testId}-filter` : undefined}
              placeholder="Filter…"
              onChange={(e) => {
                setFilter(e.target.value);
                setHighlight(0);
              }}
              onKeyDown={onKeyDown}
            />
          </div>

          <div
            id={listboxId}
            className="fsel__list"
            role="listbox"
            aria-label={label}
          >
            {/* AII-FR-39: a first-class row, not an empty panel. */}
            {shown.length === 0 && (
              <div
                className="fsel__empty"
                data-testid={testId ? `${testId}-empty` : undefined}
              >
                Nothing matches that.
              </div>
            )}
            {shown.map((option, index) => (
              <div
                key={option.id}
                id={optionDomId(index)}
                className={
                  option.disabled ? "fsel__option fsel__option--off" : "fsel__option"
                }
                role="option"
                aria-selected={option.id === value}
                aria-disabled={option.disabled || undefined}
                title={option.hint}
                data-highlighted={index === highlight}
                data-selected={option.id === value}
                data-testid={testId ? `${testId}-option-${option.id}` : undefined}
                // The highlight has to survive the press: a mousedown that moved
                // focus off the filter box would close the panel before the
                // click that selects ever lands.
                onMouseDown={(e) => e.preventDefault()}
                onMouseEnter={() => !option.disabled && setHighlight(index)}
                onClick={() => commit(option.id)}
              >
                <span className="fsel__option-label">{option.label}</span>
                {/* Why it cannot be chosen, beside it rather than in a tooltip
                    alone: an option offered without a reason reads as a bug. */}
                {option.hint && <span className="fsel__option-hint">{option.hint}</span>}
              </div>
            ))}
          </div>

          {/* AII-FR-38: a filter narrowing hundreds to a handful says what it
              did, rather than leaving the author to guess whether the rest
              still exist. */}
          <div
            className="fsel__count"
            data-testid={testId ? `${testId}-count` : undefined}
          >
            {shown.length} of {options.length} shown
          </div>
        </div>
      )}
    </div>
  );
}
