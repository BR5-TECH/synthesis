//! The fetch primitive and the credential chain it presents
//! (GTC-FR-09, GTC-FR-12 .. GTC-FR-15).

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Instant;

use git2::Repository;

use crate::changes::{self};
use crate::github_tokens::{self, GithubTokens};
use crate::global_settings::GlobalSettingsStore;
use crate::log_fields;
use crate::logging::{self, Fields, LogBuffer, LogLevel, LogSink};
use crate::progress::{self, ProgressRegistry, ProgressSink};

use super::*;

// ---------------------------------------------------------------------------
// Fetch (GTC-FR-12 .. GTC-FR-15)
// ---------------------------------------------------------------------------

/// The project has no primary remote, so there is nothing to fetch from
/// (GTC-FR-14).
pub const ERR_NO_REMOTE_CONFIGURED: &str = "no remote configured";

/// The `kind` a fetch reports itself under (GTC-FR-15, per `PRG-FR-11`).
pub const PROGRESS_KIND_GIT: &str = "git";

/// Label the status bar renders while a fetch runs. Names the remote nowhere —
/// a remote URL can carry an embedded credential, and GTC-FR-11 keeps one out
/// of everything this module publishes.
pub(crate) const FETCH_LABEL: &str = "Fetching branches";

/// Is `url` an HTTPS remote on `github.com` or on a `*.ghe.com` host — a case
/// GTC-FR-09 authenticates with the token the open project resolves? A host that
/// only a stored token names is decided by `github_tokens::resolve_remote_token`.
///
/// An `ssh://git@github.com/...` or `git@github.com:owner/repo` remote is
/// deliberately **not** one: SSH authenticates with the author's own key, which
/// is what GTC-FR-09 means by a remote that "authenticates as it otherwise
/// would". Presenting a personal access token to it would fail regardless.
pub fn is_github_https_remote(url: &str) -> bool {
    // The host is what follows the last `@` of the authority, and a port is
    // stripped, so `https://user:pw@github.com:443/...` still matches.
    github_tokens::host::https_remote_host(url)
        .is_some_and(|host| github_tokens::host::is_github_family_host(&host))
}

/// GTC-FR-14: the two failures a transfer can end in that the caller presents
/// differently — a credential the remote refused, and a remote that could not
/// be reached at all.
///
/// Returns a constant rather than libgit2's own message on purpose. GTC-FR-11
/// forbids a token secret reaching any error this module returns, and a remote
/// URL echoed by the transport is exactly where an embedded credential would
/// appear; a fixed vocabulary cannot leak one however the transport phrases its
/// failure. The trade is that a diagnostic detail is lost, which is why the two
/// classes the caller acts on are separated here rather than left to the UI.
pub(crate) fn classify_transfer_error(e: &git2::Error) -> String {
    use git2::{ErrorClass, ErrorCode};
    let refused = matches!(e.code(), ErrorCode::Auth)
        // libgit2 reports an HTTP 401/403 under its own class rather than as
        // `Auth`, and an SSH key the server would not take arrives as a
        // callback-class failure from the credential chain below.
        || matches!(e.class(), ErrorClass::Http | ErrorClass::Callback | ErrorClass::Ssh);
    if refused {
        github_tokens::ERR_INVALID_TOKEN.to_string()
    } else {
        github_tokens::ERR_GITHUB_UNREACHABLE.to_string()
    }
}

/// The credentials a fetch may present, in the order it presents them.
///
/// libgit2 calls its credential callback repeatedly for one connection — once
/// per credential it is willing to try — and handing back the same rejected
/// credential every time is an infinite loop. So the callback walks a cursor
/// over this list and fails once it runs out, which is what turns a genuinely
/// refused credential into a prompt `GIT_EAUTH` rather than a hang.
pub(crate) enum Credential {
    /// A GitHub personal access token, as HTTP basic auth (GTC-FR-09).
    Token,
    /// The author's `ssh-agent` — the ordinary path for an SSH remote, and the
    /// only one that works for a passphrase-protected key.
    SshAgent,
    /// A private key on disk, with its `.pub` beside it when there is one.
    SshKey(PathBuf),
}

/// The SSH keys to offer, most-preferred first: the modern algorithms before
/// RSA, and only the ones that actually exist on disk.
///
/// Read by path rather than into memory (which is why git2's
/// `ssh_key_from_memory` feature stays off): a private key this process never
/// holds is one it cannot leak into a log or a crash dump.
pub(crate) fn ssh_key_candidates() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|name| home.join(".ssh").join(name))
        .filter(|path| path.is_file())
        .collect()
}

