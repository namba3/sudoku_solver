# Minimal Puzzle Archive

[生成資料](../generation.md) · [CLIガイド](cli.md) · [Minimal Puzzle](minimal-puzzles.md)

## Archive

`MinimalPuzzleArchive`はヒント数に関わらず検証済みのMinimal Puzzleを保持します。各recordには問題と解、一意性・minimalityの証拠、Puzzle/Solution MinLex key、来歴を含みます。

ロード時にはJSONのformat versionとチェックサムを確認し、解、一意性、minimality、正規化keyを再検証します。シリアライズされたstatusだけを証拠として信用しません。更新は一時ファイルのflush・sync後にatomic renameします。

`analyze_minimality`は盤面と既知解を受け取り、`analyze_minimality_from_puzzle`は盤面だけから最初の解を探します。どちらも各解探索にノード上限を適用します。進捗イベント単位で上限を確認するため、訪問ノードが指定値を最大2,047超える場合があります。盤面だけのAPIで上限までに最初の解が見つからない場合は`BudgetExceeded`を返し、無制限の通常solveへ切り替えません。`VerifiedMinimal`は問題全体の一意性と、各ヒントを1つ消すと別解が生じることを確認できた場合に限ります。

Archiveの統計、検証、import、export、Fast用seed登録は[CLIガイド](cli.md)に記載しています。既知のFast基準盤面をArchiveへ登録した後、生成器へ反映する場合は[`src/minimal_puzzle_seeds.rs`](../../src/minimal_puzzle_seeds.rs)の配列と件数コメントも更新します。

## Multi-Clue Frontier

`MultiClueFrontier`はArchive内のMinimal Puzzleをヒント数ごとの探索ノードとして管理します。ノードには親ID、試した操作、完了状態を記録し、checkpointから再開できます。`frontier descend`は指定した`--from`から`--to`へ`--remove R --add S`操作を適用します。

## Minimal収集

`minimal collect`はヒント数Nの親からRヒントを外し、親の解と一致するS個の数字を加えて、`N-R+S`ヒントの候補を探します。各削除Witnessを満たす追加位置集合だけを候補に残します。新しい別解が見つかれば、未検査候補をさらに絞ります。

一意性とMinimalityを確認できた候補だけをArchiveに登録します。予算切れの親はcheckpointに未完了として残り、`--checks`または`--nodes`を増やして再開できます。探索と操作例は[CLIガイド](cli.md)を参照してください。

`scan`はヒント数別に並列探索し、Minimalと再検証できた問題だけを逐次Archiveへ登録します。worker数と予算は[CLIガイド](cli.md)、Diverseの交換アルゴリズムは[Diverse探索の詳細](diverse-details.md)を参照してください。
