//! Prefetch policy tests: window, scheduler, LRU eviction order, budgets.

use super::*;
use crate::exif::Orientation;
use crate::ifd::JpegFrame;
use crate::preview::{self, Limits, PreviewBox, Size, coefficient_bytes};

const W: Window = Window::DEFAULT;

#[test]
fn window_is_current_then_ahead_then_behind() {
    assert_eq!(
        wanted(10, 100, Direction::Forward, W),
        vec![10, 11, 12, 13, 14, 15, 9, 8]
    );
    assert_eq!(
        wanted(10, 100, Direction::Backward, W),
        vec![10, 9, 8, 7, 6, 5, 11, 12]
    );
}

#[test]
fn window_is_clipped_at_both_ends() {
    assert_eq!(
        wanted(0, 100, Direction::Forward, W),
        vec![0, 1, 2, 3, 4, 5]
    );
    assert_eq!(wanted(0, 100, Direction::Backward, W), vec![0, 1, 2]);
    assert_eq!(wanted(98, 100, Direction::Forward, W), vec![98, 99, 97, 96]);
    assert_eq!(wanted(0, 1, Direction::Forward, W), vec![0]);
    assert_eq!(wanted(0, 0, Direction::Forward, W), Vec::<usize>::new());
    assert_eq!(wanted(5, 5, Direction::Forward, W), Vec::<usize>::new());
    assert_eq!(
        wanted(usize::MAX - 1, usize::MAX, Direction::Forward, W),
        vec![usize::MAX - 1, usize::MAX - 2, usize::MAX - 3]
    );
}

#[test]
fn window_never_repeats_and_stays_in_range() {
    for len in 0..12 {
        for pos in 0..len + 2 {
            for dir in [Direction::Forward, Direction::Backward] {
                for (ahead, behind) in [(0, 0), (5, 2), (3, 6), (20, 20)] {
                    let w = wanted(pos, len, dir, Window { ahead, behind });
                    let mut seen = HashSet::new();
                    assert!(w.iter().all(|p| *p < len && seen.insert(*p)), "{w:?}");
                    if pos < len {
                        assert_eq!(w[0], pos);
                        assert!(w.len() <= 1 + ahead + behind);
                    } else {
                        assert!(w.is_empty());
                    }
                }
            }
        }
    }
}

#[test]
fn scheduler_hands_out_by_priority_and_skips_cached_and_in_flight() {
    let mut s = Scheduler::new();
    assert!(s.set_wanted([10, 11, 12, 9]).is_empty());
    let cached = |k: &usize| *k == 11;
    assert_eq!(s.next(cached), Some(10));
    assert_eq!(s.next(cached), Some(12));
    assert_eq!(s.next(cached), Some(9));
    assert_eq!(s.next(cached), None);
    assert!(s.in_flight(&10));
    assert!(s.finish(10, Outcome::Cached));
    assert_eq!(s.next(|k| *k == 10 || *k == 11), None);
    assert_eq!(s.priority(&9), Some(3));
    assert_eq!(s.priority(&99), None);
}

#[test]
fn moving_the_window_cancels_stale_work_only() {
    let mut s = Scheduler::new();
    s.set_wanted([0, 1, 2, 3]);
    let none = |_: &usize| false;
    for _ in 0..3 {
        s.next(none);
    }
    // 0, 1 and 2 in flight; the user jumps to 2.
    let mut stale = s.set_wanted([2, 3, 4]);
    stale.sort_unstable();
    assert_eq!(stale, vec![0, 1]);
    // Still in flight until finished, so not handed out twice.
    assert!(s.in_flight(&0));
    assert_eq!(s.next(none), Some(3));
    assert!(!s.finish(0, Outcome::Cancelled));
    // 2 kept running and is still wanted.
    assert!(s.finish(2, Outcome::Cached));
    // A cancelled photo is handed out again when it comes back.
    s.set_wanted([0]);
    assert_eq!(s.next(none), Some(0));
}

#[test]
fn a_failure_is_not_retried_until_asked() {
    let mut s = Scheduler::new();
    s.set_wanted([5, 6]);
    let none = |_: &usize| false;
    assert_eq!(s.next(none), Some(5));
    assert!(s.finish(5, Outcome::Failed));
    assert!(s.failed(&5));
    assert_eq!(s.next(none), Some(6));
    s.finish(6, Outcome::Cancelled);
    s.set_wanted([5, 6]);
    assert_eq!(s.next(none), Some(6));
    assert!(s.retry(&5));
    assert!(!s.retry(&5));
    assert_eq!(s.next(none), Some(5));
}

