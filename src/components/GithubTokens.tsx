import { useEffect, useState } from "react";
import * as api from "../api";
import { Icon } from "./icons";
import { GITHUB_TOKEN_ERRORS, type GithubTokenRecord } from "../types";

/**
 * The GitHub section of the Global settings window, and the Add token dialog it
 * opens — `specifications/ui/GHA-github-authentication.md` (GHA-FR-01..14).
 * Its backend is `specifications/core/GTS-github-token-storage.md`; every
 * operation name matches that spec's contract surface byte-for-byte.
 *
 * GHA-FR-03 is the invariant the whole surface is built around: no rendering
 * path here can show a token beyond its masked hint, because the backend never
 * returns more (GTS-FR-02). The one place a full secret exists is the add
 * dialog's `secret` state, and GHA-FR-09 governs how briefly it lives there.
 */

/** Render a typed backend rejection as text that says what to do about it. */
export function tokenErrorMessage(e: unknown): string {
  const raw = typeof e === "string" ? e : e instanceof Error ? e.message : "";
  switch (raw) {
    // GHA-FR-10: the three rejections are deliberately distinguishable — a bad
    // token, a network that never answered, and a keychain that refused are
    // three different things for the user to do next.
    case GITHUB_TOKEN_ERRORS.invalidToken:
      return "GitHub rejected that token. Check you pasted it whole, and that it has not expired.";
    case GITHUB_TOKEN_ERRORS.githubUnreachable:
      return "Could not reach GitHub. The token was not checked and nothing was stored.";
    case GITHUB_TOKEN_ERRORS.keychainUnavailable:
      return "The system keychain is unavailable, so the token could not be stored.";
    case GITHUB_TOKEN_ERRORS.duplicateLabel:
      return "Another token already uses that name. Pick a different one.";
    case GITHUB_TOKEN_ERRORS.unknownToken:
      return "That token is no longer stored.";
    // These two reach the surface that requested the blocked operation
    // (GHA-FR-16), which phrases them in terms of what it was trying to do.
    // Given text here too so neither can ever render as a raw slug.
    case GITHUB_TOKEN_ERRORS.selectionRequired:
      return "Choose which GitHub token this project should use.";
    case GITHUB_TOKEN_ERRORS.tokenMissing:
      return "No GitHub token is stored. Add one in Global settings → GitHub.";
    // GTS-FR-16: the token is there but names no account, so there is nobody to
    // attribute a comment to (CMT-FR-24). Verifying resolves it, which is a
    // different action from adding a token — hence a message of its own.
    case GITHUB_TOKEN_ERRORS.identityUnresolved:
      return "That token has not been checked against GitHub yet, so it names no account. Verify it in Global settings → GitHub.";
    default:
      return raw || "operation failed";
  }
}

const STATE_LABEL: Record<GithubTokenRecord["state"], string> = {
  valid: "valid",
  invalid: "invalid",
  unverified: "unverified",
  unavailable: "unavailable",
};

/**
 * GHA-FR-02: a token row's second line — the account, the granted scopes, and
 * the masked hint. Shared with the picker modal (GHA-FR-15) so the two surfaces
 * describe a token identically rather than drifting apart.
 */
export function TokenIdentityLine({ token }: { token: GithubTokenRecord }) {
  const parts = [
    token.accountLogin ? `@${token.accountLogin}` : "account not resolved",
    token.scopes.length > 0 ? token.scopes.join(", ") : "no scopes reported",
    // GHA-FR-03: four characters is the most of a token that is ever rendered.
    `••••${token.maskedHint}`,
  ];
  return (
    <div className="t-ui-sm t-muted" data-testid="token-identity">
      {parts.join(" · ")}
    </div>
  );
}

interface AddTokenDialogProps {
  onClose: () => void;
  /** Resolves once the token is stored; rejects with a typed backend error. */
  onAdd: (label: string, secret: string) => Promise<void>;
  onOpenGithub: () => Promise<void>;
}

/**
 * GHA-FR-05: one form — a pasted secret and an optional label — with a standing
 * hand-off to GitHub above it.
 *
 * There is no generate-versus-paste mode, because there is no difference to
 * choose between: only GitHub can mint a token, so every path ends in the same
 * paste. Offering the hand-off as a mode would have hidden it from the author
 * who has no token yet and gated nothing for the one who does.
 */
