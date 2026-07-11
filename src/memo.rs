//! Generates the 窓口確認メモ — a checklist the user brings to the municipal
//! office to verify how their premium was assessed. Content is tailored to
//! the household, the computed breakdown, and how far the notice deviates.

use crate::calc::PremiumBreakdown;
use crate::household::Household;
use crate::rates::{RateSchedule, ReductionTier};
use crate::verdict::{Comparison, Verdict};

#[derive(Debug, Clone)]
pub struct MemoSection {
    pub title: String,
    pub items: Vec<String>,
}

/// Format a yen amount with thousands separators (no unit suffix).
pub fn yen(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

fn tier_label(tier: ReductionTier) -> &'static str {
    match tier {
        ReductionTier::Seventy => "7割軽減",
        ReductionTier::Fifty => "5割軽減",
        ReductionTier::Twenty => "2割軽減",
        ReductionTier::None => "軽減なし",
    }
}

pub fn window_checklist(
    schedule: &RateSchedule,
    household: &Household,
    breakdown: &PremiumBreakdown,
    comparison: Option<&Comparison>,
) -> Vec<MemoSection> {
    let mut sections = Vec::new();

    sections.push(MemoSection {
        title: "持ち物".to_string(),
        items: vec![
            "保険料の決定通知書（納入通知書）".to_string(),
            "本人確認書類".to_string(),
            "前年の確定申告書の控え、または源泉徴収票（世帯全員分）".to_string(),
            "この試算メモ".to_string(),
        ],
    });

    // The single most valuable question: which income figures the levy was
    // based on. Wrong linkage (someone else's salary) and stale figures are
    // the classic failure modes.
    let mut income_items = vec![
        "保険料の算定に使われた世帯員ごとの前年所得額を教えてもらい、下の自分の申告額と突き合わせる".to_string(),
    ];
    for (i, m) in household.members.iter().enumerate() {
        income_items.push(format!(
            "世帯員{}（{}歳{}）: 自分の把握している前年総所得金額等 = {}円",
            i + 1,
            m.age,
            if m.is_insured { "・国保加入" } else { "・国保未加入" },
            yen(m.gross_income_yen),
        ));
    }
    income_items.push(
        "身に覚えのない給与・事業所得が紐付いていないか（別人の給与所得が誤って紐付き保険料が数倍になった事例が実際にある）".to_string(),
    );
    income_items.push(
        "所得情報の出どころはどれか（確定申告 / 給与支払報告書 / 年金支払報告書）".to_string(),
    );
    sections.push(MemoSection {
        title: "算定所得の確認（最重要）".to_string(),
        items: income_items,
    });

    let mut reduction_items = Vec::new();
    match breakdown.reduction {
        ReductionTier::None => {
            reduction_items.push(
                "試算では法定軽減（7割/5割/2割）の対象外。世帯全員の所得が正しく把握されているか（未申告扱いだと軽減が適用されない）".to_string(),
            );
        }
        tier => {
            reduction_items.push(format!(
                "試算では{}に該当。決定通知書の軽減欄に反映されているか（軽減判定誤りは自治体側ミスの最頻パターン）",
                tier_label(tier)
            ));
        }
    }
    if household.members.iter().any(|m| m.is_preschool && m.is_insured) {
        reduction_items.push(
            "未就学児の均等割5割軽減が適用されているか（医療分・支援金分）".to_string(),
        );
    }
    if schedule.childcare.is_some()
        && household.insured().any(|m| m.age < 18)
    {
        reduction_items.push(
            "18歳未満の子ども・子育て支援金分の均等割が全額軽減（0円）になっているか".to_string(),
        );
    }
    sections.push(MemoSection {
        title: "軽減の確認".to_string(),
        items: reduction_items,
    });

    let mut component_items = Vec::new();
    let capped: Vec<(&str, u64)> = [
        ("医療分", &breakdown.medical),
        ("後期高齢者支援金分", &breakdown.support),
        ("介護納付金分", &breakdown.care),
    ]
    .iter()
    .filter(|(_, b)| b.capped)
    .map(|(name, b)| (*name, b.subtotal_yen))
    .collect();
    for (name, cap) in capped {
        component_items.push(format!(
            "{}は賦課限度額（{}円）に達している。通知額が限度額を超えていないか",
            name,
            yen(cap)
        ));
    }
    let care_count = household
        .insured()
        .filter(|m| (40..=64).contains(&m.age))
        .count();
    component_items.push(format!(
        "介護納付金分の対象者（40〜64歳）が{}人で計算されているか",
        care_count
    ));
    sections.push(MemoSection {
        title: "区分・限度額の確認".to_string(),
        items: component_items,
    });

    if let Some(c) = comparison {
        let lead = match c.verdict {
            Verdict::Abnormal => format!(
                "試算{}円に対し通知{}円（差{}{}円）と乖離が大きい。所得の誤紐付け・軽減の適用漏れの可能性を含め、算定根拠の内訳（区分ごとの所得割・均等割）の提示を求める",
                yen(c.expected_yen),
                yen(c.notified_yen),
                if c.diff_yen >= 0 { "+" } else { "-" },
                yen(c.diff_yen.unsigned_abs()),
            ),
            Verdict::NeedsCheck => format!(
                "試算{}円に対し通知{}円。乖離の理由（月割・所得差異・料率）を確認する",
                yen(c.expected_yen),
                yen(c.notified_yen),
            ),
            Verdict::Consistent => format!(
                "試算{}円と通知{}円は概ね一致。大きな問題は見当たらないが、軽減・限度額の項目だけ念のため確認",
                yen(c.expected_yen),
                yen(c.notified_yen),
            ),
        };
        sections.push(MemoSection {
            title: "乖離について".to_string(),
            items: vec![lead],
        });
    }

    sections.push(MemoSection {
        title: "知っておくこと".to_string(),
        items: vec![
            if schedule.is_tax {
                "この自治体は国民健康保険「税」のため、納め過ぎの還付は法定納期限から5年まで遡れる".to_string()
            } else {
                "納め過ぎだった場合の還付請求権は2年で時効消滅する。おかしいと思ったら早めに確認".to_string()
            },
            "年度途中の加入・脱退は月割になるため、この試算（年額）とは差が出る".to_string(),
            format!(
                "この試算は{}が公開する令和{}年度料率に基づく機械的計算であり、税務相談・法的助言ではない",
                schedule.municipality,
                schedule.fiscal_year - 2018,
            ),
        ],
    });

    sections
}