#[test]
fn a_photo_the_cache_refused_waits_for_the_window_to_change() {
    let mut s = Scheduler::new();
    s.set_wanted([1, 2]);
    let none = |_: &usize| false;
    assert_eq!(s.next(none), Some(1));
    s.finish(1, Outcome::NotAdmitted);
    assert!(s.not_admitted(&1));
    assert_eq!(s.next(none), Some(2));
    s.finish(2, Outcome::Cached);
    assert!(!s.not_admitted(&2));
    assert_eq!(s.next(|k| *k == 2), None);
    s.set_wanted([2, 1]);
    assert!(!s.not_admitted(&1));
    assert_eq!(s.next(|k| *k == 2), Some(1));
}

#[test]
fn repeated_keys_keep_their_first_place() {
    let mut s = Scheduler::new();
    s.set_wanted([3, 4, 3]);
    assert_eq!(s.wanted(), &[3, 4]);
    assert_eq!(s.priority(&3), Some(0));
}

fn cap(max_entries: usize, max_bytes: u64) -> Capacity {
    Capacity {
        max_entries,
        max_bytes,
    }
}

fn admitted(evicted: &[u32]) -> Admission<u32> {
    Admission::Admitted {
        evicted: evicted.to_vec(),
    }
}

#[test]
fn lru_evicts_least_recently_used_outside_the_window_first() {
    let mut c = Lru::new(cap(3, 1000));
    assert_eq!(c.insert(1, 10, &[]), admitted(&[]));
    assert_eq!(c.insert(2, 10, &[]), admitted(&[]));
    assert_eq!(c.insert(3, 10, &[]), admitted(&[]));
    assert!(c.touch(&1));
    // 2 is the least recently used.
    assert_eq!(c.insert(4, 10, &[]), admitted(&[2]));
    assert_eq!(c.len(), 3);
    assert_eq!(c.bytes(), 30);
    // With 3 in the window, 1 goes even though 3 is older.
    assert_eq!(c.insert(5, 10, &[5, 3]), admitted(&[1]));
    assert!(c.contains(&3) && c.contains(&4) && c.contains(&5));
}

#[test]
fn lru_evicts_by_bytes_too() {
    let mut c = Lru::new(cap(10, 100));
    c.insert(1, 40, &[]);
    c.insert(2, 40, &[]);
    assert_eq!(c.insert(3, 50, &[]), admitted(&[1]));
    assert_eq!(c.bytes(), 90);
    // Several evictions in one insert, in order.
    c.insert(4, 10, &[]);
    assert_eq!(c.insert(5, 95, &[]), admitted(&[2, 3, 4]));
    assert_eq!(c.bytes(), 95);
}

#[test]
fn lru_evicts_the_window_from_its_lowest_priority_up() {
    let mut c = Lru::new(cap(3, 1000));
    let window = [10, 11, 12, 13];
    c.insert(10, 1, &window);
    c.insert(12, 1, &window);
    c.insert(13, 1, &window);
    // 11 outranks 13, the lowest held.
    assert_eq!(c.insert(11, 1, &window), admitted(&[13]));
    assert_eq!(c.victims(&window), vec![12, 11, 10]);
}

#[test]
fn lru_refuses_an_entry_that_would_go_first() {
    let mut c = Lru::new(cap(2, 1000));
    let window = [1, 2, 3];
    c.insert(1, 1, &window);
    c.insert(2, 1, &window);
    assert_eq!(
        c.insert(3, 1, &window),
        Admission::Refused(Refusal::LowestRank)
    );
    assert_eq!(c.len(), 2);
    assert!(!c.contains(&3));
    // Outside the window, a new entry is the most recently used and goes last among the
    // others outside it.
    let mut c = Lru::new(cap(2, 1000));
    c.insert(1, 1, &[]);
    c.insert(2, 1, &[2]);
    assert_eq!(c.insert(3, 1, &[2]), admitted(&[1]));
}

#[test]
fn lru_refuses_what_can_never_fit() {
    let mut c = Lru::new(cap(2, 100));
    assert_eq!(c.insert(1, 101, &[]), Admission::Refused(Refusal::TooLarge));
    let mut z = Lru::new(cap(0, 100));
    assert_eq!(z.insert(1, 1, &[]), Admission::Refused(Refusal::TooLarge));
    assert!(c.is_empty() && z.is_empty());
}