export function AddTokenDialog({ onClose, onAdd, onOpenGithub }: AddTokenDialogProps) {
  const [label, setLabel] = useState("");
  // GHA-FR-09: the secret lives here and nowhere else. It is cleared the moment
  // the submission resolves either way, and unmounting the dialog discards it —
  // nothing in the UI can reproduce it afterwards.
  const [secret, setSecret] = useState("");
  const [error, setError] = useState("");
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);

  // GHA-FR-07: the only client-side validation. The label is optional — the
  // backend names an unlabelled token after the account it verified it against
  // (GTS-FR-05) — so the token itself is the one thing that must be present.
  // Whether it is a usable GitHub token is the backend's call, not a regex's.
  const canAdd = secret.trim() !== "" && !submitting;

  const submit = async () => {
    if (!canAdd) return;
    setSubmitting(true);
    setError("");
    try {
      await onAdd(label.trim(), secret.trim());
      // The parent unmounts this dialog on success (GHA-FR-08).
    } catch (e) {
      // GHA-FR-10: stay open, keep the label so it need not be retyped, and
      // clear the secret — a rejected paste is the thing to redo.
      setError(tokenErrorMessage(e));
      setSecret("");
      setSubmitting(false);
    }
  };

  return (
    <div
      className="scrim"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-labelledby="add-token-title">
        <div className="modal__head">
          <Icon.Plus size={14} />
          <div className="modal__title" id="add-token-title">
            Add GitHub token
          </div>
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body">
          {/* GHA-FR-06: always offered, never a mode. The dialog stays open
              across it — the token arrives back by paste, not by any channel
              the application controls. */}
          <div className="picker-field">
            <div className="t-ui-sm t-muted" style={{ marginBottom: 6 }}>
              Don&rsquo;t have one yet? Open GitHub with the scopes Synthesis
              needs already selected, create the token, then paste it below.
            </div>
            <button
              className="btn btn--default btn--sm"
              // `.picker-field` is a flex column, so a bare child stretches to
              // the full modal width and reads as another input.
              style={{ alignSelf: "flex-start" }}
              onClick={() => void onOpenGithub()}
            >
              Open GitHub token page
            </button>
          </div>

          <div className="picker-field">
            <label className="picker-field__label" htmlFor="gh-token-secret">
              Token
            </label>
            <input
              id="gh-token-secret"
              className="input input--mono"
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder="ghp_…"
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
          </div>

          <div className="picker-field">
            <label className="picker-field__label" htmlFor="gh-token-label">
              Label (optional)
            </label>
            <input
              id="gh-token-label"
              className="input"
              placeholder="Defaults to your GitHub account"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
            />
          </div>

          {/* GHA-FR-10 / layout note: rejections attach directly above the
              action row rather than displacing the inputs. */}
          {error && (
            <span className="picker-error" data-testid="add-token-error">
              ✗ {error}
            </span>
          )}
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!canAdd}
            onClick={() => void submit()}
          >
            Add
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * The GitHub section itself (GHA-FR-01, GHA-FR-14). Every action applies
 * immediately rather than through a section save, so this section holds no
 * dirty state and never contributes to the Global settings window's
 * save-before-close sweep (GLS-FR-15, SWN-FR-08).
 */
