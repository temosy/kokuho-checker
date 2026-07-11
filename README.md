# kokuho-checker

国民健康保険料の決定通知額が正しいか検算できるツール。

「あなたの国保料、その金額で合ってる？」— 自治体の公開料率から期待額を計算し、届いた通知額と突合。乖離が大きければ「窓口で確認すべき」と判定し、確認用の質問メモを生成する。

計算機ではなく **通知が届いた後の検証** に特化する（既存の計算シミュレーターとは競合しない）。自治体側の算定ミス（軽減判定誤り・制度改正時のシステム改修漏れ・税連携不具合）は毎年全国で反復しており、発覚のきっかけの多くは住民本人の検算。還付請求権は2年で時効になる。

## Architecture

- **Engine** (`src/`): deterministic, data-driven premium calculation. No LLM,
  no network. Every yearly constant (rates, caps, reduction thresholds) comes
  from a `RateSchedule`, never from code.
  - `rates.rs` — rate master types (医療/支援/介護/子ども・子育て支援金,
    軽減判定ルール)
  - `household.rs` — household/member model
  - `calc.rs` — expected premium with 7/5/2割軽減, per-component caps,
    preschool halving
  - `verdict.rs` — expected vs notified comparison (妥当/要確認/明らかに異常)
- **Rate data** (`data/`): per-municipality JSON, hand-verified against
  official municipal pages (`sources` / `verified_on` fields carry
  provenance). Updated once a year (June).
  - FY2026 (令和8年度) coverage: all 23 Tokyo special wards — unified
    schedule for 20 wards plus separate masters for 中野区・江戸川区・目黒区,
    which set their own rates in FY2026. Includes the new 子ども・子育て
    支援金分 component (minors exempt from its per-capita levy).
- **Web** (planned): axum + SSR form, no accounts, no server-side storage of
  user input.

## Development

```sh
cargo test
cargo clippy --all-targets
```

## Scope notes

- Annual amounts only for now; monthly proration (途中加入) is future work.
- The reduction test uses gross income as-is; elderly pension special
  deduction edge cases are not yet modeled.
- 法務: 出力は「公開料率に基づく機械的試算」であり税務相談ではない。断定的な
  還付指南はしない。免責表示を必ず添える。
