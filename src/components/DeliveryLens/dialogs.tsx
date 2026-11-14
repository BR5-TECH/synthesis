/**
 * The dialogs of the Delivery lens (`MDL-map-delivery-lens.md`): the remove
 * dialog of a GitHub plan (MDL-FR-XQGD), Map Types (MDL-FR-XUJY), Publish to
 * GitHub (MDL-FR-JRGK, MDL-FR-WMGU), and Export to file (MDL-FR-QIXG).
 *
 * Each is a floating overlay of the main window (SNV-FR-56): the lens closes
 * every other overlay before it opens one. Each is modal: focus starts in it,
 * Tab stays in it, and Escape cancels it. When it closes, focus goes back to
 * the control that opened it. Where that control is gone, for example an
 * entry of the band menu, focus goes to the band menu trigger of the lens.
 */
import { useEffect, useId, useRef, useState, type ReactNode } from "react";

import type { FlowItemKind } from "../../state/flowDocument";
import type { IssueKind } from "../../types/delivery";

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** The band menu trigger ("Plan actions") of a lens (BandMenu.tsx). */
export const BAND_MENU_TRIGGER = ".dlens-menu__toggle";

/**
 * The control that takes focus when a dialog closes: the control that opened
 * it while it is still in the document, else the band menu trigger of the
 * lens that held the dialog.
 */
function focusTargetOnClose(opener: HTMLElement | null, lens: Element | null): HTMLElement | null {
  if (opener && opener !== document.body && opener.isConnected) return opener;
  if (!lens?.isConnected) return null;
  return lens.querySelector<HTMLElement>(BAND_MENU_TRIGGER);
}

/** The frame of a lens dialog: a scrim, a modal card, a title, and actions. */
export function LensDialog({
  title,
  testId,
  onCancel,
  children,
  actions,
  initialFocus,
}: {
  title: string;
  testId: string;
  onCancel: () => void;
  children: ReactNode;
  actions: ReactNode;
  /** A selector of the control that takes the first focus. */
  initialFocus?: string;
}) {
  const titleId = useId();
  const root = useRef<HTMLDivElement>(null);
  const cancel = useRef(onCancel);
  cancel.current = onCancel;
  const [opener] = useState(() => document.activeElement as HTMLElement | null);
  const focusSelector = useRef(initialFocus);

  useEffect(() => {
    const el = root.current;
    // The lens that holds the dialog, for the band menu trigger on close.
    const lens = el?.closest(".dlens-plan") ?? null;
    const first =
      (focusSelector.current ? el?.querySelector<HTMLElement>(focusSelector.current) : null) ??
      el?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        cancel.current();
        return;
      }
      if (e.key !== "Tab" || !el) return;
      const items = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (items.length === 0) return;
      const head = items[0];
      const last = items[items.length - 1];
      const current = document.activeElement;
      if (!el.contains(current)) {
        e.preventDefault();
        head.focus();
      } else if (e.shiftKey && current === head) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && current === last) {
        e.preventDefault();
        head.focus();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      focusTargetOnClose(opener, lens)?.focus();
    };
  }, [opener]);

  return (
    <div
      className="scrim"
      onMouseDown={(e) => {
        if (!root.current?.contains(e.target as Node)) cancel.current();
      }}
    >
      <div
        ref={root}
        className="modal dlens-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        data-testid={testId}
      >
        <div className="modal__head">
          <h2 id={titleId} className="modal__title">
            {title}
          </h2>
        </div>
        <div className="modal__body">{children}</div>
        <div className="modal__actions">{actions}</div>
      </div>
    </div>
  );
}

/** MDL-FR-XQGD: what the author chose in the remove dialog. */
export type RemoveMode = "remove_from_board" | "close_not_planned";

