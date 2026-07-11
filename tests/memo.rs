use kokuho_checker::*;

fn unified() -> RateSchedule {
    serde_json::from_str(include_str!("../data/tokyo_special_wards_unified_2026.json")).unwrap()
}

fn single(income: u64) -> Household {
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

fn flat(memo: &[MemoSection]) -> String {
    memo.iter()
        .flat_map(|s| std::iter::once(s.title.clone()).chain(s.items.iter().cloned()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn abnormal_memo_pushes_for_assessment_basis() {
    let schedule = unified();
    let hh = single(1_500_000);
    let b = expected_premium(&schedule, &hh);
    let c = compare(b.total_yen, b.total_yen * 4);
    assert_eq!(c.verdict, Verdict::Abnormal);

    let text = flat(&window_checklist(&schedule, &hh, &b, Some(&c)));
    assert!(text.contains("誤って紐付き"), "wrong-linkage question present");
    assert!(text.contains("算定根拠の内訳"), "asks for assessment basis");
    assert!(text.contains("2年で時効"), "mentions refund deadline");
}

#[test]
fn reduction_tier_is_reflected() {
    let schedule = unified();
    let hh = single(400_000); // 7割軽減
    let b = expected_premium(&schedule, &hh);
    assert_eq!(b.reduction, ReductionTier::Seventy);

    let text = flat(&window_checklist(&schedule, &hh, &b, None));
    assert!(text.contains("7割軽減に該当"));
    // No comparison given → no deviation section.
    assert!(!text.contains("乖離について"));
}

#[test]
fn minor_members_trigger_childcare_exemption_check() {
    let schedule = unified();
    let mut hh = single(3_000_000);
    hh.members.push(Member {
        age: 10,
        gross_income_yen: 0,
        is_insured: true,
        is_salary_or_pension_earner: false,
        is_preschool: false,
    });
    let b = expected_premium(&schedule, &hh);
    let text = flat(&window_checklist(&schedule, &hh, &b, None));
    assert!(text.contains("18歳未満の子ども・子育て支援金分"));
}
