# 開発とビルド

## 必要なもの

- Rust toolchain（Edition 2021対応）
- Dioxus CLI（プロジェクトのDioxus依存と同じ0.7系）
- WebAssembly target `wasm32-unknown-unknown`（WASM向けチェックやビルドに必要）

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
```

- `cargo fmt --check`はRustコードの整形状態を確認します。
- `cargo test`はソルバーの単体テストを実行します。
- WASM向け`cargo check`はWeb targetとしてRustコードがコンパイルできるかを確認します。実ブラウザーでの操作確認とは別です。

## GitHub Pages資材

`Dioxus.toml`の`web.app.base_path`は、GitHub PagesのプロジェクトURLに合わせて`/sudoku_solver/`になっています。公開資材は`docs/`直下に置きます。

`dx build --platform web --release`は通常のWebビルド確認に使えます。公開用バンドルは次のコマンドで作れます。

```sh
dx bundle --platform web --release --out-dir docs
```

Dioxus CLI 0.7はバンドルを`docs/public/`に作成します。このリポジトリではGitHub Pagesの公開ルートが`docs/`なので、生成後に配信対象の`index.html`、CSS、ハッシュ付きJS/WASMを`docs/`直下の構成へ反映し、不要になった古いハッシュ付き資材を整理してください。`docs/`を丸ごと消してから生成すると、現在の公開ルートやGit管理ファイルを誤って失う可能性があります。

RustやUIのソースを変更しても、既にある`docs/`のWeb資材は自動では更新されません。公開資材の更新後は`docs/index.html`が参照するJSとWASM、および`/sudoku_solver/`のbase pathを確認します。
