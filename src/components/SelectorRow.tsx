import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

/**
 * The one form every vertical-panel scope-, mode-, or lens selector takes
 * (`SNV-shell-navigation.md` SNV-FR-62, SNV-FR-63).
 *
 * A row of toggle buttons behaving as a single radio group: one button per
 * position, exactly one active, and every position one activation away — there
 * is no dropdown to open. Each button carries its position's name as a short
 * text tag in the same treatment a panel's rows give an artifact-type tag, so
 * the control and the content it filters read as one vocabulary, and discloses
 * the position's full name in a tooltip on hover and immediately on keyboard
 * focus.
 *
 * One component rather than five copies for the same reason `.panel-controls`
 * is one class: the requirement is about *every* such surface, and the way five
 * panels stay identical is by there being one of them.
 */

export interface SelectorPosition<T extends string> {
  value: T;
  /** The short text tag the row renders (SNV-FR-62). */
  tag: string;
  /**
   * The position's full name. It is both the tooltip's text and the button's
   * accessible name, so a tag shortened to fit the row is still readable in
   * full by eye and by a screen reader.
   */
  title: string;
  /**
   * An artifact type, when the tag should carry that type's own chip colouring
   * — which is what makes a lens button and the tags on the rows it admits the
   * same vocabulary rather than merely the same size.
   */
  dataType?: string;
  /** A position that exists but cannot currently be chosen. */
  disabled?: boolean;
}

/**
 * The gap between buttons, in px. Mirrors `--sp-1` as applied by
 * `.selector-row` in `kit.css`; the fit computation below has to know it
 * because it measures buttons individually rather than the row as laid out.
 */
const GAP_PX = 4;

interface SelectorRowProps<T extends string> {
  /** Names the group. Also the handle tests and the harness address it by. */
  label: string;
  positions: SelectorPosition<T>[];
  value: T;
  onChange: (value: T) => void;
}