/** MDL-FR-XQGD: the remove dialog of an item of a GitHub plan. */
export function RemoveDialog({
  name,
  kind,
  count,
  onChoose,
  onCancel,
}: {
  name: string;
  kind: FlowItemKind;
  count: number;
  onChoose: (mode: RemoveMode) => void;
  onCancel: () => void;
}) {
  return (
    <LensDialog
      title={`Remove “${name || "Unnamed"}”`}
      testId="delivery-remove-dialog"
      onCancel={onCancel}
      initialFocus="[data-cancel]"
      actions={
        // MDL-FR-XQGD wireframe: the two removals, then Cancel last.
        <>
          <button type="button" className="btn btn--sm" onClick={() => onChoose("remove_from_board")}>
            Remove from board
          </button>
          <button
            type="button"
            className="btn btn--sm btn--danger"
            onClick={() => onChoose("close_not_planned")}
          >
            Close as not planned
          </button>
          <button type="button" className="btn btn--sm" data-cancel onClick={onCancel}>
            Cancel
          </button>
        </>
      }
    >
      <p className="dlens-dialog__line">
        {count === 0
          ? "No item is under it."
          : `${count} ${count === 1 ? "item is" : "items are"} under it.`}
      </p>
      {kind === "milestone" && (
        <p className="dlens-dialog__line">The milestone closes on GitHub.</p>
      )}
      <p className="dlens-dialog__hint">
        Remove from board takes the issues off the Project and keeps them open. Close as not planned
        closes them on GitHub.
      </p>
    </LensDialog>
  );
}

/** MDL-FR-XUJY: one choice of a row of Map Types; null is no choice yet. */
export type TypeChoice = IssueKind | "exclude" | null;

const TYPE_CHOICES: { value: Exclude<TypeChoice, null>; label: string }[] = [
  { value: "epic", label: "Epic" },
  { value: "feature", label: "Feature" },
  { value: "task", label: "Task" },
  { value: "defect", label: "Defect" },
  { value: "exclude", label: "Leave out" },
];

/** MDL-FR-XUJY: the Map Types dialog, one row for each unmapped Type. */
export function MapTypesDialog({
  types,
  busy,
  error,
  onConfirm,
  onCancel,
}: {
  /** `null` stands for the issues with no Type. */
  types: { type: string | null; count: number }[];
  busy: boolean;
  error: string | null;
  onConfirm: (choices: Map<string | null, Exclude<TypeChoice, null>>) => void;
  onCancel: () => void;
}) {
  const [choices, setChoices] = useState<Map<string | null, Exclude<TypeChoice, null>>>(new Map());
  return (
    <LensDialog
      title="Map Types"
      testId="delivery-map-types-dialog"
      onCancel={onCancel}
      actions={
        <>
          <button type="button" className="btn btn--sm" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--sm btn--primary"
            disabled={choices.size === 0 || busy}
            onClick={() => onConfirm(choices)}
          >
            {busy ? "Saving…" : "Confirm"}
          </button>
        </>
      }
    >
      <p className="dlens-dialog__hint">Choose the plan kind of the issues of each Type.</p>
      <div className="dlens-types" role="group" aria-label="Unmapped Types">
        {types.map(({ type, count }) => {
          const name = type ?? "No Type";
          return (
            <label key={type ?? ""} className="dlens-types__row">
              <span className="dlens-types__name">{name}</span>
              <span className="dlens-types__count">
                {count} {count === 1 ? "issue" : "issues"}
              </span>
              <select
                className="select dlens-types__select"
                aria-label={`Kind of ${name}`}
                value={choices.get(type) ?? ""}
                onChange={(e) => {
                  const next = new Map(choices);
                  const value = e.target.value as Exclude<TypeChoice, null> | "";
                  if (value === "") next.delete(type);
                  else next.set(type, value);
                  setChoices(next);
                }}
              >
                <option value="">Choose…</option>
                {TYPE_CHOICES.map((c) => (
                  <option key={c.value} value={c.value}>
                    {c.label}
                  </option>
                ))}
              </select>
            </label>
          );
        })}
      </div>
      {error && (
        <p className="dlens-dialog__error" role="alert">
          {error}
        </p>
      )}
    </LensDialog>
  );
}

/** MDL-FR-JRGK / MDL-FR-WMGU: the state of a publish, as the dialog shows it. */
export type PublishState =
  | { phase: "confirm" }
  | { phase: "running" }
  | { phase: "failed"; error: string; createdUrls: string[] };

