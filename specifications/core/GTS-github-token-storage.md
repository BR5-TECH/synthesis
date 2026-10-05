# GitHub token storage

**Spec code:** `GTS`

## Intent
The backend home for GitHub credentials: it holds each personal access token's secret in the application's single keyring entry (`ASV-application-secret-vault.md`), under the path that entry reserves for GitHub tokens, keeps the describable facts about that token — a label, the account it authenticates as, the scopes GitHub granted it, the last four characters, when it was added and last verified — in the user-global settings store, and records which stored token each project uses. It is the backend half of `../ui/GHA-github-authentication.md`, and the only component in the application that can read a token's secret: the secret is written once from the add operation, read only by the Git module performing an authenticated operation, and never returned across the IPC boundary in any form. Out of scope: hosts other than `github.com`, so no token carries a host; credentials for any other provider; and minting a token, which only GitHub can do — this module can open the browser at the right page with the right scopes, and nothing more.

## Contract surface
The module owns the `github.tokens` namespace of the application secret vault (`ASV-application-secret-vault.md` ASV-FR-03), a metadata registry inside the user-global store (`GSS-global-settings-storage.md` GSS-FR-22), the per-project binding in that store's per-project slot (GSS-FR-23), and the Tauri commands below. Names match `../ui/GHA-github-authentication.md` byte-for-byte.

### The outbound token record
Every command that describes a token returns this shape and no other. It carries no secret material:

```
GithubTokenRecord {
  id:              string,                                   // stable, opaque; the vault key of the secret
  label:           string,                                   // author-chosen, unique across the registry
  account_login:   string | null,                            // resolved by verification
  scopes:          string[],                                 // granted scopes, as GitHub reports them
  masked_hint:     string,                                   // the secret's last four characters
  added_at:        timestamp,
  last_verified_at: timestamp | null,
  state:           "valid" | "invalid" | "unverified" | "unavailable",
}
```

### Tauri commands

Token registry (per `../ui/GHA-github-authentication.md` GitHub section):
- `"list github tokens"` → `list_github_tokens` → `GithubTokenRecord[]`, ordered by `added_at`. Reads the registry and asks the vault once for the presence of every record's secret; contacts GitHub not at all.
- `"add github token (label, secret)"` → `add_github_token(label, secret)` → `GithubTokenRecord` or a typed error (`invalid_token`, `duplicate_label`, `github_unreachable`, `keychain_unavailable`). An empty `label` is filled in from the resolved account (GTS-FR-05).
- `"validate github token"` → `validate_github_token(id)` → `GithubTokenRecord` with a refreshed `state`, `account_login`, `scopes`, and `last_verified_at`.
- `"rename github token"` → `rename_github_token(id, label)` → `GithubTokenRecord` or a typed `duplicate_label` error.
- `"remove github token"` → `remove_github_token(id)`.
- `"open github token creation page"` → `open_github_token_creation_page` — opens the OS default browser at GitHub's token-creation page with the scopes this application needs pre-selected.

Project binding (per `../ui/GHA-github-authentication.md` picker modal and `../ui/SET-project-settings.md` SET-FR-12):
- `"get project github token binding"` → `get_project_github_token_binding` → `{ token_id: string | null, resolution: "bound" | "implicit" | "selection_required" | "none_stored" }`
- `"set project github token binding"` → `set_project_github_token_binding(token_id)`

Internal (no UI consumer; not a Tauri command and unreachable from the frontend):
- `resolve_github_token_secret()` — returns the secret of the token the open project resolves to, or the same typed refusal `get_project_github_token_binding` would justify. Called by `GTC-git.md` (GTC-FR-09) and by nothing else.
- `resolve_github_identity()` — returns `{ login, display_name?, email? }` for the account the open project's token authenticates as, or the same typed refusal, and never touches a secret on the way out. Called by `CMS-comments-storage.md` (CMS-FR-12) to attribute a comment.

