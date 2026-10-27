//! Reciprocal Rank Fusion (EMB-FR-NJYA, EMB-FR-IFWD).

use std::collections::BTreeMap;

/// EMB-FR-NJYA: the usual constant of Reciprocal Rank Fusion.
pub const RRF_K: f64 = 60.0;

/// One fused chunk.
#[derive(Clone, Debug, PartialEq)]
pub struct Fused {
    pub chunk_id: String,
    pub score: f64,
    /// The 1-based rank in the BM25 list, when that list holds the chunk.
    pub bm25_rank: Option<usize>,
}

/// EMB-FR-NJYA, EMB-FR-IFWD: fuse two ranked lists of chunk IDs, best first.
///
/// A chunk's score is the sum of `1 / (60 + rank)` over the lists that hold
/// it. Equal scores are ordered by BM25 rank — a chunk the BM25 list does not
/// hold comes after one it does — and then by chunk ID.
pub fn fuse(bm25: &[String], vector: &[String]) -> Vec<Fused> {
    let mut by_id: BTreeMap<&str, Fused> = BTreeMap::new();
    for (list, is_bm25) in [(bm25, true), (vector, false)] {
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for (index, chunk_id) in list.iter().enumerate() {
            let rank = index + 1;
            // A list holds a chunk once; a repeat in the same list adds nothing.
            if !seen.insert(chunk_id.as_str()) {
                continue;
            }
            let entry = by_id.entry(chunk_id.as_str()).or_insert_with(|| Fused {
                chunk_id: chunk_id.clone(),
                score: 0.0,
                bm25_rank: None,
            });
            entry.score += 1.0 / (RRF_K + rank as f64);
            if is_bm25 {
                entry.bm25_rank = Some(rank);
            }
        }
    }
    let mut fused: Vec<Fused> = by_id.into_values().collect();
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.bm25_rank.unwrap_or(usize::MAX).cmp(&b.bm25_rank.unwrap_or(usize::MAX)))
            .then_with(|| a.chunk_id.cmp(&b.chunk_id))
    });
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    // EMB-FR-NJYA: the fused score is the sum of reciprocal ranks with k = 60.
    #[test]
    fn scores_are_sums_of_reciprocal_ranks() {
        let fused = fuse(&ids(&["a", "b"]), &ids(&["b", "c"]));
        let b = fused.iter().find(|f| f.chunk_id == "b").unwrap();
        assert!((b.score - (1.0 / 62.0 + 1.0 / 61.0)).abs() < 1e-12);
        assert_eq!(fused[0].chunk_id, "b", "a chunk both lists hold ranks first");
        let a = fused.iter().find(|f| f.chunk_id == "a").unwrap();
        assert!((a.score - 1.0 / 61.0).abs() < 1e-12);
    }

    // EMB-FR-IFWD: equal scores order by BM25 rank, then by chunk ID.
    #[test]
    fn ties_order_by_bm25_rank_then_chunk_id() {
        // "a" is BM25 rank 1, "x" is vector rank 1: equal scores.
        let fused = fuse(&ids(&["a"]), &ids(&["x"]));
        assert_eq!(fused.iter().map(|f| f.chunk_id.as_str()).collect::<Vec<_>>(), ["a", "x"]);
        // Two vector-only chunks with equal scores cannot exist, but two lists
        // of one entry each still give a total order.
        let again = fuse(&ids(&["a"]), &ids(&["x"]));
        assert_eq!(fused, again);
        let only_vector = fuse(&[], &ids(&["z", "y"]));
        assert_eq!(only_vector[0].chunk_id, "z");
        assert_eq!(only_vector[0].bm25_rank, None);
    }

    // EMB-FR-IFWD: with equal fused scores the better BM25 rank comes first.
    #[test]
    fn a_symmetric_tie_orders_by_bm25_rank() {
        let fused = fuse(&ids(&["a", "b"]), &ids(&["b", "a"]));
        assert!((fused[0].score - fused[1].score).abs() < 1e-12);
        assert_eq!(fused.iter().map(|f| f.chunk_id.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    }

    // EMB-FR-NJYA: a chunk that a list holds twice counts once.
    #[test]
    fn a_repeated_chunk_counts_once_per_list() {
        let fused = fuse(&ids(&["a"]), &ids(&["a", "a"]));
        assert!((fused[0].score - 2.0 / 61.0).abs() < 1e-12);
    }

    // EMB-FR-BWTI: with no vector list the BM25 order is kept.
    #[test]
    fn an_empty_vector_list_keeps_the_bm25_order() {
        let fused = fuse(&ids(&["c", "a", "b"]), &[]);
        assert_eq!(fused.iter().map(|f| f.chunk_id.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
    }
}
