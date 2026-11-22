//! Tests of the document cache (CMS-FR-AAMO).

use std::sync::Arc;

use super::super::cache::{self, DocumentCache};
use super::super::{DocBlock, ExtractError, ExtractedDocument};

fn document(text_len: usize) -> Arc<ExtractedDocument> {
    Arc::new(ExtractedDocument { blocks: vec![DocBlock::Text("x".repeat(text_len))] })
}

// CMS-FR-AAMO: the least recently used entry goes first when the bound is
// passed; a read makes an entry the most recently used.
#[test]
fn the_least_recently_used_entry_goes_first() {
    let one = document(100).footprint() + 1;
    let mut cache = DocumentCache::new(one * 2 + 10);
    cache.put("a", Ok(document(100)));
    cache.put("b", Ok(document(100)));
    assert!(cache.get("a").is_some());
    cache.put("c", Ok(document(100)));
    assert!(cache.contains("a"), "a was used after b");
    assert!(!cache.contains("b"), "b was the least recently used");
    assert!(cache.contains("c"));
    assert!(cache.bytes() <= one * 2 + 10);
}

// CMS-FR-AAMO: an entry larger than the whole bound is not kept, and it does
// not remove the others.
#[test]
fn an_entry_larger_than_the_bound_is_not_kept() {
    let mut cache = DocumentCache::new(1_000);
    cache.put("small", Ok(document(10)));
    cache.put("huge", Ok(document(5_000)));
    assert!(cache.contains("small"));
    assert!(!cache.contains("huge"));
}

// CMS-FR-AAMO, CMS-FR-RQTB: a second read of the same digest does not extract
// again, and a failure is kept as well.
#[test]
fn a_cached_outcome_is_not_extracted_again() {
    let mut runs = 0;
    let first = cache::get_or_extract("cache-test-digest-ok", || {
        runs += 1;
        Ok(ExtractedDocument { blocks: vec![DocBlock::Text("hello".to_string())] })
    });
    let second = cache::get_or_extract("cache-test-digest-ok", || {
        runs += 1;
        Err(ExtractError::NoText)
    });
    assert_eq!(runs, 1);
    assert_eq!(first, second);
    let mut failures = 0;
    for _ in 0..2 {
        let outcome = cache::get_or_extract("cache-test-digest-failed", || {
            failures += 1;
            Err(ExtractError::Unreadable("broken".to_string()))
        });
        assert!(outcome.is_err());
    }
    assert_eq!(failures, 1);
}