/// The credentials to offer, in order, for a remote whose token is `token` —
/// the GitHub secret when the remote is one GTC-FR-09 authenticates, and `None`
/// for every other remote, which falls through to the author's SSH credentials.
pub(crate) fn credential_chain(token: Option<&str>) -> Vec<Credential> {
    let mut chain: Vec<Credential> = Vec::new();
    if token.is_some() {
        chain.push(Credential::Token);
    }
    chain.push(Credential::SshAgent);
    chain.extend(ssh_key_candidates().into_iter().map(Credential::SshKey));
    chain
}

/// Answer one of libgit2's credential requests, advancing `cursor` past whatever
/// it offers.
///
/// A free function rather than the closure body itself, because
/// `RemoteCallbacks::credentials` is a setter with no way to invoke what it
/// installed — and the cursor discipline is the part worth testing: returning a
/// rejected credential a second time hangs a fetch instead of failing it.
pub(crate) fn next_credential(
    chain: &[Credential],
    cursor: &Cell<usize>,
    token: Option<&str>,
    username: &str,
    allowed: git2::CredentialType,
) -> Result<git2::Cred, git2::Error> {
    // An SSH URL with no user in it (`ssh://github.com/...`) makes libgit2 ask
    // for the username on its own first. Answering that is not an attempt at
    // authentication, so it does not consume a credential.
    if allowed.contains(git2::CredentialType::USERNAME) {
        return git2::Cred::username(username);
    }
    loop {
        let index = cursor.get();
        cursor.set(index + 1);
        let Some(candidate) = chain.get(index) else {
            // Every credential has been offered and refused. Failing the
            // callback is what surfaces as `GIT_EAUTH` (GTC-FR-14) rather than
            // libgit2 asking again forever.
            return Err(git2::Error::from_str("no usable credential"));
        };
        match candidate {
            Credential::Token if allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT) => {
                // GitHub takes a personal access token as the password with any
                // non-empty username over HTTPS basic auth.
                return git2::Cred::userpass_plaintext(username, token.unwrap_or_default());
            }
            Credential::SshAgent if allowed.contains(git2::CredentialType::SSH_KEY) => {
                return git2::Cred::ssh_key_from_agent(username);
            }
            Credential::SshKey(path) if allowed.contains(git2::CredentialType::SSH_KEY) => {
                let public = path.with_extension("pub");
                return git2::Cred::ssh_key(
                    username,
                    public.is_file().then_some(public.as_path()),
                    path,
                    None,
                );
            }
            // A credential of a kind this connection will not accept — try the
            // next rather than failing the whole chain on it.
            _ => continue,
        }
    }
}

/// Install the credential chain on `callbacks`.
pub(crate) fn install_credentials<'cb>(callbacks: &mut git2::RemoteCallbacks<'cb>, token: Option<&'cb str>) {
    let chain = credential_chain(token);
    let cursor = Cell::new(0usize);
    callbacks.credentials(move |_url, username_from_url, allowed| {
        next_credential(
            &chain,
            &cursor,
            token,
            username_from_url.unwrap_or("git"),
            allowed,
        )
    });
}

/// GTC-FR-12 / GTC-FR-13: bring the repository's remote-tracking refs into
/// agreement with what `remote_name` publishes, dropping the ones whose upstream
/// branch is gone.
pub(crate) fn fetch_with_prune(
    repo: &Repository,
    remote_name: &str,
    token: Option<&str>,
) -> Result<(), String> {
    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|_| ERR_NO_REMOTE_CONFIGURED.to_string())?;

    let mut callbacks = git2::RemoteCallbacks::new();
    install_credentials(&mut callbacks, token);

    let mut opts = git2::FetchOptions::new();
    // GTC-FR-13: the prune is part of the fetch rather than a second operation,
    // because a listing that keeps offering branches the remote no longer has
    // misrepresents what can be checked out. It is bounded to remote-tracking
    // refs — libgit2 deletes no local branch and touches nothing on the remote.
    opts.prune(git2::FetchPrune::On);
    opts.remote_callbacks(callbacks);

    // An empty refspec list uses the remote's own configured refspecs, which is
    // what maps its `refs/heads/*` onto this repository's remote-tracking refs.
    // Naming a refspec here instead would quietly ignore a repository that
    // fetches a narrower set than the default.
    remote
        .fetch::<&str>(&[], Some(&mut opts), None)
        .map_err(|e| classify_transfer_error(&e))
}

