# 生成・収集CLI

[生成資料](../generation.md) · [戦略概要](strategies.md) · [Archive](archive.md)

## 起動とヘルプ

CLIはnative実行用の`sudoku`バイナリに集約しています。

```sh
cargo run --release --bin sudoku -- --help
cargo run --release --bin sudoku -- diverse --help
cargo run --release --bin sudoku -- minimal --help
```

コマンド固有の全引数は`<command> --help`で確認できます。

## 保存先とcheckpoint

引数で保存先を指定しない場合、CLIは作業用ファイルを`data/`へ保存します。このディレクトリはGit管理対象外です。共有したい盤面はArchiveへ統合し、Archive自体を明示したパスへ保存してください。

| 用途 | 既定の保存先 |
| --- | --- |
| Diverseプール | `data/17-clue-pool.json` |
| Minimal Puzzle Archive | `data/minimal-puzzle-archive.json` |
| Minimal collect checkpoint | `data/minimal-collect-<ヒント数>[-to-...].json` |
| Multi-Clue Frontier checkpoint | `data/multi-clue-frontier.json` |

Archiveは形式version 1とcanonical-key version 1、レコードchecksumを持ち、読み込み時に盤面と最小性の証拠を再検証します。Minimal collectとFrontierのcheckpointはversion 1、Diverseプールcheckpointはversion 7です。Diverseの再開時は各レコードの一意性、正規化key、親子関係を再確認します。異なるversionのcheckpointは直接再開できません。

## よく使うコマンド

### Archiveの確認と統合

```sh
cargo run --release --bin sudoku -- archive stats --archive minimal-puzzles.json --nodes 1000000
cargo run --release --bin sudoku -- archive verify --archive minimal-puzzles.json --nodes 1000000
cargo run --release --bin sudoku -- archive import --source incoming.json --archive minimal-puzzles.json --nodes 1000000
cargo run --release --bin sudoku -- archive seed-fast-bases --archive minimal-puzzles.json --nodes 1000000
```

importはsourceとdestinationを全件再検証し、Puzzle MinLexで重複を除いて保存します。形式や検証条件は[Archive資料](archive.md)を参照してください。

### Minimal収集

```sh
cargo run --release --bin sudoku -- minimal collect \
  --clues 19 --to 17 --remove 3 --add 1 --target 10 \
  --nodes 1000000 --checks 100000 --checkpoint collect-19-to-17.json
```

候補操作は`--clues N --remove R --add S`で指定します。目標ヒント数は`N-R+S`です。同数交換も`--remove R --add R`で指定します。

### Frontierと並列scan

```sh
cargo run --release --bin sudoku -- frontier descend \
  --from 20 --to 17 --remove 4 --add 1 --max-sources 50 \
  --archive minimal-puzzles.json --checkpoint frontier-20-17.json

cargo run --release --bin sudoku -- scan \
  --archive minimal-puzzles.json --from 17 --to 20 \
  --seconds 3600 --nodes 1000000 --checks 1088
```

`frontier descend`はArchiveにある指定ヒント数のMinimal盤面から、より少ないヒント数を探索します。`scan`はヒント数別に並列探索し、Minimalと再検証できた盤面だけを逐次Archiveへ登録します。既定のworker数は論理CPU数の半分で、`--threads`で変更できます。

### Diverseと18ヒントからの探索

```sh
cargo run --release --bin sudoku -- diverse \
  --target 100 --seed 20261027 --strategy adaptive \
  --max-intermediate-clues 19 --output diverse-pool.json

cargo run --release --bin sudoku -- minimal collect \
  --clues 18 --remove 2 --add 2 --target 100 --attempts 1000 \
  --archive minimal-puzzles.json --checkpoint collect-18.json

cargo run --release --bin sudoku -- frontier descend \
  --from 18 --to 17 --remove 2 --add 1 \
  --archive minimal-puzzles.json --checkpoint frontier-18-to-17.json
```

Diverseの戦略選択は[Diverse探索の詳細](diverse-details.md)、Minimalの定義とヒント数を変える探索は[Minimal Puzzle](minimal-puzzles.md)を参照してください。18ヒントの収集例を実行するには、Archiveに検証済み18ヒントMinimal盤面が必要です。

## Checkpointと予算

`diverse --resume`はversion 7のcheckpointだけを受け付け、保存済みrecordを再検証してから続行します。Minimal collectは共通Archiveとcheckpointを使います。checkpointやArchiveの保存先は引数で指定してください。

`diverse`では`--max-candidates`が生成run数の上限、`--attempts`が各戦略段階の内部試行上限です。Adaptiveでは複数段階を順に試します。成功・予算切れのどちらも1 runとして`candidate_runs`に記録し、次のseed位置を`seed_cursor`に記録します。現在は1 runごとに両方を1進めます。既存checkpointを再開する場合はversion 7形式に揃えてください。

`diverse`は`--nodes`で一意性検査などのノード数、`--neighborhood-checks`で候補検査数を制限します。`minimal collect`、`frontier descend`、`scan`は`--nodes`と`--checks`を使います。`diverse --seconds`は新しい生成runへ進む全体時間の目安で、実行中の生成runを直ちに中断する期限ではありません。`scan --seconds`も生成・最小性検証の実行中には超過する場合があります。予算切れの候補は一意と見なされません。Pivotの並列数や統計ファイルは[Pivot資料](pivot.md)にまとめています。
