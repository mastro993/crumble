//! Splitting a long text into parts a small model can read at once.

use std::ops::Range;

/// Groups items, kept in order, into runs whose costs add up to at most `budget`, each run as
/// long as it can be. An item that costs more than `budget` is a run of its own.
pub(crate) fn chunks(costs: &[usize], budget: usize) -> Vec<Range<usize>> {
    let mut chunks = Vec::new();
    let (mut start, mut total) = (0, 0);
    for (index, cost) in costs.iter().enumerate() {
        if index > start && total + cost > budget {
            chunks.push(start..index);
            (start, total) = (index, 0);
        }
        total += cost;
    }
    if start < costs.len() {
        chunks.push(start..costs.len());
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::chunks;

    #[test]
    fn items_fill_each_chunk_in_order() {
        assert_eq!(chunks(&[3, 3, 3, 3, 3], 10), [0..3, 3..5]);
        assert_eq!(chunks(&[5, 5, 5], 10), [0..2, 2..3]);
        assert_eq!(chunks(&[1, 2, 3], 6), vec![0..3]);
    }

    #[test]
    fn an_item_over_budget_stands_alone() {
        assert_eq!(chunks(&[2, 20, 2, 2], 10), [0..1, 1..2, 2..4]);
        assert_eq!(chunks(&[20], 10), vec![0..1]);
    }

    #[test]
    fn nothing_makes_no_chunk() {
        assert!(chunks(&[], 10).is_empty());
    }
}
