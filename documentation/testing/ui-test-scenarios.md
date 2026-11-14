# UIテストシナリオ

ケースは現行UIと`src/main.rs`の補助関数に基づく、確認対象の契約一覧です。実行済みテストの記録ではありません。準備状況の採点は[Dioxus UIテスト導入準備状況](dioxus-test-readiness.md)、階層と識別方針は[Dioxus UIテスト設計](dioxus-ui-testing.md)を参照してください。「Ready」は既存境界からテストを記述可能という意味で、実装済みを示しません。

| Test ID | 対象 | Given | When | Then / 目的 | 推奨階層・ツール | 優先度 | 準備・前提 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| UI-01 | セル入力 | 空セル/givensロックON | 1〜9を入力 | 値が盤面へ反映し、givenは変更されない | Component / 将来`dioxus-test` | P1 | Minor Changes。盤面境界とfixtureが必要 |
| UI-02 | 不正値入力 | 編集可能セル | 空、0、10、文字を入力 | 1〜9以外は空セル扱い、表示と状態が同期 | Unit + Component | P1 | `cell_value`はReady、UI配線はMinor |
| UI-03 | 競合/候補表示 | 重複または候補なし盤面 | 入力を変更 | 競合/候補なしセルと`aria-invalid`が更新 | Unit + Component | P1 | `BoardAnalysis` UnitはReady、描画はMinor |
| UI-04 | 候補表示スイッチ | Hintタブの「候補表示」セクション、候補OFF | Show candidates switchをON/OFF | 候補数字が表示／非表示になり、switchのchecked状態が同期する | Component | P2 | Minor Changes。盤面fixture |
| UI-05 | givens編集 | givenあり、ロックON | 初期数字セルの表示を確認。「初期数字を編集」で解除し、数字入りセルへフォーカスしてGを押す | 初期数字セルの隅にロック/解錠状態に応じた薄い錠前SVGが表示される。解除中のフォーカスでは反転配色とキャレットを維持し、候補表示中は枠線が点滅する。Gでgiven状態が切り替わりUndo/Redoできる。ロック中はG操作を受け付けない | Component + Integration | P1 | Refactoring Required。board/givens fixture |
| UI-06 | Undo/Redo | 変更履歴あり | Undo後Redo | 盤面/givensが共に復元し、disabled状態も更新 | Unit + Component | P1 | 状態関数Unit Ready、再描画Minor |
| UI-07 | Reset/Clear | givenとユーザー入力あり | ResetまたはClear、続いてUndo | Resetはgiven保持、Clearは双方消去、履歴で復元 | Unit + Component | P1 | 履歴関数あり、ボタン統合Minor |
| UI-08 | Solve | 解ける/競合盤面 | Solveを押す | 解を表示するか、競合/候補切れの局所化エラーを表示 | Integration + Web E2E | P1 | solver Ready、UI操作Minor。fixture盤面 |
| UI-09 | サイドタブ | Hint表示 | Generate/Solve/Transform/Textを選ぶ | 選択tabとtabpanelが切り替わり、初期表示はHint | Component + Web E2E | P2 | Minor Changes。現行5タブ、Routerではない |
| UI-10 | 生成入力 | 問題生成tabのclue数欄 | 16/17/81/82/空/非整数を入力してGenerate | 生成フォームは他タブと同様のパネル内に表示。Fastで17〜81の整数だけをWorkerへ送り、無効値は翻訳エラー。生成中は同じボタンがCancel generationに切り替わり、ヒント数欄を固定 | Unit + Component | P1 | 範囲検証は実装済み。Worker requestのFake検証は未整備 |
| UI-11 | Fast生成完了 | 固定seed/有効clue数 | Workerから完了またはエラーを送る | 指定ヒント数の盤面だけ反映し、given/盤面が同期 | Integration + Web E2E | P1 | UI配線は実装済み。Fake Workerとブラウザー検証は未整備 |
| UI-12 | 検索進捗/完了 | 解がある盤面 | Count solutionsを押し、開始後にWorkerイベントを送る | 開始中は同じボタンがCancel searchに切り替わり、進捗/完了後はCount solutionsへ戻る。解数/ノード数を表示し解を適用可能 | Integration + Component | P1 | Refactoring Required。Fake Worker/clock |
| UI-13 | 検索上限 | 複数解盤面 | 上限到達event | 部分結果を総数と誤表示せず、上限到達を示す | Unit + Integration | P1 | 表示ロジックUnit可、Worker Fake必要 |
| UI-14 | 検索エラー | 競合または不正worker応答 | Search実行 | localized errorを表示し盤面を上書きしない | Integration | P1 | parser/status Unit Ready、配線要分離 |
| UI-15 | cancel/古いjob | 解探索または問題生成中 | 解探索はCount solutionsボタン、生成はGenerate puzzleボタンを押し、遅延応答も返す | 各ボタンが同じ位置でキャンセル操作へ切り替わり、キャンセル後の遅延応答で画面を上書きしない | Integration | P1 | job IDは実装済み、順序制御Fake必要 |
| UI-16 | 盤面変換 | 盤面/givens | band/row/stack/column/digit交換、transpose/rotate/reflect | 同じ変換を両方に適用し、Undoで復元 | Unit + Component | P1 | 純粋関数Ready、フォームMinor |
| UI-17 | drag preview | band等ハンドル | 有効/無効先へdrag/drop/cancel | source/target/invalid preview、drop後だけ変換 | Component + Web E2E | P2 | Dioxus testのdrag対応確認が前提 |
| UI-18 | テキスト保存/読込 | 9行盤面 | Save text、編集、Load text | 9×9文字列を保存し、有効データを盤面へ反映 | Unit + Component | P1 | parser/formatter Unit Ready、UI Minor |
| UI-19 | テキスト検証 | 行数/長さ/文字の不正 | Load text | 行/列付き日本語/英語エラーを表示し盤面は維持 | Unit + Component | P1 | parser/i18n Unit Ready、表示Minor |
| UI-20 | Clipboard | 盤面文字列 | CopyのPromise成功/失敗を返す | 成否feedback。外部サービスへ送信しない | Component + Web E2E | P2 | Browser Clipboard Fake/権限設定 |
| UI-21 | 多言語 | 日本語表示 | 言語toggleを操作 | ラベル/メッセージ/検索状態を英語・日本語で切替 | Unit + Web E2E | P1 | catalog Unitあり、DOM確認未整備 |
| UI-22 | 空/満盤面 | 空盤面/81 clue | 表示しセル操作 | 81セルを範囲外アクセスや表示崩れなく扱う | Unit + Web E2E | P2 | 座標IDあり。viewport fixture |
| UI-23 | responsive layout | 固定locale/fixture | 390pxと1440px viewport | 盤面/操作が読め、重なりや横はみ出しなし | Visual + Web E2E / Playwright | P2 | CSSあり、baseline/CIなし |
| UI-24 | role/name操作 | 初期画面と各タブ | role/nameで主要controlを検索・操作 | 翻訳文字列に依存せず意味的に識別できる | Render + Web E2E | P2 | ARIA/ID一部あり、監査未実施 |
| UI-25 | テーマ切替 | 初期テーマがダーク | ライト／ダーク切替を操作して再読み込み | 配色とtheme-colorが切り替わり、選択がブラウザーに保存される | Unit + Web E2E | P2 | Theme状態はUnitで確認、localStorageと表示はブラウザー確認が必要 |
| UI-26 | Naked / Hidden Single強調 | Hintタブの「Singleの強調」セクション、矛盾・候補切れのない盤面 | Single強調と「候補フィルターを考慮」を切り替える | ONではフィルター後、OFFではフィルター前の候補でSingleを判定する。Naked Singleは候補が1つの空セル、Hidden Singleは行・列・ブロック内で数字の置き場所が1つの空セルを緑系色で強調する。候補表示との併用でも対象候補を識別できる | Component + Web E2E | P2 | BoardAnalysis判定は実装済み。UI描画確認は未実施 |
| UI-27 | 残り空セル数 | 盤面に空セルあり | 数字を入力・消去・Undo/Redo | 表示数が盤面の空欄数に追従し、満盤面で0になる | Component + Web E2E | P2 | 表示処理は実装済み。UI描画確認は未実施 |
| UI-28 | 候補フィルター | Hintタブの「候補フィルター」セクション、反復回数1、各フィルターが成立する盤面 | 回数を1回から増やし、各フィルターを個別・併用でON/OFF | 1回は単一パスを行い、指定回数を上限として候補を絞る。候補に変化がなくなれば早期終了する。Locked Candidates、Naked / Hidden Subsets、Fish、XY-Wingの除外候補は取り消し線で示す。「Singleの強調」で候補フィルター考慮がONなら、Naked / Hidden Singleも絞り込み後の候補から検出される。回数欄は1〜100を受け付ける | Unit + Component + Web E2E | P2 | Locked Candidates、Pair / Triple / Quad、Fish size 2〜4、XY-Wingの単体テストあり。UI描画確認は未実施 |
| UI-29 | 候補フィルターマスター | マスターON、個別フィルターを1つ以上選択 | マスターをOFFにして再びON | OFFでは候補フィルターと反復回数欄が停止し、選択済みフィルターは適用されない。個別選択状態を保持し、ONへ戻すと再び適用する | Component + Web E2E | P2 | ロジックとUI配線は実装済み。ブラウザー確認は未実施 |
| UI-30 | 候補フィルター反復境界 | Naked Subsetで候補ペアが発生し、その後XY-Wingになる候補配置 | 反復回数を1回、続けて2回にして比較 | 1回目は同じ開始候補から全手法が除外を集めるためXY-Wingの追加除外は起きない。2回目は1回目の更新結果からXY-Wingを検出する | Unit | P1 | ラウンド単位のスナップショット処理を単体テストで確認 |

## 現行機能にないため対象外

- Router遷移、dialog: 該当機能を確認できません。
- Desktop E2E、OS設定変更、実ファイル削除、外部サービスへの書込み: 現行Web構成外です。
- Worker時間の性能閾値: 初期段階は決定的な上限制御/状態表示を検証し、端末差のある性能測定は分離します。