## Functional requirements
1. **GTS-FR-01** A token's secret is held only in the application secret vault (`ASV-application-secret-vault.md` ASV-FR-01), at the path `["github", "tokens", <id>]`, and this module holds no keyring entry of its own. No secret is written to `app_data_dir()/synthesis.toml`, to any file under a project's `.synthesis/`, to any log line, or to any error payload.
2. **GTS-FR-02** No Tauri command returns a token's secret or any part of it beyond `masked_hint`, the last four characters. `GithubTokenRecord` is the only representation of a token that crosses the IPC boundary, and it is the shape every command in the contract surface returns.
3. **GTS-FR-03** The describable facts of every token — the whole `GithubTokenRecord` minus nothing, since it carries no secret — live in the user-global registry (`GSS-global-settings-storage.md` GSS-FR-22). The vault holds secrets, the registry holds descriptions, and neither holds the other's content.
4. **GTS-FR-04** `add_github_token(label, secret)` verifies the secret against GitHub before storing anything: it resolves the account login, the account's display name and email, and the granted scopes, then writes the secret into the vault and the registry record in that order. A verification that fails, a GitHub that cannot be reached, and a vault that refuses the write each leave no secret at the token's path and no registry record behind, and each returns its own typed error.
5. **GTS-FR-05** A label is unique across the registry. `add_github_token` and `rename_github_token` return a typed `duplicate_label` error rather than storing a second token under an existing label, so the picker modal and the settings list never present two rows an author cannot tell apart. A label supplied by the author is checked before verification, so a collision costs a round trip rather than a wait on GitHub. The label is **optional**: `add_github_token` accepts an empty one and files the token under the account it just resolved, suffixing it (`raver119`, `raver119 (2)`, …) when that account already names a stored token. A derived label never fails as a duplicate — the author did not choose it, so refusing their token over it would be nonsense — and a verification that resolves no login still yields a nameable record.
6. **GTS-FR-06** The same secret may be stored more than once under different labels; this module identifies a token by its `id`, not by its secret, and never compares two secrets.
7. **GTS-FR-07** `validate_github_token(id)` reads the secret from the vault, asks GitHub who it authenticates as and what it may do, and writes the resolved `account_login`, the account's display name and email, `scopes`, `last_verified_at`, and `state` back to the registry. A token GitHub rejects becomes `invalid` and stays stored, so the author can see which of their tokens has expired rather than losing the row.
8. **GTS-FR-08** A registry record whose secret the vault does not hold, or cannot answer for — the keychain was wiped, the machine was restored from a backup that excluded it, the vault value is malformed, or the OS refuses access — reads back with `state = "unavailable"`. The record is retained rather than pruned, because the author is better served by a row they can remove deliberately than by a token that silently vanished.
9. **GTS-FR-09** `remove_github_token(id)` removes the token's path from the vault and deletes the registry record together. Neither a path the vault does not hold nor a missing registry record makes removal fail; the operation is idempotent and its outcome is that neither exists.
10. **GTS-FR-10** `get_project_github_token_binding` resolves the open project's token by these rules, and the `resolution` it returns is what tells the UI whether to prompt: no token stored at all resolves `none_stored` with a null id; a recorded binding whose token still exists resolves `bound`. A binding that is absent, or that points at a token since removed, is resolved the same way — by how many tokens remain: exactly one resolves `implicit` carrying that token's id, so a single-token author is never asked to choose between a set of one, and two or more resolves `selection_required` with a null id.
11. **GTS-FR-11** `set_project_github_token_binding(token_id)` writes the binding into the user-global per-project slot (`GSS-global-settings-storage.md` GSS-FR-23). The slot is keyed by project rather than by directory (per `GSS-global-settings-storage.md` GSS-FR-18), so the binding is one fact for the repository and survives a change of active worktree; it is never written into a project's `.synthesis/`, which is committed content.
12. **GTS-FR-12** `open_github_token_creation_page` opens the OS default browser at GitHub's token-creation page with the `repo` and `workflow` scopes pre-selected — `repo` because pull-request and HTTPS push operations need it, `workflow` because a project's artifacts may include workflow definitions a push would otherwise be refused for. It transmits nothing, receives nothing, and returns as soon as the URL has been handed to the OS; the token comes back only by the author pasting it.
13. **GTS-FR-13** `resolve_github_token_secret()` is the single path by which a GitHub token secret leaves the vault. It is not registered as a Tauri command, so no frontend call can reach it, and it refuses with the same typed distinction `get_project_github_token_binding` reports rather than returning a secret when the project resolves none.
14. **GTS-FR-14** Every typed failure the vault reports (`ASV-application-secret-vault.md` ASV-FR-31) becomes one `keychain_unavailable` error from whichever operation touched it, and leaves the registry exactly as it was. Listing tokens keeps working while the vault is unavailable, because listing reads the registry alone (GTS-FR-03).
15. **GTS-FR-15** Every command in the contract surface exists with a typed payload in the walking-skeleton build. A stub may keep the registry in memory and use the walking-skeleton vault of `ASV-application-secret-vault.md` ASV-FR-33, provided it returns the documented shapes and still never returns a secret.
16. **GTS-FR-16** The account facts verification resolves — the login, the account's display name, and its email — are retained in the registry alongside the rest of the record (GTS-FR-03), and `resolve_github_identity()` serves them for the token the open project resolves to. A display name or an email GitHub does not report is absent rather than substituted. Like `resolve_github_token_secret`, it is not registered as a Tauri command and refuses with the same typed distinction `get_project_github_token_binding` reports (GTS-FR-10); unlike it, it reads the registry rather than the vault, so it answers while the vault is unavailable and returns nothing that is or derives from a secret.
17. **GTS-FR-17** This module supplies the vault with one migration candidate per registry record: the legacy service name `com.synthesis.github-token`, the record's id as the legacy account name, and the path of GTS-FR-01 (`ASV-application-secret-vault.md` ASV-FR-20). It reads no legacy entry itself, and it supplies a candidate for every record whether or not the vault already holds that record's secret, because which secrets are adopted and which legacy entries are deleted is the vault's decision (`ASV-application-secret-vault.md` ASV-FR-21, ASV-FR-25).

## Non-functional requirements
- A secret exists in application memory only for the duration of the operation that carries it — the add that stores it, the verification that checks it, the Git operation that presents it to GitHub — and is not retained between operations or cached.
- No error message, event payload, or streamed line produced anywhere in the application can contain a secret; the only token-derived text that ever leaves this module is `masked_hint`.
- `list_github_tokens` completes without network access, so the Global settings GitHub section renders offline. It asks the vault once for the presence of every record's secret (`ASV-application-secret-vault.md` ASV-FR-30), which is local, fast, and one keyring access however many tokens are stored, and that answer is what resolves the `unavailable` state of GTS-FR-08; a vault that will not answer downgrades the records rather than failing the listing (GTS-FR-14).
- Verification is the only operation this module performs against the network, and a failure to reach GitHub is always distinguishable from a token GitHub rejected.
