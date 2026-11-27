//! Capture-time order of groups (story US-06, ADR 0028).

use super::CaptureTime;
use crate::grouping::Group;
use crate::names::fold;

/// Sort key of a group's primary path: folded components first, then as written, so the
/// order does not depend on case and is still total.
fn path_key(group: &Group) -> (Vec<String>, Vec<String>) {
    let components = group.primary.entry.components();
    (components.iter().map(|c| fold(c)).collect(), components)
}

/// Indices of `groups` in filename order: by the primary's relative path, compared folded
/// component by component, then as written, then by index. This is the scan's order
/// (`grouping` sorts groups the same way), recomputed so the result does not depend on the
/// order of the input.
#[must_use]
pub fn filename_order(groups: &[Group]) -> Vec<usize> {
    let keys: Vec<_> = groups.iter().map(path_key).collect();
    let mut idx: Vec<usize> = (0..groups.len()).collect();
    idx.sort_by(|&a, &b| keys[a].cmp(&keys[b]).then(a.cmp(&b)));
    idx
}

/// Indices of `groups` in capture-time order (US-06).
///
/// `captured[i]` is the capture time of `groups[i]`'s primary: the JPEG, or the NEF of a lone
/// NEF (`PhotoMeta::captured` read from that file). A missing entry (a shorter slice) counts
/// as no time.
///
/// - Groups with a time are ordered by [`CaptureTime::local_ms`], the camera's clock as
///   written; equal times by the primary's folded relative path, then as written.
/// - A group with no time at all keeps its place in filename order: it goes right after the
///   group that precedes it in filename order (after the last group with that group's time),
///   or first when nothing with a time precedes it. Several such groups in a row keep their
///   filename order.
///
/// The result is a permutation of `0..groups.len()` and depends only on the groups' paths
/// and times, not on the input order. Once the user has decided a photo the list is not
/// re-sorted (US-06); that is the session's rule (`order_frozen`), not this function's.
#[must_use]
pub fn capture_order(groups: &[Group], captured: &[Option<CaptureTime>]) -> Vec<usize> {
    let by_name = filename_order(groups);
    let time = |i: usize| captured.get(i).copied().flatten().map(|t| t.local_ms);

    // (anchor time, rank, filename position): dated groups have rank 0; undated ones inherit
    // the time of their filename predecessor (or i64::MIN) with rank 1. Within a rank, the
    // filename position orders equal times by path and keeps undated groups in place.
    let mut keys: Vec<(i64, u8, usize)> = vec![(0, 0, 0); groups.len()];
    let mut anchor = i64::MIN;
    for (position, &i) in by_name.iter().enumerate() {
        keys[i] = match time(i) {
            Some(t) => {
                anchor = t;
                (t, 0, position)
            }
            None => (anchor, 1, position),
        };
    }
    let mut idx: Vec<usize> = (0..groups.len()).collect();
    // Filename positions are distinct, so the keys are too: the order is total.
    idx.sort_by_key(|&i| keys[i]);
    idx
}