#[test]
fn lru_replaces_the_size_of_a_held_key() {
    let mut c = Lru::new(cap(2, 100));
    c.insert(1, 30, &[]);
    c.insert(2, 30, &[]);
    assert_eq!(c.insert(1, 60, &[]), admitted(&[]));
    assert_eq!(c.bytes(), 90);
    // Growing 2 needs room: 1 goes, not 2 itself (it counts as just used).
    assert_eq!(c.insert(2, 50, &[]), admitted(&[1]));
    assert_eq!(c.bytes(), 50);
    assert_eq!(c.len(), 1);
    assert!(c.remove(&2));
    assert!(!c.remove(&2));
    assert_eq!(c.bytes(), 0);
    assert!(!c.touch(&2));
}

#[test]
fn lru_never_exceeds_its_capacity() {
    // Seeded plain-loop generator (no proptest yet, as in grouping/tests.rs).
    let mut seed: u64 = 0x5EED;
    let mut rnd = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for _ in 0..200 {
        let capacity = cap(1 + rnd(6) as usize, 20 + rnd(100));
        let mut c = Lru::new(capacity);
        for _ in 0..100 {
            let k = rnd(20) as u32;
            let pos = rnd(20) as usize;
            let window: Vec<u32> = wanted(pos, 20, Direction::Forward, Window::DEFAULT)
                .into_iter()
                .map(|p| p as u32)
                .collect();
            match rnd(3) {
                0 => {
                    c.touch(&k);
                }
                1 => {
                    c.remove(&k);
                }
                _ => {
                    let before = c.victims(&window);
                    match c.insert(k, 1 + rnd(40), &window) {
                        Admission::Admitted { evicted } => {
                            assert!(c.contains(&k));
                            assert!(!evicted.contains(&k));
                            assert!(evicted.iter().all(|e| !c.contains(e)));
                            // Evictions come in the advertised order.
                            let order: Vec<u32> =
                                before.into_iter().filter(|v| evicted.contains(v)).collect();
                            assert_eq!(order, evicted);
                        }
                        Admission::Refused(_) => {}
                    }
                }
            }
            assert!(c.len() <= capacity.max_entries);
            assert!(c.bytes() <= capacity.max_bytes);
        }
    }
}

#[test]
fn budget_grants_waits_and_refuses() {
    let mut b = Budget::new(100);
    assert_eq!(b.reserve(60), Reservation::Granted);
    assert_eq!(b.reserve(50), Reservation::Wait);
    assert_eq!(b.reserve(101), Reservation::Never);
    assert_eq!(b.reserve(40), Reservation::Granted);
    assert_eq!(b.used(), 100);
    assert_eq!(b.reserve(0), Reservation::Granted);
    b.release(60);
    assert_eq!(b.reserve(50), Reservation::Granted);
    b.release(1000);
    assert_eq!(b.used(), 0);
    assert_eq!(b.limit(), 100);
}

fn frame(width: u32, height: u32, multi_scan: bool) -> JpegFrame {
    JpegFrame {
        length: 0,
        width,
        height,
        progressive: multi_scan,
        multi_scan,
        components: 3,
    }
}

fn need(f: &JpegFrame, o: Orientation) -> u64 {
    let p = preview::plan(
        Size::new(f.width, f.height),
        o,
        PreviewBox::G14,
        &Limits::DEFAULT,
    )
    .unwrap();
    p.working_bytes() + coefficient_bytes(f)
}

#[test]
fn default_budgets_fit_the_memory_ceiling_and_real_photos() {
    let b = Budgets::DEFAULT;
    // NFR-PERF-09: 1 GB must and 600 MB should for the process and WebView2 together. The
    // pipeline's hard ceiling (both budgets and the cache) is 432 MiB (453 MB), leaving the
    // rest for WebView2, three mounted cards and one 100% zoom (NFR estimate).
    assert_eq!(b.ceiling(Capacity::DEFAULT), 432 * 1024 * 1024);
    // A file at the input cap is never refused for the budget.
    assert!(Limits::DEFAULT.max_bytes <= b.input);
    // Four 45 MP calls, one per decode worker, run at once.
    let z8 = need(&frame(8256, 5504, false), Orientation::Rotate90);
    assert!(4 * z8 <= b.working, "{z8}");
    // A multi-scan JPEG at the 25 MP cap fits on its own, with a 24 MP call beside it.
    let multi = need(&frame(6124, 4082, true), Orientation::Normal);
    const { assert!(6124 * 4082 <= Limits::DEFAULT.max_multi_scan_pixels) };
    let z6 = need(&frame(6048, 4032, false), Orientation::Normal);
    assert!(multi + z6 <= b.working, "{multi}");
    // Two at the cap do not, which is why the pipeline runs one multi-scan decode at a time.
    assert!(2 * multi > b.working);
}
