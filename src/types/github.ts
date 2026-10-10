// GitHub tokens (GTS-github-token-storage.md / GHA-github-authentication.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

// --- GitHub tokens (GTS-github-token-storage.md / GHA-github-authentication.md)

/**
 * A stored token's verification state. `unavailable` is derived by the backend
 * at listing time for a record whose keychain entry cannot be read (GTS-FR-08)
 * — a wiped keychain, or a machine restored from a backup that excluded it.
 */
export type GithubTokenState = "valid" | "invalid" | "unverified" | "unavailable";

/**
 * GTS-FR-02: the ONLY representation of a token that crosses the IPC boundary.
 *
 * It carries no secret material by construction — `maskedHint` is the secret's
 * last four characters and is the closest this shape comes to one. There is
 * deliberately no field a secret could ride in, and the backend has no command
 * that would return one (GTS-FR-13), so no amount of UI code can render a token
 * in full (GHA-FR-03).
 *
 * Field names must match `GithubTokenRecord` in
 * `src-tauri/src/github_tokens.rs` byte-for-byte: the Rust struct carries
 * `#[serde(default)]` and does not deny unknown fields, so a misspelled key
 * here is not an error — it is silently `undefined` at runtime.
 */
export interface GithubTokenRecord {
  id: string;
  label: string;
  /** Resolved by verification; null until one succeeds. */
  accountLogin: string | null;
  /** Empty for a fine-grained token, which reports no scopes. */
  scopes: string[];
  /** The secret's last four characters. */
  maskedHint: string;
  /**
   * GTS host: the normalized GitHub host this token belongs to, for example
   * `github.com` or `company.ghe.com`. Treat a missing or empty value as
   * `github.com` (see `githubHostOf`).
   */
  host: string;
  addedAt: string;
  lastVerifiedAt: string | null;
  state: GithubTokenState;
}

/**
 * GTS-FR-10: how the open project resolves a token, and therefore whether the
 * UI must prompt.
 *
 * - `bound` — a recorded binding whose token still exists.
 * - `implicit` — nothing bound, but exactly one token is stored, so it is used
 *   without asking. The single-token author never sees the picker (GHA-FR-18).
 * - `selection_required` — two or more tokens and nothing chosen. The picker
 *   opens (GHA-FR-16).
 * - `none_stored` — nothing to pick between; route to Global settings instead
 *   (GHA-FR-19).
 */
export type GithubBindingResolution =
  | "bound"
  | "implicit"
  | "selection_required"
  | "none_stored";

export interface GithubTokenBinding {
  tokenId: string | null;
  resolution: GithubBindingResolution;
}

/**
 * The typed errors the GitHub token commands reject with, mirroring the
 * constants in `src-tauri/src/github_tokens.rs`.
 *
 * These are matched on, not just displayed: `invalid_token` and
 * `github_unreachable` call for different responses from the user (GHA-FR-10),
 * and `github_token_selection_required` versus `github_token_missing` decides
 * whether the Git panel opens the picker or routes to Global settings
 * (GHA-FR-16). A divergence from the Rust spelling is silent, which is why both
 * sides keep the list as named constants rather than inline literals.
 */
export const GITHUB_TOKEN_ERRORS = {
  invalidToken: "invalid_token",
  duplicateLabel: "duplicate_label",
  githubUnreachable: "github_unreachable",
  keychainUnavailable: "keychain_unavailable",
  unknownToken: "unknown_token",
  selectionRequired: "github_token_selection_required",
  tokenMissing: "github_token_missing",
  /**
   * GTS-FR-16: a token resolves, but the registry holds no verified account for
   * it — so a comment has nobody to be attributed to (CMT-FR-24). Distinct from
   * `tokenMissing` because the author's fix is to verify the token they have
   * rather than to add one.
   */
  identityUnresolved: "github_identity_unresolved",
  /** The domain typed in the Add token dialog is not a valid host (GHA-FR-OGNL). */
  invalidHost: "invalid_host",
  /**
   * The token of the project belongs to another host than the remote of the
   * project (GHA-FR-LBLM). It is not a selection problem, so it never opens the
   * picker.
   */
  hostMismatch: "github_host_mismatch",
} as const;

/** GHA-FR-LBLM: the words every surface uses for `github_host_mismatch`. */
export const GITHUB_HOST_MISMATCH_MESSAGE =
  "The project token belongs to another GitHub host than this remote. Pick or add a token for the host of the remote.";

/** The host of `github.com`, which is the default and is not shown in the UI. */
export const DEFAULT_GITHUB_HOST = "github.com";

/** The host of a token record. A missing or empty host means `github.com`. */
export function githubHostOf(token: { host?: string | null }): string {
  const host = (token.host ?? "").trim().toLowerCase();
  return host === "" ? DEFAULT_GITHUB_HOST : host;
}

/**
 * The typed errors a remote operation rejects with beyond the token vocabulary
 * above, mirroring the constants in `src-tauri/src/git.rs`.
 *
 * `noRemoteConfigured` never reaches a refresh's `remoteError` — the backend
 * reports it as `remoteState: "skipped"` instead (WTC-FR-23) — but it is the
 * typed rejection of the fetch primitive itself (GTC-FR-14), and the refresh
 * control names it as a distinct cause (WTS-FR-34).
 */
export const GIT_REMOTE_ERRORS = {
  noRemoteConfigured: "no remote configured",
} as const;
