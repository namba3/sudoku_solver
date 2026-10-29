# Solution-Space Pivot

[生成資料](../generation.md) · [方式の概要](strategies.md) · [Diverseの詳細](diverse-details.md) · [CLIガイド](cli.md)

## 候補の探索

`--strategy solution-space-pivot`は検証済み17ヒント盤面から1ヒントを外し、16ヒント核へ数字を1つずつ加えて候補を調べます。候補位置には行・列・ブロック上で合法な値を使います。

候補の状態は次の4つに分けます。

| 結果 | 扱い |
| --- | --- |
| `Unique` | 一意性を証明できた候補。採用候補 |
| `Multiple` | 別解Witnessを使い、後続候補を枝刈り |
| `NoSolution` | 採用しない |
| `Unknown` | 予算内に結論が出ない。枝刈りに使わない |

候補順序はseedで決まり、`--pivot-mode random`は17個の削除位置から1つを選び、`exhaustive`は全位置を調べます。

## 予算と並列処理

- `--nodes`: 候補ごとの一意性検査上限。
- `--neighborhood-checks`: 1回のPivot呼び出しで検査する候補数。
- `--seconds`: 新しい生成runへ進むかを決める全体時間の目安。実行中のrunを直ちに中断しません。
- `--threads`: 独立候補の一意性検査に使うnative worker数。

既定worker数は有効CPU数の半分（最低1）です。Linux cgroup CPU quotaが検出できる場合はその上限を考慮します。WASM APIは1スレッドです。

候補はseedで決まる順にバッチ処理し、結果を元の順序で集約します。同じseedと上限ならスレッド数を変えても報告順は保たれます。バッチ内で得た反例は次のバッチから枝刈りに反映します。`exhaustive`では未使用の候補枠を後続削除位置へ回し、`random`では選んだ1位置に残り枠を使います。

## 親盤面と重複判定

CLIの親指定方法:

- checkpointにレコードがない場合は、seedから検証済み基準盤面を選びます。
- レコードがある場合は既定で順番に親を選びます。
- `--parent-id ID`を指定すると親を固定できます。

通常APIは親の一意性も予算付きで検査します。既知親向けAPIは、呼び出し元が一意性を再検査した親を渡す用途です。プールCLIは親解との整合性を確認してから使い、Puzzle MinLexとSolution MinLexの両方で既存クラスを照合します。重複判定が両方式で異なる場合はエラーにします。

`different_solution_class`は、完成盤面の正規化クラスが親と異なるという意味です。候補問題が新規クラスかどうかは別に判定します。

## 統計と実行例

削除位置ごとの候補数、Unique/Multiple/Unknown/NoSolution数、Witness枝刈り数、時間、証明ノードを`<output>.pivot-stats.jsonl`へ追記します。出力先は`--stats-output PATH`で変更できます。`random-17-witness`の試行統計も同じ設定先を基に別のJSONLへ記録します。統計ファイルはcheckpointとは別です。

```sh
cargo run --release --bin sudoku -- diverse \
  --target 10 --seed 20261027 --strategy solution-space-pivot \
  --pivot-mode exhaustive --threads 8 --nodes 100000 \
  --neighborhood-checks 10000 --seconds 3600 \
  --output pivot-pool.json
```

新規レコードができるたびにcheckpointを保存し、`--resume`で再検証後に続行できます。

## 実行上の注意

同期MinLex正規化や`SolutionSearch::next()`の間は協調停止できないため、実時間やノード数が指定値を超える場合があります。性能比較はreleaseビルドで、同じ親・seed・削除モード・候補上限を使ってください。OSのCtrl-Cを探索キャンセルへ接続する処理はありません。

キャンセルやノード上限で終了した候補は一意性を証明したことになりません。戦略全体の安全条件は[Diverse探索の詳細](diverse-details.md)を参照してください。
