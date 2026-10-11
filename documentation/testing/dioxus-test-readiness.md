# Dioxus UIテスト導入準備状況

調査日: 2026-10-11。評価はソース・設定・CIと下記公式資料に基づきます。実ブラウザーでの動作結果を示すものではありません。

## 現状

- **Dioxus:** `Cargo.toml`は`dioxus = 0.7.10`で`web` featureを指定しています（default featuresは無効化されていません）。`Dioxus.toml`の既定ターゲットはWeb、Pages base pathは`/sudoku_solver/`。`Cargo.lock`上も0.7.10。
- **画面:** Routerなしの単一画面。`src/main.rs`の`app()`ひとつが36個のSignalと盤面、操作、3つのサイドタブ（Solve / Transform / Text）を描画します。分割されたProps/CallbackコンポーネントやContextは見つかりません。
- **状態/ロジック:** ソルバー、候補/ヒント、盤面解析/変換、生成はUIから分離済み。盤面・given・履歴・メッセージ等のUI状態遷移は一部`app()`内。検索/生成はWorkerで非同期実行し、job IDで応答を照合します。
- **外部境界:** Worker、Clipboard、Web Crypto、時刻、DOM focusが`web_sys`に接続。Fakeを注入する共通境界はありません。
- **既存テスト:** solver、生成、盤面解析/変換、翻訳、テキストparser、Worker応答parser、状態表示、履歴等にRustテストがあります。`tests/`統合テスト、`dioxus-ssr`、VirtualDom/コンポーネント操作、Playwright/E2E、Visual Regressionは確認できません。
- **CI:** `.github/workflows/ci.yml`はfmt、`cargo test --locked`、Webアプリと検索WorkerのWASM checkを実施し、ブラウザーは起動しません。
- **課題管理:** リポジトリ内に既存課題表ファイルはありません。末尾に完了条件付きのローカル課題台帳を設けます。

## dioxus-testと互換性

公式リポジトリREADMEは`dioxus-test`をexperimentalとし、DioxusコンポーネントモデルをRustから操作・検証する目的を述べますが、説明は「More details to come」で公開API利用ガイドはありません。調査時の`main`ブランチCargo.tomlはpackage version `0.1.0`と記載しますが、依存先はGitのDioxus `0.8.0-alpha.1`および`blitz-dom`/`blitz-traits` `0.3.0-beta.2`、dev dependencyもDioxus 0.8 alphaです。native rendererを基にする旨もmetadataに記載されています。本プロジェクトの0.7.10/Webと互換だとは判断できません。

調査時に参照したGitHubのReleases欄は空で、crates.ioで利用できる安定リリースは確認できませんでした。Cargo.tomlのmanifest version `0.1.0`だけではcrates.io公開を意味しません。よって導入可能性は**Blocked**です。Git main APIや特定開発版を前提にせず、依存も追加しません。公式Dioxus 0.7 Web Testingガイドは`dioxus-ssr`によるRSX比較などを示し、フル機能のhook testing libraryは現在ないと説明しています。

公式参照:

