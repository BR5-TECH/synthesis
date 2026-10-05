import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { Icon } from "./icons";
import { TokenIdentityLine, tokenErrorMessage } from "./GithubTokens";
import type { GithubTokenRecord } from "../types";

/**
 * The GitHub token picker modal —
 * `specifications/ui/GHA-github-authentication.md` (GHA-FR-15..22).
 *
 * It opens in exactly two situations, and the difference is only in what is
 * waiting on it:
 *
 * - An authenticated GitHub operation reported `selection_required`
 *   (GHA-FR-16): the operation runs on confirm and is abandoned on cancel
 *   (GHA-FR-17). This is the Git panel's route (GHA-FR-16).
 * - The author opened it from the Project settings Project section with nothing
 *   pending (GHA-FR-20): confirm rebinds, cancel leaves the binding untouched.
 *
 * It never opens when the project already resolves a token — bound, or the one
 * stored token used implicitly (GHA-FR-18) — nor when none is stored at all
 * (GHA-FR-19). Deciding that is the caller's job, because the caller is the one
 * holding the failed operation.
 */

export interface GithubTokenPickerProps {
  /** Named in the title so the choice is visibly per-project. */
  projectName: string;
  /** Preselected on open — the current binding, when there is one (GHA-FR-20). */
  currentTokenId?: string | null;
  /** GHA-FR-17 / GHA-FR-20: abandons whatever was waiting; records nothing. */
  onCancel: () => void;
  /** Resolves after the binding is persisted, so the caller can then proceed. */
  onConfirm: (tokenId: string) => void;
  /**
   * GHA-FR-20: whether the picker is being presented **within the Project
   * settings window** rather than over the main window.
   *
   * The only thing it changes is the note below the list: telling the author
   * that the choice can be changed in Project settings is useful where a Git
   * operation opened the picker, and reads as nonsense where Project settings
   * is the window they are looking at.
   */
  inProjectSettings?: boolean;
}

export function GithubTokenPicker({
  projectName,
  currentTokenId,
  onCancel,
  onConfirm,
  inProjectSettings = false,
}: GithubTokenPickerProps) {
  const [tokens, setTokens] = useState<GithubTokenRecord[] | null>(null);
  const [selected, setSelected] = useState<string | null>(currentTokenId ?? null);
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const dialogRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    let cancelled = false;
    void api
      .listGithubTokens()
      .then((list) => {
        if (cancelled) return;
        // Defensive: the modal must not crash on a backend (or a test double)
        // that answers with nothing where a list was promised.
        const rows = Array.isArray(list) ? list : [];
        setTokens(rows);
        // Preselect the current binding, else the first row, so Enter always
        // has a meaning and the modal is never a dead end.
        setSelected((prev) => prev ?? rows[0]?.id ?? null);
      })
      .catch((e) => {
        if (cancelled) return;
        setTokens([]);
        setError(tokenErrorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // GHA-FR-17: Escape is a cancel like any other.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onCancel]);

  const confirm = async () => {
    if (!selected || submitting) return;
    setSubmitting(true);
    setError("");
    try {
      await api.setProjectGithubTokenBinding(selected);
      onConfirm(selected);
    } catch (e) {
      setError(tokenErrorMessage(e));
      setSubmitting(false);
    }
  };

  /**
   * GHA-FR-21: the picker takes focus when it opens.
   *
   * The scrim stops the pointer reaching what is behind it, and until this the
   * keyboard was not stopped with it — focus stayed on whatever opened the
   * picker, and Tab walked the surface underneath rather than the dialog. That
   * was survivable while the picker only ever opened over the main window,
   * which has a menu bar and a tab strip to get back from; it is not survivable
   * in the Project settings window, which since SWN-FR-01 has neither and whose
   * parent is blocked besides (SWN-FR-02).
   *
   * The first row where there is one, so the author can arrow through the list
   * immediately; the dialog itself while the list is still loading.
   */
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    const first = dialog.querySelector<HTMLElement>('[role="radio"]');
    (first ?? dialog).focus();
  }, [tokens === null]);

  return (
    <div
      className="scrim"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="token-picker-title"
        data-testid="github-token-picker"
        // Focusable so the effect below has somewhere to put focus on a picker
        // that is still loading its list, without that place being a tab stop
        // of its own once there is one.
        tabIndex={-1}
      >
        <div className="modal__head">
          <Icon.GitPull size={14} />
          <div className="modal__title" id="token-picker-title">
            Which GitHub token for {projectName}?
          </div>
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            onClick={onCancel}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body">
          {tokens === null && <div className="t-muted">Loading…</div>}

          {/* GHA-FR-15: a single-selection list, rendered with the same fields
              as a Global settings row (GHA-FR-02) — same vocabulary, same
              masking (GHA-FR-03). GHA-FR-22: no add, rename, or remove here. */}
          <div role="radiogroup" aria-label="GitHub tokens">
            {(tokens ?? []).map((t) => (
              <div
                key={t.id}
                role="radio"
                aria-checked={selected === t.id}
                aria-label={t.label}
                tabIndex={0}
                className="card"
                data-testid="picker-token-row"
                data-active={selected === t.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  padding: "10px 14px",
                  marginBottom: 6,
                  cursor: "pointer",
                  borderColor: selected === t.id ? "var(--accent)" : undefined,
                  background: selected === t.id ? "var(--bg-active)" : undefined,
                }}
                onClick={() => setSelected(t.id)}
                onKeyDown={(e) => {
                  // GHA-NFR: operable without a pointer.
                  if (e.key === " " || e.key === "Enter") {
                    e.preventDefault();
                    setSelected(t.id);
                  }
                }}
              >
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
                    {t.label}
                  </div>
                  <TokenIdentityLine token={t} />
                </div>
              </div>
            ))}
          </div>

          <p className="t-ui-xs t-muted" style={{ marginTop: 10 }}>
            Synthesis remembers this choice for this project.
            {inProjectSettings ? "" : " Change it in Project settings."}
          </p>

          {error && (
            <span className="picker-error" data-testid="picker-error">
              ✗ {error}
            </span>
          )}
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!selected || submitting}
            onClick={() => void confirm()}
          >
            Use
          </button>
        </div>
      </div>
    </div>
  );
}
