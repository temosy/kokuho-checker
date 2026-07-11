//! axum + SSR front end. No accounts, no persistence — user input lives only
//! in the request. Rate masters are loaded from data/ at startup.

use std::fmt::Write as _;
use std::sync::Arc;

use axum::extract::{Form, State};
use axum::response::Html;
use axum::routing::{get, post};
use axum::Router;
use kokuho_checker::*;

const MAX_MEMBERS: usize = 6;

type AppState = Arc<Vec<RateSchedule>>;

#[tokio::main]
async fn main() {
    let data_dir = std::env::var("KOKUHO_DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let schedules = rates::load_dir(std::path::Path::new(&data_dir))
        .unwrap_or_else(|e| panic!("failed to load rate masters: {e}"));
    assert!(!schedules.is_empty(), "no rate masters found in {data_dir}");
    eprintln!("loaded {} rate masters from {data_dir}", schedules.len());

    let port: u16 = std::env::var("KOKUHO_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);

    let app = Router::new()
        .route("/", get(index))
        .route("/check", post(check))
        .with_state(Arc::new(schedules));

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .expect("bind");
    eprintln!("listening on http://127.0.0.1:{port}");
    axum::serve(listener, app).await.expect("serve");
}

async fn index(State(schedules): State<AppState>) -> Html<String> {
    Html(page(&render_form(&schedules)))
}

async fn check(
    State(schedules): State<AppState>,
    Form(fields): Form<Vec<(String, String)>>,
) -> Html<String> {
    let input = parse_fields(&fields);
    let schedule = match &input.choice {
        ScheduleChoice::Listed(i) => match schedules.get(*i) {
            Some(s) => s,
            None => return Html(page("<p>自治体の指定が不正です。</p>")),
        },
        ScheduleChoice::Manual(s) => {
            if s.medical.income_rate == 0.0 && s.medical.per_capita_yen == 0 {
                return Html(page(&format!(
                    "<p>手動入力を使う場合は、少なくとも医療分の所得割率か均等割を入力してください。</p>{}",
                    render_form(&schedules)
                )));
            }
            s
        }
    };
    if input.household.insured_count() == 0 {
        return Html(page(&format!(
            "<p>国保に加入している世帯員を1人以上入力してください。</p>{}",
            render_form(&schedules)
        )));
    }

    let breakdown = expected_premium(schedule, &input.household);
    let comparison = input.notified_yen.map(|n| compare(breakdown.total_yen, n));
    let memo = window_checklist(schedule, &input.household, &breakdown, comparison.as_ref());

    let mut body = String::new();
    render_result(&mut body, schedule, &breakdown, comparison.as_ref());
    render_memo(&mut body, &memo);
    body.push_str(r#"<p class="noprint"><a href="/">← 入力に戻る</a></p>"#);
    Html(page(&body))
}

enum ScheduleChoice {
    /// Index into the loaded rate masters.
    Listed(usize),
    /// Built from user-entered rates for a municipality we don't carry.
    Manual(Box<RateSchedule>),
}

struct CheckInput {
    choice: ScheduleChoice,
    notified_yen: Option<u64>,
    household: Household,
}

/// Map full-width digits and the full-width dot to their ASCII forms so
/// values pasted from Japanese pages parse.
fn normalize_digits(c: char) -> char {
    match c {
        '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
        '．' => '.',
        _ => c,
    }
}

fn parse_yen(s: &str) -> Option<u64> {
    let cleaned: String = s
        .chars()
        .map(normalize_digits)
        .filter(|c| c.is_ascii_digit())
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        cleaned.parse().ok()
    }
}

/// Parse a user-entered percentage ("7.51", "7.51%", "７．５１") into a
/// fraction, clamped to non-negative.
fn parse_percent(s: &str) -> f64 {
    let cleaned: String = s
        .chars()
        .map(normalize_digits)
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse::<f64>().map(|v| (v / 100.0).max(0.0)).unwrap_or(0.0)
}

fn field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or("")
}

/// FY2026 statutory reduction thresholds — set by national law, identical in
/// every municipality, so manual entry does not ask for them.
fn fy2026_reduction() -> ReductionRules {
    ReductionRules {
        base_yen: 430_000,
        per_earner_yen: 100_000,
        half_tier_per_insured_yen: 310_000,
        fifth_tier_per_insured_yen: 570_000,
    }
}

fn manual_component(
    fields: &[(String, String)],
    prefix: &str,
    default_cap: u64,
    minors_exempt: bool,
) -> ComponentRate {
    ComponentRate {
        income_rate: parse_percent(field(fields, &format!("{prefix}_rate"))),
        per_capita_yen: parse_yen(field(fields, &format!("{prefix}_capita"))).unwrap_or(0),
        per_capita_under18_yen: if minors_exempt { Some(0) } else { None },
        per_household_yen: parse_yen(field(fields, &format!("{prefix}_household"))).unwrap_or(0),
        cap_yen: parse_yen(field(fields, &format!("{prefix}_cap"))).unwrap_or(default_cap),
    }
}

fn manual_schedule(fields: &[(String, String)]) -> RateSchedule {
    let name = field(fields, "manual_name").trim();
    let childcare = manual_component(fields, "k", 30_000, true);
    RateSchedule {
        municipality: if name.is_empty() {
            "手動入力の自治体".to_string()
        } else {
            name.to_string()
        },
        fiscal_year: 2026,
        basic_deduction_yen: 430_000,
        medical: manual_component(fields, "m", 670_000, false),
        support: manual_component(fields, "s", 260_000, false),
        care: manual_component(fields, "c", 170_000, false),
        childcare: if childcare.income_rate == 0.0 && childcare.per_capita_yen == 0 {
            None
        } else {
            Some(childcare)
        },
        reduction: fy2026_reduction(),
        preschool_half_per_capita: true,
        is_tax: field(fields, "manual_is_tax") == "on",
        notes: vec![
            "手動入力された料率に基づく試算。転記元の自治体公式ページと数値を再度照合してください。"
                .to_string(),
        ],
        sources: vec![],
        verified_on: None,
    }
}

fn parse_fields(fields: &[(String, String)]) -> CheckInput {
    let mut members = Vec::new();
    for i in 0..MAX_MEMBERS {
        let age = field(fields, &format!("age{i}"));
        if age.is_empty() {
            continue;
        }
        let Ok(age) = age.parse::<u8>() else { continue };
        members.push(Member {
            age,
            gross_income_yen: parse_yen(field(fields, &format!("income{i}"))).unwrap_or(0),
            is_insured: field(fields, &format!("insured{i}")) == "on",
            is_salary_or_pension_earner: field(fields, &format!("earner{i}")) == "on",
            is_preschool: field(fields, &format!("preschool{i}")) == "on",
        });
    }

    let choice = if field(fields, "use_manual") == "on" {
        ScheduleChoice::Manual(Box::new(manual_schedule(fields)))
    } else {
        ScheduleChoice::Listed(field(fields, "municipality").parse().unwrap_or(usize::MAX))
    };

    CheckInput {
        choice,
        notified_yen: parse_yen(field(fields, "notified")),
        household: Household { members },
    }
}

use kokuho_checker::memo::yen;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn page(body: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>国保料チェッカー — その金額、合ってる？</title>
<style>
body {{ font-family: -apple-system, "Hiragino Sans", "Noto Sans JP", sans-serif; max-width: 46rem; margin: 0 auto; padding: 1rem; line-height: 1.7; color: #222; background: #fff; }}
h1 {{ font-size: 1.4rem; }}
h2 {{ font-size: 1.1rem; border-bottom: 2px solid #ddd; padding-bottom: .2rem; margin-top: 2rem; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid #ccc; padding: .35rem .6rem; text-align: right; }}
th {{ background: #f5f5f5; text-align: center; }}
td.name {{ text-align: left; }}
.member-row {{ display: flex; gap: .5rem; flex-wrap: wrap; align-items: center; margin-bottom: .4rem; padding: .4rem; background: #fafafa; border-radius: 4px; }}
.member-row input[type=number], .member-row input[type=text] {{ padding: .3rem; }}
input[name^=age] {{ width: 4rem; }}
input[name^=income] {{ width: 9rem; }}
label {{ white-space: nowrap; }}
button {{ font-size: 1rem; padding: .5rem 1.5rem; margin-top: .8rem; cursor: pointer; }}
.verdict {{ padding: .8rem 1rem; border-radius: 6px; font-weight: 700; margin: 1rem 0; }}
.verdict.consistent {{ background: #e6f4ea; color: #1e7e34; }}
.verdict.needs-check {{ background: #fff3cd; color: #856404; }}
.verdict.abnormal {{ background: #f8d7da; color: #842029; }}
.memo li {{ list-style: none; margin: .3rem 0; }}
.memo li::before {{ content: "☐ "; }}
.disclaimer {{ font-size: .8rem; color: #666; margin-top: 2.5rem; border-top: 1px solid #ddd; padding-top: .8rem; }}
details.manual {{ margin: 1rem 0; padding: .6rem; background: #f6f8fa; border-radius: 6px; }}
details.manual summary {{ cursor: pointer; font-weight: 600; }}
details.manual input[type=text] {{ width: 8rem; padding: .25rem; }}
details.manual td, details.manual th {{ padding: .25rem .4rem; }}
@media print {{ .noprint {{ display: none; }} body {{ max-width: none; }} }}
</style>
</head>
<body>
<h1>国保料チェッカー</h1>
{body}
<p class="disclaimer">本ツールは自治体が公開する料率に基づく機械的試算であり、税務相談・法的助言ではありません。実際の保険料は自治体の決定によります。年度途中の加入・脱退（月割）や特殊な所得は反映されません。</p>
</body>
</html>"#
    )
}

fn render_form(schedules: &[RateSchedule]) -> String {
    let mut options = String::new();
    for (i, s) in schedules.iter().enumerate() {
        let _ = write!(
            options,
            r#"<option value="{i}">{}（令和{}年度）</option>"#,
            escape(&s.municipality),
            s.fiscal_year - 2018,
        );
    }

    let mut rows = String::new();
    for i in 0..MAX_MEMBERS {
        let checked = if i == 0 { "checked" } else { "" };
        let _ = write!(
            rows,
            r#"<div class="member-row">
<span>世帯員{n}</span>
<label>年齢 <input type="number" name="age{i}" min="0" max="120" {req}></label>
<label>前年の総所得金額等（円） <input type="text" name="income{i}" placeholder="例: 3000000"></label>
<label><input type="checkbox" name="insured{i}" {checked}> 国保加入</label>
<label><input type="checkbox" name="earner{i}"> 給与・年金所得者</label>
<label><input type="checkbox" name="preschool{i}"> 未就学児</label>
</div>"#,
            n = i + 1,
            req = if i == 0 { "required" } else { "" },
        );
    }

    let manual_rows = [
        ("医療分（基礎分）", "m", "670000"),
        ("後期高齢者支援金分", "s", "260000"),
        ("介護納付金分（40〜64歳）", "c", "170000"),
        ("子ども・子育て支援金分", "k", "30000"),
    ]
    .iter()
    .map(|(label, p, cap)| {
        format!(
            r#"<tr><td class="name">{label}</td>
<td><input type="text" name="{p}_rate" placeholder="例: 7.51"></td>
<td><input type="text" name="{p}_capita" placeholder="例: 47600"></td>
<td><input type="text" name="{p}_household" placeholder="なければ空欄"></td>
<td><input type="text" name="{p}_cap" placeholder="{cap}"></td></tr>"#
        )
    })
    .collect::<String>();

    format!(
        r#"<p>届いた国民健康保険料の決定通知、その金額が正しいか検算します。自治体の公開料率から期待額を計算し、通知額と突き合わせます。</p>
<form method="post" action="/check">
<h2>自治体</h2>
<select name="municipality">{options}</select>
<details class="manual">
<summary>一覧にない自治体の場合（料率を手動入力）</summary>
<p>自治体公式サイトの「国民健康保険料（税）の計算方法」ページから令和8年度の料率を転記してください。<strong>チェックを入れると上の自治体選択より優先されます。</strong></p>
<label><input type="checkbox" name="use_manual"> 手動入力の料率で計算する</label><br>
<label>自治体名 <input type="text" name="manual_name" placeholder="例: 松本市"></label>
<label><input type="checkbox" name="manual_is_tax"> 「保険税」の自治体（公式ページの表記が国民健康保険<strong>税</strong>）</label>
<table>
<tr><th>区分</th><th>所得割率（%）</th><th>均等割（円/人）</th><th>平等割（円/世帯）</th><th>限度額（円）</th></tr>
{manual_rows}
</table>
<p>限度額は空欄なら法定額を使います。子ども・子育て支援金分は所得割率・均等割とも空欄なら区分なしとして扱います（18歳未満の均等割は0円で計算）。軽減判定基準（7割/5割/2割）は全国共通のため入力不要です。</p>
</details>
<h2>世帯（年齢を入れた行だけ計算対象。国保未加入の世帯主も所得だけ入力）</h2>
{rows}
<h2>通知された年間保険料（円・任意）</h2>
<input type="text" name="notified" placeholder="例: 419230">
<br><button type="submit">検算する</button>
</form>"#
    )
}

fn render_result(
    out: &mut String,
    schedule: &RateSchedule,
    b: &PremiumBreakdown,
    comparison: Option<&Comparison>,
) {
    let tier = match b.reduction {
        ReductionTier::Seventy => "7割軽減",
        ReductionTier::Fifty => "5割軽減",
        ReductionTier::Twenty => "2割軽減",
        ReductionTier::None => "軽減なし",
    };

    let _ = write!(
        out,
        "<h2>試算結果 — {}（令和{}年度）</h2><p>法定軽減の判定: <strong>{tier}</strong></p>",
        escape(&schedule.municipality),
        schedule.fiscal_year - 2018,
    );

    out.push_str("<table><tr><th>区分</th><th>所得割</th><th>均等割</th><th>平等割</th><th>小計</th></tr>");
    let mut row = |name: &str, c: &calc::ComponentBreakdown| {
        let _ = write!(
            out,
            r#"<tr><td class="name">{name}</td><td>{}</td><td>{}</td><td>{}</td><td>{}{}</td></tr>"#,
            yen(c.income_levy_yen),
            yen(c.per_capita_yen),
            yen(c.per_household_yen),
            yen(c.subtotal_yen),
            if c.capped { "（限度額）" } else { "" },
        );
    };
    row("医療分", &b.medical);
    row("後期高齢者支援金分", &b.support);
    row("介護納付金分（40〜64歳）", &b.care);
    if let Some(c) = &b.childcare {
        row("子ども・子育て支援金分", c);
    }
    let _ = write!(
        out,
        r#"<tr><td class="name"><strong>年間合計（期待額）</strong></td><td colspan="4"><strong>{}円</strong></td></tr></table>"#,
        yen(b.total_yen)
    );

    if !schedule.notes.is_empty() {
        out.push_str("<p><strong>この自治体の特記事項:</strong></p><ul>");
        for note in &schedule.notes {
            let _ = write!(out, "<li>{}</li>", escape(note));
        }
        out.push_str("</ul>");
    }

    if let Some(c) = comparison {
        let (class, label) = match c.verdict {
            Verdict::Consistent => ("consistent", "妥当 — 通知額は試算と概ね一致しています"),
            Verdict::NeedsCheck => ("needs-check", "要確認 — 通知額と試算に無視できない差があります"),
            Verdict::Abnormal => ("abnormal", "明らかに異常 — 窓口で算定根拠を確認してください"),
        };
        let sign = if c.diff_yen >= 0 { "+" } else { "-" };
        let _ = write!(
            out,
            r#"<div class="verdict {class}">{label}<br>通知額 {}円 / 試算 {}円（差 {sign}{}円）</div>"#,
            yen(c.notified_yen),
            yen(c.expected_yen),
            yen(c.diff_yen.unsigned_abs()),
        );
    }
}

fn render_memo(out: &mut String, memo: &[MemoSection]) {
    out.push_str(
        r#"<h2>窓口確認メモ</h2><p class="noprint">このまま印刷して持っていけます。<button onclick="window.print()">印刷</button></p><div class="memo">"#,
    );
    for section in memo {
        let _ = write!(out, "<h3>{}</h3><ul>", escape(&section.title));
        for item in &section.items {
            let _ = write!(out, "<li>{}</li>", escape(item));
        }
        out.push_str("</ul>");
    }
    out.push_str("</div>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn percent_parsing_handles_common_forms() {
        assert_eq!(parse_percent("7.51"), 0.0751);
        assert_eq!(parse_percent(" 7.51% "), 0.0751);
        assert_eq!(parse_percent("７．５１"), 0.0751); // full-width paste
        assert_eq!(parse_percent(""), 0.0);
        assert_eq!(parse_percent("abc"), 0.0);
    }

    #[test]
    fn yen_parsing_handles_separators_and_full_width() {
        assert_eq!(parse_yen("47,600"), Some(47_600));
        assert_eq!(parse_yen("４７６００"), Some(47_600));
        assert_eq!(parse_yen("47600円"), Some(47_600));
        assert_eq!(parse_yen(""), None);
    }

    #[test]
    fn manual_schedule_builds_from_form_fields() {
        let f = fields(&[
            ("use_manual", "on"),
            ("manual_name", "松本市"),
            ("manual_is_tax", "on"),
            ("m_rate", "7.5"),
            ("m_capita", "30000"),
            ("m_household", "20000"),
            ("s_rate", "2.5"),
            ("s_capita", "10000"),
            ("c_rate", "2.0"),
            ("c_capita", "12000"),
            ("k_rate", "0.27"),
            ("k_capita", "1500"),
        ]);
        let s = manual_schedule(&f);
        assert_eq!(s.municipality, "松本市");
        assert!(s.is_tax);
        assert_eq!(s.medical.income_rate, 0.075);
        assert_eq!(s.medical.per_household_yen, 20_000);
        // Caps left empty fall back to the FY2026 statutory amounts.
        assert_eq!(s.medical.cap_yen, 670_000);
        assert_eq!(s.support.cap_yen, 260_000);
        // Manual childcare always exempts minors.
        let childcare = s.childcare.unwrap();
        assert_eq!(childcare.per_capita_under18_yen, Some(0));
        assert_eq!(s.reduction.half_tier_per_insured_yen, 310_000);
        assert!(!s.notes.is_empty());
    }

    #[test]
    fn manual_schedule_without_childcare_fields_omits_component() {
        let f = fields(&[("use_manual", "on"), ("m_rate", "7.5"), ("m_capita", "30000")]);
        let s = manual_schedule(&f);
        assert!(s.childcare.is_none());
        assert_eq!(s.municipality, "手動入力の自治体");
        assert!(!s.is_tax);
    }

    #[test]
    fn manual_choice_takes_priority_over_selection() {
        let f = fields(&[
            ("municipality", "3"),
            ("use_manual", "on"),
            ("m_rate", "8.0"),
            ("age0", "45"),
            ("income0", "3,000,000"),
            ("insured0", "on"),
        ]);
        let input = parse_fields(&f);
        assert!(matches!(input.choice, ScheduleChoice::Manual(_)));
        assert_eq!(input.household.members[0].gross_income_yen, 3_000_000);
    }
}
