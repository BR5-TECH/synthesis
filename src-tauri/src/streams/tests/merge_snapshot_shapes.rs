//! The shapes of conflict a snapshot is captured from: renames and binary files
//! (`GRB-graduation-rebase.md` GRB-FR-DZQE, GRB-FR-SRVN, GRB-FR-FPYA).
//!
//! Each shape is built with real commits on the two branches, and the snapshot
//! is read back from the repository.

use super::merge_support::*;
use super::*;

const RUN: &str = "gshapes1";

/// Enough lines that Git reads a moved file as a rename and not as a deletion
/// beside an addition.
const BODY: &str = "line one\nline two\nline three\nline four\nline five\nline six\nline seven\nline eight\n";

fn markers_in(text: &str) -> bool {
    ["<<<<<<<", "|||||||", ">>>>>>>"].iter().any(|m| text.contains(m))
}

fn tree_paths(repo: &git2::Repository, commit: git2::Oid) -> Vec<String> {
    let tree = repo.find_commit(commit).unwrap().tree().unwrap();
    let mut paths = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            paths.push(format!("{dir}{}", entry.name().unwrap_or_default()));
        }
        git2::TreeWalkResult::Ok
    })
    .unwrap();
    paths.sort();
    paths
}

/// A snapshot that holds a file at the old path of a renamed file has put a
/// marker where nobody can settle it, and has lost the rename.
fn assert_old_path_is_gone(
    fx: &Fixture,
    snapshot: &merge_snapshot::Snapshot,
    old_path: &str,
) {
    let repo = fx.repo();
    assert!(
        !snapshot.unresolved_paths.iter().any(|p| p == old_path),
        "the old path is unresolved: {:?}",
        snapshot.unresolved_paths
    );
    assert!(
        !snapshot.conflicts.iter().any(|c| c.path == old_path),
        "the old path is a conflict: {:?}",
        snapshot.conflicts
    );
    assert_eq!(
        blob_text(&repo, snapshot.commit, old_path),
        None,
        "the snapshot holds a file at the old path: {:?}",
        tree_paths(&repo, snapshot.commit)
    );
    // Every path the snapshot names as unresolved holds a file there.
    for path in &snapshot.unresolved_paths {
        assert!(
            blob_text(&repo, snapshot.commit, path).is_some(),
            "{path} is unresolved and the snapshot holds no file for it"
        );
    }
}

// GRB-FR-SRVN, GRB-FR-DZQE: the base renamed a file and the stream deleted it.
// The path both sides moved away from is not a path anybody has to settle: the
// snapshot holds no file at the old path and names no unresolved path for it, and
// the renamed path is the one that carries the conflict.
#[test]
fn a_rename_against_a_delete_leaves_no_marker_at_the_old_path() {
    let fx = Fixture::new();
    commit_file(&fx.repo(), "old.txt", BODY, "seed old");
    let stream = fx.create("rename delete", None).expect("created");
    delete_file(&stream_repo(&stream), "old.txt", "stream deletes it");
    rename_file(&fx.repo(), "old.txt", "renamed.txt", "base renames it");
    let before = observe(&fx, &stream);

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_old_path_is_gone(&fx, &snapshot, "old.txt");
    assert!(
        snapshot.unresolved_paths.iter().any(|p| p == "renamed.txt"),
        "the renamed path carries the conflict: {:?}",
        snapshot.unresolved_paths
    );
    let renamed = blob_text(&fx.repo(), snapshot.commit, "renamed.txt").expect("renamed.txt");
    assert!(markers_in(&renamed), "the renamed path is marked: {renamed}");
    let after = observe(&fx, &stream);
    assert_eq!(after.base_files, before.base_files);
    assert_eq!(after.stream_files, before.stream_files);
}

// GRB-FR-SRVN, GRB-FR-DZQE: the stream renamed a file and the base deleted it.
// The same holds with the sides the other way round.
#[test]
fn a_delete_against_a_rename_leaves_no_marker_at_the_old_path() {
    let fx = Fixture::new();
    commit_file(&fx.repo(), "old.txt", BODY, "seed old");
    let stream = fx.create("delete rename", None).expect("created");
    rename_file(&stream_repo(&stream), "old.txt", "renamed.txt", "stream renames it");
    delete_file(&fx.repo(), "old.txt", "base deletes it");

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_old_path_is_gone(&fx, &snapshot, "old.txt");
    assert!(
        snapshot.unresolved_paths.iter().any(|p| p == "renamed.txt"),
        "the renamed path carries the conflict: {:?}",
        snapshot.unresolved_paths
    );
}

// GRB-FR-SRVN, GRB-FR-DZQE: both sides renamed one file to different names. The
// snapshot holds no file at the old path and names no unresolved path for it.
#[test]
fn a_rename_against_a_rename_leaves_no_marker_at_the_old_path() {
    let fx = Fixture::new();
    commit_file(&fx.repo(), "old.txt", BODY, "seed old");
    let stream = fx.create("rename rename", None).expect("created");
    rename_file(&stream_repo(&stream), "old.txt", "stream-name.txt", "stream renames it");
    rename_file(&fx.repo(), "old.txt", "base-name.txt", "base renames it");

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_old_path_is_gone(&fx, &snapshot, "old.txt");
    assert!(
        !snapshot.unresolved_paths.is_empty(),
        "a rename against a rename is a conflict someone settles"
    );
    // The ref stands for a capture that succeeded, and only then.
    assert!(merge_snapshot::has_ref(&fx.repo(), RUN));
}

