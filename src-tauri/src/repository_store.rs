//! Repository machine storage (`specifications/core/RMS-repository-machine-storage.md`).
//!
//! The per-machine home for the records this application keeps about a
//! **repository** rather than about a checkout of one: the conversation logs of
//! `CMS-comments-storage.md` and the draft statistics logs of
//! `DSS-draft-statistics-storage.md`.
//!
//! Two properties decide everything here. The store is keyed by the
//! **repository identity** — the repository's primary worktree — so every
//! worktree of one repository reads and writes one set of records and a switch
//! of active worktree selects nothing new (RMS-FR-QJVT, RMS-FR-TCAF). And the
//! store stands under `app_data_dir()`, outside every worktree, so nothing it
//! holds is staged, committed, or reported as a change of the project
//! (RMS-FR-HDNZ).
//!
//! The directory name is a readable slug plus a digest (RMS-FR-ZXHM): the digest
//! is what makes two repositories collide only by a 128-bit accident, and the
//! slug is what lets a person recognise the folder in a file manager.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::fs as fsa;

mod import;

pub use import::{import_legacy_storage, schedule_import, ImportOutcome};

/// RMS-FR-ZXHM: the folder under `app_data_dir()` the per-repository stores sit
/// in.
pub const REPOSITORIES_DIRNAME: &str = "repositories";

/// RMS-FR-KRYP: the record naming the repository a store belongs to.
pub const IDENTITY_FILE: &str = "repository.toml";

/// RMS-FR-MVDU: the conversation logs of `CMS-comments-storage.md` (CMS-FR-37).
pub const COMMENTS_DIRNAME: &str = "comments";

/// RMS-FR-MVDU: the per-draft logs of `DSS-draft-statistics-storage.md`
/// (DSS-FR-KQVN).
pub const STATISTICS_DIRNAME: &str = "statistics";

/// RMS-FR-MVDU: one draft's own conversation folder, addressed by its stable id.
pub const DRAFTS_DIRNAME: &str = "drafts";

/// RMS-FR-WGQS: the store could not be resolved or created.
pub const ERR_STORE_UNAVAILABLE: &str = "store_unavailable";

/// RMS-FR-ZXHM: how much of the identity's digest names the folder.
///
/// 32 hexadecimal characters is 128 bits, the same width `comments::log_id`
/// takes for the same reason: far past the point where a collision inside one
/// application data directory is a thing that happens.
const DIGEST_CHARS: usize = 32;

/// RMS-FR-ZXHM: the longest readable prefix a store folder carries.
const SLUG_CHARS: usize = 24;

/// RMS-FR-ZXHM: what a slug reduces to when the identity's last segment holds
/// no character a slug may carry.
const SLUG_FALLBACK: &str = "repository";

/// RMS-FR-BNKD: one spelling of a repository identity.
///
/// The path is made absolute and canonical where the filesystem can answer,
/// every `\` becomes `/`, and a trailing `/` is removed. A path that cannot be
/// canonicalised — a repository on a volume that is not mounted right now — is
/// normalised as given rather than refused, so a store already written for it
/// stays reachable.
pub fn normalize_identity(path: &Path) -> String {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let text = canonical.to_string_lossy().replace('\\', "/");
    // A root path is `/` and must not normalise to the empty string.
    let trimmed = text.trim_end_matches('/');
    if trimmed.is_empty() {
        text
    } else {
        trimmed.to_string()
    }
}

/// RMS-FR-ZXHM: the folder name a normalised identity maps to.
///
/// `<slug>-<digest>`. The slug is derived from the identity's last path segment
/// and carries no information the digest does not; it is there to be read.
pub fn store_dirname(identity: &str) -> String {
    format!("{}-{}", slug_of(identity), digest_of(identity))
}

/// RMS-FR-ZXHM: the full store path for an identity, given the application data
/// root.
pub fn store_dir_in(app_data: &Path, identity: &str) -> PathBuf {
    app_data
        .join(REPOSITORIES_DIRNAME)
        .join(store_dirname(identity))
}

