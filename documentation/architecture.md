# アプリとコードの構成

## 全体

このリポジトリは、数独の探索処理を提供するRustライブラリと、それを呼び出すDioxus Web UIで構成されています。ソルバーは画面に依存せず、`Matrix`を受け取る関数として公開されています。

```mermaid
flowchart LR
    U[ブラウザーUI<br/>src/main.rs] -->|Matrixを渡す| L[公開API<br/>src/lib.rs]
    L --> S[探索・候補管理<br/>src/solver.rs]
    S -->|解けたかと更新済みMatrix| U
    U --> C[スタイル<br/>public/app.css]
    C -->|ビルドでコピー| P[docs/ GitHub Pages資材]
```

## ディレクトリと主要ファイル

| パス | 役割 |
| --- | --- |
| `src/lib.rs` | `Matrix`型、`solve`、`candidates_for`、`find_hint`を公開するライブラリ入口 |
| `src/solver.rs` | 重複チェック、候補計算、ヒント判定、再帰探索、単体テスト |
| `src/main.rs` | 初期盤面、セル入力、テキスト入出力、Solve/Clear、Undo/Redo、探索UI |
| `src/bin/sudoku_search_worker.rs` | Web Workerから解列挙ライブラリを呼び、探索イベントを送信 |
| `public/worker/entry.js` | ブラウザーWorkerのメッセージをWASM検索器へ中継 |
| `public/app.css` | Web UIのスタイル。ビルド時に配信資材へコピー |
| `sample_problem/` | テキスト形式の問題例 |
| `documentation/` | 編集する解説資料 |
| `docs/` | GitHub Pagesに配置する生成済みWeb資材 |
| `Cargo.toml` / `Cargo.lock` | Rust依存関係と解決済みバージョン |
| `Dioxus.toml` | Webターゲット、公開先、スタイル、Pagesのbase path |

## ソルバーの公開API

```rust
pub type Matrix = [[u8; 9]; 9];
pub fn solve(mtx: &mut Matrix) -> bool;
pub fn candidates_for(mtx: &Matrix, x: usize, y: usize) -> Vec<u8>;
pub fn find_hint(mtx: &Matrix) -> Result<Option<Hint>, HintError>;
pub struct SolutionSearch;
impl SolutionSearch {
    pub fn new(mtx: &Matrix) -> Result<Self, SearchError>;
}
impl Iterator for SolutionSearch {
    type Item = SearchEvent;
}
```

`solve`は盤面を直接更新します。解を見つけると`true`を返し、初期数字の重複や探索失敗では`false`を返します。成功時には数字1〜9だけで埋まった盤面になります。戻り値は成否のみで、失敗理由や解の個数は返しません。

`candidates_for`は空欄セルの行・列・3×3ブロックを調べ、置ける数字を昇順の`Vec<u8>`で返します。埋まったセルや盤面外の座標では空のベクターになります。

`find_hint`は唯一候補と隠れたシングル（行・列・3×3ブロック内で、ある数字を置ける場所が1セルだけ）を調べます。盤面を変更せず、位置・数字・手法を返します。探索や仮置きはせず、対応する手法で決まらないことは「解がない」ことを意味しません。

`SolutionSearch`は探索上限を持たないIteratorで、解と2,048ノードごとの進捗イベントを返します。全探索の終端まで消費した場合にだけ解数が確定します。途中でdropすれば探索を止められます。元の盤面は変更しません。UI WorkerはこのIteratorを消費し、UI向けの解数・ノード上限を適用します。

詳細は[アルゴリズム解説](algorithm.md)と[盤面形式・APIの使い方](usage.md)を参照してください。

## UIのデータの流れ

画面は盤面、元のヒント数字、メッセージ、テキスト欄をDioxus Signalで保持します。表示時に盤面から重複セルを計算し、該当セルの強調と`aria-invalid`へ反映します。

1. 数字セルの入力を`Matrix`へ反映します。入力したヒントセルは通常の入力セルとして扱います。
2. 入力中に重複を検出し、同じ数字が同じ行・列・3×3ブロックにあるセルを強調します。
3. 候補表示をオンにすると、各空欄について`candidates_for`の結果をセル内へ表示します。候補が0個のセルは別途強調します。
4. Solveボタンで重複と候補切れを先に確認し、問題がなければ盤面のコピーをソルバーへ渡します。
5. Count solutionsは現在の盤面をWeb Workerへ渡します。Workerは解を最大100件、探索ノードを約500,000件まで調べ、見つけた盤面と進捗をメッセージで返します。ノード上限はイベント間隔により最大2,047ノード超過する場合があります。
6. 探索結果は`job_id`を付けて受け取り、現在の検索と異なる古い結果を無視します。上限到達と全探索完了を別状態として表示します。
7. 盤面操作で結果を書き換えず、利用者が選んだ解だけをUndo可能な盤面変更として適用します。
8. Undo/Redoは盤面と初期ヒントの組を最大100件保持し、盤面を変える操作を復元します。新しい盤面変更が行われるとRedo履歴は破棄されます。
9. 成功時に解を画面へ反映します。探索で解が見つからない場合はメッセージを表示します。
10. テキスト読み込み前に9行×9文字か、使える記号かを検証します。`1`〜`9`は数字、`0`・`.`・`_`は空欄です。
11. テキスト出力では空欄を`_`に置き換えます。
12. 矢印キーで隣のセルへ移動し、数字入力後は次の空欄へフォーカスします。セル上のCtrl/Cmd+Z/Y操作も盤面履歴へ接続します。

## 生成物と編集元を分ける

CSSの編集元は`public/app.css`、Rust UIの編集元は`src/main.rs`です。`docs/`は公開用の生成資材なので、手書き解説を置かず、Webアプリの再ビルドで更新します。解説資料の編集元は`documentation/`です。
