//! Golden tests against the real FY2026 (令和8年度) Tokyo special-ward rate
//! masters under data/. Expected values are hand-derived from the official
//! rates (東京都保健医療局一覧表, verified 2026-07-12); they lock both the
//! data files and the engine arithmetic.

use kokuho_checker::*;

fn load(json: &str) -> RateSchedule {
    serde_json::from_str(json).expect("rate master must deserialize")
}

fn unified() -> RateSchedule {
    load(include_str!("../data/tokyo_special_wards_unified_2026.json"))
}

fn single_freelancer(income: u64) -> Household {
    Household {
        members: vec![Member {
            age: 45,
            gross_income_yen: income,
            is_insured: true,
            is_salary_or_pension_earner: false,
            is_preschool: false,
        }],
    }
}

#[test]
fn all_rate_masters_hold_fy2026_invariants() {
    let schedules = rates::load_dir(std::path::Path::new("data")).unwrap();
    assert_eq!(schedules.len(), 14, "4 Tokyo masters + 10 designated cities");

    for s in &schedules {
        let name = &s.municipality;
        assert_eq!(s.fiscal_year, 2026, "{name}");
        assert_eq!(s.basic_deduction_yen, 430_000, "{name}");
        // FY2026 statutory reduction thresholds, verified per municipality.
        assert_eq!(s.reduction.base_yen, 430_000, "{name}");
        assert_eq!(s.reduction.per_earner_yen, 100_000, "{name}");
        assert_eq!(s.reduction.half_tier_per_insured_yen, 310_000, "{name}");
        assert_eq!(s.reduction.fifth_tier_per_insured_yen, 570_000, "{name}");
        // FY2026 caps: medical is 670,000 everywhere except Osaka (660,000,
        // prefecture-unified schedule lags the cabinet-order revision).
        assert!(
            s.medical.cap_yen == 670_000 || (name == "大阪市" && s.medical.cap_yen == 660_000),
            "{name}: medical cap {}",
            s.medical.cap_yen
        );
        assert_eq!(s.support.cap_yen, 260_000, "{name}");
        assert_eq!(s.care.cap_yen, 170_000, "{name}");
        // The FY2026 childcare component exists everywhere and exempts minors.
        let childcare = s.childcare.as_ref().unwrap_or_else(|| panic!("{name}: childcare"));
        assert_eq!(childcare.cap_yen, 30_000, "{name}");
        assert_eq!(childcare.per_capita_under18_yen, Some(0), "{name}");
        assert!(s.preschool_half_per_capita, "{name}");
        assert!(!s.sources.is_empty(), "{name}");
        assert!(s.verified_on.is_some(), "{name}");
    }
}

#[test]
fn unified_single_freelancer_income_3m() {
    let p = expected_premium(&unified(), &single_freelancer(3_000_000));

    assert_eq!(p.reduction, ReductionTier::None);
    // Levy base: 3,000,000 - 430,000 = 2,570,000
    assert_eq!(p.medical.subtotal_yen, 240_607); // ×7.51% + 47,600
    assert_eq!(p.support.subtotal_yen, 89_560); // ×2.80% + 17,600
    assert_eq!(p.care.subtotal_yen, 80_251); // ×2.43% + 17,800
    assert_eq!(p.childcare.as_ref().unwrap().subtotal_yen, 8_812); // ×0.27% + 1,873
    assert_eq!(p.total_yen, 419_230);
}

#[test]
fn unified_family_with_preschool_child() {
    let hh = Household {
        members: vec![
            Member {
                age: 35,
                gross_income_yen: 2_000_000,
                is_insured: true,
                is_salary_or_pension_earner: true,
                is_preschool: false,
            },
            Member {
                age: 33,
                gross_income_yen: 0,
                is_insured: true,
                is_salary_or_pension_earner: false,
                is_preschool: false,
            },
            Member {
                age: 3,
                gross_income_yen: 0,
                is_insured: true,
                is_salary_or_pension_earner: false,
                is_preschool: true,
            },
        ],
    };
    let p = expected_premium(&unified(), &hh);

    // 2割 threshold: 430,000 + 570,000 × 3 = 2,140,000 ≥ 2,000,000
    assert_eq!(p.reduction, ReductionTier::Twenty);
    assert_eq!(p.medical.income_levy_yen, 117_907); // 1,570,000 × 7.51%
    // 38,080 × 2 adults + 19,040 preschool (80% payable, then halved)
    assert_eq!(p.medical.per_capita_yen, 95_200);
    // Childcare: adults pay 1,873 × 0.8 = 1,498 each; the minor pays 0.
    assert_eq!(p.childcare.as_ref().unwrap().per_capita_yen, 2_996);
    assert_eq!(p.care.subtotal_yen, 0); // nobody aged 40-64
    assert_eq!(p.total_yen, 299_502);
}

#[test]
fn edogawa_differs_from_unified() {
    let edogawa = load(include_str!("../data/edogawa_2026.json"));
    let p = expected_premium(&edogawa, &single_freelancer(3_000_000));

    assert_eq!(p.medical.subtotal_yen, 250_131); // ×7.83% + 48,900
    assert_eq!(p.support.subtotal_yen, 90_388); // ×2.84% + 17,400
    assert_eq!(p.care.subtotal_yen, 80_365); // ×2.45% + 17,400
    assert_eq!(p.childcare.as_ref().unwrap().subtotal_yen, 8_809); // + 1,870
    assert_eq!(p.total_yen, 429_693);

    let unified_total = expected_premium(&unified(), &single_freelancer(3_000_000)).total_yen;
    assert!(p.total_yen > unified_total);
}

#[test]
fn wrong_income_linkage_is_flagged_abnormal() {
    // The original pain: someone else's salary got linked to the account
    // and the notice came out at roughly 4x. The expected amount for the
    // real income must flag that notice as abnormal.
    let expected = expected_premium(&unified(), &single_freelancer(1_500_000)).total_yen;
    let notified = expected_premium(&unified(), &single_freelancer(6_000_000)).total_yen;

    assert_eq!(compare(expected, notified).verdict, Verdict::Abnormal);
}
