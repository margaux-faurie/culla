//! Summary statistics over measured values: nearest-rank quantiles (as `culla preview`) and the
//! median of runs (benchmarks.md §4.4). Pure.

use std::time::Duration;

/// Milliseconds of `d`.
#[must_use]
pub(crate) fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The `q`-quantile (0 to 1) of `v` by nearest rank, `None` when `v` is empty. `v` need not be
/// sorted; values that are not finite are left out.
#[must_use]
pub(crate) fn quantile(v: &[f64], q: f64) -> Option<f64> {
    let mut s: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if s.is_empty() {
        return None;
    }
    s.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a rank within the slice"
    )]
    let rank = ((q.clamp(0.0, 1.0) * s.len() as f64).ceil() as usize).clamp(1, s.len());
    Some(s[rank - 1])
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
            p50: quantile(v, 0.5)?,
            p95: quantile(v, 0.95)?,
            p99: quantile(v, 0.99)?,
            max: quantile(v, 1.0)?,
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
        assert_eq!(quantile(&v, 0.5), Some(10.0));
        assert_eq!(quantile(&v, 0.95), Some(19.0));
        assert_eq!(quantile(&v, 0.99), Some(20.0));
        assert_eq!(quantile(&v, 1.0), Some(20.0));
        assert_eq!(quantile(&v, 0.0), Some(1.0));
        assert_eq!(quantile(&[], 0.5), None);
        assert_eq!(quantile(&[f64::NAN], 0.5), None);
        assert_eq!(quantile(&[3.0, f64::NAN, 1.0], 1.0), Some(3.0));
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
