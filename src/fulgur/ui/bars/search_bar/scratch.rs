//! Whole-buffer scan buffers shared by every search bar of the UI thread.
//!
//! Each editor tab owns its own search bar, so per-bar buffers would grow with the
//! number of searched tabs. Scans run synchronously on the UI thread and never
//! nest, so one set of buffers serves them all.

use super::matching::LengthChange;
use std::cell::RefCell;

/// The reusable buffers one scan works in
#[derive(Default)]
pub(super) struct SearchScratch {
    pub(super) text: String,
    pub(super) lowercase_text: String,
    pub(super) length_changes: Vec<LengthChange>,
}

thread_local! {
    static SEARCH_SCRATCH: RefCell<SearchScratch> = RefCell::new(SearchScratch::default());
}

/// Run a scan with exclusive access to the shared buffers
///
/// ### Arguments
/// - `scan`: The scan to run, which must not start another scan
///
/// ### Returns
/// - `R`: The result of `scan`
pub(super) fn with_search_scratch<R>(scan: impl FnOnce(&mut SearchScratch) -> R) -> R {
    SEARCH_SCRATCH.with_borrow_mut(scan)
}

/// Release the memory held by the shared buffers
pub(super) fn free_search_scratch() {
    SEARCH_SCRATCH.with_borrow_mut(|scratch| *scratch = SearchScratch::default());
}

/// Measure the memory held by the shared buffers
///
/// ### Returns
/// - `(usize, usize, usize)`: The capacities of the text, lowercase text and length change buffers
#[cfg(all(test, feature = "gpui-test-support"))]
pub(super) fn search_scratch_capacities() -> (usize, usize, usize) {
    SEARCH_SCRATCH.with_borrow(|scratch| {
        (
            scratch.text.capacity(),
            scratch.lowercase_text.capacity(),
            scratch.length_changes.capacity(),
        )
    })
}
