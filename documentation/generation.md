# 問題生成

生成API、探索CLI、検証済み問題の保存方法を案内します。生成履歴や作業台帳はここには置きません。

## まず読む資料

| 知りたいこと | 資料 |
| --- | --- |
| FastとDiverseの違い、方式の選び方 | [生成方式](generation/strategies.md) |
| Diverseの17ヒント探索のしくみ | [Diverse探索の詳細](generation/diverse-details.md) |
| Minimal Puzzleの定義とヒント数を変える探索 | [Minimal Puzzle](generation/minimal-puzzles.md) |
| CLIコマンドの入口と例 | [CLIガイド](generation/cli.md) |
| Puzzle/Solution MinLexと同値判定 | [対称性正規化](generation/symmetry.md) |
| Solution-Space Pivot探索 | [Pivot](generation/pivot.md) |
| 検証済み問題のArchiveとFrontier | [Archive](generation/archive.md) |

## 概要

Fastは検証済みの基準盤面から指定数の一意解問題を作ります。Web UIで使うのはFastだけです。Diverse、minimal問題の収集、Archive操作はCLIから利用できます。

生成アルゴリズムの説明は[ソルバーのアルゴリズム](algorithm.md)、コードの配置は[アーキテクチャ](architecture.md)、一般的な実行方法は[使い方](usage.md)を参照してください。
