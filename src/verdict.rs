use serde::{Deserialize, Serialize};

/// How the notified amount compares to the expected amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Within normal tolerance — the notice is plausible.
    Consistent,
    /// Noticeable gap — worth asking the municipal office about.
    NeedsCheck,
    /// Large gap of the kind produced by wrong income linkage or a missed
    /// reduction — check with the municipal office.
    Abnormal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub expected_yen: u64,
    pub notified_yen: u64,
    /// notified - expected (positive = overcharged relative to expectation).
    pub diff_yen: i64,
    pub diff_ratio: f64,
    pub verdict: Verdict,
}

/// Gaps under this ratio are treated as model noise (rounding, proration,
/// municipal quirks the rate master does not capture).
const CONSISTENT_RATIO: f64 = 0.05;
/// Gaps under this absolute amount are always Consistent, so tiny premiums
/// do not trip the ratio test.
const CONSISTENT_ABS_YEN: i64 = 3_000;
const ABNORMAL_RATIO: f64 = 0.20;

pub fn compare(expected_yen: u64, notified_yen: u64) -> Comparison {
    let diff_yen = notified_yen as i64 - expected_yen as i64;
    let diff_ratio = if expected_yen == 0 {
        if notified_yen == 0 { 0.0 } else { f64::INFINITY }
    } else {
        diff_yen.unsigned_abs() as f64 / expected_yen as f64
    };

    let verdict = if diff_yen.abs() <= CONSISTENT_ABS_YEN || diff_ratio <= CONSISTENT_RATIO {
        Verdict::Consistent
    } else if diff_ratio <= ABNORMAL_RATIO {
        Verdict::NeedsCheck
    } else {
        Verdict::Abnormal
    };

    Comparison {
        expected_yen,
        notified_yen,
        diff_yen,
        diff_ratio,
        verdict,
    }
}