export function SelectorRow<T extends string>({
  label,
  positions,
  value,
  onChange,
}: SelectorRowProps<T>) {
  const rowRef = useRef<HTMLDivElement>(null);
  /**
   * Each position's natural width, cached the first time it is laid out. A
   * clipped button is removed from the DOM, so without this cache widening the
   * panel could never bring it back: there would be no element left to measure
   * and no width to decide with.
   *
   * A tag's text never changes for a given position, so a width measured once
   * stays true.
   */
  const widths = useRef(new Map<string, number>());
  /** The positions currently clipped away (SNV-FR-63). */
  const [clipped, setClipped] = useState<ReadonlySet<string>>(() => new Set());

  const leading = positions[0]?.value;
  const trailing = positions[positions.length - 1]?.value;

  /**
   * SNV-FR-63: the row is one line clipped at its trailing edge, except that
   * its leading button, its trailing button, and whichever button is active
   * render at every width — the two positions that bound what the selector can
   * admit, and the one naming what it admits right now.
   *
   * Measured rather than left to CSS because `overflow: hidden` clips strictly
   * from the trailing edge, which would take an active button sitting in the
   * middle of a narrow row. The exemption is the whole point of the
   * requirement, so it is computed rather than approximated.
   *
   * Held in a ref as well as run below, so the observer — created once — always
   * calls the current closure. Recomputing directly rather than through a state
   * bump is what keeps a resize from costing a render even when the fit is
   * unchanged, which matters because the observer watches every button.
   */
  const recompute = () => {
    const row = rowRef.current;
    if (!row) return;

    // Re-measure everything on screen on every pass rather than trusting the
    // cache. Widths are not fixed: the UI font size is a live preference
    // (`GLS-FR-16`) applied by writing a custom property on the document root,
    // which resizes every tag without resizing the row that holds them. A cache
    // written once would then be deciding the fit from metrics that no longer
    // exist, and the first thing to go would be the trailing button SNV-FR-63
    // says must always render. A button absent from the DOM keeps whatever
    // width it was last measured at — there is nothing left to measure it on,
    // and it is re-measured the moment it comes back.
    for (const el of row.querySelectorAll<HTMLElement>("[data-value]")) {
      const w = el.offsetWidth;
      if (w > 0) widths.current.set(el.dataset.value!, w);
    }

    const available = row.clientWidth;
    // Nothing has been laid out — a detached tree, a display:none ancestor, or
    // jsdom, which reports every box as zero. Clipping on a measurement that
    // does not exist would hide buttons that fit perfectly well, so the row
    // renders whole until it can be measured.
    if (available <= 0) {
      setClipped((prev) => (prev.size === 0 ? prev : new Set()));
      return;
    }
    if (positions.some((p) => !widths.current.has(p.value))) return;

    const width = (v: string) => widths.current.get(v) ?? 0;
    const spanOf = (vals: string[]) =>
      vals.reduce((sum, v) => sum + width(v), 0) +
      Math.max(0, vals.length - 1) * GAP_PX;

    const exempt = new Set<string>(
      [leading, trailing, value].filter((v): v is T => v != null),
    );
    // The exempt set renders whatever it costs; the rest are added in order and
    // the first one that does not fit ends the row, because the clip is at the
    // trailing edge rather than a choice of which buttons to drop.
    let shown = positions.filter((p) => exempt.has(p.value)).map((p) => p.value);
    for (const p of positions) {
      if (exempt.has(p.value)) continue;
      const trial = positions
        .filter((x) => shown.includes(x.value) || x.value === p.value)
        .map((x) => x.value);
      if (spanOf(trial) > available) break;
      shown = trial;
    }

    const next = new Set(
      positions.filter((p) => !shown.includes(p.value)).map((p) => p.value),
    );
    setClipped((prev) =>
      prev.size === next.size && [...next].every((v) => prev.has(v))
        ? prev
        : next,
    );
  };
  const recomputeRef = useRef(recompute);
  recomputeRef.current = recompute;

  /**
   * After every render, so a freshly-mounted button is measured and the closure
   * the observer holds is current. Deliberately without a dependency array:
   * `setClipped` above is guarded on the value it would write, so a pass that
   * changes nothing schedules no render and this cannot loop.
   */
  useLayoutEffect(() => {
    recomputeRef.current();
  });

  /**
   * The panel's width is what reaches the rest of the row (SNV-FR-33) — and the
   * buttons are watched alongside it, because the other thing that changes the
   * fit is the tags themselves growing under a live font-size change, which
   * moves no box the row's own observation would notice.
   *
   * `observe` on an already-watched target is a no-op, so re-observing after
   * each render adds the buttons that just appeared and nothing else. Guarded
   * because jsdom has no ResizeObserver; there it simply never fires, which is
   * right — nothing there has a width to observe.
   */
  const observerRef = useRef<ResizeObserver | null>(null);
  useEffect(() => {
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => recomputeRef.current());
    observerRef.current = observer;
    return () => {
      observer.disconnect();
      observerRef.current = null;
    };
  }, []);
  useEffect(() => {
    const row = rowRef.current;
    const observer = observerRef.current;
    if (!row || !observer) return;
    observer.observe(row);
    for (const el of row.querySelectorAll<HTMLElement>("[data-value]")) {
      observer.observe(el);
    }
  });

  /**
   * Arrow keys move between the positions the row is rendering, which is the
   * keyboard behaviour a radio group is expected to have. Roving tabindex keeps
   * the group a single tab stop.
   */
  const rendered = positions.filter((p) => !clipped.has(p.value));
  const onKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      const delta =
        e.key === "ArrowRight" || e.key === "ArrowDown"
          ? 1
          : e.key === "ArrowLeft" || e.key === "ArrowUp"
            ? -1
            : 0;
      if (delta === 0) return;
      const usable = rendered.filter((p) => !p.disabled);
      if (usable.length === 0) return;
      e.preventDefault();
      const at = usable.findIndex((p) => p.value === value);
      const next = usable[(at + delta + usable.length) % usable.length];
      onChange(next.value);
      rowRef.current
        ?.querySelector<HTMLElement>(`[data-value="${next.value}"]`)
        ?.focus();
    },
    [rendered, value, onChange],
  );

  return (
    <div
      ref={rowRef}
      className="selector-row"
      role="radiogroup"
      aria-label={label}
      data-selector={label}
      onKeyDown={onKeyDown}
    >
      {rendered.map((p) => {
        const active = p.value === value;
        return (
          <button
            key={p.value}
            type="button"
            role="radio"
            className="selector-row__btn"
            data-value={p.value}
            data-active={active}
            aria-checked={active}
            aria-label={p.title}
            disabled={p.disabled}
            tabIndex={active ? 0 : -1}
            onClick={() => {
              if (!p.disabled && !active) onChange(p.value);
            }}
          >
            <span className="chip-type" data-type={p.dataType}>
              {p.tag}
            </span>
            {/* SNV-FR-62: the full name, on hover and immediately on keyboard
                focus. Shown by CSS rather than by state so there is no delay to
                sit through and no listener per button; hidden from the
                accessibility tree because `aria-label` above already carries
                the same text as the button's own name. */}
            <span className="selector-row__tip" aria-hidden="true">
              {p.title}
            </span>
          </button>
        );
      })}
    </div>
  );
}
