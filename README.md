# Sudoku Solver

Rust製の数独ソルバーと、Dioxusで作ったブラウザー向けUIです。盤面の入力・解答・一意解の問題生成に加え、候補数字や解の数を表示できます。ソルバーは一手ヒント判定も備えていますが、ブラウザーUIのヒント操作は一時的に非表示です。テキスト形式の問題読み込み、重複や候補切れの表示にも対応しています。

公開中のアプリ: <https://namba3.github.io/sudoku_solver/>

## 資料

詳しい説明は[`documentation/`](documentation/README.md)にまとめています。ここでは入口と基本的な開発コマンドを案内します。

- [アルゴリズム](documentation/algorithm.md)：ビットマスク、候補数字、MRV、バックトラック
- [アプリとコードの構成](documentation/architecture.md)：ライブラリ、Web UI、ファイルの役割
- [使い方と盤面形式](documentation/usage.md)：ブラウザー操作、テキスト形式、Rust API
- [開発とビルド](documentation/development.md)：必要なツール、確認コマンド

## 開発を始める

RustとDioxus CLIを使います。CLIはプロジェクトのDioxus依存関係と同じ0.7系を使用してください。

```sh
cargo test
dx serve --platform web
```

GitHub Pages用の生成物は`docs/`に置きます。ここは公開用Web資材の出力先であり、解説資料の置き場ではありません。資料の編集先は`documentation/`です。