export function GithubTokens() {
  const [tokens, setTokens] = useState<GithubTokenRecord[] | null>(null);
  const [error, setError] = useState("");
  const [adding, setAdding] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const load = () =>
    api
      .listGithubTokens()
      .then((list) => {
        // Defensive: the section must not crash on a backend (or a test double)
        // that answers with nothing where a list was promised.
        setTokens(Array.isArray(list) ? list : []);
        setError("");
      })
      .catch((e) => {
        setTokens([]);
        setError(tokenErrorMessage(e));
      });

  useEffect(() => {
    void load();
  }, []);

  // GHA-FR-11: update the row in place from the returned record rather than
  // rebuilding the list, so a verify does not reorder or re-render the rest.
  const onVerify = async (id: string) => {
    setBusyId(id);
    setError("");
    try {
      const updated = await api.validateGithubToken(id);
      setTokens((prev) =>
        (prev ?? []).map((t) => (t.id === updated.id ? updated : t)),
      );
    } catch (e) {
      setError(tokenErrorMessage(e));
    } finally {
      setBusyId(null);
    }
  };

  // GHA-FR-12: a colliding label is refused by the backend and surfaces here.
  const onRename = async (token: GithubTokenRecord) => {
    const next = window.prompt("Token name", token.label)?.trim();
    if (!next || next === token.label) return;
    setBusyId(token.id);
    setError("");
    try {
      const updated = await api.renameGithubToken(token.id, next);
      setTokens((prev) =>
        (prev ?? []).map((t) => (t.id === updated.id ? updated : t)),
      );
    } catch (e) {
      setError(tokenErrorMessage(e));
    } finally {
      setBusyId(null);
    }
  };

  // GHA-FR-13: confirmation first, and it says what removal costs — projects
  // using this token will ask again which token to use.
  const onRemove = async (id: string) => {
    setBusyId(id);
    setError("");
    try {
      await api.removeGithubToken(id);
      setTokens((prev) => (prev ?? []).filter((t) => t.id !== id));
    } catch (e) {
      setError(tokenErrorMessage(e));
    } finally {
      setBusyId(null);
      setConfirmRemove(null);
    }
  };

  return (
    <div data-testid="github-tokens">
      <p className="t-p" style={{ marginBottom: 20 }}>
        Tokens Synthesis uses to talk to GitHub — opening pull requests, reading
        review comments, pushing over HTTPS. Each is stored in this machine's
        keychain; Synthesis never shows one again after you add it.
      </p>

      {tokens === null && <div className="t-muted">Loading…</div>}

      {/* GHA-FR-04: a first-class empty state, not an error. */}
      {tokens !== null && tokens.length === 0 && (
        <div
          className="card"
          style={{ padding: "32px 20px", textAlign: "center", color: "var(--fg-3)" }}
          data-testid="github-tokens-empty"
        >
          <div style={{ fontSize: "var(--fs-ui-md)" }}>No GitHub tokens yet.</div>
          <div className="t-ui-xs" style={{ marginTop: 6 }}>
            Add one to open pull requests and push over HTTPS from Synthesis.
          </div>
        </div>
      )}

      {tokens !== null && tokens.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {tokens.map((t) => (
            <div
              key={t.id}
              className="card"
              data-testid="github-token-row"
              // SWN-FR-03: this section lives in a window fixed at 800 px wide,
              // so the row can never be given more room than it has here. The
              // identity column asks for a real basis and the actions cluster
              // wraps to its own line when it cannot sit beside one, rather than
              // squeezing the label into a column one word-fragment wide.
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                flexWrap: "wrap",
                gap: 12,
                padding: "10px 14px",
              }}
            >
              <div style={{ flex: "1 1 260px", minWidth: 0 }}>
                <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
                  {t.label}
                  <span
                    className={
                      t.state === "valid"
                        ? "badge badge--ok"
                        : t.state === "unverified"
                          ? "badge"
                          : "badge badge--warn"
                    }
                    style={{ marginLeft: 6 }}
                    data-testid="token-state"
                  >
                    {STATE_LABEL[t.state]}
                  </span>
                </div>
                <TokenIdentityLine token={t} />
              </div>
              {/* GHA-NFR: a verification older than this session is rendered as
                  the state at its recorded time, not as a live fact. */}
              {/* GHA-FR-02: the added date is part of the row. The
                  verification date sits beside it when there is one, rendered
                  as the state at its recorded time rather than as a live fact
                  (GHA-NFR). */}
              {/* The dates and the three actions travel together: they are
                  what wraps when the identity column needs the width. */}
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  marginLeft: "auto",
                }}
              >
                <span className="t-meta" style={{ whiteSpace: "nowrap" }}>
                  added {t.addedAt.slice(0, 10)}
                  {t.lastVerifiedAt
                    ? ` · checked ${t.lastVerifiedAt.slice(0, 10)}`
                    : ""}
                </span>
                <button
                  className="btn btn--ghost btn--sm"
                  disabled={busyId === t.id}
                  onClick={() => void onVerify(t.id)}
                >
                  Verify
                </button>
                <button
                  className="btn btn--ghost btn--sm"
                  disabled={busyId === t.id}
                  onClick={() => void onRename(t)}
                >
                  Rename
                </button>
                <button
                  className="btn btn--danger btn--sm"
                  aria-label={`Remove ${t.label}`}
                  disabled={busyId === t.id}
                  onClick={() => setConfirmRemove(t.id)}
                >
                  Remove
                </button>
              </div>
            </div>
          ))}
        </div>
      )}

      <button
        className="btn btn--default btn--sm"
        style={{ alignSelf: "flex-start", marginTop: 12 }}
        onClick={() => setAdding(true)}
      >
        <Icon.Plus size={12} /> Add token
      </button>

      {error && (
        <span className="picker-error" style={{ display: "block", marginTop: 8 }}>
          ✗ {error}
        </span>
      )}

      {adding && (
        <AddTokenDialog
          onClose={() => setAdding(false)}
          onOpenGithub={() => api.openGithubTokenCreationPage()}
          onAdd={async (label, secret) => {
            const created = await api.addGithubToken(label, secret);
            setTokens((prev) => [...(prev ?? []), created]);
            setAdding(false);
          }}
        />
      )}

      {/* GHA-FR-13: the confirmation names the consequence rather than asking a
          bare "are you sure?". */}
      {confirmRemove && (
        <div
          className="scrim"
          onClick={(e) => {
            if (e.target === e.currentTarget) setConfirmRemove(null);
          }}
        >
          <div className="modal" role="dialog" aria-labelledby="remove-token-title">
            <div className="modal__head">
              <div className="modal__title" id="remove-token-title">
                Remove token
              </div>
            </div>
            <div className="modal__body">
              <p className="t-p">
                Any project currently using this token will ask again which token
                to use. The token itself is not revoked on GitHub.
              </p>
            </div>
            <div className="modal__actions">
              <button
                className="btn btn--ghost"
                onClick={() => setConfirmRemove(null)}
              >
                Cancel
              </button>
              <button
                className="btn btn--danger"
                onClick={() => void onRemove(confirmRemove)}
              >
                Remove
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
