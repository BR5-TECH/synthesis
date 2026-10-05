//! Ordinals and the single total cap (SCC-FR-10, SCC-FR-11).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// SCC-FR-10 — ordinals are enumeration order and are stable
// -----------------------------------------------------------------------

#[test]
fn ts13_ordinals_are_enumeration_order_and_repeat_across_runs() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for n in 0..20 {
        write(root, &format!("d{n:02}/f.md"), "needle\n");
    }

    let candidates = scanning::candidate_files(&scanning::scan(root));
    let enumeration: Vec<String> = candidates.iter().map(|c| c.path.clone()).collect();

    let (first, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let ordered: Vec<String> = first.ordered_hits().into_iter().map(|h| h.path).collect();
    assert_eq!(ordered, enumeration, "ordering by ordinal is enumeration order");

    let (second, _) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    let again: Vec<String> = second.ordered_hits().into_iter().map(|h| h.path).collect();
    assert_eq!(again, ordered, "an unchanged tree orders identically");
}

// -----------------------------------------------------------------------
// SCC-FR-11 — the cap is a single total
// -----------------------------------------------------------------------

#[test]
fn ts14_capped_stops_at_the_total_cap_without_reading_the_rest_of_the_tree() {
    // SCC-FR-11 in full, including the clause a hit count alone cannot show:
    // "files later in the enumeration were never read". The producer and
    // every consumer must stop at the cap rather than finish the tree — and
    // the only way to see that from outside is the consumed count the sweep
    // reports through `on_progress`.
    //
    // `read < total` is a race unless the enumeration is enormous, and the
    // size that makes it safe is NOT a multiple of `QUEUE_CAPACITY`. That
    // bound sits on the work channel, which throttles the producer against
    // the consumers; the hit channel the consumers send on is unbounded, so
    // nothing throttles the consumers against the collector. They are free
    // to drain the whole enumeration, of any size, before the collector has
    // taken its 40th hit and cancelled. Sized to the cap alone this was a
    // few hundred files — a millisecond of consumer work against one
    // scheduling quantum — and it failed about once in thirty full-suite
    // runs while passing every time in isolation.
    //
    // So the enumeration is decoupled from the tree: the candidates cycle
    // over a small real tree, and the count is set high enough that reading
    // all of it is hundreds of milliseconds of continuous work against the
    // collector's forty channel receives. The margin is ~4 orders of
    // magnitude rather than ~1, which is what makes the assertion an
    // invariant instead of a coin flip. It is also FASTER than the old
    // version: the capped sweep stops after ~40 files either way, and the
    // tree on disk is now a fraction of the size.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    // Comfortably past the cap, so `full` below still proves exhaustion
    // against a tree the cap would have truncated.
    let on_disk = CAPPED_HIT_LIMIT * 2;
    for n in 0..on_disk {
        write(root, &format!("f{n:04}.md"), "needle\n");
    }
    let files = scanning::candidate_files(&scanning::scan(root));
    assert_eq!(files.len(), on_disk);

    let total = 100_000;
    let candidates = Arc::new(
        (0..total)
            .map(|n| Candidate {
                ordinal: n as u64,
                ..files[n % on_disk].clone()
            })
            .collect::<Vec<_>>(),
    );

    let read = Arc::new(AtomicU64::new(0));
    let capped = Recorder::default();
    let reason = run_search(
        &capped,
        root,
        Arc::clone(&candidates),
        Arc::new(Matcher::compile("needle", SearchMode::LiteralInsensitive).unwrap()),
        SearchScope::Capped,
        &Stop::new(),
        "s",
        &{
            let read = Arc::clone(&read);
            move |consumed| read.store(consumed, Ordering::SeqCst)
        },
    );

    assert_eq!(reason, EndReason::Capped);
    assert_eq!(capped.end_reason(), EndReason::Capped);
    assert_eq!(
        capped.hits().len(),
        CAPPED_HIT_LIMIT,
        "the cap is a single total across all groups"
    );
    let read = read.load(Ordering::SeqCst) as usize;
    assert!(
        read < total,
        "the sweep read the whole enumeration ({read} of {total}) despite \
         capping: the producer and consumers did not stop at the cap"
    );

    // And `full` runs the tree to exhaustion.
    let (full, reason) = search(root, "needle", SearchMode::LiteralInsensitive, SearchScope::Full);
    assert_eq!(reason, EndReason::Completed);
    assert_eq!(full.hits().len(), on_disk, "full runs to exhaustion");
}
