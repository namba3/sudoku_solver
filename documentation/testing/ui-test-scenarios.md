# UIテストシナリオ

ケースは現行UIと`src/main.rs`の補助関数に基づきます。準備状況の採点は[Dioxus UIテスト導入準備状況](dioxus-test-readiness.md)、階層と識別方針は[Dioxus UIテスト設計](dioxus-ui-testing.md)を参照してください。「Ready」は既存境界からテストを記述可能という意味で、実装済みを示しません。

| Test ID | 対象 | Given | When | Then / 目的 | 推奨階層・ツール | 優先度 | 準備・前提 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UI-01 | セル入力 | 空セル/givensロックON | 1〜9を入力 | 値が盤面へ反映し、givenは変更されない | Component / 将来`dioxus-test` | P1 | Minor Changes。盤面境界とfixtureが必要 |
| UI-02 | 不正値入力 | 編集可能セル | 空、0、10、文字を入力 | 1〜9以外は空セル扱い、表示と状態が同期 | Unit + Component | P1 | `cell_value`はReady、UI配線はMinor |
| UI-03 | 競合/候補表示 | 重複または候補なし盤面 | 入力を変更 | 競合/候補なしセルと`aria-invalid`が更新 | Unit + Component | P1 | `BoardAnalysis` UnitはReady、描画はMinor |
| UI-04 | 候補toggle | 候補OFF | Show candidatesを押す | 候補数字とpressed状態/ラベルが更新し、再操作で隠れる | Component | P2 | Minor Changes。盤面fixture |
| UI-05 | givens編集 | givenあり、ロックON | 解除してgivenセルを編集 | givenから通常入力へ変わりUndo対象となる | Component + Integration | P1 | Refactoring Required。board/givens fixture |
| UI-06 | Undo/Redo | 変更履歴あり | Undo後Redo | 盤面/givensが共に復元し、disabled状態も更新 | Unit + Component | P1 | 状態関数Unit Ready、再描画Minor |
| UI-07 | Reset/Clear | givenとユーザー入力あり | ResetまたはClear、続いてUndo | Resetはgiven保持、Clearは双方消去、履歴で復元 | Unit + Component | P1 | 履歴関数あり、ボタン統合Minor |
| UI-08 | Solve | 解ける/競合盤面 | Solveを押す | 解を表示するか、競合/候補切れの局所化エラーを表示 | Integration + Web E2E | P1 | solver Ready、UI操作Minor。fixture盤面 |
| UI-09 | サイドタブ | Solve表示 | Transform/Textを選ぶ | 選択tabとtabpanelが切り替わる | Component + Web E2E | P2 | Minor Changes。現行3タブ、Routerではない |
| UI-10 | 生成入力 | clue数欄 | 16/17/81/82/空/非整数でGenerate | 17〜81の整数だけ送信し、無効値は翻訳エラー | Unit + Component | P1 | validation抽出とWorker境界が必要 |
| UI-11 | 生成成功 | 固定seed/有効clue数 | Generate完了イベント | clue数が一致し、given/盤面が同期し成功状態 | Integration + Web E2E | P1 | Fake Worker必要。UIは現在暗号乱数seed |
| UI-12 | 検索進捗/完了 | 解がある盤面 | 開始後にWorkerイベントを送る | 開始/進捗/完了、解数/ノード数を表示し解を適用可能 | Integration + Component | P1 | Refactoring Required。Fake Worker/clock |
| UI-13 | 検索上限 | 複数解盤面 | 上限到達event | 部分結果を総数と誤表示せず、上限到達を示す | Unit + Integration | P1 | 表示ロジックUnit可、Worker Fake必要 |
| UI-14 | 検索エラー | 競合または不正worker応答 | Search実行 | localized errorを表示し盤面を上書きしない | Integration | P1 | parser/status Unit Ready、配線要分離 |
| UI-15 | cancel/古いjob | job A実行中、job B開始 | cancelしAの遅延応答も返す | Aを停止/無視し、B以外で画面を上書きしない | Integration | P1 | job IDは実装済み、順序制御Fake必要 |
| UI-16 | 盤面変換 | 盤面/givens | band/row/stack/column/digit交換、transpose/rotate/reflect | 同じ変換を両方に適用し、Undoで復元 | Unit + Component | P1 | 純粋関数Ready、フォームMinor |
| UI-17 | drag preview | band等ハンドル | 有効/無効先へdrag/drop/cancel | source/target/invalid preview、drop後だけ変換 | Component + Web E2E | P2 | Dioxus testのdrag対応確認が前提 |
| UI-18 | テキスト保存/読込 | 9行盤面 | Save text、編集、Load text | 9×9文字列を保存し、有効データを盤面へ反映 | Unit + Component | P1 | parser/formatter Unit Ready、UI Minor |
| UI-19 | テキスト検証 | 行数/長さ/文字の不正 | Load text | 行/列付き日本語/英語エラーを表示し盤面は維持 | Unit + Component | P1 | parser/i18n Unit Ready、表示Minor |
| UI-20 | Clipboard | 盤面文字列 | CopyのPromise成功/失敗を返す | 成否feedback。外部サービスへ送信しない | Component + Web E2E | P2 | Browser Clipboard Fake/権限設定 |
| UI-21 | 多言語 | 日本語表示 | 言語toggleを操作 | ラベル/メッセージ/検索状態を英語・日本語で切替 | Unit + Web E2E | P1 | catalog Unitあり、DOM確認未整備 |
| UI-22 | 空/満盤面 | 空盤面/81 clue | 表示しセル操作 | 81セルを範囲外アクセスや表示崩れなく扱う | Unit + Web E2E | P2 | 座標IDあり。viewport fixture |
| UI-23 | responsive layout | 固定locale/fixture | 390pxと1440px viewport | 盤面/操作が読め、重なりや横はみ出しなし | Visual + Web E2E / Playwright | P2 | CSSあり、baseline/CIなし |
| UI-24 | role/name操作 | 初期画面と各タブ | role/nameで主要controlを検索・操作 | 翻訳文字列に依存せず意味的に識別できる | Render + Web E2E | P2 | ARIA/ID一部あり、監査未実施 |

## 現行機能にないため対象外

- Router遷移、dialog、theme toggle: 該当機能を確認できません。
- Desktop E2E、OS設定変更、実ファイル削除、外部サービスへの書込み: 現行Web構成外です。
- Worker時間の性能閾値: 初期段階は決定的な上限制御/状態表示を検証し、端末差のある性能測定は分離します。

## 実装順

1. UI-01〜08とUI-18〜19のロジック/状態境界を既存`cargo test`で維持・補強します。
2. UI-10〜15のWorkerとclockをFake可能にします。
3. 盤面、操作、タブのcomponent境界とaccessible selectorを整え、UI-01〜09、UI-16〜17を試します。
4. Playwrightを導入可能な環境でUI-21〜24を含む主要利用フローを実ブラウザーで確認します。
5. `dioxus-test`の成熟度ゲート通過後、E2Eと重複しない小さなPoCを選びます。
