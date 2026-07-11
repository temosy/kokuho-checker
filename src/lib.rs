//! Core engine for verifying Japanese National Health Insurance (国民健康保険)
//! premium notices: compute the expected annual premium from published
//! municipal rates and compare it against the notified amount.
//!
//! The engine is deterministic and data-driven — every yearly constant
//! (rates, caps, reduction thresholds) comes from a [`rates::RateSchedule`],
//! never from code.

pub mod calc;
pub mod household;
pub mod memo;
pub mod rates;
pub mod verdict;

pub use calc::{expected_premium, PremiumBreakdown};
pub use household::{Household, Member};
pub use memo::{window_checklist, MemoSection};
pub use rates::{ComponentRate, RateSchedule, ReductionRules, ReductionTier};
pub use verdict::{compare, Comparison, Verdict};
