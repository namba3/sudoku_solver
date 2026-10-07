# 開発とビルド

## 必要なもの

- Rust toolchain（Edition 2021対応）
- Dioxus CLI（プロジェクトのDioxus依存と同じ0.7系）
- WebAssembly target `wasm32-unknown-unknown`（WASM向けチェックやビルドに必要）
- `wasm-bindgen` CLI（`Cargo.lock`内のwasm-bindgenと同じバージョン。検索Workerのビルドに必要）

## ローカル開発

```sh
dx serve --platform web
```

CLIはDioxus本体と同じバージョン系列を使います。異なる世代のCLIでは、依存crateのfeature構成やビルド手順が合わず、ビルドに失敗することがあります。

## 確認コマンド

```sh
cargo fmt --check
cargo test
cargo check --target wasm32-unknown-unknown
# Optional: report native source coverage when cargo-llvm-cov is installed
cargo llvm-cov --summary-only
```

- `cargo fmt --check`はRustコードの整形状態を確認します。
- `cargo test`はソルバー・問題生成ライブラリの単体テストに加え、アプリの翻訳処理とテキスト盤面パーサーの単体テストを実行します。
- `cargo llvm-cov --summary-only`は、利用可能な場合にnative単体テストのソースカバレッジを表示します。WASM Workerや実ブラウザー上のDOM・操作はこの計測に含まれません。
- WASM向け`cargo check`はWeb targetとしてRustコードがコンパイルできるかを確認します。実ブラウザーでの操作確認とは別です。

## GitHub Actions

`.github/workflows/ci.yml`はpushとpull requestで次の確認を実行します。

- `cargo fmt --check`でRustコードの整形状態を確認します。
- `cargo test --locked`で単体テストを実行します。
- `wasm32-unknown-unknown`向けにWebアプリと検索Workerをそれぞれコンパイルします。

このCIはコンパイルと単体テストを確認します。ブラウザー上の操作確認やGitHub Pagesへの公開は行いません。

## GitHub Pages資材

`Dioxus.toml`の`web.app.base_path`は、GitHub PagesのプロジェクトURLに合わせて`/sudoku_solver/`になっています。公開資材は`docs/`直下に置きます。

`dx build --platform web --release`は通常のWebビルド確認に使えます。公開用バンドルは次のコマンドで作れます。解探索WorkerのWASM生成、Dioxusバンドル、GitHub Pages用の配置をまとめて行います。

```sh
bash scripts/build_pages.sh
```

Dioxus CLI 0.7はバンドルを`docs/public/`に作成します。スクリプトは参照中のindex、CSS、ハッシュ付きJS/WASMと検索Workerだけを`docs/`公開ルートへ反映し、不要になった生成資材を整理します。

RustやUIのソースを変更しても、既にある`docs/`のWeb資材は自動では更新されません。公開資材の更新後は`docs/index.html`が参照するJSとWASM、`docs/worker/`内のWorkerファイル、および`/sudoku_solver/`のbase pathを確認します。
