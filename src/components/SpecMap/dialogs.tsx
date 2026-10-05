import { useEffect, useId, useRef, useState, type RefObject } from "react";
import { Icon } from "../icons";

/** SMO-FR-HCQN: the limits of the two fields. */
export const NAME_MAX = 60;
export const SUMMARY_MAX = 400;

export interface GroupDialogValues {
  label: string;
  summary: string;
  moveTo: string | null;
}

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * A map dialog is modal: focus starts inside it, Tab stays inside it, Escape
 * cancels it, and closing it returns focus to the control that opened it.
 */
function useModalFocus(root: RefObject<HTMLDivElement | null>, onCancel: () => void) {
  // Read during the first render, before the dialog takes focus.
  const [opener] = useState(() => document.activeElement as HTMLElement | null);
  const cancel = useRef(onCancel);
  cancel.current = onCancel;
  useEffect(() => {
    const el = root.current;
    el?.querySelector<HTMLElement>(FOCUSABLE)?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancel.current();
        return;
      }
      if (e.key !== "Tab" || !el) return;
      const items = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const current = document.activeElement;
      if (!el.contains(current)) {
        e.preventDefault();
        first.focus();
      } else if (e.shiftKey && current === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && current === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener && opener.isConnected) opener.focus();
    };
  }, [root, opener]);
}

/**
 * SMO-FR-HCQN / SMO-FR-PZEI / SMO-FR-SLNC: create or edit an index node. The
 * Move to list shows only in an edit, and only when the node has somewhere to go.
 */
export function GroupDialog({
  title,
  confirmLabel,
  initial,
  moveTargets,
  onCancel,
  onConfirm,
}: {
  title: string;
  confirmLabel: string;
  initial: { label: string; summary: string };
  moveTargets: { id: string; label: string }[] | null;
  onCancel: () => void;
  onConfirm: (values: GroupDialogValues) => void;
}) {
  const id = useId();
  const [label, setLabel] = useState(initial.label);
  const [summary, setSummary] = useState(initial.summary);
  const [moveTo, setMoveTo] = useState("");
  const root = useRef<HTMLDivElement>(null);
  useModalFocus(root, onCancel);

  const trimmed = label.trim();
  const valid = trimmed.length > 0 && trimmed.length <= NAME_MAX && summary.length <= SUMMARY_MAX;
  const submit = () => {
    if (!valid) return;
    onConfirm({ label: trimmed, summary, moveTo: moveTo === "" ? null : moveTo });
  };

  return (
    <div
      className="scrim"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div ref={root} className="modal" role="dialog" aria-modal="true" aria-labelledby={`${id}-title`}>
        <div className="modal__head">
          <Icon.Layers size={14} />
          <div className="modal__title" id={`${id}-title`}>
            {title}
          </div>
        </div>
        <div className="modal__body">
          <div className="picker-field">
            <label className="picker-field__label" htmlFor={`${id}-name`}>
              Name
            </label>
            <input
              id={`${id}-name`}
              className="input"
              maxLength={NAME_MAX}
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submit();
              }}
            />
          </div>
          <div className="picker-field">
            <label className="picker-field__label" htmlFor={`${id}-summary`}>
              Summary
            </label>
            <textarea
              id={`${id}-summary`}
              className="textarea"
              maxLength={SUMMARY_MAX}
              rows={3}
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
            />
          </div>
          {moveTargets && moveTargets.length > 0 && (
            <div className="picker-field">
              <label className="picker-field__label" htmlFor={`${id}-move`}>
                Move to
              </label>
              <select
                id={`${id}-move`}
                className="select"
                value={moveTo}
                onChange={(e) => setMoveTo(e.target.value)}
              >
                <option value="">Keep the current parent</option>
                {moveTargets.map((t) => (
                  <option key={t.id} value={t.id}>
                    {t.label}
                  </option>
                ))}
              </select>
            </div>
          )}
        </div>
        <div className="modal__actions">
          <button type="button" className="btn btn--ghost" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="btn btn--primary" disabled={!valid} onClick={submit}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

/** SMO-FR-IMXW: confirm a delete, naming the node and its direct children. */
export function DeleteNodeDialog({
  title,
  label,
  childCount,
  onCancel,
  onConfirm,
}: {
  title: string;
  label: string;
  childCount: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  useModalFocus(root, onCancel);
  return (
    <div
      className="scrim"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div ref={root} className="modal" role="dialog" aria-modal="true" aria-labelledby={`${id}-title`}>
        <div className="modal__head">
          <Icon.Trash size={14} />
          <div className="modal__title" id={`${id}-title`}>
            {title}
          </div>
        </div>
        <div className="modal__body">
          <p className="smap-dialog__text">
            Delete {label}? It holds {childCount} direct {childCount === 1 ? "child" : "children"}.
          </p>
        </div>
        <div className="modal__actions">
          <button type="button" className="btn btn--ghost" onClick={onCancel}>
            Cancel
          </button>
          <button type="button" className="btn btn--danger" onClick={onConfirm}>
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
