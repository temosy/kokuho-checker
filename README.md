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
  - FY2026 (令和8年度) coverage: 14 masters — all 23 Tokyo special wards
    (unified schedule for 20 wards plus 中野区・江戸川区・目黒区, which set
    their own rates in FY2026) and 10 designated cities (札幌・さいたま・
    横浜・川崎・名古屋・京都・大阪・神戸・広島・福岡). Includes the new
    子ども・子育て支援金分 component (minors exempt from its per-capita
    levy) and the 保険料/保険税 distinction (さいたま市 is a tax, which
    changes the refund limitation period to 5 years).
  - Municipality quirks the schema cannot express (川崎市の独自所得割控除,
    名古屋市の均等割2,000円控除, 札幌市の10円単位切り捨て等) are carried in
    each master's `notes` and surfaced with the result.
- **Web** (`src/bin/web.rs`): axum + SSR form. No accounts, no JS framework,
  user input never stored server-side. Outputs the expected-premium
  breakdown, a 3-tier verdict against the notified amount, and a printable
  窓口確認メモ (questions to ask at the municipal office, tailored to the
  household and the size of the gap).

## Development

```sh
cargo test
cargo clippy --all-targets
cargo run --bin web   # http://127.0.0.1:8787 (KOKUHO_PORT / KOKUHO_DATA_DIR to override)
```

## Deployment

Container image via the bundled `Dockerfile` (static musl build, rate
masters baked in). Environment:

- `KOKUHO_BIND` — bind address (image default `0.0.0.0`; native default
  `127.0.0.1`)
- `KOKUHO_PORT` — port (default `8787`)
- `KOKUHO_BASE_PATH` — public URL prefix when served under a sub-path
  behind a reverse proxy (e.g. `/kokuho` for temosy.com/kokuho/)
- `KOKUHO_DATA_DIR` — rate master directory

Production runs as a service of the `temosy-wordpress` compose stack on
the home Fedora server (its nginx terminates TLS for temosy.com and
proxies `/kokuho/` here; the fedora-edge SNI passthrough is untouched).

### Auto-deploy (CI)

Merging to `main` deploys automatically via
[`.github/workflows/deploy.yml`](.github/workflows/deploy.yml): a
GitHub-hosted runner runs `cargo test` + `clippy`, then a self-hosted
runner on the Fedora box syncs the sources into the build context and
rebuilds only the `kokuho` service (`sudo podman compose up -d --build
kokuho`), finally verifying `https://temosy.com/kokuho/`. The box is
LAN-only, so the runner is pull-based; no SSH keys or secrets are needed.

For a manual deploy, `scripts/sync.sh` rsyncs sources from a dev machine
to the Fedora box; then run the same `podman compose` rebuild there.

## Scope notes

- Annual amounts only for now; monthly proration (途中加入) is future work.
- The reduction test uses gross income as-is; elderly pension special
  deduction edge cases are not yet modeled.
- 法務: 出力は「公開料率に基づく機械的試算」であり税務相談ではない。断定的な
  還付指南はしない。免責表示を必ず添える。
