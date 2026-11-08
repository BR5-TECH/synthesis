//! The names and paths of a run's throwaway checkouts
//! (`../ai/GRL-graduation-loop.md` GRL-FR-YKRI, GRL-FR-BIQX, GRL-FR-PQLM;
//! `GRD-graduation.md` GRD-FR-TYVT, GRD-FR-XKGC).

use super::*;

// GRL-FR-PQLM, GRD-FR-XKGC: a checkout name is the stem alone, or the stem, a
// dash, and exactly 8 lowercase hexadecimal digits. Nothing else is one.
#[test]
fn only_the_stem_and_the_stem_with_a_nonce_are_checkout_names() {
    let rows: &[(&str, &str, bool)] = &[
        ("rv", "rv", true),
        ("rv", "rv-0123abcd", true),
        ("rv", "rv-89abcdef", true),
        ("ps", "ps", true),
        ("ps", "ps-0123abcd", true),
        ("rv", "rv-0123ABCD", false),
        ("rv", "rv-0123abc", false),
        ("rv", "rv-0123abcde", false),
        ("rv", "rv-0123abcg", false),
        ("rv", "rv-", false),
        ("rv", "rvx", false),
        ("rv", "rv-XYZ", false),
        ("rv", "rv_0123abcd", false),
        ("rv", "rv-rv-0123abcd", false),
        ("rv", "ps-0123abcd", false),
        ("rv", "ps", false),
        ("rv", "mw", false),
        ("rv", "logs", false),
        ("rv", "", false),
    ];
    for (stem, name, expected) in rows {
        assert_eq!(is_checkout_name(stem, name), *expected, "{stem} / {name:?}");
    }
}

// GRL-FR-BIQX: a nonce is 8 lowercase hexadecimal digits, and draws in one
// process differ. 16 draws of 32 bits clash by chance about once in 3e7 runs.
#[test]
fn a_nonce_is_eight_lowercase_hex_digits_and_draws_differ() {
    let drawn: Vec<String> = (0..16).map(|_| new_checkout_nonce()).collect();
    for nonce in &drawn {
        assert_eq!(nonce.len(), 8, "{nonce}");
        assert!(nonce.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)), "{nonce}");
        assert!(is_checkout_name("rv", &format!("rv-{nonce}")), "{nonce}");
    }
    let unique: std::collections::BTreeSet<&String> = drawn.iter().collect();
    assert_eq!(unique.len(), drawn.len(), "every draw differs: {drawn:?}");
}

// GRL-FR-YKRI, GRD-FR-TYVT: a checkout stands directly under the run home, as
// `rv-<nonce>` or `ps-<nonce>`.
#[test]
fn a_checkout_path_is_the_stem_and_the_nonce_under_the_run_home() {
    let base = StoreBase::new(PathBuf::from("/store"));
    assert_eq!(base.review_checkout("g1", "0123abcd"), base.home("g1").join("rv-0123abcd"));
    assert_eq!(base.pr_summary_checkout("g1", "0123abcd"), base.home("g1").join("ps-0123abcd"));
    assert_eq!(base.home("g1"), PathBuf::from("/store/g/g1"));
}

// GRL-FR-PQLM: the checkouts under a run home are its directories with a
// checkout name. A file and a link of that name are not checkouts.
#[test]
fn the_checkouts_under_a_run_home_are_its_directories_with_a_checkout_name() {
    let store = tempfile::TempDir::new().unwrap();
    let root = std::fs::canonicalize(store.path()).unwrap();
    let base = StoreBase::new(root.clone());
    let home = base.home("g1");
    for dir in ["rv", "rv-0123abcd", "rv-89abcdef", "ps-0123abcd", "rvx", "rv-XYZ", "mw", "logs"] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
    }
    std::fs::write(home.join("rv-fedcba98"), "a file\n").unwrap();
    let outside = tempfile::TempDir::new().unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), home.join("rv-76543210")).unwrap();
    let fs = fsa::FsAccess::builder().owned_root(&root).build().unwrap();

    let mut review = base.checkouts_under(&fs, "g1", "rv");
    review.sort();
    assert_eq!(review, vec![home.join("rv"), home.join("rv-0123abcd"), home.join("rv-89abcdef")]);
    assert_eq!(base.checkouts_under(&fs, "g1", "ps"), vec![home.join("ps-0123abcd")]);

    // A run home that does not stand, or that is a file, holds none.
    assert!(base.checkouts_under(&fs, "g2", "rv").is_empty());
    std::fs::write(base.home("g3"), "not a directory\n").unwrap();
    assert!(base.checkouts_under(&fs, "g3", "rv").is_empty());
}

// GRD-FR-VLFO: the run order holds every run, so no two runs share an id, also
// where they are made in the same microsecond.
#[test]
fn runs_made_at_the_same_time_get_different_ids() {
    let ids: Vec<String> = (0..1000).map(|_| new_run_id()).collect();
    let distinct: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len());
    assert!(ids
        .iter()
        .all(|id| id.starts_with('g') && id.chars().skip(1).all(|c| c.is_ascii_hexdigit())));
}
