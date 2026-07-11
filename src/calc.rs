use serde::{Deserialize, Serialize};

use crate::household::{Household, Member};
use crate::rates::{ComponentRate, RateSchedule, ReductionTier};

/// Full annual premium breakdown for a household.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PremiumBreakdown {
    pub reduction: ReductionTier,
    pub medical: ComponentBreakdown,
    pub support: ComponentBreakdown,
    pub care: ComponentBreakdown,
    pub childcare: Option<ComponentBreakdown>,
    pub total_yen: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentBreakdown {
    pub income_levy_yen: u64,
    pub per_capita_yen: u64,
    pub per_household_yen: u64,
    /// Sum of the three levies before applying the statutory cap.
    pub uncapped_yen: u64,
    /// Final amount for this component (capped).
    pub subtotal_yen: u64,
    pub capped: bool,
}

/// Compute the expected annual premium for a household under a schedule.
///
/// Amounts are floored to whole yen per component, matching the common
/// municipal practice. Monthly proration for mid-year enrollment is out of
/// scope here; the result assumes a full year of coverage.
pub fn expected_premium(schedule: &RateSchedule, household: &Household) -> PremiumBreakdown {
    let reduction = determine_reduction(schedule, household);

    let all_insured: Vec<&Member> = household.insured().collect();
    let care_insured: Vec<&Member> = all_insured
        .iter()
        .copied()
        .filter(|m| (40..=64).contains(&m.age))
        .collect();

    let medical = component(schedule, &schedule.medical, &all_insured, reduction);
    let support = component(schedule, &schedule.support, &all_insured, reduction);
    let care = component(schedule, &schedule.care, &care_insured, reduction);
    let childcare = schedule
        .childcare
        .as_ref()
        .map(|c| component(schedule, c, &all_insured, reduction));

    let total_yen = medical.subtotal_yen
        + support.subtotal_yen
        + care.subtotal_yen
        + childcare.as_ref().map_or(0, |c| c.subtotal_yen);

    PremiumBreakdown {
        reduction,
        medical,
        support,
        care,
        childcare,
        total_yen,
    }
}

/// Statutory 7割/5割/2割 reduction test on household income.
pub fn determine_reduction(schedule: &RateSchedule, household: &Household) -> ReductionTier {
    let rules = &schedule.reduction;
    let income = household.reduction_test_income_yen();
    let insured = household.insured_count();
    let extra_earners = household.earner_count().saturating_sub(1);
    let base = rules.base_yen + rules.per_earner_yen * extra_earners;

    if income <= base {
        ReductionTier::Seventy
    } else if income <= base + rules.half_tier_per_insured_yen * insured {
        ReductionTier::Fifty
    } else if income <= base + rules.fifth_tier_per_insured_yen * insured {
        ReductionTier::Twenty
    } else {
        ReductionTier::None
    }
}

fn component(
    schedule: &RateSchedule,
    rate: &ComponentRate,
    insured: &[&Member],
    reduction: ReductionTier,
) -> ComponentBreakdown {
    let levy_base: u64 = insured
        .iter()
        .map(|m| m.gross_income_yen.saturating_sub(schedule.basic_deduction_yen))
        .sum();
    // Exact integer arithmetic: rates always have at most 2 decimal places
    // in percent, so a per-myriad integer represents them losslessly and
    // avoids f64 floor() being off by one yen.
    let rate_permyriad = (rate.income_rate * 10_000.0).round() as u64;
    let income_levy_yen = levy_base * rate_permyriad / 10_000;

    let (num, den) = reduction.payable_ratio();
    let per_capita_yen: u64 = insured
        .iter()
        .map(|m| {
            let base = if m.age < 18 {
                rate.per_capita_under18_yen.unwrap_or(rate.per_capita_yen)
            } else {
                rate.per_capita_yen
            };
            let mut amount = base * num / den;
            if schedule.preschool_half_per_capita && m.is_preschool {
                amount /= 2;
            }
            amount
        })
        .sum();

    let per_household_yen = if insured.is_empty() {
        0
    } else {
        rate.per_household_yen * num / den
    };

    let uncapped_yen = income_levy_yen + per_capita_yen + per_household_yen;
    let capped = uncapped_yen > rate.cap_yen;
    let subtotal_yen = uncapped_yen.min(rate.cap_yen);

    ComponentBreakdown {
        income_levy_yen,
        per_capita_yen,
        per_household_yen,
        uncapped_yen,
        subtotal_yen,
        capped,
    }
}
