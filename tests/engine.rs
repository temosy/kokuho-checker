use kokuho_checker::*;

/// Hand-computable synthetic schedule. Not real municipal rates — real
/// schedules live under data/ and get their own golden tests.
fn schedule() -> RateSchedule {
    RateSchedule {
        municipality: "テスト市".to_string(),
        fiscal_year: 2026,
        basic_deduction_yen: 430_000,
        medical: ComponentRate {
            income_rate: 0.08,
            per_capita_yen: 40_000,
            per_capita_under18_yen: None,
            per_household_yen: 0,
            cap_yen: 660_000,
        },
        support: ComponentRate {
            income_rate: 0.03,
            per_capita_yen: 15_000,
            per_capita_under18_yen: None,
            per_household_yen: 0,
            cap_yen: 260_000,
        },
        care: ComponentRate {
            income_rate: 0.025,
            per_capita_yen: 16_000,
            per_capita_under18_yen: None,
            per_household_yen: 0,
            cap_yen: 170_000,
        },
        childcare: None,
        reduction: ReductionRules {
            base_yen: 430_000,
            per_earner_yen: 100_000,
            half_tier_per_insured_yen: 305_000,
            fifth_tier_per_insured_yen: 560_000,
        },
        preschool_half_per_capita: true,
        is_tax: false,
        notes: vec![],
        sources: vec![],
        verified_on: None,
    }
}

fn member(age: u8, income: u64) -> Member {
    Member {
        age,
        gross_income_yen: income,
        is_insured: true,
        is_salary_or_pension_earner: false,
        is_preschool: false,
    }
}

#[test]
fn single_freelancer_no_reduction() {
    let hh = Household {
        members: vec![member(45, 3_000_000)],
    };
    let p = expected_premium(&schedule(), &hh);

    assert_eq!(p.reduction, ReductionTier::None);
    // Levy base: 3,000,000 - 430,000 = 2,570,000
    assert_eq!(p.medical.income_levy_yen, 205_600); // ×0.08
    assert_eq!(p.medical.per_capita_yen, 40_000);
    assert_eq!(p.medical.subtotal_yen, 245_600);
    assert_eq!(p.support.subtotal_yen, 92_100); // 77,100 + 15,000
    assert_eq!(p.care.subtotal_yen, 80_250); // 64,250 + 16,000
    assert_eq!(p.total_yen, 417_950);
}

#[test]
fn low_income_gets_seventy_percent_reduction() {
    let hh = Household {
        members: vec![member(30, 400_000)],
    };
    let p = expected_premium(&schedule(), &hh);

    assert_eq!(p.reduction, ReductionTier::Seventy);
    assert_eq!(p.medical.income_levy_yen, 0); // income below basic deduction
    assert_eq!(p.medical.per_capita_yen, 12_000); // 40,000 × 0.3
    assert_eq!(p.support.per_capita_yen, 4_500);
    assert_eq!(p.care.subtotal_yen, 0); // under 40
    assert_eq!(p.total_yen, 16_500);
}

#[test]
fn high_income_hits_component_cap() {
    let hh = Household {
        members: vec![member(45, 10_000_000)],
    };
    let p = expected_premium(&schedule(), &hh);

    // Medical uncapped: 9,570,000 × 0.08 + 40,000 = 805,600 > 660,000 cap
    assert!(p.medical.capped);
    assert_eq!(p.medical.uncapped_yen, 805_600);
    assert_eq!(p.medical.subtotal_yen, 660_000);
    // Support (302,100 > 260,000) and care (255,250 > 170,000) also cap,
    // so the total is the sum of all three caps.
    assert!(p.support.capped);
    assert!(p.care.capped);
    assert_eq!(p.total_yen, 660_000 + 260_000 + 170_000);
}

#[test]
fn family_with_preschool_child_twenty_percent_reduction() {
    let mut earner = member(35, 2_000_000);
    earner.is_salary_or_pension_earner = true;
    let spouse = member(33, 0);
    let mut child = member(3, 0);
    child.is_preschool = true;

    let hh = Household {
        members: vec![earner, spouse, child],
    };
    let p = expected_premium(&schedule(), &hh);

    // Threshold for 2割: 430,000 + 560,000 × 3 insured = 2,110,000 ≥ 2,000,000
    assert_eq!(p.reduction, ReductionTier::Twenty);
    assert_eq!(p.medical.income_levy_yen, 125_600); // 1,570,000 × 0.08
    // Per capita: 32,000 + 32,000 + 16,000 (preschool halved after reduction)
    assert_eq!(p.medical.per_capita_yen, 80_000);
    assert_eq!(p.care.subtotal_yen, 0); // nobody aged 40-64... 35 and 33 are under 40
}

#[test]
fn extra_earners_raise_reduction_threshold() {
    let mut a = member(50, 500_000);
    a.is_salary_or_pension_earner = true;
    let mut b = member(48, 30_000);
    b.is_salary_or_pension_earner = true;

    // Two earners: base = 430,000 + 100,000 = 530,000 ≥ 530,000 total income
    let hh = Household {
        members: vec![a, b],
    };
    assert_eq!(
        calc::determine_reduction(&schedule(), &hh),
        ReductionTier::Seventy
    );
}

#[test]
fn care_component_age_boundaries() {
    for (age, expect_care) in [(39, false), (40, true), (64, true), (65, false)] {
        let hh = Household {
            members: vec![member(age, 1_000_000)],
        };
        let p = expected_premium(&schedule(), &hh);
        assert_eq!(p.care.subtotal_yen > 0, expect_care, "age {age}");
    }
}

#[test]
fn non_insured_head_counts_for_reduction_test_only() {
    let mut head = member(50, 5_000_000);
    head.is_insured = false; // e.g. head on employer insurance
    let dependent = member(20, 0);

    let hh = Household {
        members: vec![head, dependent],
    };
    let p = expected_premium(&schedule(), &hh);

    // Head's income blocks the reduction...
    assert_eq!(p.reduction, ReductionTier::None);
    // ...but the head is not levied: only the dependent's per-capita remains.
    assert_eq!(p.medical.income_levy_yen, 0);
    assert_eq!(p.medical.per_capita_yen, 40_000);
}

#[test]
fn verdict_thresholds() {
    assert_eq!(compare(200_000, 208_000).verdict, Verdict::Consistent); // 4%
    assert_eq!(compare(200_000, 230_000).verdict, Verdict::NeedsCheck); // 15%
    assert_eq!(compare(200_000, 260_000).verdict, Verdict::Abnormal); // 30%
    // Small absolute gaps never alarm, even at high ratios.
    assert_eq!(compare(10_000, 12_000).verdict, Verdict::Consistent);
    // The original Togetter case: a premium ~4x the expected amount.
    assert_eq!(compare(120_000, 480_000).verdict, Verdict::Abnormal);
}