/** MDL-FR-JRGK / MDL-FR-WMGU: Publish to GitHub. */
export function PublishDialog({
  state,
  projectTitle,
  onConfirm,
  onCancel,
  onOpenLink,
}: {
  state: PublishState;
  projectTitle: string | null;
  onConfirm: () => void;
  onCancel: () => void;
  /** GDP-FR-PIAQ: open a created item through `"open tracker link (url)"`. */
  onOpenLink: (url: string) => void;
}) {
  const running = state.phase === "running";
  // The primary button is disabled while the publish runs, so the browser
  // takes focus from it. When the publish ends in the dialog, focus goes back
  // to that button (now Retry), so that it does not stay on the page body.
  const primary = useRef<HTMLButtonElement>(null);
  const wasRunning = useRef(running);
  useEffect(() => {
    const ended = wasRunning.current && !running;
    wasRunning.current = running;
    if (!ended) return;
    const dialog = primary.current?.closest('[role="dialog"]');
    if (dialog && !dialog.contains(document.activeElement)) primary.current?.focus();
  }, [running]);
  return (
    <LensDialog
      title="Publish to GitHub"
      testId="delivery-publish-dialog"
      onCancel={() => {
        if (!running) onCancel();
      }}
      actions={
        <>
          <button type="button" className="btn btn--sm" disabled={running} onClick={onCancel}>
            {state.phase === "failed" ? "Close" : "Cancel"}
          </button>
          <button
            ref={primary}
            type="button"
            className="btn btn--sm btn--primary"
            disabled={running}
            onClick={onConfirm}
          >
            {state.phase === "failed" ? "Retry" : running ? "Publishing…" : "Publish"}
          </button>
        </>
      }
    >
      <p className="dlens-dialog__line">
        The plan is written to {projectTitle ? `the GitHub Project “${projectTitle}”` : "GitHub"}, and
        GitHub becomes the source of the delivery. The Flow file stays on disk unchanged.
      </p>
      {running && (
        <p className="dlens-dialog__progress" role="status">
          <span className="dlens-spinner" aria-hidden="true" />
          Publishing the plan to GitHub…
        </p>
      )}
      {state.phase === "failed" && (
        <div className="dlens-dialog__failure" role="alert">
          <p className="dlens-dialog__error">{state.error}</p>
          {state.createdUrls.length > 0 && (
            <>
              <p className="dlens-dialog__hint">
                These items were made on GitHub before the publish stopped. Retry makes only the rest.
              </p>
              <ul className="dlens-dialog__links" data-testid="delivery-publish-created">
                {state.createdUrls.map((url) => (
                  <li key={url}>
                    <button type="button" className="dlens-link" onClick={() => onOpenLink(url)}>
                      {url}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
    </LensDialog>
  );
}

/** MDL-FR-QIXG: a project-relative path that ends in `.flow`. */
export function isFlowPath(path: string): boolean {
  const p = path.trim();
  return p.endsWith(".flow") && p.length > ".flow".length && !p.startsWith("/") && !p.split("/").includes("..");
}

/** MDL-FR-QIXG: the path the Export dialog starts with, from the Project title. */
export function exportPathFor(title: string): string {
  const slug = title
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `${slug || "delivery"}.flow`;
}

/** MDL-FR-QIXG: Export to file. */
export function ExportDialog({
  initialPath,
  busy,
  error,
  onConfirm,
  onCancel,
}: {
  initialPath: string;
  busy: boolean;
  error: string | null;
  onConfirm: (path: string) => void;
  onCancel: () => void;
}) {
  const [path, setPath] = useState(initialPath);
  const inputId = useId();
  const valid = isFlowPath(path);
  return (
    <LensDialog
      title="Export to file"
      testId="delivery-export-dialog"
      onCancel={onCancel}
      initialFocus="input"
      actions={
        <>
          <button type="button" className="btn btn--sm" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--sm btn--primary"
            disabled={!valid || busy}
            onClick={() => onConfirm(path.trim())}
          >
            {busy ? "Exporting…" : "Export"}
          </button>
        </>
      }
    >
      <p className="dlens-dialog__hint">
        The plan is written to a new Flow file, and the file becomes the source of the delivery.
        Nothing changes on GitHub.
      </p>
      <label className="dlens-field" htmlFor={inputId}>
        <span className="dlens-field__label">Path in the project</span>
        <input
          id={inputId}
          className="input input--mono"
          value={path}
          spellCheck={false}
          aria-invalid={!valid}
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && valid && !busy) onConfirm(path.trim());
          }}
        />
      </label>
      {!valid && <p className="dlens-dialog__hint">The path must end in .flow.</p>}
      {error && (
        <p className="dlens-dialog__error" role="alert">
          {error}
        </p>
      )}
    </LensDialog>
  );
}
