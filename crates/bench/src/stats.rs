//! Summary statistics over measured values: nearest-rank quantiles (as `culla preview`) and the
//! median of runs (benchmarks.md §4.4). Pure.

use std::time::Duration;

/// Milliseconds of `d`.
#[must_use]
pub(crate) fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The `percent`-th percentile (0 to 100, more is taken as 100) of `v` by nearest rank: the
/// value at rank `ceil(percent * n / 100)`, at least 1. Integer arithmetic, so p95 of 100
/// values is exactly the 95th. `None` when `v` has no finite value. `v` need not be sorted;
/// values that are not finite are left out.
#[must_use]
pub(crate) fn quantile(v: &[f64], percent: u32) -> Option<f64> {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return None;
    }
    s.sort_by(f64::total_cmp);
    Some(s[rank(s.len(), percent) - 1])
}

/// The 1-based nearest rank of the `percent`-th percentile among `n` values (`n` at least 1).
#[must_use]
fn rank(n: usize, percent: u32) -> usize {
    let p = u128::from(percent.min(100));
    let r = (p * n as u128).div_ceil(100);
    usize::try_from(r).unwrap_or(n).clamp(1, n)
}

/// The median: the middle value, or the mean of the two middle ones. `None` when empty.
#[must_use]
pub(crate) fn median(v: &[f64]) -> Option<f64> {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return None;
    }
    s.sort_by(f64::total_cmp);
    let n = s.len();
    Some(if n % 2 == 1 {
        s[n / 2]
    } else {
        f64::midpoint(s[n / 2 - 1], s[n / 2])
    })
}

/// The p50, p95, p99 and max of a set of samples, with their count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Summary {
    pub n: usize,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub max: f64,
}

impl Summary {
    /// `None` when there is no finite sample.
    #[must_use]
    pub(crate) fn of(v: &[f64]) -> Option<Self> {
        Some(Self {
            n: v.iter().filter(|x| x.is_finite()).count(),
            p50: quantile(v, 50)?,
            p95: quantile(v, 95)?,
            p99: quantile(v, 99)?,
            max: quantile(v, 100)?,
        })
    }
}

/// `bytes` over `d` in MB/s (10^6 bytes), `None` for a zero duration.
#[must_use]
pub(crate) fn mb_per_s(bytes: u64, d: Duration) -> Option<f64> {
    let s = d.as_secs_f64();
    #[allow(clippy::cast_precision_loss, reason = "a measured rate")]
    (s > 0.0).then(|| bytes as f64 / 1_000_000.0 / s)
}

/// `n` events over `d`, per second, `None` for a zero duration.
#[must_use]
pub(crate) fn per_s(n: usize, d: Duration) -> Option<f64> {
    let s = d.as_secs_f64();
    #[allow(clippy::cast_precision_loss, reason = "a measured rate")]
    (s > 0.0).then(|| n as f64 / s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_by_nearest_rank() {
        let v: Vec<f64> = (1..=20).rev().map(f64::from).collect();
        assert_eq!(quantile(&v, 50), Some(10.0));
        assert_eq!(quantile(&v, 95), Some(19.0));
        assert_eq!(quantile(&v, 99), Some(20.0));
        assert_eq!(quantile(&v, 100), Some(20.0));
        assert_eq!(quantile(&v, 0), Some(1.0));
        assert_eq!(quantile(&v, 250), Some(20.0));
        assert_eq!(quantile(&[], 50), None);
        assert_eq!(quantile(&[f64::NAN], 50), None);
        assert_eq!(quantile(&[3.0, f64::NAN, 1.0], 100), Some(3.0));
        assert_eq!(quantile(&[f64::INFINITY, 2.0], 100), Some(2.0));
    }

    #[test]
    fn nearest_rank_edge_cases() {
        // One value: every percentile is that value.
        for p in [0, 1, 50, 95, 99, 100] {
            assert_eq!(rank(1, p), 1);
        }
        // Exact multiples land on the value itself, with no rounding up past it.
        assert_eq!(rank(100, 95), 95);
        assert_eq!(rank(100, 99), 99);
        assert_eq!(rank(1000, 95), 950);
        assert_eq!(rank(20, 95), 19);
        // Otherwise the next rank up.
        assert_eq!(rank(101, 99), 100);
        assert_eq!(rank(101, 50), 51);
        assert_eq!(rank(3, 50), 2);
        assert_eq!(rank(2, 50), 1);
        assert_eq!(rank(2, 51), 2);
        // Bounds.
        assert_eq!(rank(7, 0), 1);
        assert_eq!(rank(7, 100), 7);
        assert_eq!(rank(7, u32::MAX), 7);
        assert_eq!(rank(usize::MAX, 100), usize::MAX);
        assert_eq!(rank(usize::MAX, 1), usize::MAX.div_ceil(100));
    }

    #[test]
    fn p95_of_100_is_the_95th_value() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        let s = Summary::of(&v).unwrap();
        assert_eq!(
            (s.n, s.p50, s.p95, s.p99, s.max),
            (100, 50.0, 95.0, 99.0, 100.0)
        );
        assert_eq!(Summary::of(&[]), None);
    }

    #[test]
    fn median_of_odd_and_even_counts() {
        assert_eq!(median(&[5.0, 1.0, 3.0, 2.0, 4.0]), Some(3.0));
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(median(&[7.0]), Some(7.0));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn rates() {
        assert_eq!(mb_per_s(90_000_000, Duration::from_secs(1)), Some(90.0));
        assert_eq!(mb_per_s(1, Duration::ZERO), None);
        assert_eq!(per_s(6, Duration::from_secs(2)), Some(3.0));
        assert_eq!(per_s(6, Duration::ZERO), None);
        assert!((ms(Duration::from_micros(1500)) - 1.5).abs() < 1e-12);
    }
}
