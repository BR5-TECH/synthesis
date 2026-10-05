//! Enumerating the project's streams.

use super::*;

// WKS-FR-SGCM / WKS-FR-AXRD — listing
// ---------------------------------------------------------------------------

// WKS-FR-SGCM: the listing answers for every live stream, with what each holds.
#[test]
fn the_listing_returns_every_stream_with_how_far_ahead_it_stands() {
    let fx = Fixture::new();
    fx.create("one", None).expect("created");
    fx.create("two", None).expect("created");

    let listed = list_work_streams(fx.app.clone()).expect("listed");
    assert_eq!(listed.len(), 2);
    for summary in &listed {
        assert_eq!(summary.ahead_of_base, 0);
        assert_eq!(summary.queued_run_count, 0);
    }
}

// WKS-FR-SGCM: an unknown id is a typed refusal rather than an empty answer.
#[test]
fn an_unknown_stream_is_refused() {
    let fx = Fixture::new();
    assert_eq!(
        get_work_stream(fx.app.clone(), "wdeadbeef".into()).expect_err("refused"),
        ERR_UNKNOWN_STREAM
    );
}

// WKS-FR-AXRD: a stream whose directory has gone is reported rather than
// removed, and it is reported as missing.
#[test]
fn a_stream_whose_directory_is_gone_is_reported_as_missing() {
    let fx = Fixture::new();
    let stream = fx.create("vanishing", None).expect("created");
    std::fs::remove_dir_all(&stream.worktree_path).expect("removed");

    let read = get_work_stream(fx.app.clone(), stream.id.clone()).expect("still reported");
    assert!(read.is_missing);
    assert_eq!(read.branch, stream.branch);

    let listed = list_work_streams(fx.app.clone()).expect("listed");
    assert_eq!(listed.len(), 1);
    assert!(listed[0].stream.is_missing);
}

// WKS-FR-AXRD: the missing flag is recomputed on every read rather than stored.
#[test]
fn the_missing_flag_is_recomputed_rather_than_read_from_disk() {
    let fx = Fixture::new();
    let mut stream = fx.create("present", None).expect("created");
    // A record claiming the working copy is gone, for a working copy that is
    // there. Reading it must answer the disk rather than the record — which a
    // stored flag could not do.
    stream.is_missing = true;
    let fs = fx
        .app
        .state::<crate::fs::FsAccessState>()
        .get()
        .expect("instance");
    let store = StreamStore::new(canonical(fx.store.path()));
    store::write_stream(&fs, &store, &stream).expect("written");
    let raw = std::fs::read_to_string(store.record(&stream.id)).expect("record");
    assert!(
        raw.contains("isMissing = true"),
        "precondition: the record really claims the working copy is gone: {raw}",
    );

    let read = get_work_stream(fx.app.clone(), stream.id).expect("read");
    assert!(
        !read.is_missing,
        "the flag answers the disk rather than the record, so a stale record cannot make a live stream read as missing",
    );
}

// ---------------------------------------------------------------------------