fn digest_of(identity: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(identity.as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .chars()
        .take(DIGEST_CHARS)
        .collect()
}

fn slug_of(identity: &str) -> String {
    let last = identity.rsplit('/').find(|s| !s.is_empty()).unwrap_or("");
    let reduced: String = last
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(SLUG_CHARS)
        .collect();
    // A reduction that is only separators reads as nothing, so it is treated as
    // nothing rather than left as a run of dashes in front of the digest.
    if reduced.chars().all(|c| c == '-' || c == '_') {
        return SLUG_FALLBACK.to_string();
    }
    reduced
}

/// What a store's `repository.toml` holds (RMS-FR-KRYP).
#[derive(serde::Serialize, serde::Deserialize)]
struct IdentityRecord {
    identity: String,
}

/// RMS-FR-KRYP / RMS-FR-WGQS: the store directory for `identity`, created where
/// it does not exist.
///
/// The directory and its `repository.toml` are created on the first call for a
/// project and preserved unchanged afterwards, so a record an earlier build
/// wrote is never rewritten. Every failure — an unresolvable `app_data_dir()`,
/// a directory that cannot be created — is the one typed `store_unavailable`,
/// because a caller can do exactly one thing about any of them.
pub fn ensure_store(
    identity: &str,
    access: &std::sync::Arc<fsa::FsAccess>,
) -> Result<PathBuf, String> {
    let app_data = fsa::app_data_dir().map_err(|_| ERR_STORE_UNAVAILABLE.to_string())?;
    ensure_store_in(&app_data, identity, access)
}

/// [`ensure_store`] against a named application data root, which is what a test
/// hands it so a suite never writes into the author's real application data.
pub fn ensure_store_in(
    app_data: &Path,
    identity: &str,
    access: &std::sync::Arc<fsa::FsAccess>,
) -> Result<PathBuf, String> {
    let dir = store_dir_in(app_data, identity);
    let root = fsa::RootFs::new(app_data.to_path_buf(), access.clone());
    root.create_dir_under(app_data, PathBuf::from(REPOSITORIES_DIRNAME))
        .or_else(already_there)
        .map_err(|_| ERR_STORE_UNAVAILABLE.to_string())?;
    root.create_dir_under(
        app_data.join(REPOSITORIES_DIRNAME),
        PathBuf::from(store_dirname(identity)),
    )
    .or_else(already_there)
    .map_err(|_| ERR_STORE_UNAVAILABLE.to_string())?;

    // RMS-FR-KRYP: written once and preserved. A record already there is left
    // exactly as it is, whatever it says, so nothing this build does can rename
    // a store an earlier one created. Matched on **absence** rather than on any
    // failure to stat: a record this process cannot read is still the record,
    // and rewriting it would be the one thing this clause forbids.
    let marker = dir.join(IDENTITY_FILE);
    if matches!(root.file_info(&marker), Err(fsa::FsError::NotFound { .. })) {
        let record = IdentityRecord {
            identity: identity.to_string(),
        };
        root.write_toml_atomic(&marker, &record)
            .map_err(|_| ERR_STORE_UNAVAILABLE.to_string())?;
    }
    Ok(dir)
}

/// A directory that already exists is an already completed creation.
fn already_there(e: fsa::FsError) -> Result<(), fsa::FsError> {
    match e {
        fsa::FsError::AlreadyExists { .. } => Ok(()),
        other => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RMS-FR-BNKD: one spelling, whatever separator and trailing slash the
    /// caller held.
    #[test]
    fn an_identity_normalises_to_one_spelling() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().to_path_buf();
        let canonical = normalize_identity(&path);
        assert_eq!(canonical, normalize_identity(&path.join("")));
        assert!(!canonical.ends_with('/'), "{canonical} keeps a trailing slash");
        assert!(!canonical.contains('\\'), "{canonical} keeps a backslash");
    }

    /// RMS-FR-BNKD: a path the filesystem cannot answer for is normalised as
    /// given rather than refused.
    #[test]
    fn an_unmounted_identity_still_normalises() {
        let normalized = normalize_identity(Path::new("/nowhere/acme-main/"));
        assert_eq!(normalized, "/nowhere/acme-main");
    }

    /// RMS-FR-ZXHM: the folder name is a readable slug and a 32-character
    /// digest.
    #[test]
    fn a_store_folder_carries_a_slug_and_a_digest() {
        let name = store_dirname("/home/raver/code/Acme Repo");
        let (slug, digest) = name.rsplit_once('-').expect("a digest");
        assert_eq!(slug, "acme-repo");
        assert_eq!(digest.len(), DIGEST_CHARS);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
    }

    /// RMS-FR-ZXHM: a last segment that reduces to nothing readable still names
    /// a folder.
    #[test]
    fn a_nameless_identity_falls_back_to_a_readable_slug() {
        assert!(store_dirname("/....").starts_with("repository-"));
        assert!(store_dirname("/").starts_with("repository-"));
    }

    /// RMS-FR-LPWG: one identity always selects one store, and two identities
    /// always select two.
    #[test]
    fn rms_the_mapping_is_stable_and_collision_resistant() {
        let a = "/home/raver/code/acme";
        let b = "/home/raver/code/acme-2";
        assert_eq!(store_dirname(a), store_dirname(a));
        assert_ne!(store_dirname(a), store_dirname(b));
        // Two repositories whose last segment is the same are told apart by the
        // digest rather than by the slug.
        let c = "/elsewhere/acme";
        assert_ne!(store_dirname(a), store_dirname(c));
        assert_eq!(
            store_dirname(a).split('-').next(),
            store_dirname(c).split('-').next()
        );
    }

    /// RMS-FR-KRYP: the store and its identity record are created once and then
    /// preserved.
    #[test]
    fn rms_a_store_is_created_once_and_preserved() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let access = std::sync::Arc::new(
            fsa::FsAccess::builder()
                .allow_root(dir.path())
                .build()
                .expect("access"),
        );
        let identity = "/home/raver/code/acme";
        let store = ensure_store_in(dir.path(), identity, &access).expect("store");
        assert!(store.join(IDENTITY_FILE).is_file());
        let before = std::fs::read_to_string(store.join(IDENTITY_FILE)).expect("record");
        assert!(before.contains(identity));

        let again = ensure_store_in(dir.path(), identity, &access).expect("store");
        assert_eq!(store, again);
        let after = std::fs::read_to_string(store.join(IDENTITY_FILE)).expect("record");
        assert_eq!(before, after);
    }

    /// RMS-FR-BNKD / RMS-FR-LPWG: two spellings of one repository select one
    /// store, and no worktree path reaches the name.
    ///
    /// The selection through `ProjectState` is
    /// `project::tests::rms_the_store_follows_the_anchor_rather_than_the_active_worktree`;
    /// what this pins is the mapping underneath it.
    #[test]
    fn rms_two_spellings_of_one_repository_select_one_store() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let app_data = Path::new("/data/synthesis");
        // The same directory named two ways — with a trailing separator, and
        // through a `.` segment — is one identity and therefore one store.
        let plain = store_dir_in(app_data, &normalize_identity(dir.path()));
        let trailing = store_dir_in(app_data, &normalize_identity(&dir.path().join("")));
        let dotted = store_dir_in(app_data, &normalize_identity(&dir.path().join(".")));
        assert_eq!(plain, trailing);
        assert_eq!(plain, dotted);
        // And the name carries nothing of a worktree: a linked checkout beside
        // it selects the same store only through its own anchor, never through
        // its own path.
        let linked = tempfile::TempDir::new().expect("tempdir");
        assert_ne!(plain, store_dir_in(app_data, &normalize_identity(linked.path())));
    }

    /// RMS-FR-MVDU: a populated store holds the identity record, the two log
    /// folders, and one folder per draft — and nothing of the project.
    #[test]
    fn rms_a_store_holds_the_named_folders_and_the_identity_record() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let access = std::sync::Arc::new(
            fsa::FsAccess::builder()
                .allow_root(dir.path())
                .build()
                .expect("access"),
        );
        let store = ensure_store_in(dir.path(), "/home/raver/code/acme", &access).expect("store");
        assert!(store.join(IDENTITY_FILE).is_file());
        // Each folder is its owner's to create, so an empty store holds none of
        // them (RMS-FR-KRYP) — and a populated one holds exactly these.
        for owned in [COMMENTS_DIRNAME, STATISTICS_DIRNAME, DRAFTS_DIRNAME] {
            assert!(!store.join(owned).exists(), "{owned} is its owner's to create");
            std::fs::create_dir_all(store.join(owned)).expect("the owner's write");
        }
        let mut held: Vec<String> = std::fs::read_dir(&store)
            .expect("the store")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        held.sort();
        assert_eq!(
            held,
            vec![
                COMMENTS_DIRNAME.to_string(),
                DRAFTS_DIRNAME.to_string(),
                IDENTITY_FILE.to_string(),
                STATISTICS_DIRNAME.to_string(),
            ],
            "the store holds the layout RMS-FR-MVDU draws and nothing besides",
        );
    }

    /// RMS-FR-WGQS: an application data root the instance cannot reach is the
    /// one typed `store_unavailable`, and nothing is created.
    #[test]
    fn rms_an_unreachable_application_data_root_is_store_unavailable() {
        let allowed = tempfile::TempDir::new().expect("tempdir");
        let out_of_reach = tempfile::TempDir::new().expect("tempdir");
        let access = std::sync::Arc::new(
            fsa::FsAccess::builder()
                .allow_root(allowed.path())
                .build()
                .expect("access"),
        );
        let err = ensure_store_in(out_of_reach.path(), "/home/raver/code/acme", &access)
            .expect_err("refused");
        assert_eq!(err, ERR_STORE_UNAVAILABLE);
        assert!(
            !out_of_reach.path().join(REPOSITORIES_DIRNAME).exists(),
            "nothing was created",
        );
    }

    /// RMS-FR-HDNZ: the store stands under the application data root and inside
    /// no worktree.
    #[test]
    fn rms_a_store_stands_outside_every_worktree() {
        let app_data = Path::new("/data/synthesis");
        let store = store_dir_in(app_data, "/home/raver/code/acme");
        assert!(store.starts_with(app_data.join(REPOSITORIES_DIRNAME)));
        assert!(!store.starts_with("/home/raver/code/acme"));
    }
}
