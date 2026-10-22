//! Keeping a `ListState` in step with the rows it shows.

use gpui_kit::ListState;
use std::ops::Range;

/// Tells `list` its rows went from `old` to `new`, splicing only those that changed, so the
/// list keeps its measured rows and its scroll position when rows change above or below.
/// Returns where the new rows sit.
pub(super) fn splice<T: PartialEq>(list: &ListState, old: &[T], new: &[T]) -> Range<usize> {
    let kept = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[kept..]
        .iter()
        .rev()
        .zip(new[kept..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    if old.is_empty() {
        // Splicing into an empty list would count the new rows as above the scroll top.
        list.reset(new.len());
    } else {
        list.splice(kept..old.len() - tail, new.len() - kept - tail);
    }
    kept..new.len() - tail
}