// GRB-FR-SRVN: a rename conflict is handed off with the same unresolved paths a
// run is told to settle, and the clean paths beside it are merged.
#[test]
fn a_rename_conflict_beside_a_clean_change_hands_off_the_clean_change_merged() {
    let fx = Fixture::new();
    commit_file(&fx.repo(), "old.txt", BODY, "seed old");
    let stream = fx.create("rename and clean", None).expect("created");
    delete_file(&stream_repo(&stream), "old.txt", "stream deletes it");
    commit_file(&stream_repo(&stream), "stream-only.txt", "kept\n", "stream only");
    rename_file(&fx.repo(), "old.txt", "renamed.txt", "base renames it");

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_old_path_is_gone(&fx, &snapshot, "old.txt");
    assert_eq!(
        blob_text(&fx.repo(), snapshot.commit, "stream-only.txt").as_deref(),
        Some("kept\n")
    );
    assert!(snapshot.changed_paths.iter().any(|p| p == "stream-only.txt"));
}

// GRB-FR-DZQE: one side deleted a binary file and the other changed it. There is
// no text to mark, so the surviving bytes stand exactly as they are, with no
// marker, and the path stays unresolved. Both directions.
#[test]
fn a_delete_against_a_changed_binary_file_keeps_the_surviving_bytes_and_no_marker() {
    // Bytes that are not text: a NUL, bytes no UTF-8 text holds, and a line that
    // looks like a marker.
    let seed: &[u8] = &[0, 1, 2, 0xFF, 0xFE];
    let changed: &[u8] = &[0, 159, 146, 150, 0xFF, b'<', b'<', b'<', b'<', b'<', b'<', b'<', b'\n'];

    // The base deleted it, and the stream changed it.
    let fx = Fixture::new();
    commit_bytes(&fx.repo(), "data.bin", seed, "seed binary");
    let stream = fx.create("binary delete", None).expect("created");
    commit_bytes(&stream_repo(&stream), "data.bin", changed, "stream changes it");
    delete_file(&fx.repo(), "data.bin", "base deletes it");
    let snapshot = capture_snapshot(&fx, &stream, RUN);
    assert_eq!(snapshot.unresolved_paths, vec!["data.bin"]);
    assert_eq!(
        blob_bytes(&fx.repo(), snapshot.commit, "data.bin").as_deref(),
        Some(changed),
        "the stream's surviving bytes stand exactly"
    );

    // The stream deleted it, and the base changed it.
    let fx = Fixture::new();
    commit_bytes(&fx.repo(), "data.bin", seed, "seed binary");
    let stream = fx.create("binary delete reverse", None).expect("created");
    delete_file(&stream_repo(&stream), "data.bin", "stream deletes it");
    commit_bytes(&fx.repo(), "data.bin", changed, "base changes it");
    let snapshot = capture_snapshot(&fx, &stream, RUN);
    assert_eq!(snapshot.unresolved_paths, vec!["data.bin"]);
    assert_eq!(
        blob_bytes(&fx.repo(), snapshot.commit, "data.bin").as_deref(),
        Some(changed),
        "the base's surviving bytes stand exactly"
    );
}

// GRB-FR-DZQE: both sides changed a binary file differently. The snapshot keeps
// the base side's bytes exactly, writes no marker, and the path stays unresolved.
#[test]
fn a_binary_against_a_binary_keeps_the_base_sides_bytes_and_no_marker() {
    let base_side: &[u8] = &[0, 7, 7, 7, 0xFF, 0x00, 0x80];
    let stream_side: &[u8] = &[0, 9, 9, 9, 0xFE, 0x00, 0x81];
    let fx = Fixture::new();
    commit_bytes(&fx.repo(), "image.bin", &[0, 1, 2, 3], "seed binary");
    let stream = fx.create("two binaries", None).expect("created");
    commit_bytes(&stream_repo(&stream), "image.bin", stream_side, "stream binary");
    commit_bytes(&fx.repo(), "image.bin", base_side, "base binary");

    let snapshot = capture_snapshot(&fx, &stream, RUN);

    assert_eq!(snapshot.unresolved_paths, vec!["image.bin"]);
    let bytes = blob_bytes(&fx.repo(), snapshot.commit, "image.bin").expect("image.bin");
    assert_eq!(bytes, base_side, "the base side's bytes stand exactly");
    assert!(!markers_in(&String::from_utf8_lossy(&bytes)), "no marker was written");
}

// GRB-FR-DZQE: a binary file on one side against text on the other keeps the
// base side's bytes, whichever side is the binary one.
#[test]
fn a_binary_against_text_keeps_the_base_sides_bytes() {
    let binary: &[u8] = &[0, 5, 5, 5, 0xFF];

    // The base side is binary and the stream side is text.
    let fx = Fixture::new();
    commit_file(&fx.repo(), "mixed.dat", "seed text\n", "seed text");
    let stream = fx.create("binary base", None).expect("created");
    commit_file(&stream_repo(&stream), "mixed.dat", "stream text\n", "stream text");
    commit_bytes(&fx.repo(), "mixed.dat", binary, "base binary");
    let snapshot = capture_snapshot(&fx, &stream, RUN);
    assert_eq!(
        blob_bytes(&fx.repo(), snapshot.commit, "mixed.dat").as_deref(),
        Some(binary)
    );

    // The base side is text and the stream side is binary.
    let fx = Fixture::new();
    commit_file(&fx.repo(), "mixed.dat", "seed text\n", "seed text");
    let stream = fx.create("binary stream", None).expect("created");
    commit_bytes(&stream_repo(&stream), "mixed.dat", binary, "stream binary");
    commit_file(&fx.repo(), "mixed.dat", "base text\n", "base text");
    let snapshot = capture_snapshot(&fx, &stream, RUN);
    assert_eq!(
        blob_bytes(&fx.repo(), snapshot.commit, "mixed.dat").as_deref(),
        Some(&b"base text\n"[..])
    );
    assert_eq!(snapshot.unresolved_paths, vec!["mixed.dat"]);
}