- [Dioxus 0.7 Web Testing guide](https://dioxuslabs.com/learn/0.7/guides/testing/web/)
- [DioxusLabs/dioxus-test README](https://github.com/DioxusLabs/dioxus-test)
- [dioxus-test Cargo.toml](https://github.com/DioxusLabs/dioxus-test/blob/main/Cargo.toml)
- [Dioxus 0.7 Web platform docs](https://dioxuslabs.com/learn/0.7/guides/platforms/web/)

導入検証直前に公式リポジトリ、crate公開状況、互換版を再確認してください。これは2026-10-11のブランチ内容を記録したスナップショットです。

## テスト容易性評価

各UI領域を8軸（独立性、状態管理、外部依存分離、識別、Fixture、非同期制御、副作用安全性、決定性）各0〜5点で評価します。高得点ほど容易です。`総合点 = 合計 / (軸数 × 5) × 100`。以下は全て8軸・40点満点。UI領域の総合点は`全領域合計 / 全配点 × 100`で算出します。純粋ロジックは別評価にし、UI点へ混ぜません。

| UI領域 | 独立 | 状態 | 外部 | 識別 | Fixture | 非同期 | 副作用 | 決定性 | 合計 | 実装可能性・理由 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 画面全体・言語/メッセージ | 0 | 1 | 1 | 3 | 2 | 1 | 2 | 2 | 12/40 = 30 | Minor Changes。単一`app()`が多数Signal/RSXを保持。単独起動不可 |
| 盤面・セル入力 | 1 | 3 | 4 | 4 | 3 | 5 | 4 | 4 | 28/40 = 70 | Minor Changes。セルID/`aria-invalid`あり。イベントが親Signal/履歴と結合 |
| ツールバー・Undo/Redo | 1 | 2 | 4 | 3 | 3 | 5 | 3 | 4 | 25/40 = 63 | Minor Changes。履歴関数はUnit対象、操作クロージャはapp内 |
| 検索・生成・結果選択 | 2 | 3 | 1 | 3 | 2 | 1 | 2 | 2 | 16/40 = 40 | Refactoring Required。Worker、job ID、時刻、状態を結ぶFake境界なし |
| 変換パネル/ドラッグ | 1 | 3 | 4 | 3 | 3 | 5 | 4 | 4 | 27/40 = 68 | Minor Changes。変換関数は純粋だがUIイベントはappに結合 |
| テキスト入出力/コピー | 1 | 3 | 1 | 4 | 3 | 1 | 1 | 2 | 16/40 = 40 | Refactoring Required。parserはテスト済み。Clipboard Promiseはブラウザー直結 |
| **UI領域計** |  |  |  |  |  |  |  |  | **124/240 = 52** | **Minor Changes**。小さな境界整理でComponent候補を作れるが独立性は低い |

| 別対象 | 実装可能性 | 理由 |
| --- | --- | --- |
| solver/analysis/transform/generator | Ready | UIから分離され、既存Unitテストと固定seedを利用可能 |
| SSR構造検証 | Minor Changes | `dioxus-ssr`依存とブラウザー非依存の表示境界が必要 |
| `dioxus-test` Component検証 | Blocked | 実験段階でDioxus 0.8 alpha依存。0.7.10/Web互換性未確認 |
| Web E2E | Minor Changes | Playwrightとbundleを使えるがworkflow/fixture未整備 |
| Desktop E2E | Blocked | desktop feature/target未構成 |

点数は構造上のテスト容易性であり、カバレッジや動作正しさの測定値ではありません。

## 導入ロードマップ

| Phase | 内容 | 開始条件 | 完了条件 | 依存関係 |
| --- | --- | --- | --- | --- |
| 1. 現行環境で整備 | `cargo test`でロジック/状態遷移を補い、必要に応じSSR構造やPlaywright E2Eを導入 | 現行0.7.10/CI維持 | 重要契約をUnitで検証し、採用したrender/E2Eの実行方法と証拠境界を記録 | 追加依存は個別に必要性評価 |
| 2. テスト容易性改善 | `app()`から盤面、操作、検索状態を小刻みに分割。selector、Fixture、Worker/Clipboard/clock境界を用意 | Phase 1で未検証ケースを特定 | UI境界をfixtureで再現でき、非同期状態をFakeで制御 | 既存CIで小差分ごとに確認 |
| 3. 導入可否調査 | 本体互換、安定API、イベント、Props/再描画、async、Web/CI、保守状況、破壊変更リスクを再調査 | Phase 2で代表境界がある | 版/API/targetと採用/見送り理由、PoC範囲を記録 | 成熟基準未達ならBlockのまま |
| 4. 試験導入 | 代表componentで操作/再描画PoC、Playwrightとの差分・速度・安定性を計測 | Phase 3の全ゲート通過 | CIで再現、flaky率/時間が許容内、責務分担が明確 | ゲート未達なら停止しPhase 1/2を継続 |
| 5. 本格導入 | 重要UIへ展開、CI統合、ケース更新、エージェント検証に接続 | PoCを複数回安定実行 | 主要ケースに層・実行先があり、CIで原因診断可能 | Phase 4完了。E2E/Visualを代替しない |

### Phase 3以降の成熟度ゲート

- 追跡可能な安定配布版があり、Git mainへの直依存を要しない。
- Dioxus 0.7.10互換性を宣言または再現可能な試験で確認。0.8専用なら、本体更新を別途計画するまで対象外。
- click/input、Props更新後の再描画、async成功/失敗を公開APIだけで試験できる。
- 安定Rust/CIで動き、Playwrightに対する補完価値がある。
- 保守状況、破壊変更への方針、更新担当を確認できる。

## 課題台帳

リポジトリに既存の課題表がないため、ここに導入準備の課題を登録します。

| ID | 優先度 | 対象 | 課題・状態 | 完了条件 |
| --- | --- | --- | --- | --- |
| TEST-UI-01 | P1 | `src/main.rs` | 巨大`app()`に責務集中。Refactoring Required | 盤面/基本操作/検索結果などにProps/Callback境界を設け、Web/WASM挙動を維持 |
| TEST-UI-02 | P1 | Worker検索/生成 | Fake可能なWorker/clock境界がない。Refactoring Required | 進捗、成功、失敗、cancel、古いjob IDをI/Oなしに再現 |
| TEST-UI-03 | P2 | Clipboard | PromiseがブラウザーAPIに直結。Minor Changes | Fakeで成功/失敗を検証し、E2Eで外部送信なしに確認 |
| TEST-UI-04 | P2 | UI識別 | 一部ARIA/IDあり、規則と全操作確認は未整備。Minor Changes | role/nameを優先した一覧を作り、必要な要素のみ一意IDを追加 |
| TEST-UI-05 | P2 | Fixture/Mock | UIの初期状態を構築するFixture/Fakeなし。Minor Changes | 空盤面、競合、givens、検索結果/失敗を独立再現 |
| TEST-UI-06 | P1 | `dioxus-test` | 実験段階、Dioxus 0.8 alpha依存。Blocked | 安定配布、0.7.10互換、必要イベント/async/CIの根拠を揃えGo/No-Go |
| TEST-UI-07 | P2 | SSR/E2E/Visual CI | 自動実行なし。Minor Changes | 価値あるフローを選び、ローカル/CI実行と失敗診断を文書化 |

## 対象外・未解決

独立component、ブラウザーE2E、Visual検証を実行した証拠はありません。Router遷移、dialog、theme切替、Desktop機能もUIに見つからないためシナリオに含めません。画面サイズ別layoutは現行CSSにあるため将来のPlaywright候補です。Issue/PRとして外部登録はしていません。
