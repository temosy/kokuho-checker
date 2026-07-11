use serde::{Deserialize, Serialize};

/// Annual premium rates for one municipality, or a unified region such as
/// Tokyo's 23 special wards. All amounts are yen per year. Every yearly
/// constant lives here so the calculation engine stays data-driven.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateSchedule {
    pub municipality: String,
    /// Japanese fiscal year, e.g. 2026 for FY2026 (Reiwa 8).
    pub fiscal_year: u16,
    /// Deduction subtracted from each member's gross income to get the
    /// levy base (賦課基準額). 430,000 yen as of FY2026.
    pub basic_deduction_yen: u64,
    /// Medical component (医療分・基礎分).
    pub medical: ComponentRate,
    /// Late-elderly support component (後期高齢者支援金分).
    pub support: ComponentRate,
    /// Long-term care component (介護納付金分), members aged 40-64 only.
    pub care: ComponentRate,
    /// Child-care support component (子ども・子育て支援金分), if the
    /// municipality levies it as a separate component from FY2026.
    #[serde(default)]
    pub childcare: Option<ComponentRate>,
    pub reduction: ReductionRules,
    /// Whether the per-capita levy for preschool children is halved
    /// (applied after the income-based reduction).
    pub preschool_half_per_capita: bool,
    /// Official municipal pages the values were verified against.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Date the values were last verified (ISO 8601).
    #[serde(default)]
    pub verified_on: Option<String>,
}

/// Rates for one premium component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRate {
    /// Income levy rate (所得割率) as a fraction, e.g. 0.0869 for 8.69%.
    /// Must have at most 4 decimal places (2 decimals in percent) — the
    /// engine converts it to a per-myriad integer for exact yen arithmetic.
    pub income_rate: f64,
    /// Equalized per-capita levy (均等割) per insured member.
    pub per_capita_yen: u64,
    /// Per-capita levy for members under 18, where it differs from
    /// `per_capita_yen`. The FY2026 childcare component (子ども・子育て
    /// 支援金分) exempts minors entirely, so it sets this to 0.
    /// None means the same as `per_capita_yen`.
    #[serde(default)]
    pub per_capita_under18_yen: Option<u64>,
    /// Flat per-household levy (平等割). 0 where the municipality has none
    /// (e.g. Tokyo special wards).
    pub per_household_yen: u64,
    /// Statutory annual cap (賦課限度額) for this component.
    pub cap_yen: u64,
}

/// Thresholds for the statutory per-capita/per-household levy reduction
/// (7割/5割/2割軽減). The per-insured addition amounts are revised most years.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionRules {
    /// Base of the qualification threshold. 430,000 yen as of FY2026.
    pub base_yen: u64,
    /// Added per salary/pension earner beyond the first. 100,000 yen.
    pub per_earner_yen: u64,
    /// Per-insured addition for the 5割 (50%) tier.
    pub half_tier_per_insured_yen: u64,
    /// Per-insured addition for the 2割 (20%) tier.
    pub fifth_tier_per_insured_yen: u64,
}

/// Reduction tier applied to per-capita and per-household levies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReductionTier {
    /// 7割軽減 — 70% reduced.
    Seventy,
    /// 5割軽減 — 50% reduced.
    Fifty,
    /// 2割軽減 — 20% reduced.
    Twenty,
    None,
}

impl ReductionTier {
    /// Fraction of the per-capita/per-household levy that remains payable,
    /// as (numerator, denominator) for exact integer arithmetic.
    pub fn payable_ratio(self) -> (u64, u64) {
        match self {
            ReductionTier::Seventy => (3, 10),
            ReductionTier::Fifty => (5, 10),
            ReductionTier::Twenty => (8, 10),
            ReductionTier::None => (1, 1),
        }
    }
}
