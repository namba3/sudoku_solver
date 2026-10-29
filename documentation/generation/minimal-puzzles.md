# Minimal Puzzle

[生成資料](../generation.md) · [CLIガイド](cli.md) · [Archive](archive.md)

## Minimalの定義

盤面がMinimalであるとは、盤面全体が一意解を持ち、どのヒントを1つ消しても一意解でなくなることです。検証予算内に結論が出ない場合はUnknownとして扱い、Minimalとは認定しません。

この判定は、その配置にある各ヒントが必要であることを示します。同じ解を持つ別配置の盤面が存在しないことや、その解に必要なヒント数が最小であることまでは示しません。

## ヒント数を変える探索

`minimal collect`と`frontier descend`は、ヒント数Nの検証済みMinimal盤面からR個を外し、親の解に一致するS個を加えて、`N - R + S`個のヒントを持つ候補を調べます。たとえば19から17へ進む場合は`--remove 3 --add 1`、18から17へ進む場合は`--remove 2 --add 1`を指定します。

削除した各ヒントの別解Witnessを使って追加位置を絞り、候補の一意性とMinimalityを改めて検証します。確認済みの結果だけがヒント数共通のArchiveへ登録されます。親盤面、checkpoint、予算の指定方法は[CLIガイド](cli.md)、保存形式と探索ノードの管理は[Archive資料](archive.md)を参照してください。
