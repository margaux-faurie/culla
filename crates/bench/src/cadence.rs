//! The decision sequence and the swipe cadences of `culla-bench swipe` (benchmarks.md §4.3).
//! Pure: the runner in `swipe` replays what is decided here against the real pipeline.
//!
//! Every default here is a proposal, open in SPEC §10.

use std::time::Duration;

use culla_core::prefetch::Direction;

use crate::stats::quantile;

/// How fast the simulated user decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cadence {
    /// One advance every 400 ms (a careful cull).
    Steady,
    /// One advance every 200 ms (5 photos/s).
    Fast,
    /// 10 advances 100 ms apart, then a 2 s pause.
    Burst,
    /// From 1 photo/s, 0.5 photo/s faster every 50 advances, until p95 is over the budget
    /// (B-10).
    Ramp,
    /// Each advance as soon as the current photo is shown (NFR-PERF-04, throughput).
    Max,
}

impl Cadence {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "steady" => Self::Steady,
            "fast" => Self::Fast,
            "burst" => Self::Burst,
            "ramp" => Self::Ramp,
            "max" => Self::Max,
            _ => return None,
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Steady => "steady",
            Self::Fast => "fast",
            Self::Burst => "burst",
            Self::Ramp => "ramp",
            Self::Max => "max",
        }
    }

    /// The time from advance `i - 1` to advance `i` (0-based; for `i == 0`, from the first
    /// photo being shown). The runner never advances before the current photo is shown: a
    /// user cannot decide on a photo they have not seen.
    #[must_use]
    pub(crate) fn gap(self, i: usize, ramp: &Ramp) -> Duration {
        match self {
            Self::Steady => Duration::from_millis(400),
            Self::Fast => Duration::from_millis(200),
            Self::Burst if i > 0 && i.is_multiple_of(BURST_LEN) => BURST_PAUSE,
            Self::Burst => Duration::from_millis(100),
            Self::Ramp => Duration::from_secs_f64(1.0 / ramp.rate(i)),
            Self::Max => Duration::ZERO,
        }
    }
}

/// Advances per burst.
const BURST_LEN: usize = 10;
/// The pause after a burst.
const BURST_PAUSE: Duration = Duration::from_secs(2);

/// One advance: go to `position` of the swipe order, moving in `direction`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Move {
    pub position: usize,
    pub direction: Direction,
}

/// A small deterministic generator (SplitMix64), so a seed replays the same sequence.
#[derive(Clone, Debug)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// True with probability `percent` / 100.
    pub(crate) fn chance(&mut self, percent: u8) -> bool {
        self.next_u64() % 100 < u64::from(percent)
    }
}

/// The advances after the first photo of a swipe order of `len` photos: forward one photo at
/// a time from position 0, except that with probability `back_percent` an advance steps back
/// one photo (Shift+←, never before the first). Ends at the last photo or after `max_steps`
/// advances.
#[must_use]
pub(crate) fn moves(
    len: usize,
    max_steps: Option<usize>,
    back_percent: u8,
    seed: u64,
) -> Vec<Move> {
    let mut rng = Rng::new(seed);
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 1 < len && max_steps.is_none_or(|m| out.len() < m) {
        let m = if pos > 0 && rng.chance(back_percent) {
            pos -= 1;
            Move {
                position: pos,
                direction: Direction::Backward,
            }
        } else {
            pos += 1;
            Move {
                position: pos,
                direction: Direction::Forward,
            }
        };
        out.push(m);
    }
    out
}

/// The ramp of B-10: the rate starts at `start` photos/s and goes up by `step` every `every`
/// advances. Each block of `every` advances is judged on its p95 against `budget_ms`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ramp {
    pub start: f64,
    pub step: f64,
    pub every: usize,
    pub budget_ms: f64,
}

impl Ramp {
    /// benchmarks.md §4.3: 1 photo/s, +0.5 every 50 photos; the 50 ms budget of B-09.
    pub(crate) const DEFAULT: Self = Self {
        start: 1.0,
        step: 0.5,
        every: 50,
        budget_ms: 50.0,
    };

    /// The rate of advance `i`, photos per second.
    #[must_use]
    pub(crate) fn rate(&self, i: usize) -> f64 {
        #[allow(clippy::cast_precision_loss, reason = "a block count")]
        let block = (i / self.every.max(1)) as f64;
        self.start + self.step * block
    }
}

/// One finished block of the ramp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Block {
    pub rate: f64,
    pub p95_ms: f64,
    pub passed: bool,
}

/// Follows the ramp as latencies come in and says when to stop.
#[derive(Clone, Debug)]
pub(crate) struct RampTracker {
    ramp: Ramp,
    current: Vec<f64>,
    index: usize,
    blocks: Vec<Block>,
}

impl RampTracker {
    pub(crate) fn new(ramp: Ramp) -> Self {
        Self {
            ramp,
            current: Vec::new(),
            index: 0,
            blocks: Vec::new(),
        }
    }

