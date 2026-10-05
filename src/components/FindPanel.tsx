/**
 * `EFR-editor-find-replace.md`: the Find and Find & Replace panels.
 *
 * Both occupy the band directly above the editing surface, in place of the
 * formatting toolbar, for as long as one is open (EFR-FR-ABVQ). They are the same
 * component in two forms: `find` is the query row alone, `replace` adds the
 * replacement row and its two actions (EFR-FR-EWRP). The band belongs to the tab's
 * own chrome rather than being a floating overlay, so nothing here participates
 * in the main window's overlay mutual exclusion (STB-FR-12).
 */
import { useEffect, useRef } from "react";
import { Icon } from "./icons";
import { QUERY_MODES } from "../hooks/useSearchQueryMode";
import type { FindForm } from "../state/findState";
import type { SearchMode } from "../types";

export interface FindPanelProps {
  form: FindForm;
  query: string;
  replacement: string;
  mode: SearchMode;
  /** Ordinal of the current match (1-based), or 0 when there is none. */
  ordinal: number;
  total: number;
  /** EFR-FR-ENKY: false when a `regex` query does not compile. */
  valid: boolean;
  /** Which input takes focus when the panel opens or changes form. */
  focus: "query" | "replacement";
  onQueryChange: (value: string) => void;
  onReplacementChange: (value: string) => void;
  onModeChange: (mode: SearchMode) => void;
  onStep: (delta: 1 | -1) => void;
  onReplace: () => void;
  onReplaceAll: () => void;
  /** EFR-FR-CELO: close outright — the formatting toolbar returns. */
  onClose: () => void;
}

export function FindPanel({
  form,
  query,
  replacement,
  mode,
  ordinal,
  total,
  valid,
  focus,
  onQueryChange,
  onReplacementChange,
  onModeChange,
  onStep,
  onReplace,
  onReplaceAll,
  onClose,
}: FindPanelProps) {
  const queryRef = useRef<HTMLInputElement>(null);
  const replacementRef = useRef<HTMLInputElement>(null);

  /**
   * EFR-FR-AYNZ/EFR-FR-BJUY: opening the panel focuses its query input with any
   * existing query selected, so typing replaces it; expanding Find into Find &
   * Replace focuses the replacement input instead. Keyed on `focus` and `form`
   * so a re-render for a keystroke does not keep stealing the caret.
   */
  useEffect(() => {
    if (focus === "replacement" && form === "replace") {
      replacementRef.current?.focus();
      replacementRef.current?.select();
      return;
    }
    queryRef.current?.focus();
    queryRef.current?.select();
  }, [focus, form]);

  // EFR-FR-CELO: Escape closes the panel from anywhere within it. Bound on the
  // container rather than per-input so the mode toggles and the action buttons
  // answer it too.
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    onClose();
  };

  /**
   * EFR-FR-EGQB: Enter is next-match and ⇧Enter is previous-match, so a query can
   * be walked without leaving the keyboard. In the replacement input Enter
   * performs the replacement instead — that is the action the caret is sitting
   * in — and ⇧Enter still steps backwards.
   */
  const stepOnEnter =
    (onEnter: () => void) => (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (e.key !== "Enter") return;
      e.preventDefault();
      if (e.shiftKey) onStep(-1);
      else onEnter();
    };

  const counter = !valid
    ? "Invalid pattern"
    : total === 0
      ? query === ""
        ? ""
        : "No results"
      : `${ordinal}/${total}`;

  return (
    <div
      className="editor__find"
      data-testid="find-panel"
      data-form={form}
      role="search"
      // Distinct from the query input's own "Find" label, so a lookup by
      // accessible name never resolves to both.
      aria-label={
        form === "replace" ? "Find and replace in file" : "Find in file"
      }
      onKeyDown={onKeyDown}
    >
      <>
        <div className="editor__find-row">
          <div className="search-input editor__find-input">
            {/* EFR-FR-CLZF: the same three query modes the universal search input
                offers (SCH-FR-12), of which exactly one is active — but over
                this artifact's own value, never the user-global preference. */}
            <div
              role="radiogroup"
              aria-label="Query mode"
              data-testid="find-mode-toggles"
              className="search-input__modes"
            >
              {QUERY_MODES.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  role="radio"
                  aria-checked={mode === option.value}
                  aria-label={`${option.title} — ${option.description}`}
                  className="search-input__mode"
                  data-active={mode === option.value}
                  onClick={() => onModeChange(option.value)}
                >
                  {option.label}
                  <span className="search-input__tip" aria-hidden="true">
                    <strong>{option.title}</strong>
                    {option.description}
                  </span>
                </button>
              ))}
            </div>
            <input
              ref={queryRef}
              aria-label="Find"
              placeholder="Find"
              value={query}
              spellCheck={false}
              onChange={(e) => onQueryChange(e.target.value)}
              onKeyDown={stepOnEnter(() => onStep(1))}
            />
          </div>
          {/* EFR-FR-DXTV: the current match's ordinal within the total, with a
              zero state when the query is empty or nothing matches, and the
              invalid-pattern indication in its place (EFR-FR-EPYP). */}
          <span
            className="editor__find-count"
            data-testid="find-count"
            data-invalid={!valid}
            role="status"
          >
            {counter}
          </span>
          <button
            type="button"
            className="btn btn--ghost btn--icon"
            aria-label="Previous match"
            title="Previous match"
            disabled={total === 0}
            onClick={() => onStep(-1)}
          >
            <Icon.CaretUp size={14} />
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--icon"
            aria-label="Next match"
            title="Next match"
            disabled={total === 0}
            onClick={() => onStep(1)}
          >
            <Icon.Caret size={14} />
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--icon"
            aria-label="Close find"
            title="Close"
            onClick={onClose}
          >
            <Icon.X size={14} />
          </button>
        </div>

        {form === "replace" && (
          <div className="editor__find-row">
            <div className="search-input editor__find-input">
              <input
                ref={replacementRef}
                aria-label="Replace with"
                placeholder="Replace with"
                value={replacement}
                spellCheck={false}
                onChange={(e) => onReplacementChange(e.target.value)}
                onKeyDown={stepOnEnter(onReplace)}
              />
            </div>
            {/* EFR-FR-FWOU: both do nothing when the match set is empty, so both
                are disabled rather than silently inert. */}
            <button
              type="button"
              className="btn btn--default btn--sm"
              disabled={total === 0}
              onClick={onReplace}
            >
              Replace
            </button>
            <button
              type="button"
              className="btn btn--default btn--sm"
              disabled={total === 0}
              onClick={onReplaceAll}
            >
              Replace All
            </button>
          </div>
        )}
      </>
    </div>
  );
}
