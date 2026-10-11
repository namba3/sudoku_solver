# Dioxus UIテスト設計

この資料は既存テストを維持しつつ、UIの各層で確認する責務を定めます。現状評価と導入判断は[Dioxus UIテスト導入準備状況](dioxus-test-readiness.md)、具体的な操作ケースは[UIテストシナリオ](ui-test-scenarios.md)を参照してください。

## 現在のUI境界

アプリは`Dioxus.toml`でWebを既定ターゲットとし、`Cargo.toml`はDioxus 0.7.10の`web` featureを指定しています（default featuresは無効化されていません）。Routerはなく、`src/main.rs`の単一`app()`が盤面、言語、候補表示、履歴、検索/生成、変換、テキスト欄など36個のSignalとRSXを所有します。Solve / Transform / Textの3つのサイドタブがあります。

ソルバー、盤面解析、対称変換、問題生成はRustロジックとしてUI外に分離されています。検索と生成はWeb Worker経由です。Clipboard、Web Crypto、時刻、DOM focusは`web_sys`に直接接続します。スタイルは`public/app.css`、`docs/`はPages向け生成資材です。

## テスト階層

| 階層 | 責務・対象 | 検証する範囲 | 対象外 | 推奨方式・外部依存 | CI方針 |
| --- | --- | --- | --- | --- | --- |
| Unit | ソルバー、候補/ヒント、解析/変換、生成、パーサー、履歴 | 入出力、境界値、状態遷移、seed再現性、翻訳 | RSX、DOM、CSS | `cargo test`。fixture盤面と固定seed。I/Oなし | 必須。既存`cargo test --locked`を維持 |
| Render | RSX/HTMLの静的構造 | 初期表示、role、label、条件表示 | イベント、CSS layout、実focus | 将来`dioxus-ssr`。ブラウザー依存のない表示境界 | 導入後Rustテストへ追加 |
| Component | 操作とSignal更新後の再描画 | click/input、Props更新、タブ/状態変化 | browser API、Worker実装、ピクセル精度 | 成熟後`dioxus-test`でPoC。公開安定APIとFakeを使用 | 安定性と実行時間を確認後に採用 |
| Integration | UIとapplication処理/Worker契約 | request/response、失敗/取消、古い`job_id`の破棄 | 実DOM/CSS/OS | Rustの変換関数とFake worker/service | 再現可能な部分を必須CIへ |
| Web E2E | 配信Webアプリのブラウザー操作 | 入力から表示、Worker、言語切替、許可時のClipboard | 全組合せ、内部関数、OS統合 | PlaywrightとPages bundle。外部書き込みなし | 初期は任意/定期、安定後に主要フローをPR CIへ |
| Visual | CSSと画面サイズの回帰 | desktop/狭幅のレイアウト | ロジックの正しさ、全ブラウザー差 | Playwright screenshot。viewport/locale/data/font固定 | 専用jobで基準画像更新をレビュー |
| Desktop E2E | OS統合・ネイティブウィンドウ | platform固有機能 | 現行Webの通常フロー | desktop依存なし。必要時に別途選定 | 現状対象外 |

`dioxus-test`は主にComponent層を補う候補で、既存Unitやブラウザー検証の全面代替ではありません。公式Dioxus 0.7 Web Testingガイドは`dioxus-ssr`等の例を示し、フル機能のhook testing libraryはないとしています。本プロジェクトには`dioxus-ssr`依存もまだありません。

## コンポーネント設計

- ドメイン判断はUIイベントクロージャから純粋関数または小さな状態遷移関数へ移し、入力と結果を明示します。solver/analysis/transformのテスト可能な境界を維持します。
- 盤面、操作ツールバー、検索結果、変換、テキスト入出力など意味のある表示単位に分け、PropsとCallbackで親子の契約を表現します。SignalやBrowser APIを暗黙共有するためだけの抽出は避けます。
- Propsは表示入力に絞り、操作を名前付きCallbackとして返します。Props変更による表示更新も契約として確認します。
- 言語・盤面状態は必要な範囲だけに渡します。Context導入は複数領域で共有する必要が確認できた場合に限ります。
- Worker、Clipboard、Web Crypto、DOM focus、時刻を境界に閉じ、Fakeまたは固定値を注入できるようにします。
- fixtureは盤面/givens/履歴/検索状態を組み合わせて小さく再現します。問題生成は固定seedにします。
- 利用者に見える結果（表示、role、disabled/pressed、盤面、error）を検証し、Signal名やDOM順序だけに依存しません。

## UI要素の識別とアクセシビリティ

選択順はroleとアクセシブルネーム、label/意味のある既存ID、必要な場合のみ`data-testid`です。翻訳後の文字列を安定IDにしません。

テスト専用IDは`<領域>.<要素>`形式（例:`board.cell.r1c1`、`search.cancel`、`text.load`）。動的要素は安定キーを末尾に付け（`search.solution.<index>`）、盤面座標は1始まりにし、現在の値や翻訳語を含めません。再利用コンポーネントは親の名前空間を呼び出し側から受け取り、ID衝突を防ぎます。

IDは文書内で一意にします。既存の`cell-{y}-{x}`、`generation-clue-count`、`puzzle-text`、tab/tabpanel role・IDを使います。ARIA属性は実際のアクセシビリティ契約として設定し、テスト都合では追加しません。`data-testid`はアクセシブルな識別方法がない重複要素等に限定します。

## Fixture、Fake、非同期処理

- 盤面fixtureはテスト内の定数か小さなBuilderにします。空盤面を既定とし、セル/givens/言語/状態だけ上書きできる程度に留めます。
- solver、変換、履歴は純粋関数を実入力でテストします。UI統合ではFake Workerから進捗、成功、失敗、取消、古いjob応答を順序指定で返します。
- 非同期完了をsleepで推測せず、Fakeイベントを明示的に送り状態更新を待ちます。時刻は入力化して固定します。
- 問題生成は固定seedを用い、暗号乱数や`Math.random`を合否条件にしません。E2Eは生成結果のclue数などの契約を確認します。
- ClipboardはUIテストでFake、E2Eではブラウザー内の短い盤面文字列に限定し、外部へ送信しません。実ファイル削除、OS設定変更、外部書き込みは行いません。
- 各テストで独立VirtualDom/ページと状態を用い、Signal、localStorage、Workerを残しません。現行アプリはlocalStorage永続化を実装していません。

## 命名・実行・CI

Rustテスト名は`<対象>_<条件>_<期待結果>`の英語snake_case、シナリオ台帳IDは`UI-<連番>`です。1テストで1つの利用者向け契約を確認し、ロジック組合せ網羅はUnit層に置きます。

現在の`.github/workflows/ci.yml`は`cargo fmt --check`、`cargo test --locked`、WebアプリとWorkerのWASM `cargo check`を実行します。SSR/Component依存、Playwright job、UI全体のリファクタリングはまだありません。ブラウザーやVisualの結果はRust/WASMコンパイル結果と分けて報告します。Pages bundleの動作確認時は`bash scripts/build_pages.sh`で生成し、生成JavaScript/WASMを直接編集しません。