    /// Adds the latency of the next advance (`None` when its photo failed: it counts as an
    /// advance but has no latency). Returns true when the block it ends failed: stop there.
    pub(crate) fn push(&mut self, latency_ms: Option<f64>) -> bool {
        let rate = self.ramp.rate(self.index);
        self.index += 1;
        if let Some(l) = latency_ms {
            self.current.push(l);
        }
        if !self.index.is_multiple_of(self.ramp.every.max(1)) {
            return false;
        }
        let p95 = quantile(&self.current, 0.95).unwrap_or(f64::INFINITY);
        let passed = p95 <= self.ramp.budget_ms;
        self.blocks.push(Block {
            rate,
            p95_ms: p95,
            passed,
        });
        self.current.clear();
        !passed
    }

    pub(crate) fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The highest rate whose block passed with every block before it passing (B-10), `None`
    /// when the first block failed or none finished.
    #[must_use]
    pub(crate) fn sustained(&self) -> Option<f64> {
        self.blocks
            .iter()
            .take_while(|b| b.passed)
            .last()
            .map(|b| b.rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadences_and_gaps() {
        let r = Ramp::DEFAULT;
        assert_eq!(Cadence::Steady.gap(7, &r), Duration::from_millis(400));
        assert_eq!(Cadence::Fast.gap(0, &r), Duration::from_millis(200));
        let burst: Vec<u128> = (0..22)
            .map(|i| Cadence::Burst.gap(i, &r).as_millis())
            .collect();
        assert_eq!(burst[0], 100);
        assert_eq!(burst[9], 100);
        assert_eq!(burst[10], 2000);
        assert_eq!(burst[11], 100);
        assert_eq!(burst[20], 2000);
        assert_eq!(Cadence::Ramp.gap(0, &r), Duration::from_secs(1));
        assert_eq!(Cadence::Ramp.gap(49, &r), Duration::from_secs(1));
        assert_eq!(Cadence::Ramp.gap(50, &r).as_millis(), 666);
        assert_eq!(Cadence::Ramp.gap(100, &r), Duration::from_millis(500));
        assert_eq!(Cadence::Max.gap(3, &r), Duration::ZERO);
        for c in ["steady", "fast", "burst", "ramp", "max"] {
            assert_eq!(Cadence::parse(c).unwrap().name(), c);
        }
        assert_eq!(Cadence::parse("slow"), None);
    }

    #[test]
    fn forward_only_without_back_steps() {
        let m = moves(5, None, 0, 1);
        let pos: Vec<usize> = m.iter().map(|m| m.position).collect();
        assert_eq!(pos, [1, 2, 3, 4]);
        assert!(m.iter().all(|m| m.direction == Direction::Forward));
        assert!(moves(1, None, 10, 1).is_empty());
        assert!(moves(0, None, 10, 1).is_empty());
        assert_eq!(moves(100, Some(7), 0, 1).len(), 7);
    }

    #[test]
    fn back_steps_are_one_photo_and_about_ten_percent() {
        let m = moves(5000, None, 10, 42);
        let mut prev = 0usize;
        let mut back = 0usize;
        for mv in &m {
            match mv.direction {
                Direction::Forward => assert_eq!(mv.position, prev + 1),
                Direction::Backward => {
                    assert_eq!(mv.position + 1, prev);
                    back += 1;
                }
            }
            prev = mv.position;
        }
        assert_eq!(m.last().unwrap().position, 4999);
        #[allow(clippy::cast_precision_loss, reason = "a test ratio")]
        let ratio = back as f64 / m.len() as f64;
        assert!((0.08..0.12).contains(&ratio), "{ratio}");
        // The same seed replays the same sequence; another one differs.
        assert_eq!(m, moves(5000, None, 10, 42));
        assert_ne!(m, moves(5000, None, 10, 43));
    }

    #[test]
    fn never_steps_back_before_the_first_photo() {
        let m = moves(3, Some(1000), 50, 7);
        assert!(m.iter().all(|m| m.position < 3));
        assert_eq!(m.last().unwrap().position, 2);
    }

    #[test]
    fn ramp_stops_at_the_first_failed_block() {
        let ramp = Ramp {
            start: 1.0,
            step: 0.5,
            every: 4,
            budget_ms: 50.0,
        };
        let mut t = RampTracker::new(ramp);
        // Block 1 (1/s) passes, block 2 (1.5/s) passes, block 3 (2/s) has p95 over budget.
        let lat = [
            10.0, 20.0, 5.0, 50.0, 1.0, 2.0, 3.0, 4.0, 1.0, 1.0, 1.0, 80.0,
        ];
        let stops: Vec<bool> = lat.iter().map(|l| t.push(Some(*l))).collect();
        assert_eq!(stops.iter().filter(|s| **s).count(), 1);
        assert!(stops[11]);
        assert_eq!(t.blocks().len(), 3);
        assert_eq!(t.sustained(), Some(1.5));
        assert!((t.blocks()[2].rate - 2.0).abs() < 1e-12);
        assert!((t.blocks()[2].p95_ms - 80.0).abs() < 1e-12);
    }

    #[test]
    fn ramp_without_a_passing_block() {
        let mut t = RampTracker::new(Ramp {
            every: 2,
            ..Ramp::DEFAULT
        });
        assert!(!t.push(None));
        // Only failures in the block: no latency, so it fails.
        assert!(t.push(None));
        assert_eq!(t.sustained(), None);
        let t = RampTracker::new(Ramp::DEFAULT);
        assert_eq!(t.sustained(), None);
    }
}