/// **The** fetch primitive (GTC-FR-12): update the repository's remote-tracking
/// refs from the project's primary remote and drop the stale ones.
///
/// Deliberately **not** a `#[tauri::command]`, so no frontend `invoke` reaches
/// it: `WTC-worktree-context.md` WTC-FR-22 composes it, exactly as WTC-FR-10
/// composes `checkout_branch_at`. One fetch in the codebase means one place the
/// credential is resolved and one place the prune is decided.
///
/// Writes nothing but refs (GTC-FR-12): no local branch is created, moved, or
/// deleted, `HEAD` is unchanged, and no worktree's index or working tree is
/// touched — so a fetch never changes what is checked out anywhere.
///
/// The sink is both the progress emitter and the log sink: an `AppHandle` is
/// both, and a fetch that reported its progress to one consumer and its outcome
/// to nowhere would be exactly the invisible operation the Logs panel exists to
/// prevent.
pub fn fetch_remote_branches<S>(
    sink: &S,
    buffer: &'static LogBuffer,
    registry: &ProgressRegistry,
    root: &Path,
    store: &GlobalSettingsStore,
    tokens: &GithubTokens,
    project_key: &str,
) -> Result<(), String>
where
    S: ProgressSink + LogSink + Clone + Send + 'static,
{
    let repo = changes::open_repo(root)
        .inspect_err(|e| log_failure(sink, buffer, TRANSFER, MSG_FETCH_FAILED, e, Fields::new()))?;
    let Some(remote_name) = changes::primary_remote_name(&repo) else {
        log_failure(
            sink,
            buffer,
            TRANSFER,
            MSG_FETCH_FAILED,
            ERR_NO_REMOTE_CONFIGURED,
            Fields::new(),
        );
        return Err(ERR_NO_REMOTE_CONFIGURED.to_string());
    };
    let url = match repo.find_remote(&remote_name) {
        Ok(remote) => remote.url().unwrap_or_default().to_string(),
        Err(_) => String::new(),
    };

    // GTC-FR-09 / GTC-FR-14: the credential is resolved BEFORE any transport is
    // opened, so a project that resolves no token makes no network request and
    // writes no ref — and the typed refusal reaches the caller unchanged, which
    // is what lets the UI tell "pick a token" from "there is none to pick".
    //
    // The secret is obtained at the moment of the operation and lives no longer
    // than this call: it is never cached and never part of a return payload.
    //
    // The refusal is logged here rather than left to the caller: it is the one
    // failure that happens *before* a transfer, and the record is what explains
    // a refresh that reached no remote at all.
    let token = github_tokens::resolve_remote_token(store, tokens, project_key, &url)
        .inspect_err(|e| {
            log_failure(
                sink,
                buffer,
                TRANSFER,
                MSG_FETCH_FAILED,
                e,
                log_fields! { "remote" => &remote_name },
            )
        })?;

    // The remote's *name*, never its URL, and a boolean for the credential —
    // whether a token was presented is the thing worth knowing, and the secret
    // itself appears in no record (GTC-FR-11).
    logging::log_info(
        sink,
        buffer,
        TRANSFER,
        "fetching remote branches",
        log_fields! { "remote" => &remote_name, "authenticated" => token.is_some() },
    );
    let started = Instant::now();

    // GTC-FR-15: reported through `PRG-progress-reporting.md` under
    // `kind = "git"` for as long as it runs, and emitting nothing on the
    // `"git output line"` channel — a fetch the author did not invoke in the Git
    // panel leaves no transcript in its push/pull output area.
    let result = progress::attribute(
        sink,
        registry,
        PROGRESS_KIND_GIT,
        FETCH_LABEL,
        Some(root.to_path_buf()),
        || fetch_with_prune(&repo, &remote_name, token.as_deref()),
    );

    let fields = log_fields! { "remote" => &remote_name, "durationMs" => duration_ms(started) };
    match &result {
        Ok(()) => log_ok(
            sink,
            buffer,
            LogLevel::Info,
            TRANSFER,
            "remote branches fetched",
            fields,
        ),
        Err(error) => log_failure(sink, buffer, TRANSFER, MSG_FETCH_FAILED, error, fields),
    }
    result
}
